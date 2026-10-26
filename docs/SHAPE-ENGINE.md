# openink-shape: the open shape recogniser (replacing MyScript Shape)


## Why and what
Boox Notes snaps a held stroke to a shape through MyScript Shape, which is closed and licensed
(docs/SHAPE-RECOGNITION.md, docs/SHAPE-RE.md). These findings are reimplemented here,
with our own trained weights. The choices were:
- **training data: synthetic shapes + the author's notes** (as rejects), plus open data (Quick, Draw!);
- **models: "Classifier + segmenter"**, the same split MyScript uses.

The crate is `impl/crates/openink-shape`: pure Rust with no dependencies (`serde_json` only behind the
`dataset` feature, for the training-data generator).

## Pipeline (the same shape as MyScript's, but our own code, features and weights)
1. **Normalise** (`features::Norm`): resample the ink by arc length to `diag / 64`. Each resampled point keeps
   its stroke, its time and the index of its source raw sample (so synthetic labels carry over).
2. **Classifier** (`nn::Classifier`): an MLP over 40 per-ink features → 8 families (`model::Family`):
   Line, Polyline, Polygon, Ellipse, Arc, Arrow, ScratchOut, Reject.
3. **Segmenter** (`nn::Segmenter`): a bidirectional GRU over 10 per-point features → straight / curve / corner
   per point.
4. **Fit** (`fit`): total-least-squares lines, line intersection for vertices, Kåsa circles, a direct ellipse
   fit (Fitzgibbon's constraint, solved with Halíř–Flusser's 3×3 reduction, in f64 on centred and scaled data),
   elliptic arcs with a signed sweep, and a two-way residual (ink→model and model→ink).
5. **Specialise and beautify** (to do): the most specific class whose tolerances hold wins (square > rhombus /
   rectangle > parallelogram > trapezoid > quadrilateral; circle > ellipse; equilateral / right isosceles >
   right / isosceles > triangle). Cost = −ln P(family) + k · residual / diag − specialisation bonus.
   Our tolerances are chosen by us, with magnitudes like MyScript's (about 10°).
6. **Output** (`model`): the primitives `Line` and `Arc` with arrow-head decorations, as in MyScript's output
   model; also `ScratchOut { hull }` and `Rejected`. `ShapeClass::knote2_type()` maps a class to knote2's
   `ShapeFactory` id (e.g. circle 0, rectangle 1, line 7, triangle 8, rhombus 18, trapezoid 26, arrow 28); classes
   knote2 has no id for go to `SHAPE_UNIVERSAL` (40), its vertex-list shape.

## Features
**Per ink (40):**
- strokes − 1;
- `tanh(ln(h/w))`, path length over the diagonal, end-to-end over length, closure gap;
- hull area over box area, hull circularity;
- total, net and consistent turning; sharp-corner count; largest turn; PCA axis ratio; reversals;
- an 8-bin undirected direction histogram;
- an 8-bin radial-distance histogram and its spread;
- straight fraction; turning in the first and last 15 % (arrow heads, hooks);
- self-intersections, speed variation, longest straight run, mean |turn|;
- crossings of the box's centre lines.

**Per point (10):** turning at scales 1, 2 and 4; |turn| summed over ±3; chord/arc over ±4; position along
the ink; distance to the stroke's nearest end; log relative speed; pen-down flag; radial deviation.

## Training data
- **Synthetic** (`synth`, deterministic per seed). It draws every class:
  - lines and polylines, zigzags;
  - five triangle kinds, six quadrilateral kinds, regular 5-, 6- and 8-gons;
  - circles and ellipses (93–112 % sweep), arcs;
  - six arrow kinds, with the head in the same stroke or separate;
  - scratch-outs, and scribbles labelled Reject.

  It distorts them with a hand model:
  - axis bias;
  - corner rounding (smoothing);
  - low-frequency normal wobble and sample jitter;
  - start hooks, pen lifts at corners;
  - slowing at corners and ends, 3–8 ms sampling.

  Each raw sample is labelled straight, curve or corner. For polygons, stroke ends count as corners.
- **The author's notes** (KSync, `ksync_decode.py --keep-removed`): 98 pages, 21,576 live pen strokes and 770
  removed ones. **There are no labelled raw shapes.** knote2 replaces a snapped stroke with a type-40
  vertex list (75 of them: 61 lines, 2 triangles, 12 quadrilaterals), and no removed pen stroke overlaps any of
  them (bbox IoU > 0.6: 0 of 75). So the notes are used as **Reject** examples (real writing must not snap),
  and their shapes as size and aspect priors.
- **Quick, Draw!** (Google, **CC BY 4.0**, needs attribution; raw ndjson with timing), downloaded locally to
  `data/quickdraw/` (gitignored): the first 30,000 drawings of each of circle, square, triangle, line, hexagon,
  octagon, diamond, zigzag and squiggle (25–71 MB per file). It was drawn with mouse or finger in 20 s, so the
  drawings are sloppy; use only `recognized: true`. Mapping: circle → Ellipse; square, triangle, hexagon,
  octagon, diamond → Polygon; line → Line; zigzag → Polyline; squiggle → Reject.
  ```sh
  curl -sS "https://storage.googleapis.com/quickdraw_dataset/full/raw/circle.ndjson" | head -n 30000 > data/quickdraw/circle.ndjson
  ```
- **DIDI** (Digital Ink Diagram data, Google, CC BY 4.0, 58,655 diagram drawings from 364 people, arXiv
  2002.09303). Its prompts are graph diagrams (dot/xdot), the likely source of arrows and ellipses. Not
  downloaded yet; it has no per-stroke labels, so its strokes must be aligned to the prompt's nodes and edges
  first.

## Status (checkpoint, 2026-10-17)
- Done:
  - `geom`, `model`, `features`, `nn` (MLP + BiGRU inference, `.oisw` weight format), `fit` and `synth`;
  - 9 unit tests (ellipse from full and half arcs, circle, intersection, resampling, hull, PCA,
    generator determinism and labels);
  - clippy clean; `impl/check.sh` PASS.
- To do:
  1. ~~the `shape-dataset` binary~~ **written** (`src/bin/shape-dataset.rs`, feature `dataset`). It writes
     flat little-endian arrays: `clf_x.f32` [N,40], `clf_y.u8` (family), `clf_src.u8` (0 synth, 1 Quick, Draw!,
     2 notes), `seg_x.f32` [P,10], `seg_y.u8`, `seg_len.u32`. Only synthetic ink feeds the segmenter. Note strokes
     that might be honest shapes (end-to-end/length > 0.9, or closed loops) are skipped, not labelled Reject.
     Commands (run and verified: 178,462 train rows, 37k test rows):
     `impl/target/release/shape-dataset --out data/shape/train --seed 1 --synth 60000 --quickdraw data/quickdraw --qd-take 12000 --notes <notes.json>`;
     test set: `--out data/shape/test --seed 2 --synth 10000 --quickdraw data/quickdraw --qd-skip 12000 --qd-take 3000` (no notes yet; it needs a `--notes-skip`).
  2. `docs/scripts/train_shape.py` (numpy: MLP, and BiGRU trained by backpropagation through time) → `weights/shape.oisw`;
  3. `segment` + `recognize` (candidates, specialisation, beautify, arrows, scratch-out);
  4. accuracy tests on a held-out synthetic set and on Quick, Draw!;
  5. JNI and knote2 hookup;
  6. DIDI alignment.

- **First trained classifier**: `docs/scripts/train_shape.py` (numpy, MLP 40→64→64→8 with
  tanh, Adam, class-balanced loss, 8 epochs, about 4 min) → `data/shape/clf.oisw` (classifier tensors only, so it
  cannot load until the segmenter is trained). **Held-out test accuracy: 94.5 %**. Per family: Line 97.5, Polyline 86.4,
  Polygon 96.7, Ellipse 98.0, Arc 97.1, Arrow 99.4, ScratchOut 98.0, Reject 83.5. The main confusion is Polyline ↔ Reject
  (393 and 431 cases, mostly Quick, Draw! zigzag vs squiggle). Next: the BiGRU segmenter in the same script, then
  recognize.


## Open models (survey, 2026-10-17)
- **Shapes:** I found no openly licensed, pretrained "hand-drawn shape → clean primitive" model.
  - Research systems: the Primitive Mapping Network (arXiv 2207.13543, code at ExplainableML/sketch-primitives;
    its license and weights are unchecked); SketchGPT (arXiv 2405.03099, weights promised); PaleoSketch (classic
    rules, academic).
  - Template recognisers ($1 / $P / $Q) are open algorithms with no weights.
  - Hence our own model.
- **Handwriting (online ink → text):**
  - Google ML Kit Digital Ink is free but closed.
  - InkSight (Google Research) has open code and weights ("Small-p", trained on public data), but it is a large
    vision-language model that turns images into ink, too heavy for real-time use on the device.
  - InkFM's weights were not confirmed public.
  - Offline (image) recognisers are open (TrOCR, IAM-trained HTR models, CENSUS-HWR), but they read rendered
    images, not strokes.
  - IAM-OnDB, the standard online-ink dataset, is free for non-commercial research only.


## Hookup (2026-10-17)
- **Rust:** `openink_shape::recognize::Recognizer::builtin()` (weights embedded with `include_bytes!`);
  `openink_shape::wire::encode` writes the flat `float[]` result. Explicit class codes 0–24; layout in `wire.rs`.
- **JNI:** `openink-jni/src/shape.rs` → `org.openink.shape.ShapeRecognizer.recognize(float[][] strokes)` (each
  stroke `[x, y, tMs]*`, any unit), returning `Shape` (status, candidates best first, primitives with arrow
  flags, scratch-out hull). `ShapeClass.knote2Type` gives knote2's ShapeFactory id.
- **Demo app** (`InkCanvas.snap`, `ShapeSnap`): knote2's gesture, with the same hold rule (every point within ±2 view
  px in x and y for > 500 ms, docs/SHAPE-RECOGNITION.md).
  - The resting tail is cut, and the stroke is recognised in its own coordinates (raw digitiser units, or view
    px).
  - A shape replaces the stroke's points (lines 24 segments, arcs 64 per turn, arrow barbs at 15 % of the
    length and ±25°, drawn tip → barb → tip → barb → tip), so save and undo just work.
  - A scratch-out removes itself, plus every stroke with ≥ 60 % of its points inside the hull, as one
    undoable edit.
  - The "Shapes: on/off" button toggles it, and each snap logs its class and time.
  - Not yet checked on the device. Caveat: in raw digitiser space a circle becomes an ellipse if the raw → screen
    map is anisotropic.
- **Gates:** Rust tests (wire layout, distinct codes, builtin weights load, accuracy floor ≥ 80 % on 300 synthetic
  examples); `ShapeWireCheck` (Java decodes the same arrays); `ShapeSnapCheck` (hold, arrow path, closed circle,
  point-in-polygon). All are in `impl/check.sh`.

## Atlas's box regressions (2026-10-17 night)
Atlas vendored the crate, then reported that long thin boxes (200×80) were called `Ellipse` when upright and
`Rejected` when turned 0.3–0.785 rad. Repro: `tests/atlas_boxes.rs`, with Atlas's generator: 30 points per side and
`wobble(i) = (sin 1.7i + cos 0.37i) · 0.9`, added to x at index i and to y at i + 3.
- **Main cause: sample-level zigzag.** `sin(1.7 i)` flips sign almost every sample, so on the short sides (2.7
  units between points) consecutive points zigzag by up to ±1.8. The turning and direction features read that as
  curvature, and the classifier gave Ellipse 0.92. Without the wobble the same boxes were Polygon 0.99. Aspect
  ratio was not the cause: synthetic rectangles span h/w 0.35–0.8 and scored 93.5 %.
  **Fix:** `Norm::new` now smooths the resampled ink (`smooth()`: two passes of [1, 2, 1] / 4, ends kept), so
  every feature, the segmenter input and the fits all see it. After that, every box in the 3 heights × 5 turns
  sweep is Polygon ≥ 0.87, even on weights trained without smoothing.
- **Missed corners on turned boxes** (the segmenter merged a short and a long side). `segment::dp_pieces`
  (Douglas–Peucker, tolerance `DP_TOL` = 0.035 × diagonal; a closed stroke splits at its farthest point from the
  start) is a second corner source: Polygon and Polyline candidates are fitted from both piece sets, and after the
  sort only the cheapest candidate of each class is kept.
- **Even-pace corners in training:** `synth::humanise` keeps 70–100 % of the speed through corners in 35 % of
  drawings (`corner_speed`); the rest brake to 25 % as before.
- **`residual_weight`** is now a `Recognizer` field (default `RESIDUAL_WEIGHT` 0.15), and `shape-eval`'s 5th argument.
  Sweep (n = 2000, seed 3, weights trained with the even-pace corners): exact class 0.876 at 0.15 and 0.3,
  0.859 at 0.5, 0.820 at 0.8, so it stays 0.15.
- Numbers: the retrain with even-pace corners (data/shape2) got 0.879 exact / 0.936 family (before: 0.876 / 0.932).
  Adding smoothing on those weights gave 0.873 / 0.928, and a retrain on smoothed features is running (data/shape3).
- **Retrained on smoothed features** (data/shape3): 0.882 exact / 0.939 family. This is the shipped `weights/shape.oisw`.
- **Chamfered corners** (2026-10-18): a 200×40 box turned 0.4 rad came out as a 5-gon, because one short side was
  split at a bend just over `ANGLE_TOL`. `polygon()` now also tries the polygon without its shortest side while
  that side is under `CHAMFER` (0.5) × the median side (`polygon_of` builds each). The residual picks between
  them. Exact class: 0.885 at seed 3 and 0.881 at seed 5 (0.886 with CHAMFER 0.35, which missed this case).
- Tests: `atlas_boxes.rs`, 5 tests (upright, tilted, constant speed, the zigzag sweep, the 200×120 control) plus
  an ignored `debug_costs` that prints probabilities and candidate costs.


## Closed arrow heads (2026-10-18)

**Finding.** The author's freehand arrows (15 crops from their notes, ids listed in) were run
through `shape-crops` (`cargo build --release -p openink-shape --features dataset --bins`, then
`impl/target/release/shape-crops data/hwr/words.json ID...`; `SHAPE_REJECT_MARGIN=100` shows what a reject
hides). Only 5/15 came out as Arrow. Most of the author's arrows are a shaft plus a **closed triangle head in
its own stroke**, often drawn *before* the shaft and sometimes gone over twice. The synth drew only open V
heads, and the fitter only looked for barbs, so all of those were Rejected.

**Changes.**
- `Decoration::TriangleHead`, wire code 2 (`wire.rs`). Java: `Shape.Head {NONE, OPEN, CLOSED}` on each
  primitive end; `arrowAtStart/arrowAtEnd` stay true for any head, so older callers still draw an arrow. The
  demo's `ShapeSnap.trace` draws a closed head as tip → barb → barb → tip.
- `recognize::closed_head`: another stroke that (a) has a box diagonal between 0.05 and 0.6 × the shaft,
  (b) touches the shaft end (end inside its hull, or within 0.3 × its size), (c) goes at least 0.85 of the way
  round its hull (an open V covers ~0.74), and (d) is nearly a triangle: the largest triangle on its hull
  covers ≥ 0.7 of the hull (circle 0.41, square 0.5, so lollipops and boxes fail). Checked before barbs.
- The residual ignored heads, so a big drawn head counted as misfit against every arrow. `Cand` is now a
  struct with an `explained` point set (the head stroke, or the barb pieces), added to the model in
  `fit::residual_explained`. Arrow costs on the author's crops fell (e.g. 0.22 → 0.17, 0.74 → 0.56).
- Synth (`synth::arrow`): 35 % closed heads, head length 0.12–0.3 × size, spread 0.4–0.85 rad, any start
  vertex, the head stroke before or after the shaft, 30 % with one side drawn again. Retrain into
  `data/shape4` (same commands as the "Atlas's box regressions" retrain).
- Tests: `a_triangle_is_a_closed_head_and_a_lollipop_is_not`, `largest_inscribed_triangle`, and the Java
  `ShapeWireCheck` closed-head case.

Still open: double-line shafts (⇒, crop 800); crops that include neighbouring scribbles (968). (A closed head drawn
in the same stroke as the shaft is handled since: `is_triangle_head`, below.)

**One-stroke closed heads (2026-10-18).** Atlas's tail → tip → b1 → b2 → tip arrow at length 120 was Rejected:
the barbs were found (Arrow cost 0.83) but the classifier gave Reject 54 % vs Arrow 45 % (reject cost 0.62).
`arrow()` now also checks the shaft stroke's own ink beyond each end (`is_triangle_head`): small next to the
shaft, leaving from and returning to the end, ≥ 0.85 of the way round its hull, largest inscribed triangle ≥ 0.65
of the hull — the same tests as a separate head stroke. A V head fails (it ends at its second barb), so V heads
stay `ArrowHead`. A verified one gets `TriangleHead` and the closed-head bonus: cost −0.68. shape-eval unchanged
(exact 0.887, family 0.941, 4000 seed 3); the author's freehand arrows still 11/15. Test
`one_stroke_closed_head_is_an_arrow` (tests/atlas_more.rs).

**Less skew (2026-10-18).** Shapes usually came out skewed. A quadrilateral was a rectangle only with
every corner within 10° of square, so a hand-drawn box with one corner at 78° became a parallelogram or trapezoid; and
shapes were turned onto the axes only within 6°. Now: rectangle when every corner is within 18° (`RIGHT_TOL`) **and**
both pairs of sides are within 14° of parallel (`RIGHT_PAR_TOL`, which keeps trapezoids), and the axis snap is 12°.
shape-eval (4000, seed 3): exact 0.887 → **0.890** (trapezoid 0.989, rectangle 0.971), family 0.941; the author's arrows
11/15. A rhombus now needs clearly skewed corners (a 77° diamond reads as a square). In Atlas's Notepad the snap also
waits 650 ms (was 500) and needs a stroke at least 40 dp across (letters do not snap), except an arrow head by its shaft.
