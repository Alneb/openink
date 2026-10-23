#!/usr/bin/env python3
"""Decode knote2 (BOOX Notes) KSync documents into JSON: pages -> strokes -> points.

Reference decoder for docs/KNOTE2.md "KSync tree". Read-only, stdlib only.

    ksync_decode.py [--keep-removed] <ksync-root> [docId ...] > notes.json
        <ksync-root> holds document/<docId>/ and point/<docId>/ (a copy of /sdcard/.ksync).
        With no docId, every document is decoded.
        Shapes the user erased or deleted (shapeStatus REMOVED) are skipped; --keep-removed
        includes them with "removed": true.

Output: {"documents": [{"id", "pages": [{"id", "index", "origin", "width", "height",
         "strokes": [{"id", "shapeType", "pen", "reference", "color", "alpha", "thickness",
                      "zorder", "createdAt", "matrix", "removed", "points": [[x, y, pressure 0..1, tMs], ...]}]}]}]}
Coordinates are page pixels (1860 x 2480 on the Note Air 5C) after the shape's matrix; a stroke
may run past the page edge. `origin` places the page on the document canvas (page 2 can sit to
the right of page 1, page 3 below). `reference` strokes are copies shown on a neighbouring page:
skip them. Shapes that are not freehand (text, images, geometry) have pen null.
Pressure is normalised by the shape's own createArgs.maxPressure (4095 on this device).
A file that cannot be decoded is reported on stderr and skipped; the run goes on.
"""
import json
import os
import struct
import sys
import zipfile

# --- protobuf wire format (no schema needed: the field numbers are in KNOTE2.md) ---

def fields(buf):
    """Yield (field_number, wire_type, value) of one message."""
    i, n = 0, len(buf)
    while i < n:
        key, i = varint(buf, i)
        num, wt = key >> 3, key & 7
        if wt == 0:
            v, i = varint(buf, i)
        elif wt == 1:
            v, i = buf[i:i + 8], i + 8
        elif wt == 2:
            ln, i = varint(buf, i)
            v, i = buf[i:i + ln], i + ln
        elif wt == 5:
            v, i = buf[i:i + 4], i + 4
        else:
            raise ValueError(f"wire type {wt} at {i}")
        yield num, wt, v


def varint(buf, i):
    shift = v = 0
    while True:
        b = buf[i]
        i += 1
        v |= (b & 0x7F) << shift
        if b < 0x80:
            return v, i
        shift += 7


def as_int32(v):
    v &= 0xFFFFFFFF  # ints are sign-extended to 64 bits on the wire
    return v - (1 << 32) if v >= 1 << 31 else v


def as_f32(v):
    return struct.unpack("<f", v)[0]


# --- NoteShapeDocProto.ShapeInfoProto (field numbers: KNOTE2.md) ---

SHAPE_FIELDS = {1: "uniqueId", 2: "createdAt", 3: "updatedAt", 4: "color", 5: "thickness", 6: "zorder",
                7: "boundingRect", 8: "matrixValues", 9: "textStyle", 10: "text", 11: "createArgs",
                12: "shapeType", 13: "groupId", 14: "resource", 15: "shapeStatus", 16: "revisionId",
                17: "lineStyle", 18: "infoRevisionId", 19: "tagIdList", 20: "extra", 21: "linkIdList",
                22: "richText", 23: "fillColor", 24: "pointList", 25: "connectionBean", 26: "optionsRepo"}

# ShapeFactory.SHAPE_* that are freehand strokes, and the pen they are drawn with.
PENS = {2: "pencil-path", 3: "pencil-path", 4: "pencil-path", 5: "fountain", 15: "marker",
        21: "brush", 22: "charcoal", 47: "square", 60: "calligraphy-latin", 61: "calligraphy-asia"}
# A shape that shows another shape (e.g. a stroke crossing onto the next page); points are its own.
SHAPE_REFERENCE = 2000
# com.onyx.android.sdk.data.note.ShapeStatus: ENABLED 0, REMOVED 1 (erased or deleted).
STATUS_REMOVED = 1
DEFAULT_MAX_PRESSURE = 4095.0


def warn(msg):
    print(f"[ksync] WARNING: {msg}", file=sys.stderr)


def shape(msg):
    s = {}
    for num, wt, v in fields(msg):
        name = SHAPE_FIELDS.get(num, f"field{num}")
        if wt == 0:
            s[name] = as_int32(v) if name in ("color", "fillColor", "shapeType", "zorder", "shapeStatus") else v
        elif wt == 5:
            s[name] = as_f32(v)
        elif wt == 2:
            try:
                txt = v.decode("utf-8")
                s[name] = json.loads(txt) if txt[:1] in "{[" else txt
            except (UnicodeDecodeError, json.JSONDecodeError):
                s[name] = v.hex()
    return s


def shapes_of_zip(path):
    """A shape zip holds one entry: a message whose repeated field 1 is ShapeInfoProto."""
    with zipfile.ZipFile(path) as z:
        for name in z.namelist():
            for num, wt, v in fields(z.read(name)):
                if num == 1 and wt == 2:
                    yield shape(v)


def max_pressure(s):
    ca = s.get("createArgs")
    mp = ca.get("maxPressure") if isinstance(ca, dict) else None
    return float(mp) if isinstance(mp, (int, float)) and mp > 0 else DEFAULT_MAX_PRESSURE


# --- point files: <pageId>#<revisionId>#points, big-endian (KNOTE2.md "Point document format") ---

def point_file(path):
    """{shapeId: [(x, y, tiltPacked, pressureRaw, tMs), ...]}"""
    b = open(path, "rb").read()
    (xref,) = struct.unpack(">i", b[-4:])
    out = {}
    for k in range((len(b) - 4 - xref) // 44):
        e = b[xref + 44 * k: xref + 44 * (k + 1)]
        sid = e[:36].decode().strip()
        off, ln = struct.unpack(">ii", e[36:])
        n = (ln - 4) // 16
        out[sid] = [struct.unpack_from(">ffhhi", b, off + 4 + 16 * i) for i in range(n)]
    return out


def matrix_of(s):
    """3x3 row-major [sx, kx, tx, ky, sy, ty, 0, 0, 1] or None."""
    m = s.get("matrixValues")
    ref = (s.get("connectionBean") or {}).get("shapeReferenceBean") if isinstance(s.get("connectionBean"), dict) else None
    if ref and ref.get("matrixValues"):
        m = ref["matrixValues"]
    return m if isinstance(m, list) and len(m) == 9 else None


def apply(m, x, y):
    if not m:
        return x, y
    return m[0] * x + m[1] * y + m[2], m[3] * x + m[4] * y + m[5]


# --- pages: the newest virtual/page/pb file lists them, field 4 (float) is the 1-based index ---

def page_list(doc_dir):
    vp = os.path.join(doc_dir, "virtual", "page", "pb")
    best = None
    for f in sorted(os.listdir(vp)) if os.path.isdir(vp) else []:
        pages = []
        stamp = 0
        for num, wt, v in fields(open(os.path.join(vp, f), "rb").read()):
            if num != 1:
                continue
            p = {}
            for n2, w2, v2 in fields(v):
                if n2 == 1:
                    p["id"] = v2.decode()
                elif n2 == 3:
                    stamp = max(stamp, v2)
                elif n2 == 4 and w2 == 5:
                    p["index"] = as_f32(v2)
                elif n2 == 6:
                    r = json.loads(v2)  # where the page sits on the document canvas
                    p["origin"] = [r["left"], r["top"]]
                elif n2 == 8:
                    r = json.loads(v2)
                    p["width"], p["height"] = r["right"] - r["left"], r["bottom"] - r["top"]
            pages.append(p)
        if best is None or stamp > best[0]:
            best = (stamp, pages)
    return sorted(best[1], key=lambda p: p.get("index", 0)) if best else []


def decode_document(root, doc_id, keep_removed=False):
    doc_dir = os.path.join(root, "document", doc_id)
    pts_dir = os.path.join(root, "point", doc_id)
    latest = {}  # (pageId, uniqueId) -> shape with the highest updatedAt
    shape_dir = os.path.join(doc_dir, "shape")
    for f in sorted(os.listdir(shape_dir)) if os.path.isdir(shape_dir) else []:
        page_id = f.split("#", 1)[0]
        try:
            found = list(shapes_of_zip(os.path.join(shape_dir, f)))
        except (zipfile.BadZipFile, ValueError, IndexError, OSError) as e:
            warn(f"{doc_id}: shape zip {f} skipped ({type(e).__name__}: {e})")
            continue
        for s in found:
            key = (page_id, s.get("uniqueId"))
            if key not in latest or s.get("updatedAt", 0) >= latest[key].get("updatedAt", 0):
                latest[key] = s
    # The newest version decides: a stroke erased later is REMOVED in its latest entry.
    removed = {k for k, s in latest.items() if s.get("shapeStatus") == STATUS_REMOVED}
    if removed and not keep_removed:
        print(f"[ksync] {doc_id}: {len(removed)} removed (erased/deleted) shapes skipped", file=sys.stderr)
        latest = {k: s for k, s in latest.items() if k not in removed}
    pages = page_list(doc_dir)
    known = {p["id"] for p in pages}
    for page_id in sorted({k[0] for k in latest} - known):  # pages with shapes but no layout entry
        pages.append({"id": page_id, "index": None})
    cache = {}
    missing = {}  # point file -> freehand strokes left without points
    for p in pages:
        strokes = []
        for (pid, sid), s in latest.items():
            if pid != p["id"]:
                continue
            rev = s.get("revisionId")
            path = os.path.join(pts_dir, pid, f"{pid}#{rev}#points")
            if path not in cache:
                cache[path] = None  # None = missing or unreadable
                if os.path.exists(path):
                    try:
                        cache[path] = point_file(path)
                    except (struct.error, UnicodeDecodeError, ValueError, OSError) as e:
                        warn(f"{doc_id}: point file {os.path.basename(path)} skipped ({type(e).__name__}: {e})")
            if cache[path] is None and PENS.get(s.get("shapeType")):
                missing.setdefault(path, 0)
                missing[path] += 1
            raw = (cache[path] or {}).get(sid, [])
            mp = max_pressure(s)
            m = matrix_of(s)
            color = s.get("color", -16777216) & 0xFFFFFFFF
            strokes.append({
                "id": sid,
                "shapeType": s.get("shapeType"),
                "pen": PENS.get(s.get("shapeType")),
                # A display copy of a stroke owned by another page: skip it when converting.
                "reference": s.get("shapeType") == SHAPE_REFERENCE,
                "color": "#%06x" % (color & 0xFFFFFF),
                "alpha": (color >> 24) / 255,
                "thickness": s.get("thickness"),
                "zorder": s.get("zorder", 0),
                # When the shape was made (ms): the order the strokes were written in (word crops need it).
                "createdAt": s.get("createdAt"),
                "matrix": m,
                "removed": s.get("shapeStatus") == STATUS_REMOVED,
                "points": [[*apply(m, x, y), pr / mp, t] for x, y, _tilt, pr, t in raw],
            })
        strokes.sort(key=lambda st: (st["zorder"], st["id"]))
        p["strokes"] = strokes
    for path, n in sorted(missing.items()):
        warn(f"{doc_id}: point file {os.path.basename(path)} missing/unreadable: {n} strokes have no points")
    return {"id": doc_id, "pages": pages}


def main():
    args = sys.argv[1:]
    keep_removed = "--keep-removed" in args
    args = [a for a in args if a != "--keep-removed"]
    if not args:
        sys.exit(__doc__)
    root = args[0]
    ids = args[1:] or sorted(os.listdir(os.path.join(root, "document")))
    docs = []
    for d in ids:
        doc = decode_document(root, d, keep_removed)
        n = sum(len(p["strokes"]) for p in doc["pages"])
        pts = sum(len(s["points"]) for p in doc["pages"] for s in p["strokes"])
        print(f"[ksync] {d}: {len(doc['pages'])} pages, {n} shapes, {pts} points", file=sys.stderr)
        docs.append(doc)
    json.dump({"documents": docs}, sys.stdout)


if __name__ == "__main__":
    main()
