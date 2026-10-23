#!/usr/bin/env python3
"""Inventory of the Onyx code inside the stock surfaceflinger (docs/SF-ONYX.md).

The binary is stripped, but carries Android MiniDebugInfo (.gnu_debugdata: an xz'd ELF whose
.symtab names the local functions). This extracts it, lists every function in the contiguous
Onyx block (MemoryPainter.cpp .. ThemeManager) plus Onyx classes placed elsewhere, and writes:
  <out>/debugdata.elf        the decompressed symbol ELF (Ghidra can import its names)
  <out>/onyx_funcs.tsv       addr, size, demangled signature, class   (one row per function)
  <out>/onyx_classes.tsv     class, count, bytes
Usage: sf_inventory.py <surfaceflinger> <outdir>      (needs binutils nm + c++filt via nm -C)
"""
import collections, lzma, os, re, struct, subprocess, sys

# Onyx classes that live outside the block (RenderEngine shaders etc.): matched by name.
OUTSIDE = re.compile(r"CfaShaderFactory|CfaGrayDitherShaderFactory|BWGrayDitherShaderFactory|"
                     r"android::(EpdcManager|HWEpdcManager|SWEpdcManager|SurfaceFlingerHelper)::")
# Library noise inside the block (std templates, Clipper2, Skia inlines) is not Onyx logic.
NOISE = re.compile(r"^(std::|void std::|bool std::|unsigned |non-virtual thunk|virtual thunk|"
                   r"Clipper2Lib|Sk|google|perfetto)")


def section(path, name):
    """Bytes of one ELF64-LE section, by name."""
    b = open(path, "rb").read()
    shoff, = struct.unpack_from("<Q", b, 0x28)
    shentsize, shnum, shstrndx = struct.unpack_from("<HHH", b, 0x3A)
    def sh(i):
        return struct.unpack_from("<IIQQQQIIQQ", b, shoff + i * shentsize)
    stroff = sh(shstrndx)[4]
    for i in range(shnum):
        s = sh(i)
        nm = b[stroff + s[0]: b.index(b"\0", stroff + s[0])].decode()
        if nm == name:
            return b[s[4]: s[4] + s[5]]
    sys.exit(f"[inventory] FAILED: no section {name} in {path}")


def cls_of(sig):
    s = sig.split("(")[0]
    for _ in range(3):
        s = re.sub(r"<[^<>]*>", "", s)
    parts = s.split("::")
    return "::".join(parts[:-1]) or "(free)"


def main():
    sf, out = sys.argv[1], sys.argv[2]
    os.makedirs(out, exist_ok=True)
    dbg = os.path.join(out, "debugdata.elf")
    open(dbg, "wb").write(lzma.decompress(section(sf, ".gnu_debugdata")))
    print(f"[inventory] 1/3 MiniDebugInfo extracted → {dbg}")

    rows = []
    for line in subprocess.run(["nm", "-nSC", dbg], capture_output=True, text=True, check=True).stdout.splitlines():
        m = re.match(r"([0-9a-f]+) ([0-9a-f]+) ([TtWw]) (.*)", line)  # rows without a size are skipped
        if m:
            rows.append((int(m[1], 16), int(m[2], 16), m[4]))
    print(f"[inventory] 2/3 {len(rows)} named functions in the symbol table")

    first = next(a for a, _, s in rows if s.startswith("MemoryPainter::"))
    last = max(a for a, _, s in rows if s.startswith("android::ThemeManager::"))
    onyx, seen = [], set()
    for a, sz, s in rows:
        inside = first <= a <= last
        if (inside and not NOISE.match(s)) or (not inside and OUTSIDE.search(s)):
            if (a, s) not in seen:  # identical-code-folded aliases appear twice
                seen.add((a, s))
                onyx.append((a, sz, s, cls_of(s)))
    with open(os.path.join(out, "onyx_funcs.tsv"), "w") as f:
        for a, sz, s, c in onyx:
            f.write(f"{a:#x}\t{sz}\t{s}\t{c}\n")
    per = collections.OrderedDict()
    for _, sz, _, c in onyx:
        n, b = per.get(c, (0, 0))
        per[c] = (n + 1, b + sz)
    with open(os.path.join(out, "onyx_classes.tsv"), "w") as f:
        for c, (n, b) in per.items():
            f.write(f"{c}\t{n}\t{b}\n")
    print(f"[inventory] 3/3 block {first:#x}..{last:#x}: {len(onyx)} Onyx functions, "
          f"{sum(x[1] for x in onyx)} bytes, {len(per)} classes → {out}/onyx_funcs.tsv")


if __name__ == "__main__":
    main()
