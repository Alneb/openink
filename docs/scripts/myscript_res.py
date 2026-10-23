#!/usr/bin/env python3
"""Dump a MyScript resource (.res, e.g. shk-diagram.res) as a tree (docs/SHAPE-RE.md).

Container format, read from the bytes:
  object   := MAGIC(90 fe 01 01) size:u32le body[size]      (size counts body)
  body     := type:u32le header... children...
  children := object (01 object)*                           (01 = "another sibling follows")
Leaves (type 0xfa0, a shape class) carry: 00 00 00 01, len:u16, name[len], then f32le values.
Bytes of an object after its last child are reported as a raw tail (trained data lives there).
Unknown types are dumped as hex + their f32/u32 reading so new layouts can be read off the output.
Usage: myscript_res.py <file.res> [--floats N]   (MyScript's data, kept local; output is not committed)
"""
import struct
import sys

MAGIC = b"\x90\xfe\x01\x01"
TAILS: list = []


def read_obj(b: bytes, at: int, depth: int, out: list):
    assert b[at:at + 4] == MAGIC, f"no object at {at:#x}"
    size = struct.unpack_from("<I", b, at + 4)[0]
    start, end = at + 8, at + 8 + size
    typ = struct.unpack_from("<I", b, start)[0]
    first_child = b.find(MAGIC, start + 4, end)
    head = b[start + 4:first_child if first_child >= 0 else end]
    idx = len(out)
    out.append([depth, at, typ, size, head, 0])
    p = first_child
    while 0 <= p < end:
        p = read_obj(b, p, depth + 1, out)
        if p < end and b[p] == 1 and b[p + 1:p + 5] == MAGIC:
            p += 1
        elif p < end and b[p:p + 4] != MAGIC:
            out[idx][5] = end - p  # raw tail after the children
            TAILS.append((at, p, end))
            break
    return end


def describe(typ: int, head: bytes) -> str:
    if typ == 0xFA0 and len(head) >= 6:
        n = struct.unpack_from("<H", head, 4)[0]
        name = head[6:6 + n].decode("latin-1")
        rest = head[6 + n:]
        fl = [round(x, 4) for x in struct.unpack_from(f"<{len(rest) // 4}f", rest[:len(rest) // 4 * 4])]
        return f"class {name!r} {fl}"
    k = len(head) // 4
    fl = [round(x, 4) for x in struct.unpack_from(f"<{k}f", head)] if k else []
    return f"head {len(head)} B: {head[:48].hex(' ')}{' …' if len(head) > 48 else ''} | f32 {fl[:16]}"


def main():
    b = open(sys.argv[1], "rb").read()
    out: list = []
    p = 0
    while p < len(b) and b[p:p + 4] == MAGIC:
        p = read_obj(b, p, 0, out)
        if p < len(b) and b[p] == 1:
            p += 1
    for depth, at, typ, size, head, tail in out:
        t = f" + raw tail {tail} B" if tail else ""
        print(f"{'  ' * depth}{at:#07x} type {typ:#x} size {size}{t}: {describe(typ, head)}")
    for owner, s0, e0 in TAILS:
        print(f"[tail of {owner:#x}] {s0:#x}..{e0:#x}: first 64 B {b[s0:s0 + 64].hex(' ')}")
    print(f"[myscript-res] {len(out)} objects, parsed to {p:#x} of {len(b):#x}", file=sys.stderr)


if __name__ == "__main__":
    main()
