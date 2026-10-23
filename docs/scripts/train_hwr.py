#!/usr/bin/env python3
"""Train the online handwriting recogniser: a bidirectional GRU stack with a CTC head (torch, CPU).

    train_hwr.py pretrain SYNTH.npz[,SYNTH2.npz...] OUT.pt [EPOCHS]
    train_hwr.py finetune OUT.pt IN.pt SYNTH.npz [EPOCHS]        # user labels + a synthetic replay mix
    train_hwr.py eval MODEL.pt                                     # CER / word accuracy on the held-out labels
    train_hwr.py export MODEL.pt OUT.oisw
    train_hwr.py fixture MODEL.pt OUT.json FONT_DIR    # Rust parity: synthetic strokes, features, log-probs

Real data: data/hwr/words.json (strokes) + data/hwr/labels-all.tsv (id -> text). Ids with id % 5 == 0 are the
held-out test set and never trained on. Design and numbers: docs/HWR-ENGINE.md.
"""
import json
import os
import random
import struct
import sys
import time

import numpy as np
import torch
from torch import nn

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import hwr_kinds  # noqa: E402
import hwr_synth as hs  # noqa: E402

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
WORDS = os.path.join(ROOT, "data/hwr/words.json")
LABELS = os.path.join(ROOT, "data/hwr/labels-all.tsv")
# The user's Rnote lecture notes (rnote_to_notes.py → hwr_words.py; strokes already in writing order).
RNOTE_WORDS = os.path.join(ROOT, "data/hwr-rnote/words.json")
RNOTE_LABELS = os.path.join(ROOT, "data/hwr-rnote/labels.tsv")
CLASSES = len(hs.ALPHABET) + 1
HIDDEN, LAYERS = 96, 2  # 4 CPU threads: ~0.9 s per 64-word batch (128x3 is 3x slower)
torch.set_num_threads(4)  # more threads is slower for GRUs this small


class Model(nn.Module):
    def __init__(self):
        super().__init__()
        self.register_buffer("mu", torch.zeros(hs.POINT_FEATURES))
        self.register_buffer("sd", torch.ones(hs.POINT_FEATURES))
        self.register_buffer("deslant", torch.zeros(1))  # 1: features after slant correction
        self.gru = nn.GRU(hs.POINT_FEATURES, HIDDEN, LAYERS, bidirectional=True, dropout=0.15)
        self.out = nn.Linear(2 * HIDDEN, CLASSES)

    def forward(self, x, lens):  # x: (T, N, F)
        x = (x - self.mu) / self.sd
        packed = nn.utils.rnn.pack_padded_sequence(x, lens.cpu(), enforce_sorted=False)
        h, _ = self.gru(packed)
        h, _ = nn.utils.rnn.pad_packed_sequence(h)
        return self.out(h).log_softmax(-1)  # (T, N, C)


# ---------------------------------------------------------------- data

def load_npz(paths):
    xs, ys = [], []
    for p in paths.split(","):
        d = np.load(p)  # each d[key] re-reads the archive: read every array once
        x, y, xl, yl = d["x"], d["y"].astype(np.int64), d["x_len"], d["y_len"]
        xs += np.split(x, np.cumsum(xl)[:-1])
        ys += np.split(y, np.cumsum(yl)[:-1])
    return xs, ys


def labelled_strokes(ids):
    """Strokes of the labelled words. words.json is ~130 MB, so the subset is cached in words-labelled.json."""
    cache = os.path.join(os.path.dirname(WORDS), "words-labelled.json")
    if os.path.exists(cache) and os.path.getmtime(cache) >= max(os.path.getmtime(WORDS), os.path.getmtime(LABELS)):
        return {int(k): v for k, v in json.load(open(cache)).items()}
    words = {w["id"]: {"strokes": w["strokes"]} for w in json.load(open(WORDS))["words"] if w["id"] in ids}
    json.dump(words, open(cache, "w"))
    return words


# Which labels to train/evaluate on: "text" (prose, the default) or "math"; the two are separate recognisers.
KIND = os.environ.get("HWR_KIND", "text")
DESLANT = os.environ.get("HWR_DESLANT") == "1"  # feature variant; must match how the synth npz was made


def writing_order(strokes):
    """The crops' strokes come in the decoder's order (by shape id: random), not as written. Until the decoder keeps
    createdAt, put them left to right by where each starts, with small marks (dots, crosses, accents: under a fifth
    of the word height) after the stroke they sit over, the way a hand adds them."""
    ss = [np.asarray(s, float) for s in strokes if len(s)]
    if len(ss) < 2:
        return ss
    allp = np.concatenate(ss)
    h = max(np.ptp(allp[:, 1]), 1e-6)
    small = [max(np.ptp(s[:, 0]), np.ptp(s[:, 1])) < 0.2 * h for s in ss]
    main = sorted((i for i in range(len(ss)) if not small[i]), key=lambda i: ss[i][0, 0])
    if not main:
        return sorted(ss, key=lambda s: s[0, 0])
    after = {i: [] for i in main}
    for i in (i for i in range(len(ss)) if small[i]):
        cx = ss[i][:, 0].mean()
        host = min(main, key=lambda m: 0 if ss[m][:, 0].min() <= cx <= ss[m][:, 0].max() else min(abs(cx - ss[m][:, 0].min()), abs(cx - ss[m][:, 0].max())))
        after[host].append(i)
    return [ss[k] for m in main for k in [m] + sorted(after[m], key=lambda i: ss[i][0, 0])]


def real_data():
    """(train, test) lists of (features, label ids, text) from the user's labelled words."""
    labels = read_labels(LABELS)
    words = labelled_strokes(set(labels))
    train, test = [], []
    for i, t in sorted(labels.items()):
        if i not in words:
            continue
        lab = hs.encode(t)
        f = hs.features([s[:, :2] for s in writing_order(words[i]["strokes"])], DESLANT)
        if not lab or len(f) < len(lab):
            continue
        (test if i % 5 == 0 else train).append((f, np.array(lab, np.int64), t))
    rtrain, _ = rnote_data()
    return train + rtrain, test


def read_labels(path):
    """id → text for labels of KIND; labels with a character outside the alphabet are left out (encode would drop
    it silently)."""
    out = {}
    for line in open(path, encoding="utf-8"):
        k, _, t = line.rstrip("\n").partition("\t")
        if k.isdigit() and t and hwr_kinds.kind(t) == KIND and all(c in hs.CLASS for c in t):
            out[int(k)] = t
    return out


def rnote_data():
    """(train, test) from the Rnote notes (id % 5 == 0 held out), [] when they are not there."""
    if not (os.path.exists(RNOTE_WORDS) and os.path.exists(RNOTE_LABELS)):
        return [], []
    labels = read_labels(RNOTE_LABELS)
    words = {w["id"]: w for w in json.load(open(RNOTE_WORDS))["words"] if w["id"] in labels}
    train, test = [], []
    for i, t in sorted(labels.items()):
        if i not in words:
            continue
        lab = hs.encode(t)
        f = hs.features([np.asarray(s, float)[:, :2] for s in words[i]["strokes"]], DESLANT)
        if not lab or len(f) < len(lab):
            continue
        (test if i % 5 == 0 else train).append((f, np.array(lab, np.int64), t))
    return train, test


def augment(f, rng):
    """Light jitter for real examples (features already normalised): scale dx/dy, wobble y."""
    g = f.copy()
    k = rng.uniform(0.85, 1.15)
    g[:, 0] *= k * rng.uniform(0.85, 1.15)
    g[:, 1] *= k
    g[:, 3] = g[:, 3] * k + rng.gauss(0, 0.03)
    return g


def batches(xs, ys, bs, rng, max_points=12000):
    """Length-sorted buckets so padding stays small; yields (x, x_len, y, y_len)."""
    order = sorted(range(len(xs)), key=lambda i: len(xs[i]) + rng.random() * 8)
    groups, cur = [], []
    for i in order:
        if cur and (len(cur) >= bs or (len(cur) + 1) * len(xs[i]) > max_points):
            groups.append(cur)
            cur = []
        cur.append(i)
    if cur:
        groups.append(cur)
    rng.shuffle(groups)
    for g in groups:
        yield collate([xs[i] for i in g], [ys[i] for i in g])


def collate(xs, ys):
    T = max(len(x) for x in xs)
    x = np.zeros((T, len(xs), hs.POINT_FEATURES), np.float32)
    for j, a in enumerate(xs):
        x[:len(a), j] = a
    return (torch.from_numpy(x), torch.tensor([len(a) for a in xs]),
            torch.from_numpy(np.concatenate(ys)), torch.tensor([len(a) for a in ys]))


# ---------------------------------------------------------------- decode / metrics

def greedy(logp, lens):
    out = []
    best = logp.argmax(-1).T.numpy()  # (N, T)
    for j, n in enumerate(lens.tolist()):
        prev, s = BLANK, []
        for c in best[j, :n]:
            if c != prev and c != BLANK:
                s.append(hs.ALPHABET[c - 1])
            prev = c
        out.append("".join(s))
    return out


BLANK = hs.BLANK


def edit(a, b):
    d = list(range(len(b) + 1))
    for i, ca in enumerate(a, 1):
        prev, d[0] = d[0], i
        for j, cb in enumerate(b, 1):
            prev, d[j] = d[j], min(d[j] + 1, d[j - 1] + 1, prev + (ca != cb))
    return d[-1]


def evaluate(model, test, show=0):
    model.eval()
    errs = chars = ok = 0
    rows = []
    with torch.no_grad():
        for k in range(0, len(test), 64):
            chunk = test[k:k + 64]
            x, xl, _, _ = collate([c[0] for c in chunk], [c[1] for c in chunk])
            for (f, _, t), h in zip(chunk, greedy(model(x, xl), xl)):
                e = edit(h, t)
                errs, chars, ok = errs + e, chars + len(t), ok + (h == t)
                rows.append((t, h))
    model.train()
    for t, h in rows[:show]:
        print(f"    {t!r:28} -> {h!r}")
    return errs / max(chars, 1), ok / max(len(test), 1)


# ---------------------------------------------------------------- training

def fit(model, xs, ys, epochs, lr, test, tag, save, real=None, real_weight=0):
    """Adam + CTC. With `real`, each epoch also replays the user's examples `real_weight` times (augmented).
    Saves `save` after every epoch (a killed run keeps its progress) and the lowest-CER epoch as `*-best.pt`."""
    ctc = nn.CTCLoss(blank=BLANK, zero_infinity=True)
    opt = torch.optim.Adam(model.parameters(), lr=lr)
    sched = torch.optim.lr_scheduler.ExponentialLR(opt, 0.85 ** (1 / max(1, epochs // 6)))
    rng = random.Random(1)
    best = float("inf")
    for ep in range(epochs):
        t0, tot, nb = time.time(), 0.0, 0
        exs, eys = list(xs), list(ys)
        if real:
            for _ in range(real_weight):
                exs += [augment(f, rng) for f, _, _ in real]
                eys += [lab for _, lab, _ in real]
        for x, xl, y, yl in batches(exs, eys, 64, rng):
            loss = ctc(model(x, xl), y, xl, yl)
            opt.zero_grad()
            loss.backward()
            nn.utils.clip_grad_norm_(model.parameters(), 5.0)
            opt.step()
            tot, nb = tot + loss.item(), nb + 1
        sched.step()
        cer, wacc = evaluate(model, test)
        print(f"[{tag}] epoch {ep + 1}/{epochs}  loss {tot / max(nb, 1):.3f}  test CER {cer:.3f}  word acc {wacc:.3f}"
              f"  ({time.time() - t0:.0f}s)", flush=True)
        torch.save(model.state_dict(), save)
        if cer < best:
            best = cer
            torch.save(model.state_dict(), save.replace(".pt", "-best.pt"))


def set_scaler(model, xs):
    allx = np.concatenate(xs[:20000])
    model.mu.copy_(torch.from_numpy(allx.mean(0)))
    model.sd.copy_(torch.from_numpy(np.maximum(allx.std(0), 1e-3)))
    model.mu[2], model.sd[2] = 0.0, 1.0  # pen_up flag stays 0/1


def load_model(path):
    """Checkpoints from before the deslant flag existed read raw ink (flag 0)."""
    sd = torch.load(path)
    sd.setdefault("deslant", torch.zeros(1))
    model = Model()
    model.load_state_dict(sd)
    if bool(model.deslant.item()) != DESLANT:
        sys.exit(f"{path} has deslant={bool(model.deslant.item())}; set HWR_DESLANT to match")
    return model


def write_oisw(model, path):
    """Tensors in torch's GRU layout (gates r, z, n; n = tanh(Wn x + bn + r * (Un h + cn)))."""
    t = {"hwr.deslant": model.deslant, "hwr.mu": model.mu, "hwr.sd": model.sd, "hwr.out.w": model.out.weight, "hwr.out.b": model.out.bias}
    for layer in range(LAYERS):
        for d, suf in (("f", ""), ("b", "_reverse")):
            p = f"hwr.l{layer}.{d}"
            t[p + ".w"] = getattr(model.gru, f"weight_ih_l{layer}{suf}")
            t[p + ".u"] = getattr(model.gru, f"weight_hh_l{layer}{suf}")
            t[p + ".b"] = getattr(model.gru, f"bias_ih_l{layer}{suf}")
            t[p + ".c"] = getattr(model.gru, f"bias_hh_l{layer}{suf}")
    alpha = np.frombuffer(hs.ALPHABET.encode("utf-32-le"), np.uint32).astype(np.float32)
    with open(path, "wb") as fh:
        fh.write(b"OISW" + struct.pack("<II", 1, len(t) + 1))
        items = [(k, v.detach().numpy().astype(np.float32)) for k, v in t.items()] + [("hwr.alphabet", alpha)]
        for k, a in items:
            name = k.encode()
            fh.write(struct.pack("<H", len(name)) + name + struct.pack("<B", a.ndim))
            fh.write(b"".join(struct.pack("<I", d) for d in a.shape))
            fh.write(a.astype("<f4").tobytes())


def main():
    cmd, a = sys.argv[1], sys.argv[2:]
    train, test = real_data()
    print(f"user labels: {len(train)} train, {len(test)} test (id % 5 == 0)")
    if cmd == "pretrain":
        xs, ys = load_npz(a[0])
        model = Model()
        model.deslant.fill_(float(DESLANT))
        set_scaler(model, xs)
        fit(model, xs, ys, int(a[2]) if len(a) > 2 else 10, 2e-3, test, "pretrain", a[1])
    elif cmd == "finetune":
        model = load_model(a[1])
        xs, ys = load_npz(a[2])
        fit(model, xs[:20000], ys[:20000], int(a[3]) if len(a) > 3 else 15, 5e-4, test, "finetune", a[0], train, 12)
    elif cmd == "eval":
        model = load_model(a[0])
        cer, wacc = evaluate(model, test, show=40)
        print(f"test CER {cer:.3f}  word acc {wacc:.3f}")
        _, rtest = rnote_data()
        if rtest:
            cer, wacc = evaluate(model, rtest, show=10)
            print(f"rnote test ({len(rtest)} words) CER {cer:.3f}  word acc {wacc:.3f}")
    elif cmd == "fixture":
        model = load_model(a[0])
        model.eval()
        fonts, rng = hs.Fonts(a[2]), random.Random(7)
        cases = []
        for text in ["energy", "Ψ(x)", "9/4/26", "i"]:
            st = [s.astype(np.float32) for s in hs.render(fonts, text, rng)]
            f = hs.features(st, bool(model.deslant.item()))
            with torch.no_grad():
                lp = model(torch.from_numpy(f)[:, None], torch.tensor([len(f)]))[:, 0]
            cases.append({"text": text, "strokes": [s.tolist() for s in st], "features": f.tolist(),
                          "log_probs": lp.numpy().tolist()})
        json.dump({"deslant": bool(model.deslant.item()), "cases": cases}, open(a[1], "w"))
        print(f"wrote {a[1]}")
    elif cmd == "export":
        model = load_model(a[0])
        write_oisw(model, a[1])
        print(f"wrote {a[1]} ({os.path.getsize(a[1])} bytes)")


if __name__ == "__main__":
    main()
