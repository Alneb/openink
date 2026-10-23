#!/usr/bin/env python3
"""Train the openink-shape classifier (docs/SHAPE-ENGINE.md) from shape-dataset output, numpy only.

Classifier: MLP 40 -> 64 -> 64 -> 8 (tanh hidden, softmax), Adam, class-balanced loss.
Segmenter: bidirectional GRU (10 -> 2x24) + dense -> straight/curve/corner per point, trained by
backpropagation through time; gate layout and maths match `Gru::run` in impl/crates/openink-shape/src/nn.rs.
Writes a complete .oisw file (format in nn.rs) that `Models::from_bytes` loads.
Quick, Draw! data is Google's, CC BY 4.0: credit it wherever the weights ship.

usage: python3 -I docs/scripts/train_shape.py data/shape/train data/shape/test out.oisw [clf_epochs [seg_epochs]]
"""
import struct
import sys

import numpy as np

FEAT, FAMS = 40, 8
PFEAT, LABELS, HID = 10, 3, 24
NAMES = ["Line", "Polyline", "Polygon", "Ellipse", "Arc", "Arrow", "ScratchOut", "Reject"]


def load(d):
    x = np.fromfile(f"{d}/clf_x.f32", "<f4").reshape(-1, FEAT)
    y = np.fromfile(f"{d}/clf_y.u8", "u1").astype(np.int64)
    assert len(x) == len(y), "clf_x and clf_y disagree"
    return x, y


def forward(params, x):
    acts = [x]
    for i, (w, b) in enumerate(params):
        z = acts[-1] @ w.T + b
        acts.append(np.tanh(z) if i < len(params) - 1 else z)
    z = acts[-1] - acts[-1].max(1, keepdims=True)
    p = np.exp(z)
    return acts, p / p.sum(1, keepdims=True)


def train(x, y, epochs, rng):
    sizes = [FEAT, 64, 64, FAMS]
    params = [(rng.normal(0, 1 / np.sqrt(i), (o, i)).astype(np.float32), np.zeros(o, np.float32))
              for i, o in zip(sizes, sizes[1:])]
    m = [[np.zeros_like(a) for a in l] for l in params]
    v = [[np.zeros_like(a) for a in l] for l in params]
    cw = len(y) / (FAMS * np.maximum(np.bincount(y, minlength=FAMS), 1))  # balance the classes
    lr, step = 2e-3, 0
    for ep in range(epochs):
        order = rng.permutation(len(x))
        for s in range(0, len(x), 256):
            idx = order[s:s + 256]
            acts, p = forward(params, x[idx])
            wgt = cw[y[idx]][:, None]
            g = p.copy()
            g[np.arange(len(idx)), y[idx]] -= 1
            g *= wgt / wgt.sum()
            step += 1
            for i in reversed(range(len(params))):
                w, b = params[i]
                gw, gb = g.T @ acts[i], g.sum(0)
                if i:
                    g = (g @ w) * (1 - acts[i] ** 2)
                for j, (a, ga) in enumerate(((w, gw), (b, gb))):
                    m[i][j] = 0.9 * m[i][j] + 0.1 * ga
                    v[i][j] = 0.999 * v[i][j] + 0.001 * ga * ga
                    a -= lr * (m[i][j] / (1 - 0.9 ** step)) / (np.sqrt(v[i][j] / (1 - 0.999 ** step)) + 1e-8)
        lr *= 0.85
        print(f"epoch {ep + 1}/{epochs}: train acc {accuracy(params, x, y):.4f}", flush=True)
    return params


def accuracy(params, x, y):
    return float((forward(params, x)[1].argmax(1) == y).mean())


class Adam:
    def __init__(self, arrays, lr):
        self.a, self.lr, self.t = arrays, lr, 0
        self.m = [np.zeros_like(x) for x in arrays]
        self.v = [np.zeros_like(x) for x in arrays]

    def step(self, grads):
        self.t += 1
        for a, g, m, v in zip(self.a, grads, self.m, self.v):
            np.clip(g, -5, 5, out=g)
            m *= 0.9
            m += 0.1 * g
            v *= 0.999
            v += 0.001 * g * g
            a -= self.lr * (m / (1 - 0.9 ** self.t)) / (np.sqrt(v / (1 - 0.999 ** self.t)) + 1e-8)


def sigmoid(x):
    return 1 / (1 + np.exp(-x))


def gru_forward(w, u, b, x):
    """x [B,T,I] -> hidden states [B,T,H] plus the cache for backward. Padding sits at the end."""
    bsz, steps, _ = x.shape
    h = u.shape[1]
    a = x @ w.T + b
    hs = np.zeros((bsz, steps + 1, h), x.dtype)
    cache = []
    for t in range(steps):
        hp = hs[:, t]
        uh = hp @ u.T
        z = sigmoid(a[:, t, :h] + uh[:, :h])
        r = sigmoid(a[:, t, h:2 * h] + uh[:, h:2 * h])
        n = np.tanh(a[:, t, 2 * h:] + r * uh[:, 2 * h:])
        hs[:, t + 1] = (1 - z) * n + z * hp
        cache.append((z, r, n, uh[:, 2 * h:]))
    return hs, cache


def gru_backward(w, u, x, hs, cache, dh_out):
    h = u.shape[1]
    steps = x.shape[1]
    da = np.zeros((x.shape[0], steps, 3 * h), x.dtype)
    du = np.zeros_like(u)
    dh = np.zeros((x.shape[0], h), x.dtype)
    for t in reversed(range(steps)):
        z, r, n, uhn = cache[t]
        hp = hs[:, t]
        dh = dh + dh_out[:, t]
        dz = dh * (hp - n) * z * (1 - z)
        dn = dh * (1 - z) * (1 - n * n)
        dr = dn * uhn * r * (1 - r)
        duh = np.concatenate([dz, dr, dn * r], 1)
        da[:, t] = np.concatenate([dz, dr, dn], 1)
        du += duh.T @ hp
        dh = dh * z + duh @ u
    dw = np.einsum("bti,btj->ij", da, x)
    return dw, du, da.sum((0, 1))


def load_seg(d):
    x = np.fromfile(f"{d}/seg_x.f32", "<f4").reshape(-1, PFEAT)
    y = np.fromfile(f"{d}/seg_y.u8", "u1").astype(np.int64)
    lens = np.fromfile(f"{d}/seg_len.u32", "<u4").astype(np.int64)
    assert lens.sum() == len(x) == len(y), "seg arrays disagree"
    return x, y, lens, np.concatenate([[0], np.cumsum(lens)])


def batch(x, y, lens, starts, idx):
    """Padded forward input, per-sequence reversed input, labels and mask."""
    steps = int(lens[idx].max())
    xb = np.zeros((len(idx), steps, PFEAT), np.float32)
    xr = np.zeros_like(xb)
    yb = np.zeros((len(idx), steps), np.int64)
    mask = np.zeros((len(idx), steps), np.float32)
    for k, i in enumerate(idx):
        s, n = starts[i], lens[i]
        xb[k, :n] = x[s:s + n]
        xr[k, :n] = x[s:s + n][::-1]
        yb[k, :n] = y[s:s + n]
        mask[k, :n] = 1
    return xb, xr, yb, mask


def unreverse(hr, lens):
    out = np.zeros_like(hr)
    for k, n in enumerate(lens):
        out[k, :n] = hr[k, :n][::-1]
    return out


def seg_forward(p, xb, xr, lens):
    hf, cf = gru_forward(p[0], p[1], p[2], xb)
    hr, cr = gru_forward(p[3], p[4], p[5], xr)
    hb = unreverse(hr[:, 1:], lens)
    cat = np.concatenate([hf[:, 1:], hb], 2)
    z = cat @ p[6].T + p[7]
    z -= z.max(2, keepdims=True)
    pr = np.exp(z)
    pr /= pr.sum(2, keepdims=True)
    return pr, (hf, cf, hr, cr, cat)


def train_seg(x, y, lens, starts, epochs, rng):
    def init(*shape):
        return rng.normal(0, 1 / np.sqrt(shape[-1]), shape).astype(np.float32)
    p = [init(3 * HID, PFEAT), init(3 * HID, HID), np.zeros(3 * HID, np.float32),
         init(3 * HID, PFEAT), init(3 * HID, HID), np.zeros(3 * HID, np.float32),
         init(LABELS, 2 * HID), np.zeros(LABELS, np.float32)]
    opt = Adam(p, 3e-3)
    cw = (len(y) / (LABELS * np.bincount(y, minlength=LABELS))).astype(np.float32)
    order_by_len = np.argsort(lens, kind="stable")
    batches = [order_by_len[i:i + 128] for i in range(0, len(lens), 128)]  # similar lengths -> little padding
    for ep in range(epochs):
        loss_sum, correct, total = 0.0, 0, 0
        for bi in rng.permutation(len(batches)):
            idx = batches[bi]
            xb, xr, yb, mask = batch(x, y, lens, starts, idx)
            pr, (hf, cf, hr, cr, cat) = seg_forward(p, xb, xr, lens[idx])
            wgt = cw[yb] * mask
            norm = wgt.sum()
            pick = np.take_along_axis(pr, yb[..., None], 2)[..., 0]
            loss_sum += float(-(wgt * np.log(pick + 1e-9)).sum() / norm)
            correct += int(((pr.argmax(2) == yb) * mask).sum())
            total += int(mask.sum())
            g = pr.copy()
            np.put_along_axis(g, yb[..., None], np.take_along_axis(g, yb[..., None], 2) - 1, 2)
            g *= (wgt / norm)[..., None]
            gwo = np.einsum("btk,btj->kj", g, cat)
            gbo = g.sum((0, 1))
            dcat = g @ p[6]
            dhf = dcat[..., :HID]
            dhr = np.zeros_like(dhf)
            for k, n in enumerate(lens[idx]):
                dhr[k, :n] = dcat[k, :n, HID:][::-1]
            gf = gru_backward(p[0], p[1], xb, hf, cf, dhf)
            gr = gru_backward(p[3], p[4], xr, hr, cr, dhr)
            opt.step([*gf, *gr, gwo, gbo])
        opt.lr *= 0.8
        print(f"seg epoch {ep + 1}/{epochs}: loss {loss_sum / len(batches):.4f} point acc {correct / total:.4f}", flush=True)
    return p


def eval_seg(p, x, y, lens, starts):
    conf = np.zeros((LABELS, LABELS), int)
    order = np.argsort(lens, kind="stable")
    for i in range(0, len(lens), 256):
        idx = order[i:i + 256]
        xb, xr, yb, mask = batch(x, y, lens, starts, idx)
        pr, _ = seg_forward(p, xb, xr, lens[idx])
        m = mask.astype(bool)
        np.add.at(conf, (yb[m], pr.argmax(2)[m]), 1)
    print(f"seg test point acc {np.trace(conf) / conf.sum():.4f}  (rows = truth, cols = predicted)")
    for i, name in enumerate(["Straight", "Curve", "Corner"]):
        print(f"{name:8} recall {conf[i, i] / conf[i].sum():.3f} precision {conf[i, i] / conf[:, i].sum():.3f} {conf[i].tolist()}")


def write_oisw(path, tensors):
    with open(path, "wb") as f:
        f.write(b"OISW" + struct.pack("<II", 1, len(tensors)))
        for name, a in tensors:
            a = np.ascontiguousarray(a, "<f4")
            n = name.encode()
            f.write(struct.pack("<H", len(n)) + n + struct.pack("<B", a.ndim))
            f.write(struct.pack(f"<{a.ndim}I", *a.shape) + a.tobytes())


def main():
    tr, te, out = sys.argv[1:4]
    epochs = int(sys.argv[4]) if len(sys.argv) > 4 else 12
    seg_epochs = int(sys.argv[5]) if len(sys.argv) > 5 else 6
    rng = np.random.default_rng(1)
    x, y = load(tr)
    mu, sd = x.mean(0), x.std(0) + 1e-6
    params = train((x - mu) / sd, y, epochs, rng)
    xt, yt = load(te)
    pred = forward(params, (xt - mu) / sd)[1].argmax(1)
    print(f"test acc {float((pred == yt).mean()):.4f}  (rows = truth, cols = predicted)")
    conf = np.zeros((FAMS, FAMS), int)
    np.add.at(conf, (yt, pred), 1)
    for i, row in enumerate(conf):
        if row.sum():
            print(f"{NAMES[i]:10} {row[i] / row.sum():.3f} {row.tolist()}")
    tensors = [("clf.mu", mu), ("clf.sd", sd)]
    for k, (w, b) in enumerate(params):
        tensors += [(f"clf.l{k}.w", w), (f"clf.l{k}.b", b)]
    sx, sy, slen, sst = load_seg(tr)
    smu, ssd = sx.mean(0), sx.std(0) + 1e-6
    sp = train_seg((sx - smu) / ssd, sy, slen, sst, seg_epochs, rng)
    tx, ty, tlen, tst = load_seg(te)
    eval_seg(sp, (tx - smu) / ssd, ty, tlen, tst)
    tensors += [("seg.mu", smu), ("seg.sd", ssd)]
    for name, a in zip(["seg.f.w", "seg.f.u", "seg.f.b", "seg.b.w", "seg.b.u", "seg.b.b", "seg.out.w", "seg.out.b"], sp):
        tensors.append((name, a))
    write_oisw(out, tensors)
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
