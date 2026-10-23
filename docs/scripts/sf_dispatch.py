#!/usr/bin/env python3
"""Decode SurfaceFlingerHelper::processOnyxRequests' jump tables → code → handler (docs/SF-ONYX.md).

The dispatcher (0x60d008, fw 4.2.1) switches on the transact code through two halfword tables
(`ldrh w11,[x9,x8,lsl#1]; add x10,adr,x11,lsl#2; br x10`): one for 0x10002a+, one for 0xff0000+.
They are found by shape in the disassembly, not hardcoded. Each case is summarised by the named
calls it makes before it leaves (`b` back to the shared tail or `ret`).
Usage: sf_dispatch.py <surfaceflinger> <inv dir> <sf_protocol.json>  → <inv>/dispatch.tsv + stdout table
"""
import json
import re
import struct
import subprocess
import sys
from pathlib import Path

FUNC, SIZE = 0x60d008, 6020


def exempt(c: int) -> bool:
    """SurfaceFlingerHelper::CheckTransactCodeCredentials (0x60cee8), transcribed: True = any caller,
    no ACCESS_SURFACE_FLINGER check (onTransact skips the AOSP permission test for these)."""
    def bit(k, mask, top):
        return 0 <= k <= top and (mask >> k) & 1 == 1
    if c > 0x1003ff:
        return bit(c - 0xff0000, 0xfdff5effffd3, 0x2f) or bit(c - 0xff0060, 0x103ff, 0x10) or c == 0x100400
    if c <= 0x100070:
        return bit(c - 0x10002a, 0x7fc0e7c0003f, 0x2e)
    if c > 0x1000ff:
        return 0 <= c - 0x100100 < 3 or 0 <= c - 0x100200 < 2 or c == 0x100300
    return bit(c - 0x100071, 0x381ff81ff, 0x21)  # (movn imm) & 0x3ffffffff


def load_labels(inv: Path) -> dict[int, str]:
    out = {}
    for line in (inv / "labels.tsv").read_text().splitlines():
        f = line.split("\t")
        out[int(f[0], 16)] = f[2]
    return out


def disasm(sf: Path) -> list[tuple[int, str]]:
    txt = subprocess.run(["llvm-objdump", "-d", "--no-show-raw-insn", f"--start-address={FUNC:#x}",
                          f"--stop-address={FUNC + SIZE:#x}", str(sf)], capture_output=True, text=True, check=True).stdout
    rows = []
    for m in re.finditer(r"^\s+([0-9a-f]+):\s+(.*)$", txt, re.M):
        rows.append((int(m.group(1), 16), m.group(2).split("//")[0].strip()))
    return rows


def find_tables(rows) -> list[tuple[int, int, int, int]]:
    """(first code, count, table vaddr, base) per `sub/cmp/adrp/add/adr/ldrh/br` sequence."""
    tables = []
    for i, (a, ins) in enumerate(rows):
        if not ins.startswith("br\t") or i < 6:
            continue
        win = [r[1] for r in rows[i - 7:i]]
        adrp = next((re.search(r"adrp\s+x\d+, 0x([0-9a-f]+)", w) for w in win if w.startswith("adrp")), None)
        add = next((re.search(r"add\s+x\d+, x\d+, #0x([0-9a-f]+)", w) for w in win if w.startswith("add\tx9")), None)
        adr = next((re.search(r"adr\s+x\d+, 0x([0-9a-f]+)", w) for w in win if w.startswith("adr\t")), None)
        cmp_ = next((re.search(r"cmp\s+w8, #0x([0-9a-f]+)", r[1]) for r in rows[i - 9:i] if r[1].startswith("cmp\tw8")), None)
        if not (adrp and add and adr and cmp_):
            continue
        tables.append((a, int(cmp_.group(1), 16) + 1, int(adrp.group(1), 16) + int(add.group(1), 16), int(adr.group(1), 16)))
    return tables


def find_tail(rows) -> int:
    """The shared `return 0` tail: the most common unconditional branch target."""
    from collections import Counter
    c = Counter(int(m.group(1), 16) for _, ins in rows if (m := re.match(r"b\s+0x([0-9a-f]+)", ins)))
    return c.most_common(1)[0][0]


def find_compares(rows) -> dict[int, int]:
    """Codes matched outside the tables: `mov w8,#lo; movk w8,#hi,lsl #16; cmp w21,w8; b.eq|b.ne`."""
    out = {}
    for i in range(len(rows) - 3):
        lo = re.match(r"mov\s+w8, #0x([0-9a-f]+)", rows[i][1])
        hi = re.match(r"movk\s+w8, #0x([0-9a-f]+), lsl #16", rows[i + 1][1])
        br = re.match(r"b\.(eq|ne)\s+0x([0-9a-f]+)", rows[i + 3][1])
        if lo and hi and rows[i + 2][1].startswith("cmp\tw21, w8") and br:
            code = int(hi.group(1), 16) << 16 | int(lo.group(1), 16)
            out[code] = int(br.group(2), 16) if br.group(1) == "eq" else rows[i + 4][0]
    return out


def case_summary(rows, idx: dict[int, int], start: int, labels) -> list[str]:
    calls = []
    for a, ins in rows[idx[start]:]:
        m = re.match(r"bl\s+0x([0-9a-f]+)", ins)
        if m:
            name = labels.get(int(m.group(1), 16), "sub_" + m.group(1))
            calls.append(name.replace("_ZNK7android6Parcel", "Parcel::"))
        if ins.startswith("ret") or (ins.startswith("b\t") and a != start):
            break
    return calls


def main():
    sf, inv, spec = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])
    blob = sf.read_bytes()  # vaddr == file offset for the R and R+E segments of this binary
    labels = load_labels(inv)
    names = {v: k for k, v in json.loads(spec.read_text())["codes"].items()}
    rows = disasm(sf)
    idx = {a: i for i, (a, _) in enumerate(rows)}
    tables = find_tables(rows)
    print(f"[dispatch] {len(tables)} jump tables: " + ", ".join(f"{n} cases @ {t:#x}" for _, n, t, _ in tables))
    tail = find_tail(rows)
    print(f"[dispatch] shared tail (return 0) @ {tail:#x}")
    # The case index is code - bias; the bias is the sub/subs chain right before each table.
    biases = [0x10002a, 0xff0000]
    cases = dict(find_compares(rows))
    for (br, n, tab, base), bias in zip(tables, biases):
        for k in range(n):
            cases[bias + k] = base + 4 * struct.unpack_from("<H", blob, tab + 2 * k)[0]
    out = []
    for code in sorted(set(cases) | set(names)):
        tgt = cases.get(code, tail)  # below the first table, not compared → default → tail
        if tgt == tail and code not in names:
            continue
        calls = ["(no-op: return 0)"] if tgt == tail else case_summary(rows, idx, tgt, labels)
        out.append((code, names.get(code, "?"), tgt, calls))
    with open(inv / "dispatch.tsv", "w") as w, open(inv / "dispatch.md", "w") as md:
        md.write("| code | name | any app? | case | calls (in order, until the case leaves) |\n|---|---|---|---|---|\n")
        for code, name, tgt, calls in out:
            w.write(f"{code:#x}\t{name}\t{tgt:#x}\t{int(exempt(code))}\t{' > '.join(calls)}\n")
            short = " → ".join(dict.fromkeys(c.replace("plt_", "").replace("android::", "") for c in calls))
            md.write(f"| `{code:#x}` | {name} | {'yes' if exempt(code) else 'no'} | `{tgt:#x}` | {short} |\n")
    for code, name, tgt, calls in out:
        print(f"{code:#x} {name:<36} {tgt:#x}  {' > '.join(c[:60] for c in calls[:4])}")
    noop = [n for c, n, t, _ in out if t == tail]
    print(f"[dispatch] {len(out)} codes, {len(out) - len(noop)} handled here; no-op here: " + ", ".join(noop))


if __name__ == "__main__":
    main()
