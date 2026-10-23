#!/usr/bin/env python3
"""Find arm64 BL/B callers of kernel symbols by scanning every PT_LOAD word (no disassembler).
Usage: kxrefs.py <vmlinux.elf> <kallsyms.txt> <symbol>...   → caller function+offset per hit."""
import bisect
import pathlib
import struct
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from annotate_kstrings import load_segments  # noqa: E402


def main():
    elf, ksyms, targets = sys.argv[1], sys.argv[2], sys.argv[3:]
    rows = sorted((int(p[0], 16), p[2]) for p in (l.split() for l in open(ksyms))
                  if len(p) == 3 and p[1] in "tTwW")
    addrs = [a for a, _ in rows]
    by_name = {n: a for a, n in rows}
    data, segs = load_segments(elf)
    want = {by_name[t]: t for t in targets if t in by_name}
    for t in targets:
        if t not in by_name:
            print(f"[xref] MISSING {t}")
    for vaddr, off, size in segs:
        words = struct.unpack_from(f"<{size // 4}I", data, off)
        for i, w in enumerate(words):
            op = w & 0xFC000000
            if op not in (0x94000000, 0x14000000):  # BL / B
                continue
            imm = w & 0x3FFFFFF
            if imm & 0x2000000:
                imm -= 0x4000000
            pc = vaddr + 4 * i
            dst = pc + 4 * imm
            if dst in want:
                k = bisect.bisect_right(addrs, pc) - 1
                kind = "bl" if op == 0x94000000 else "b "
                print(f"[xref] {want[dst]} <- {kind} {rows[k][1]}+{pc - rows[k][0]:#x}")


if __name__ == "__main__":
    main()
