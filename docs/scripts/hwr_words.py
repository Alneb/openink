#!/usr/bin/env python3
"""Group the author's pen strokes into words and render numbered contact sheets for transcription.

    hwr_words.py notes.json OUT_DIR [--per-sheet 40] [--text-only]

--text-only sheets just the word-shaped crops (width >= 1.8 x height, >= 2 strokes): math and drawings are
mostly skipped, so a sheet holds more transcribable words. Ids are the same either way.

Writes OUT_DIR/words.json ({"words": [{id, doc, page, strokes: [[x, y, t_ms], ...] per stroke, bbox}]}) and
OUT_DIR/sheet-NNN.png. A word = strokes linked by small horizontal gaps on the same text line, plus the small
marks around them (i dots, apostrophes, accents). Long strokes (rules, diagram lines) are left out.
Design and the labelling workflow: docs/HWR-ENGINE.md.
"""
import json
import statistics
import sys

from PIL import Image, ImageDraw, ImageFont

PEN = 5  # knote2 shapeType of a free pen stroke
WORD_GAP = 0.8  # a word break: a gap wider than this x the line's median stroke height


def bbox(pts):
    xs, ys = [p[0] for p in pts], [p[1] for p in pts]
    return [min(xs), min(ys), max(xs), max(ys)]


class Union:
    def __init__(self, n):
        self.p = list(range(n))

    def find(self, i):
        while self.p[i] != i:
            self.p[i] = self.p[self.p[i]]
            i = self.p[i]
        return i

    def join(self, a, b):
        self.p[self.find(a)] = self.find(b)


def page_words(strokes):
    """strokes: list of point lists. Returns lists of stroke indices, one per word, in reading order.

    Two stages with local scale (pages mix large headings and tiny subscripts): strokes that overlap
    vertically and sit close together form a text line; a line splits into words at gaps wider than
    WORD_GAP x its median stroke height. Small marks (dots, apostrophes) then join the nearest word.
    """
    boxes = [bbox(s) for s in strokes]
    hs = [max(b[3] - b[1], 1.0) for b in boxes]
    heights = [x for x in hs if x > 4]
    if not heights:
        return []
    h_page = statistics.median(heights)
    # Rules and diagram lines are long and flat or very tall: not text.
    text = [i for i, b in enumerate(boxes) if (b[2] - b[0]) < 25 * h_page and hs[i] < 9 * h_page]
    big = [i for i in text if max(boxes[i][2] - boxes[i][0], hs[i]) >= 0.35 * h_page]
    big_set = set(big)
    small = [i for i in text if i not in big_set]
    u = Union(len(strokes))
    for a_i, a in enumerate(big):
        for b in big[a_i + 1:]:
            A, B = boxes[a], boxes[b]
            overlap = min(A[3], B[3]) - max(A[1], B[1])
            gap = max(A[0], B[0]) - min(A[2], B[2])
            if overlap > 0.3 * min(hs[a], hs[b]) and gap < 1.6 * max(hs[a], hs[b]):
                u.join(a, b)
    lines = {}
    for i in big:
        lines.setdefault(u.find(i), []).append(i)
    words = []
    for line in lines.values():
        line.sort(key=lambda i: boxes[i][0])
        h_line = statistics.median(hs[i] for i in line)
        cur, right = [line[0]], boxes[line[0]][2]
        for i in line[1:]:
            if boxes[i][0] - right > WORD_GAP * h_line:
                words.append(cur)
                cur = []
            cur.append(i)
            right = max(right, boxes[i][2])
        words.append(cur)
    # Small marks join the word whose box (grown around it) contains their centre.
    wboxes = [bbox([p for k in w for p in strokes[k]]) for w in words]
    for i in small:
        cx, cy = (boxes[i][0] + boxes[i][2]) / 2, (boxes[i][1] + boxes[i][3]) / 2
        best = None
        for w, wb in zip(words, wboxes):
            hh = wb[3] - wb[1]
            if wb[0] - 0.2 * hh <= cx <= wb[2] + 0.2 * hh and wb[1] - 0.6 * hh <= cy <= wb[3] + 0.2 * hh:
                d = abs(cx - (wb[0] + wb[2]) / 2)
                if best is None or d < best[0]:
                    best = (d, w)
        if best:
            best[1].append(i)
    def key(w):
        b = bbox([p for k in w for p in strokes[k]])
        return (round(((b[1] + b[3]) / 2) / (1.5 * h_page)), b[0])
    return [sorted(w) for w in sorted(words, key=key)]


def render(word, scale_h=44, pad=6):
    pts = [p for s in word["strokes"] for p in s]
    b = bbox(pts)
    h = max(b[3] - b[1], 1)
    k = min(scale_h / h, 380 / max(b[2] - b[0], 1))
    w_px, h_px = int((b[2] - b[0]) * k) + 2 * pad, int(h * k) + 2 * pad
    img = Image.new("L", (max(w_px, 20), max(h_px, 20)), 255)
    dr = ImageDraw.Draw(img)
    for s in word["strokes"]:
        xy = [((p[0] - b[0]) * k + pad, (p[1] - b[1]) * k + pad) for p in s]
        if len(xy) > 1:
            dr.line(xy, fill=0, width=2)
        elif xy:
            dr.ellipse([xy[0][0] - 1, xy[0][1] - 1, xy[0][0] + 1, xy[0][1] + 1], fill=0)
    return img


def word_shaped(w):
    b = w["bbox"]
    return len(w["strokes"]) >= 2 and (b[2] - b[0]) >= 1.8 * max(b[3] - b[1], 1)


def sheets(words, out, per, prefix="sheet"):
    font = ImageFont.load_default()
    for n in range(0, len(words), per):
        chunk = words[n:n + per]
        cells = [(w["id"], render(w)) for w in chunk]
        cols, cw, ch = 4, 420, 80
        rows = (len(cells) + cols - 1) // cols
        sheet = Image.new("L", (cols * cw, rows * ch), 255)
        dr = ImageDraw.Draw(sheet)
        for i, (wid, img) in enumerate(cells):
            x, y = (i % cols) * cw, (i // cols) * ch
            dr.text((x + 2, y + 2), str(wid), fill=0, font=font)
            sheet.paste(img.crop((0, 0, min(img.width, cw - 30), min(img.height, ch - 4))), (x + 28, y + 2))
            dr.rectangle([x, y, x + cw - 1, y + ch - 1], outline=200)
        sheet.save(f"{out}/{prefix}-{n // per:03d}.png")


def main():
    src, out = sys.argv[1], sys.argv[2]
    per = int(sys.argv[sys.argv.index("--per-sheet") + 1]) if "--per-sheet" in sys.argv else 40
    d = json.load(open(src))
    words = []
    for doc in d["documents"]:
        for pi, p in enumerate(doc["pages"]):
            pen = [s for s in p["strokes"] if s["shapeType"] == PEN and not s["removed"] and s["points"]]
            strokes = [[[q[0], q[1], q[3]] for q in s["points"]] for s in pen]
            made = [s.get("createdAt") for s in pen]
            for w in page_words([[(q[0], q[1]) for q in s] for s in strokes]):
                # Grouping (and so the word ids the labels use) is unchanged; inside a word the strokes go in the
                # order they were written when the decoder kept createdAt (the page list is by shape id: random).
                if all(made[k] is not None for k in w):
                    w = sorted(w, key=lambda k: made[k])
                ws = [strokes[k] for k in w]
                words.append({"id": len(words), "doc": doc["id"], "page": pi, "strokes": ws,
                              "bbox": bbox([q for s in ws for q in s])})
    json.dump({"words": words}, open(f"{out}/words.json", "w"))
    if "--text-only" in sys.argv:
        words = [w for w in words if word_shaped(w)]
        sheets(words, out, per, "text")
    else:
        sheets(words, out, per)
    print(f"{len(words)} words sheeted, {(len(words) + per - 1) // per} sheets in {out}")


if __name__ == "__main__":
    main()
