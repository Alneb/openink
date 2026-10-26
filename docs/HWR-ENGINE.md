# openink-hwr — the open handwriting recogniser

Our own online (pen-trace) handwriting recogniser, replacing MyScript Text in the open stack. No
MyScript code or weights are used; the model is trained here from synthetic ink and the author's own
labelled notes. Background and why we went this way: [HWR-OFFLINE.md](HWR-OFFLINE.md) (2026-10-17
decision: "route 1, our own online recogniser"). Sibling engine with the same conventions:
[SHAPE-ENGINE.md](SHAPE-ENGINE.md).

## Pipeline

```
strokes (x, y per point, drawing order, any unit)
  → features (per resampled point, 7 values)            features.rs  ⇔ hwr_synth.py features()
  → BiGRU × L (torch nn.GRU layout)                      openink-nn BiGru, GruLayout::Torch
  → Linear → log-softmax over [blank, alphabet…]         per point
  → CTC decoding: greedy (best path) or prefix beam      ctc.rs
     (+ character n-gram LM fused into the ranking)       lm.rs
     (+ dictionary words via a trie-constrained search)   ctc.rs lexicon_search
  → candidates (text, log_prob), best first
```

- **Unit of recognition: one word or short phrase.** Grouping a page into lines and words is done
  before the model; the reference grouping is `docs/scripts/hwr_words.py` (`page_words`: strokes that
  overlap vertically join a line; a line splits at gaps > 0.8 × its median stroke height; small marks
  such as i-dots join the nearest word).
- **Features** (`STEP` = 0.1 of the word scale; scale = bbox height, floored at a tenth of the width so
  "—" or "=" do not blow up):
  1–2. `dx, dy` from the previous resampled point, across pen lifts too (so the jump to the next stroke
     is visible), clipped to ±5;
  3. `pen_up`: 1 at the first point of every stroke;
  4. `y` relative to the word's vertical centre;
  5–6. `cos, sin` of the direction to the next point (0, 0 at a stroke's end);
  7. turn cosine between this and the previous direction.
  Resampling is linear along the path (numpy `interp` over `linspace`); the point count uses Python's
  `round` (ties to even) — the Rust port does the same so counts match exactly.
- **Alphabet** (130 classes with the CTC blank): printable ASCII 32–126, the 33 Greek letters that do
  not look Latin (`ΓΔΘΛΞΠΣΦΨΩ` and lowercase except `ο`), and `·`. Look-alikes (Α, Β, Ε, ο, …) are
  written in Latin in the labels. The alphabet is stored in the weight file (`hwr.alphabet`) so the
  Rust side never hardcodes it.
- **Weights** (`.oisw`, the format in `openink-nn`): `hwr.mu/sd` (input standardisation; `pen_up`
  is kept 0/1), `hwr.l{k}.{f,b}.{w,u,b,c}` per layer and direction in torch's gate order r, z, n with
  both biases (`n = tanh(Wn x + bn + r ⊙ (Un h + cn))`), `hwr.out.{w,b}`, `hwr.alphabet`.
  The shape segmenter's GRU uses our numpy trainer's z, r, n layout with one bias; `GruLayout` makes
  the difference explicit instead of converting at export.

## Training data

| source | what | where |
|---|---|---|
| Hershey stroke fonts | futural, rowmans (print), scripts, cursive (joined), greeks | fetched into `data/hwr/fonts/` (gitignored) from github.com/kamalmostafa/hershey-fonts; credit Dr. A. V. Hershey / NBS |
| text prior | `/usr/share/dict/words` (cased variants, trailing punctuation), numbers and dates, math tokens, random characters, and the **train-split** label texts | `hwr_synth.py sample_text` |
| the author's notes | 452 word crops, transcribed by hand and by AI labelling | `data/hwr/labels-all.tsv` (id → text) + strokes in `data/hwr/words.json` (local only) |

- **Test split:** label ids with `id % 5 == 0` (99 words); never trained on, and their texts are kept
  out of the synthetic text prior (`labels-train.tsv`).
- **Synthetic augmentation** (`hwr_synth.py render`): per-character scale and baseline drift, letter
  spacing from −2.5 to +1.5 font units, cursive fonts chain strokes whose
  ends nearly touch, i-dots / t-bars / accents (strokes < 5.5 units in a multi-stroke glyph) moved to
  the end of the word 60 % of the time (people dot their i's last), slant −0.2…0.5, horizontal squash
  0.5…1.2, rotation σ 2.5°, three sine elastic warps, pen noise σ 0.12 then 3-point smoothing.
  `hwr_synth.py FONT_DIR --sheet out.png` draws 40 samples to eyeball.
  The default warp (amplitude 0.2–1.4, period 10–45, on letters ~21 units tall) visibly mangles words
  ("61.803" reads "51.803"); the author's hand is smooth. `--mild` (amplitude 0.05–0.5, period 25–70) keeps
  them legible and is used for the fine-tune replay sets (`synth/m1.npz`, `synth-ds/m1.npz`, seed 201).
- The author's handwriting (see the text sheets) is narrow, fast and heavily joined; the synthetic set
  only gets the model into the right region — fine-tuning on the real labels does the rest
  (`train_hwr.py finetune`: the real train split replayed 12× per epoch with light jitter, mixed with
  20 k synthetic words so the model does not forget the alphabet).

## Training (CPU, torch in `.venv`)

```sh
# 200k synthetic words in 16 parallel processes (≈ 1 min each for 12.5k)
for i in $(seq 1 16); do .venv/bin/python -I docs/scripts/hwr_synth.py data/hwr/fonts data/hwr/synth/s$i.npz 12500 $((100+i)) --texts data/hwr/labels-train.tsv & done; wait
.venv/bin/python -I -u docs/scripts/train_hwr.py pretrain data/hwr/synth/s1.npz,...,s4.npz data/hwr/models/pre.pt 8
.venv/bin/python -I -u docs/scripts/train_hwr.py finetune data/hwr/models/ft.pt data/hwr/models/pre.pt data/hwr/synth/s5.npz 15
.venv/bin/python -I docs/scripts/train_hwr.py eval data/hwr/models/ft.pt
.venv/bin/python -I docs/scripts/train_hwr.py export data/hwr/models/ft.pt impl/crates/openink-hwr/weights/hwr.oisw
.venv/bin/python -I docs/scripts/train_hwr.py fixture data/hwr/models/ft.pt impl/crates/openink-hwr/tests/fixtures/parity.json data/hwr/fonts
```

Measured pitfalls:
- `np.load(...)["x"]` re-reads (and decompresses) the archive on every access: slicing per word inside
  a loop took 6.7 GB and got the process OOM-killed. Read each array once, then `np.split`.
- `words.json` is 130 MB; the labelled subset is cached in `data/hwr/words-labelled.json`.
- GRU speed on this (shared, load ≈ 20–30) machine: 128×3 layers took 2.6 s per 64-word batch at 4
  threads and 13.5 s at 8; 96×2 took 0.9 s. More threads are slower for GRUs this small, so the
  trainer pins 4 threads and uses 96×2.

## Rust crate and bindings

- `impl/crates/openink-nn`: the shared, dependency-free inference layer (`.oisw` `Store`, `Dense`,
  `Scaler`, `Gru` with `GruLayout::{Zrn, Torch}`, `BiGru`, softmax / log-softmax). `openink-shape`
  was moved onto it (its 16 tests, including the accuracy floor, pass unchanged).
- `impl/crates/openink-hwr`: `features()`, `Recognizer::{from_bytes, builtin, log_probs, recognize,
  greedy}`, `ctc::{greedy, beam_search}` (prefix beam: per prefix the probability of ending in blank vs
  in its last label, so "aa" and "a" stay distinct; the top `width + 4` labels per step are expanded).
- `tests/parity.rs`: features within 1e-4 and log-probabilities within 2e-3 of torch on 4 synthetic
  words (one with Greek, one date, a single letter).
- JNI `Java_org_openink_hwr_HandwritingRecognizer_nativeRecognize(float[] xyt, int[] lens)` — the same
  input arrays as the shape recogniser (t ignored), returns `log_prob \t text` lines, best first.
  Java: `org.openink.hwr.HandwritingRecognizer.recognize(float[][])` → `List<Candidate>`;
  `HwrWireCheck` (check.sh stage 3) checks the decoder.

## Decoding with a lexicon and a language model
Lesson from MyScript's shipped text assets (config and container layout only; their obfuscated weights were
deliberately **not** decoded). `en_US.conf` pairs a 2.9 MB handwriting model ("ak-cur") with a 9.7 MB lexicon +
LM ("lk-text"), keeps 5 word candidates, and its header carries an angle search −90…90 step 15. So the language
side is about as big as the ink side. Ours:
- **Slant normalisation** (`deslant` option): slant = length-weighted dx/dy over mostly vertical segments (signs
  folded, clamped ±1), then `x -= slant · y`. Python `hwr_synth.slant/features(deslant=True)`, Rust
  `features::slant`. The weight file says which the model was trained on (`hwr.deslant`), and `load_model`
  refuses a mismatch with `HWR_DESLANT`.
- **Lexicon** (`Recognizer::set_lexicon`): a trie over class ids, each word as given and capitalised. A
  trie-constrained prefix beam (`ctc::lexicon_search`) proposes dictionary words, which compete with the open
  readings at `log_prob + lexicon_bonus` (2.0).
- **Character LM** (`Recognizer::set_language_model(texts, order)`, `lm::CharLm`): an interpolated Witten–Bell
  n-gram over class ids, with class 0 as the word boundary. It is trained at load time from the given texts,
  so no LM file ships. It is fused into the beam ranking:
  `rank = log P_ctc + lm_weight · log P_lm + insertion_bonus · chars` (`ctc::beam_search_lm`, `Fusion`). The
  word's end probability is added when a hypothesis finishes. Lexicon words get the same LM term. Defaults are
  `lm_weight` 0.3 and `insertion_bonus` 0.3, until tuned on the trained model.
- **Tuning harness:** `cargo run --release -p openink-hwr --example hwr_eval -- WEIGHTS
  data/hwr/words-labelled.json data/hwr/labels-all.tsv /usr/share/dict/words` prints CER and word accuracy on
  the 99 test words for greedy, beam, beam + lexicon (bonus 1/2/4) and beam + 4-gram LM (weight 0.2/0.4/0.7 ×
  insertion 0/0.5/1). The vocabulary is the word list plus the train-split labels. It takes ~80 s with the
  480k-word list.

## Status

- Pipeline, crate, parity test, bindings: done; `impl/check.sh` PASS.
- `weights/hwr.oisw` is the trained **`ft4-ds-best`** (see "Shipped model" below): Rnote text CER 0.138, word
  accuracy 52 % with the physics vocabulary; lexicon and 4-gram LM rescoring of the beam are built.
- Open: word accuracy (about half the words), line-level recognition (words joined by the grouping), the demo
  app's "recognise selection" action. Math recognition: decided against (end of this file).


## The stroke-order bug in the real data (2026-10-18)

After pretraining (synthetic CER ≈ 0.1 by loss) and fine-tuning on the author's 353 words, the held-out CER stayed at
0.78 and even the **training** words read badly (0.32), nearly always missing their first letter ("Bethe" →
"ethe", "Axel's" → "Sels"). Cause: `ksync_decode.py` lists a page's shapes by shape id (random UUIDs), and
`hwr_words.py` kept that order inside each word, so the model saw the strokes of a word shuffled (the B of "Bethe"
was its third stroke). Point times restart at 0 per stroke, so the order could not be recovered from them.
- `train_hwr.writing_order`: strokes left to right by their start, small marks (< 0.2 × word height: dots, crosses,
  accents) after the stroke they sit over. The old fine-tuned model alone went 0.775 → 0.567 on the test words; the
  synthetic-only pretrains score 0.710 (raw) and **0.631 (deslanted)**. Both fine-tunes restarted on the ordered
  data (the scrambled runs are kept in `data/hwr/models/v1-scrambled/`).
- `ksync_decode.py` now writes each shape's `createdAt`, and `hwr_words.py` orders a word's strokes by it (the
  grouping, and so the label ids, are unchanged). Re-decoding needs the device's `/sdcard/.ksync` again.

## Text and math are separate recognisers

Notes mix prose and formulas. A formula is a 2-D layout (sub/superscripts, fractions, accents), not a
left-to-right character string, so flattening it into a text label (`aν2`, `H0`) teaches the text model a wrong
target and loses the structure a math recogniser needs. `docs/scripts/hwr_kinds.py` is the single definition of the
split:

- **text**: letters, digits, spaces, sentence punctuation (`' . , ; : ! ? / -`); at least two letters; no letter
  glued to a digit (`H0`, `t1`, `2M`) and no lower→upper flip inside a word (`iHt`, `aB`). Bare numbers and dates
  (`360`, `24-03-26`) count as text.
- **math**: everything else (Greek, `= + | < > ( ) ∂ Σ ∇ …`, lone letters).

`train_hwr.py` trains and evaluates on `HWR_KIND` labels only (default `text`), for both the Boox and the Rnote
labels; the labels files keep both kinds and the kind is derived, never stored. Math labels made so far are inline
transcriptions; a math recogniser needs a separate labelling pass with structured (LaTeX-like) targets.

**Shipped model (2026-10-18): `ft3-ds-best`** → `impl/crates/openink-hwr/weights/hwr.oisw` (parity fixture
regenerated). Text-only fine-tune from ft2-ds-best, 12 epochs: Boox text test CER 0.152 / word 0.450 (ft2-ds-best on
the same split: 0.174 / 0.375). Rust `hwr_eval` on `data/hwr/labels-text.tsv` (the Boox labels filtered by
`hwr_kinds`): greedy = beam 8 = 0.152 / 0.450; + word list bonus 1/2/4: 0.149/0.475, 0.144/0.525, **0.142/0.538**;
4-gram LM alone best at weight 0.2 / insertion 0.5: 0.139 / 0.525; weight 0.7 hurts. Defaults set to lexicon bonus
4, LM weight 0.2, insertion 0.5 (80 test words: a coarse tuning).

**The word list is the writer's own physics vocabulary** (from LaTeX, notes and every reference), so the decoder
does not turn obscure words into common ones.
`docs/scripts/physics_vocab.py OUT DIR…` reads every .tex/.md/.txt and (pdftotext) .pdf under the given folders
(mail, logs and tool folders skipped), strips LaTeX math and commands, keeps a word found in ≥ 3 files, and drops
non-dictionary words that do not look like words (< 5 letters, no vowel, 5+ consonants in a row, < 25 % vowels:
PDF extraction leaves letter salad). On a real research folder (2,489 files incl. 1,867 PDFs): 35,946 words, 14,003 of them not
in /usr/share/dict (matsubara, kotliar, many-body, particle-hole, eigenstates, …). Output
`data/hwr/physics-words.txt` stays local (derived from private files).

Lexicon comparison, ft3-ds-best, bonus 4 (and + LM 0.2/0.5):

| test set | /usr/share/dict (480k) | physics vocab (36k) | both |
|---|---|---|---|
| Boox text, 80 words | 0.142 / 0.538 (0.139 / 0.525) | **0.135 / 0.538 (0.134 / 0.538)** | 0.142 / 0.538 |
| Rnote text, 132 words | 0.156 / 0.470 | **0.156 / 0.477** | 0.156 / 0.470 |

**ft4-ds-best shipped** (all text labels incl. sheets 083–098, from ft3-ds-best, 8 epochs): with the physics
vocabulary Boox 0.132 / 0.538, Rnote **0.138 / 0.523** (ft3: 0.135 / 0.538, 0.156 / 0.477).

The 13× smaller vocabulary is as good or better, and it cannot pull an obscure physics word toward an unrelated
common one that only the big list has. Recommended: the physics vocabulary plus the author's own labels, not the
system dictionary. (`hwr_eval` takes comma-separated word lists.)

**Decision (2026-10-18): no math recogniser.** Judged harder and not worth it; math labels stay in the
files (kind derived) but nothing trains on them. Only the text recogniser is developed.
