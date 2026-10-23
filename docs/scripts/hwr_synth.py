#!/usr/bin/env python3
"""Synthetic online handwriting from Hershey stroke fonts, plus the per-point features the HWR model reads.

    hwr_synth.py FONT_DIR OUT.npz N [SEED] [--texts labels.tsv] [--deslant] [--mild]  # pre-generate N words
                                     # (--mild: gentler elastic warp, closer to a smooth hand)
    hwr_synth.py FONT_DIR --sheet OUT.png [SEED] [--mild]           # eyeball 40 samples

The feature code here is the reference the Rust crate (openink-hwr) must match. Design: docs/HWR-ENGINE.md.
"""
import math
import os
import random
import sys

import numpy as np

GREEK_UP = "ΑΒΓΔΕΖΗΘΙΚΛΜΝΞΟΠΡΣΤΥΦΧΨΩ"  # greeks.jhf maps A..X to these, in Greek alphabetical order
GREEK_LO = "αβγδεζηθικλμνξοπρστυφχψω"
# Greek letters that look like Latin ones are left out: labels spell those in Latin.
GREEK = "ΓΔΘΛΞΠΣΦΨΩ" + "αβγδεζηθικλμνξπρστυφχψω"
ALPHABET = "".join(chr(c) for c in range(32, 127)) + GREEK + "·"
BLANK = 0  # CTC blank; character k is class k + 1
CLASS = {c: i + 1 for i, c in enumerate(ALPHABET)}

FONTS = {"futural": 0.3, "rowmans": 0.25, "scripts": 0.25, "cursive": 0.2}
POINT_FEATURES = 7
STEP = 0.1  # resampling step, as a fraction of the word scale


def load_font(path):
    """jhf: per glyph, a 5-char id, 3-char vertex count, left/right bounds, then x/y pairs offset by 'R';
    ' R' lifts the pen. Returns glyph list indexed by codepoint - 32: (left, right, strokes)."""
    glyphs = []
    for line in open(path, encoding="ascii"):
        line = line.rstrip("\n")
        if len(line) < 10:
            continue
        body = line[8:]
        left, right = ord(body[0]) - 82, ord(body[1]) - 82
        strokes = [[]]
        for i in range(2, len(body) - 1, 2):
            pair = body[i:i + 2]
            if pair == " R":
                strokes.append([])
            else:
                strokes[-1].append((ord(pair[0]) - 82, ord(pair[1]) - 82))
        glyphs.append((left, right, [s for s in strokes if s]))
    return glyphs


class Fonts:
    def __init__(self, font_dir):
        self.latin = {n: load_font(os.path.join(font_dir, n + ".jhf")) for n in FONTS}
        self.greek = load_font(os.path.join(font_dir, "greeks.jhf"))

    def glyph(self, font, ch):
        if ch in GREEK_UP:
            return self.greek[ord("A") - 32 + GREEK_UP.index(ch)]
        if ch in GREEK_LO:
            return self.greek[ord("a") - 32 + GREEK_LO.index(ch)]
        if ch == "·":  # a period raised to mid-height
            l, r, st = self.latin[font][ord(".") - 32]
            return l, r, [[(x, y - 8) for x, y in s] for s in st]
        return self.latin[font][ord(ch) - 32]


# ---------------------------------------------------------------- text sampling

def dictionary(path="/usr/share/dict/words"):
    try:
        ws = [w.strip() for w in open(path, encoding="utf-8", errors="ignore")]
    except OSError:
        return ["the", "of", "and", "energy", "field", "wave", "state"]
    return [w for w in ws if w.isascii() and w.isalpha() and 1 <= len(w) <= 14]


MATH = ["=", "+", "-", "/", "(", ")", "^", "·", "<", ">", "|", ",", "'"]


def sample_text(rng, words, label_texts):
    r = rng.random()
    if label_texts and r < 0.2:
        return rng.choice(label_texts)
    if r < 0.62:
        w = rng.choice(words)
        k = rng.random()
        w = w.lower() if k < 0.6 else w.capitalize() if k < 0.9 else w.upper() if len(w) < 6 else w
        if rng.random() < 0.12:
            w += rng.choice(".,:;)'")
        return w
    if r < 0.72:  # numbers, dates
        k = rng.random()
        if k < 0.4:
            return str(rng.randint(0, 10 ** rng.randint(1, 4)))
        if k < 0.7:
            return f"{rng.randint(0, 99)}.{rng.randint(0, 999)}"
        return f"{rng.randint(1, 31)}/{rng.randint(1, 12)}/{rng.randint(0, 99):02d}"
    if r < 0.88:  # math-ish tokens
        sym = "abcdefghknmprstuvwxyzABEFHLMNPRTVXYZ" + GREEK
        parts = []
        for _ in range(rng.randint(1, 4)):
            t = rng.random()
            parts.append(rng.choice(sym) if t < 0.5 else str(rng.randint(0, 9)) if t < 0.7 else rng.choice(MATH))
        return "".join(parts).strip() or "x"
    return "".join(rng.choice(ALPHABET[1:]) for _ in range(rng.randint(1, 6)))  # any character


# ---------------------------------------------------------------- rendering

def render(fonts, text, rng, mild=False):
    """Strokes (list of (n,2) float arrays, y down) for `text` written in a random style.
    `mild`: a gentler elastic warp, closer to the author's smooth hand (letters ~21 units tall)."""
    font = rng.choices(list(FONTS), weights=list(FONTS.values()))[0]
    script = font in ("scripts", "cursive")
    strokes, delayed = [], []
    x, drift = 0.0, 0.0
    track = rng.uniform(-2.5, 0.5) if script else rng.uniform(-2.5, 1.5)
    for ch in text:
        left, right, st = fonts.glyph(font, ch)
        k = rng.gauss(1.0, 0.06)
        drift += rng.gauss(0, 0.35)
        for s in st:
            a = np.array(s, float)
            a = np.stack([x + (a[:, 0] - left) * k, a[:, 1] * k + drift], 1)
            ext = np.ptp(a, 0)
            small = len(st) > 1 and max(ext) < 5.5
            (delayed if small and rng.random() < 0.6 else strokes).append(a)
        x += (right - left) * k + track + rng.gauss(0, 0.4)
    if not strokes:
        strokes, delayed = delayed, []
    strokes = join_close(strokes, rng, 1.6 if script else 0.6)
    strokes += delayed
    # Per word: slant, aspect, rotation, smooth elastic warp, pen noise.
    slant = rng.uniform(-0.2, 0.5)
    aspect = rng.uniform(0.5, 1.2)
    rot = math.radians(rng.gauss(0, 2.5))
    c, s_ = math.cos(rot), math.sin(rot)
    amp, per = ((0.05, 0.5), (25, 70)) if mild else ((0.2, 1.4), (10, 45))
    waves = [(rng.uniform(*amp), rng.uniform(*per), rng.uniform(0, 6.3), rng.randint(0, 1)) for _ in range(3)]
    out = []
    for a in strokes:
        a = densify(a, 0.5)
        y_base = a[:, 1] - 9  # Hershey baseline at y = 9
        b = np.stack([(a[:, 0] - slant * y_base) * aspect, a[:, 1]], 1)
        for amp, period, ph, axis in waves:
            b[:, axis] += amp * np.sin(2 * math.pi * b[:, 1 - axis] / period + ph)
        b = b @ np.array([[c, s_], [-s_, c]])
        b += np.random.default_rng(rng.randint(0, 2 ** 31)).normal(0, 0.12, b.shape)
        if len(b) >= 3:
            b[1:-1] = (b[:-2] + b[1:-1] + b[2:]) / 3
        out.append(b)
    return out


def join_close(strokes, rng, tol):
    """Cursive: chain a stroke into the previous one when the pen would barely have to lift."""
    out = [strokes[0]]
    for a in strokes[1:]:
        if np.hypot(*(a[0] - out[-1][-1])) < tol and rng.random() < 0.8:
            out[-1] = np.concatenate([out[-1], a])
        else:
            out.append(a)
    return out


def densify(a, step):
    if len(a) < 2:
        return a.astype(float)
    pts = [a[0]]
    for p, q in zip(a[:-1], a[1:]):
        n = max(1, int(np.hypot(*(q - p)) / step))
        pts.extend(p + (q - p) * (i / n) for i in range(1, n + 1))
    return np.array(pts, float)


# ---------------------------------------------------------------- features (mirror in Rust)

def resample(a, step):
    """Points every `step` along the path; endpoints kept."""
    if len(a) < 2:
        return a[:1].copy()
    seg = np.hypot(*np.diff(a, axis=0).T)
    cum = np.concatenate([[0], np.cumsum(seg)])
    if cum[-1] < 1e-9:
        return a[:1].copy()
    n = max(1, int(round(cum[-1] / step)))
    t = np.linspace(0, cum[-1], n + 1)
    return np.stack([np.interp(t, cum, a[:, 0]), np.interp(t, cum, a[:, 1])], 1)


def slant(strokes):
    """Writing slant as dx/dy: length-weighted over the mostly vertical segments (|dy| > |dx|), signs
    folded so up- and down-strokes agree. 0 when there are none. Clamped to +-1 (45 degrees)."""
    num = den = 0.0
    for s in strokes:
        if len(s) < 2:
            continue
        d = np.diff(s, axis=0)
        v = np.abs(d[:, 1]) > np.abs(d[:, 0])
        if not v.any():
            continue
        dx, dy = d[v, 0] * np.sign(d[v, 1]), np.abs(d[v, 1])
        w = np.hypot(dx, dy)
        num += float((w * dx / dy).sum())
        den += float(w.sum())
    return float(np.clip(num / den, -1, 1)) if den > 0 else 0.0


def features(strokes, deslant=False):
    """Per point: dx, dy (from the previous point, across pen lifts), pen_up (1 at each stroke start),
    y relative to the word centre, cos/sin of the direction to the next point, and the turn cosine.
    Lengths are in units of the word scale: bbox height, floored at a tenth of the width."""
    strokes = [np.asarray(s, float)[:, :2] for s in strokes if len(s)]
    if deslant:  # shear upright: x -= slant * y (y down, so a forward slant has dx/dy < 0)
        k = slant(strokes)
        strokes = [np.stack([s[:, 0] - k * s[:, 1], s[:, 1]], 1) for s in strokes]
    allp = np.concatenate(strokes)
    lo, hi = allp.min(0), allp.max(0)
    scale = max(hi[1] - lo[1], 0.1 * (hi[0] - lo[0]), 1e-6)
    yc = (lo[1] + hi[1]) / 2
    pts, up = [], []
    for s in strokes:
        r = resample((s - [lo[0], yc]) / scale, STEP)
        pts.append(r)
        up.append(np.r_[1.0, np.zeros(len(r) - 1)])
    p = np.concatenate(pts)
    up = np.concatenate(up)
    d = np.diff(p, axis=0, prepend=p[:1])
    nxt = np.diff(p, axis=0, append=p[-1:])
    ln = np.hypot(*nxt.T)
    dirs = np.where(ln[:, None] > 1e-9, nxt / np.maximum(ln, 1e-9)[:, None], 0)
    prev_dirs = np.vstack([dirs[:1], dirs[:-1]])
    turn = (dirs * prev_dirs).sum(1)
    f = np.stack([np.clip(d[:, 0], -5, 5), np.clip(d[:, 1], -5, 5), up, p[:, 1], dirs[:, 0], dirs[:, 1], turn], 1)
    return f.astype(np.float32)


def encode(text):
    return [CLASS[c] for c in text if c in CLASS]


# ---------------------------------------------------------------- CLI

def generate(fonts, n, seed, label_texts, words, deslant=False, mild=False):
    rng = random.Random(seed)
    np.random.seed(seed)
    xs, ys = [], []
    while len(xs) < n:
        t = sample_text(rng, words, label_texts)
        lab = encode(t)
        if not lab:
            continue
        f = features(render(fonts, t, rng, mild), deslant)
        if len(f) < len(lab) * 2:
            continue
        xs.append(f)
        ys.append(np.array(lab, np.int16))
    return xs, ys


def pack(xs, ys, out):
    np.savez(out, x=np.concatenate(xs), x_len=np.array([len(a) for a in xs], np.int32),
             y=np.concatenate(ys), y_len=np.array([len(a) for a in ys], np.int32))


def sheet(fonts, out, seed, mild=False):
    from PIL import Image, ImageDraw
    rng = random.Random(seed)
    words = dictionary()
    img = Image.new("L", (4 * 300, 10 * 70), 255)
    dr = ImageDraw.Draw(img)
    for i in range(40):
        t = sample_text(rng, words, [])
        st = render(fonts, t, rng, mild)
        allp = np.concatenate(st)
        lo = allp.min(0)
        k = min(50 / max(np.ptp(allp[:, 1]), 1), 270 / max(np.ptp(allp[:, 0]), 1))
        ox, oy = (i % 4) * 300 + 10, (i // 4) * 70 + 14
        dr.text((ox, oy - 12), t, fill=0)
        for s in st:
            dr.line([(ox + (x - lo[0]) * k, oy + (y - lo[1]) * k) for x, y in s], fill=0, width=2)
    img.save(out)


def main():
    a = sys.argv[1:]
    fonts = Fonts(a[0])
    if a[1] == "--sheet":
        sheet(fonts, a[2], int(a[3]) if len(a) > 3 and not a[3].startswith("--") else 1, "--mild" in a)
        return
    out, n = a[1], int(a[2])
    seed = int(a[3]) if len(a) > 3 and not a[3].startswith("--") else 1
    texts = []
    if "--texts" in a:
        texts = [l.rstrip("\n").split("\t")[1] for l in open(a[a.index("--texts") + 1], encoding="utf-8") if "\t" in l]
    xs, ys = generate(fonts, n, seed, texts, dictionary(), "--deslant" in a, "--mild" in a)
    pack(xs, ys, out)
    print(f"{len(xs)} words, {sum(len(x) for x in xs)} points -> {out}")


if __name__ == "__main__":
    main()
