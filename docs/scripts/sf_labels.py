#!/usr/bin/env python3
"""Labels for Ghidra from surfaceflinger's own metadata (docs/SF-ONYX.md):
  - every MiniDebugInfo function (debugdata.elf from sf_inventory.py), and
  - every PLT stub, named after its import (llvm-objdump resolves them).
Writes <out>/labels.tsv: addr, size (0 = label only), ghidra-safe name, mangled name, signature.
Usage: sf_labels.py <surfaceflinger> <inv-dir>
"""
import re, subprocess, sys

SAFE = re.compile(r"[^A-Za-z0-9_:~.<>,\-]")


def safe(sig):
    """Qualified name without parameters, made legal for a Ghidra label."""
    depth, cut = 0, len(sig)
    for i, ch in enumerate(sig):  # cut at the first '(' outside template brackets
        if ch == "<":
            depth += 1
        elif ch == ">":
            depth -= 1
        elif ch == "(" and depth == 0 and not sig.startswith("(anonymous"):
            cut = i
            break
    return SAFE.sub("_", sig[:cut])[:200] or "unnamed"


def plt_labels(sf):
    """PLT stubs as llvm-objdump names them (entries are padded, so no fixed stride)."""
    d = subprocess.run(["llvm-objdump", "-d", "--section=.plt", sf], capture_output=True, text=True, check=True).stdout
    return [(int(a, 16), n) for a, n in re.findall(r"^([0-9a-f]+) <(.+)@plt>:", d, re.M)]


def main():
    sf, inv = sys.argv[1], sys.argv[2]
    rows = []  # mangled names come from nm without -C; c++filt gives the readable signature
    nm = subprocess.run(["nm", "-nS", f"{inv}/debugdata.elf"], capture_output=True, text=True, check=True).stdout
    found = [m for m in (re.match(r"([0-9a-f]+) ([0-9a-f]+) ([TtWw]) (.*)", l) for l in nm.splitlines()) if m]
    sigs = subprocess.run(["c++filt"], input="\n".join(m[4] for m in found), capture_output=True, text=True, check=True).stdout.splitlines()
    for m, sig in zip(found, sigs, strict=True):
        rows.append((int(m[1], 16), int(m[2], 16), safe(sig), m[4], sig))
    plt = plt_labels(sf)
    demangled = subprocess.run(["c++filt"], input="\n".join(n for _, n in plt), capture_output=True, text=True, check=True).stdout.splitlines()
    rows += [(a, 0, "plt_" + safe(d), n, d) for (a, n), d in zip(plt, demangled)]
    with open(f"{inv}/labels.tsv", "w") as f:
        for a, sz, n, mangled, sig in rows:
            f.write(f"{a:#x}\t{sz}\t{n}\t{mangled}\t{sig}\n")
    print(f"[labels] {len(rows) - len(plt)} functions + {len(plt)} PLT imports → {inv}/labels.tsv")


if __name__ == "__main__":
    main()
