# openink-hwr weights

`hwr.oisw`: the handwriting model, written by `docs/scripts/train_hwr.py export`. Regenerate
`tests/fixtures/parity.json` from the same `.pt` (`train_hwr.py fixture`) whenever it changes.

**Current file: `ft4-ds-best`** (2026-10-18): BiGRU 96×2, deslanted features, pretrained on Hershey synthetic
ink, fine-tuned on the user's text words (Boox + all Rnote sheets, text labels only — math is not trained). With the
user's physics vocabulary (bonus 4): Boox text CER 0.132 / word 0.538, Rnote text 0.138 / 0.523 (greedy 0.147 / 0.463
and 0.167 / 0.379). Details: docs/HWR-ENGINE.md.

Training data credits: synthetic ink drawn from the Hershey fonts (Dr. A. V. Hershey, U.S. National
Bureau of Standards; jhf files from github.com/kamalmostafa/hershey-fonts, free to use with credit),
words from the system dictionary, and the user's own labelled notes (local only, never committed).
