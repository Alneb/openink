#!/usr/bin/env python3
"""Regression test for ksync_decode.py on a synthetic KSync tree (stdlib only).

Covers: point-file decoding, per-shape maxPressure, newest version wins, REMOVED shapes skipped
(kept with --keep-removed), a corrupt shape zip reported and skipped, missing point files counted.
    python3 docs/scripts/test_ksync_decode.py      → "[test-ksync] PASS" or exit 1
"""
import io
import json
import os
import pathlib
import struct
import subprocess
import sys
import tempfile
import zipfile

HERE = pathlib.Path(__file__).resolve().parent


def varint(n):
    n &= (1 << 64) - 1  # negative ints go on the wire sign-extended to 64 bits
    out = bytearray()
    while True:
        out.append((n & 0x7F) | (0x80 if n > 0x7F else 0))
        n >>= 7
        if not n:
            return bytes(out)


def field(num, value):
    if isinstance(value, float):
        return varint(num << 3 | 5) + struct.pack("<f", value)
    if isinstance(value, int):
        return varint(num << 3) + varint(value)
    b = value.encode() if isinstance(value, str) else value
    return varint(num << 3 | 2) + varint(len(b)) + b


def shape_msg(uid, updated, shape_type, rev, status=0, max_pressure=4095):
    m = field(1, uid) + field(3, updated) + field(4, -16777216) + field(5, 2.5) + field(12, shape_type)
    m += field(11, json.dumps({"maxPressure": max_pressure, "dpi": 300}))
    m += field(16, rev)
    if status:
        m += field(15, status)
    return m


def shape_zip(path, shapes):
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as z:
        z.writestr("shape.pb", b"".join(field(1, s) for s in shapes))
    path.write_bytes(buf.getvalue())


def point_file(path, strokes):
    """strokes: {uid(36 chars): [(x, y, tilt, pressure, t), ...]} → the big-endian point format."""
    body, xref = bytearray(), bytearray()
    for uid, pts in strokes.items():
        off = len(body)
        chunk = struct.pack(">hh", 0, 0) + b"".join(struct.pack(">ffhhi", *p) for p in pts)
        body += chunk
        xref += uid.encode().ljust(36) + struct.pack(">ii", off, len(chunk))
    path.write_bytes(bytes(body) + bytes(xref) + struct.pack(">i", len(body)))


def run(root, *extra):
    r = subprocess.run([sys.executable, "-I", str(HERE / "ksync_decode.py"), *extra, str(root)],
                       capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f"[test-ksync] FAILED: decoder exited {r.returncode}: {r.stderr}")
    return json.loads(r.stdout), r.stderr


def main():
    failures = []
    check = lambda ok, what: ok or failures.append(what)
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        doc, page, rev = "d" * 32, "p" * 32, "r1"
        a, b, c = "a" * 36, "b" * 36, "c" * 36
        sd = root / "document" / doc / "shape"
        sd.mkdir(parents=True)
        # v1: a, b, c all enabled; v2 (newer): b erased, a updated with maxPressure 2047.
        shape_zip(sd / f"{page}#i1#1000.zip", [shape_msg(a, 1, 5, rev), shape_msg(b, 1, 5, rev), shape_msg(c, 1, 2000, rev)])
        shape_zip(sd / f"{page}#i2#2000.zip", [shape_msg(a, 2, 5, rev, max_pressure=2047), shape_msg(b, 2, 5, rev, status=1)])
        (sd / f"{page}#i3#3000.zip").write_bytes(b"PK\x03\x04 not really a zip")
        pd = root / "point" / doc / page
        pd.mkdir(parents=True)
        point_file(pd / f"{page}#{rev}#points", {a: [(10.0, 20.0, 0, 2047, 0), (11.0, 21.0, 0, 1024, 5)],
                                                  b: [(1.0, 1.0, 0, 100, 0)]})
        # A second page whose point file is missing.
        shape_zip(sd / f"{'q' * 32}#i1#1000.zip", [shape_msg("e" * 36, 1, 5, "r9")])

        out, err = run(root)
        strokes = {s["id"]: s for p in out["documents"][0]["pages"] for s in p["strokes"]}
        check(set(strokes) == {a, c, "e" * 36}, f"removed b skipped, others kept: {sorted(strokes)}")
        pa = strokes.get(a, {}).get("points", [])
        check(len(pa) == 2 and pa[0][:2] == [10.0, 20.0], f"a's points decoded: {pa}")
        check(len(pa) == 2 and abs(pa[0][2] - 1.0) < 1e-6 and abs(pa[1][2] - 1024 / 2047) < 1e-6,
              "pressure uses the newest version's maxPressure (2047)")
        check(strokes.get(c, {}).get("reference") is True, "shapeType 2000 is a reference")
        check("1 removed" in err, f"removed count reported: {err!r}")
        check("i3#3000.zip skipped" in err, "corrupt zip reported and skipped")
        check("1 strokes have no points" in err, "missing point file counted")

        out, _ = run(root, "--keep-removed")
        kept = {s["id"]: s for p in out["documents"][0]["pages"] for s in p["strokes"]}
        check(kept.get(b, {}).get("removed") is True and len(kept[b]["points"]) == 1, "--keep-removed keeps b, marked")

    for f in failures:
        print(f"[test-ksync] FAIL {f}")
    if failures:
        sys.exit(1)
    print("[test-ksync] PASS (8 checks)")


if __name__ == "__main__":
    main()
