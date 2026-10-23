#!/usr/bin/env python3
"""Rnote (.rnote, gzipped JSON, format 0.11) → the notes.json shape ksync_decode.py writes, so hwr_words.py and the
shape dataset can read the user's Rnote lecture notes too (local only: data/rnote/, gitignored).

    rnote_to_notes.py OUT.json FILE.rnote [FILE.rnote ...]

Each file is one document; its canvas is cut into Rnote's page format. Every brush stroke becomes a pen stroke
in **writing order** (Rnote's chrono counter `t`, kept as createdAt), its points [x, y, pressure, tMs]. Rnote keeps
no per-point time, so tMs counts 2 ms per point (the pen's ~480 Hz). Shapes and text are skipped.
"""
import gzip
import json
import sys

PEN_SHAPE = 5  # the knote2 free-pen shapeType hwr_words.py keeps


def pages_of(path):
    """The canvas split into Rnote's own page format (document.format width × height): each stroke goes to the page
    its box centre falls on (hwr_words groups lines per page, and a whole canvas at once is quadratic)."""
    snap = json.load(gzip.open(path))["data"]["engine_snapshot"]
    fmt = snap.get("document", {}).get("format") or {}
    pw, ph = fmt.get("width", 1123.0), fmt.get("height", 1587.0)  # older files keep no format: A4 at 96 dpi
    pages = {}
    for st in strokes_of(snap):
        xs = [q[0] for q in st["points"]]
        ys = [q[1] for q in st["points"]]
        key = (int(((min(ys) + max(ys)) / 2) // ph), int(((min(xs) + max(xs)) / 2) // pw))
        pages.setdefault(key, []).append(st)
    return [{"id": f"{path}#{r},{c}", "index": i, "origin": [c * pw, r * ph], "strokes": st}
            for i, ((r, c), st) in enumerate(sorted(pages.items()))]


def strokes_of(snap):
    out = []
    for comp, chrono in zip(snap["stroke_components"], snap["chrono_components"]):
        bs = (comp.get("value") or {}).get("brushstroke")
        if not bs:
            continue
        p = bs["path"]
        pts = [p["start"]] + [next(iter(seg.values()))["end"] for seg in p["segments"]]
        t = (chrono.get("value") or {}).get("t", 0)
        width = next(iter(bs["style"].values())).get("stroke_width")
        out.append({"id": f"{t:08d}", "shapeType": PEN_SHAPE, "pen": "ballpoint", "reference": False, "color": "#000000",
                    "alpha": 1.0, "thickness": width, "zorder": 0, "createdAt": t, "matrix": None, "removed": False,
                    "points": [[q["pos"][0], q["pos"][1], q.get("pressure", 0.5), 2 * i] for i, q in enumerate(pts)]})
    out.sort(key=lambda s: s["createdAt"])
    return out


def main():
    out, files = sys.argv[1], sys.argv[2:]
    docs = []
    for f in files:
        pages = pages_of(f)
        if pages:
            docs.append({"id": f, "pages": pages})
        print(f"[rnote] {f}: {sum(len(p['strokes']) for p in pages)} strokes on {len(pages)} pages", file=sys.stderr)
    json.dump({"documents": docs}, open(out, "w"))


if __name__ == "__main__":
    main()
