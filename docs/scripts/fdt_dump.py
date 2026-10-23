#!/usr/bin/env python3
"""Minimal flattened-device-tree → dts-like text (no dtc on the host).
Usage: fdt_dump.py <dtb|dtbo.img> [> out.dts]
Handles concatenated DTBs and Android DTBO images (magic 0xd7b7ab1e: one overlay per entry)."""
import struct
import sys


def show(val: bytes) -> str:
    parts = val.split(b"\0")
    if val.endswith(b"\0") and all(p and all(32 <= c < 127 for c in p) for p in parts[:-1]):
        return ", ".join(f'"{p.decode()}"' for p in parts[:-1])
    if len(val) % 4 == 0 and val:
        return "<" + " ".join(f"0x{x:x}" for x in struct.unpack(f">{len(val)//4}I", val)) + ">"
    return "[" + val.hex(" ") + "]"


def dump(d: bytes, base: int, out) -> int:
    magic, total, off_struct, off_strings = struct.unpack_from(">IIII", d, base)
    assert magic == 0xD00DFEED, hex(magic)
    strings = base + off_strings
    p, depth = base + off_struct, 0
    while True:
        tok, = struct.unpack_from(">I", d, p); p += 4
        if tok == 1:  # BEGIN_NODE
            end = d.index(b"\0", p); name = d[p:end].decode() or "/"
            p = base + ((end + 4 - base) & ~3)  # 4-byte alignment is relative to the tree
            out.write("  " * depth + name + " {\n"); depth += 1
        elif tok == 2:  # END_NODE
            depth -= 1; out.write("  " * depth + "};\n")
        elif tok == 3:  # PROP
            ln, nameoff = struct.unpack_from(">II", d, p); p += 8
            name = d[strings + nameoff:d.index(b"\0", strings + nameoff)].decode()
            out.write("  " * depth + f"{name} = {show(d[p:p+ln])};\n"); p = base + ((p + ln + 3 - base) & ~3)
        elif tok == 4:
            continue
        elif tok == 9:
            return total
        else:
            raise ValueError(f"bad token {tok} at {p-4:#x}")


def main():
    d = open(sys.argv[1], "rb").read()
    if d[:4] == b"\xd7\xb7\xab\x1e":  # Android dtbo: header then dt_table_entry[count]
        _, _, _, esize, count, eoff = struct.unpack_from(">6I", d, 0)
        for i in range(count):
            size, off, id_, rev, *custom = struct.unpack_from(">8I", d, eoff + i * esize)
            sys.stdout.write(f"/* dtbo entry #{i} at {off:#x}, {size} bytes, id {id_:#x} rev {rev:#x}"
                             f" custom {[hex(c) for c in custom]} */\n")
            dump(d, off, sys.stdout)
        return
    base, n = 0, 0
    while d[base:base + 4] == b"\xd0\x0d\xfe\xed":
        total, = struct.unpack_from(">I", d, base + 4)
        if base + total > len(d):  # truncated trailer, not a real tree
            sys.stdout.write(f"/* stray magic at {base:#x}, size {total} > file: skipped */\n")
            break
        sys.stdout.write(f"/* dtb #{n} at {base:#x}, {total} bytes */\n")
        dump(d, base, sys.stdout)
        base, n = base + ((total + 3) & ~3), n + 1


if __name__ == "__main__":
    main()
