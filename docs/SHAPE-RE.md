# MyScript Shape (the closed recogniser behind Boox Notes' shape snapping): static RE


Boox Notes' shape recogniser is closed (see SHAPE-RECOGNITION.md, step 4), so it was studied statically. The
target shape set: whatever knote2 supports, plus line, rectangle, ellipse, triangle, arrows and polygons.
**MyScript is third-party licensed code, not Onyx's.** This RE is for understanding only: the open recogniser is
our own design. No MyScript code, weights or tables are copied into the repo, and the extracted files stay
local under `firmware/myscript/` (gitignored).

## What ships (fw 4.2.1, `system/`)
| file | size | role |
|---|---|---|
| `lib64/libMyScriptShape.so` | 494 KB (.text 404 KB) | the shape recogniser and beautifier; **fully stripped**: one export, `MyScriptShape` (a data table = the module's entry vector), no `.symtab`, no `.gnu_debugdata`; needs only libc/libm (`atan2f`, `sincosf`, `hypotf`, `tanhf`, `fmodf`); no readable strings |
| `lib64/libMyScriptAnalyzer.so` | 617 KB | groups strokes (text lines vs drawing) before shape recognition |
| `lib64/libMyScriptEngine.so` | 2 MB | the engine and configuration scripting: `AddResource`, `SetShapeRecognizerConf`, `SetShapeBeautifierConf`; errors such as `must AddResource SHAPE_KNOWLEDGE before %s`, `%s expects a ShapeRecognizer or a ShapeBeautifier` |
| `recognition_assets/conf/diagram.conf` | text | bundle `diagram`: `analyzer` = `AddResource analyzer/ank-diagram.res`, `SetProcessingLevel TEXT_LINES`; `shape` = `AddResource shape/shk-diagram.res` |
| `recognition_assets/resources/shape/shk-diagram.res` | 193 KB | **the shape knowledge**: class taxonomy, per-family parameters, two trained networks |
| `recognition_assets/resources/analyzer/ank-diagram.res` | 178 KB | the analyzer's knowledge (same class names) |

`tanhf` among the imports already hints at neural networks.

## The `.res` container (decoded: `docs/scripts/myscript_res.py`)
- `object := 90 fe 01 01, size:u32le, body[size]`; `body := type:u32le, header, children, [raw tail]`.
- Siblings are separated by a `01` byte ("another follows"); the last child has none.
- A shape class (type `0xfa0`) is `00 00 00 01, len:u16, name, f32le × 3`.
- Bytes after an object's last child are a raw tail; the trained models live in one.

The tree of `shk-diagram.res` (all 37 objects; the parse covers the file to the last byte):
```
0xfa1 root
  0xfb0  [96, 2, 4]                                   recogniser settings
    0xfb2  [1.0, 3]                                   the class set
      0xfb5  lines        params [0.65, 0.25, 0.5, 15, 20, 0.05, 0.025, 20, 20, 0.1 …]
        line [0, 0.35, 1.08]   polyline [0, 0.3, 1.08]   polygon [0, 0.3, 1.2]
        0xfb6  triangles  params [0.25, 0.6, 0.1745 (10°), 0.2269 (13°), 0.3491 (20°)]
          triangle [0, 0.4, 1.5]  isosceles [0, 0.4, 1.51]  right [0, 0.4, 1.52]
          right isosceles [0, 0.4, 1.53]  equilateral [0, 0.4, 1.54]
        0xfb7  quadrilaterals params [0.5, 0.6, 0.1571 (9°), 0.2967 (17°), 0.3491 (20°), 0.2793 (16°), 0.3142 (18°)]
          quadrilateral [0, 0.5, 1.5]  trapezoid [0, 0.5, 1.51]  parallelogram [0, 0.5, 1.52]
          rhombus [0, 0.5, 2.5]  rectangle [0, 0.5, 1.6]  square [0, 0.5, 2.6]
      0xfb4  conics       params [0.25, 10, 9.4248 (3π), 5.76, 2, 0.393 (π/8), 5.89, 0.1, 4, 0.25 …]
        ellipse [0.02, 0.25, 1.5]  circle [0.02, 0.25, 2.718]
        arc of ellipse [0.03, 0.5, 1.02]  arc of circle [0.03, 0.5, 1.05]
      0xfb8  arrows       params [6, 100, 8, 0.1745 (10°), 6.46, 0.25, 0.35, 0.15, 2.18, 0.6 …]
        arrow, double arrow, polyline arrow, polyline double arrow, curved arrow, curved double arrow  (all [0, 0.35, 3.0])
      0xfb9  [~2e-5, ~1e-5, 0.8, 1000, 0.5]
      0xfba  [0.4, 0.4, 0.5, 5, 0.075, 0.3, 4, 0]
      raw tail 191 542 B: header u32 [2, 4, 30, 1, 26, 1, 1], then two type-0x5f objects (below)
    0xfbc  [50, 0.25, 0.2]
      0xfbb  [0.15, 2, 7, 100, 100, 2, 0.225, 0.1]
  0xfb1  [0.15, 0.15, 0.2, 20, …, 1.25, 1.25, 1.5, 2.0, 0.15]   beautifier settings (likely)
```
Reading of the per-class triple (an inference; to be confirmed in the code):
- The first value is a minimum (non-zero only for conics).
- The second is a family-wide tolerance (0.3–0.5).
- The third is a **specialisation bonus**: ordered so the most constrained class wins when several fit. Square
  2.6 > rhombus 2.5 > rectangle 1.6 > parallelogram 1.52 > trapezoid 1.51 > quadrilateral 1.5; circle 2.718 >
  ellipse 1.5; equilateral 1.54 > … > triangle 1.5.

Classes form a **hierarchy** (line ⊂ polyline ⊂ polygon → triangle/quadrilateral families → special cases),
and each family carries angle tolerances of 9°–20°.

## The two trained models (in the 191 KB tail)
Both blobs are float32 (99.9% of the values are finite and of plausible magnitude), with no nested objects.
- **Model 2 (61 872 B)**, header `1, 5, [1,43], [1,100], [1,50], [1,100], [1,8]`: a 5-layer MLP,
  **43 → 100 → 50 → 100 → 8**. Parameter count 43·100+100 + 100·50+50 + 50·100+100 + 100·8+8 = 15 358,
  against 15 467 floats in the blob minus its header. This is a **whole-stroke classifier**: 43 global features
  → 8 outputs.
- **Model 1 (129 596 B)**, header `2 ×` (`3, 1, 26, 3, 50, 4, 8`): two networks over **26 features per point**,
  hidden layer type 3 with 50 units, output 8. A plain 26→50→8 needs about 1 758 floats; the blob holds 32 398.
  An LSTM-sized hidden layer (4·(26·50 + 50·50 + 50) ≈ 15 400 per network) fits. So these are most likely a
  **bidirectional recurrent pair** labelling each point (segmenting a stroke into primitives: line, arc, corner…).

**So MyScript's shape recognition is learned, not geometric rules.** A neural segmenter and a neural
classifier propose; the family parameters and class bonuses decide; the beautifier snaps.

## Recovered: the output model (strings in `libMyScriptShape.so`)
- A recognised shape (`shapeRecognized`, with `label` and `cost` per candidate) is a set of **primitives**: `line`
  (p1, p2) and `ellipticArc` (centre, `phi` rotation, `startAngle`, `sweepAngle`), plus the decorated forms
  `decoratedLine` (`p1Decoration`, `p2Decoration`) and `decoratedEllipticArc` (`firstDecoration`,
  `lastDecoration`). A decoration is `arrow-head` or `none`, so an arrow is a decorated line or arc.
- Other results: `shapeScratchOut` (a scratch-out gesture, with the `erasedRange` it erases), `shapeErased`,
  `shapeRejected`.
- Two steps that can be switched separately: `doRecognition` and `doBeautification`. Settings include
  `resolution` and `quantization`. Parsing errors include `unknown primitive type %s`.

## Recovered: the 43 whole-stroke features (`FUN_0015177c`, Ghidra address = ELF + 0x100000)
`FUN_0015177c` (8.7 KB, 2 callers) fills a float vector that feeds the 43-input classifier. Ratios are squashed
with `tanh`. Slots decoded so far:
| slots | feature |
|---|---|
| 0, 1 | mean and standard deviation of a per-point quantity over the resampled points (`fVar37`/`fVar38`) |
| 2 | a 0.5-quantile (median) helper, `FUN_001635d0(0.5, …)` |
| 3 | bounding-box aspect, signed: `tanh(h/w − 1)` if h ≥ w, else `−tanh(w/h − 1)` |
| 4 | `tanh(path length / end-to-end distance − 1)`: how far the stroke is from straight (or closed) |
| 5, 6 | end-to-end displacement over width / height (signed direction of travel) |
| 7–10 | `tanh(·/width)` and `tanh(·/height)` of four extent measures |
| 11–13 | gaps between the strokes of a group: `tanh(Σgap/Σlen)`, `tanh(min gap/Σlen)`, `tanh(max gap/Σlen)` |
| 14 | number of strokes in the group |
| 15–22 | **8-bin direction histogram**: each segment's undirected angle (mod π) soft-split between its two nearest bins (quadratic weight), weighted by segment length, normalised by total length |
| 23–30 | **8-column ink profile**: the bounding box squared to its longer side and cut into 8 columns; the length of ink projected onto each, normalised to sum 1 |
| 31–38 | the same 8-row profile |
| 39, 40 | crossings of the column / row centre lines, `tanh(count / 8)` (how often the stroke doubles back) |
| 41, 42 | not yet located (set elsewhere) |

The function also has branches per candidate kind (`iVar11 - 0x12 < 9` …), so the same code computes features
for primitive candidates; the 26 per-point features of the recurrent segmenter come from a separate path that is
still to be read.

These features are generic stroke geometry (direction histogram, projection profiles, straightness, aspect);
an open recogniser may use the same *kinds* of measures with its own code, thresholds and (if any) its own
trained model.

## Recovered: how a candidate is scored (line family, `FUN_0014294c`; cost helper `FUN_00144964`)
- Each family is a class in MyScript's object system: small methods return the type id (0xfb4 conics, 0xfb5
  lines, 0xfb8 arrows) and the object size. The output types are 0xfa6 line, 0xfa7 decoratedLine, 0xfa8
  ellipticArc, 0xfa9 decoratedEllipticArc, 0xfac shapeRecognized, 0xfad shapeScratchOut, 0xfae shapeErased,
  0xfaf rejected (`FUN_00108a68`).
- The line family first splits the ink into a polyline (two fitters, chosen by the ink's kind, with thresholds
  0.9/0.9 and 0.9/0.7). It then scores the polyline with a geometric cost (`FUN_00144964`), described here in
  words:
  - each fitted segment's angle deviation from the ink between its vertices, weighted by ink length;
  - the RMS and maximum distance from the ink to the fitted segments, over the scale;
  - the RMS of local angle changes at interior points.

  These are blended by means and maxima into one number.
- The final candidate cost is `class offset − ln(p)`, where `p` comes from the class's model. Lower is better,
  and candidates are kept sorted. So MyScript, too, combines a learned probability with a geometric residual.
  openink-shape uses the same idea (docs/SHAPE-ENGINE.md), with its own formula.

## What this means for the open recogniser
- A behavioural clone of MyScript would mean running *their* trained weights. Those are MyScript's data, so they
  cannot be copied or shipped. The trained part has to be our own: either geometric fitting, or a model we train
  ourselves on our own strokes.
- What the RE does give us, as a specification:
  1. **The class set and hierarchy**: 24 classes. They cover everything knote2 offers, the author's four, and the
     arrows and polygons.
  2. **The decision structure**: per-point segmentation into primitives → whole-stroke family → the most
     specialised class that fits, by a bonus order.
  3. **Tolerance magnitudes**: angles of 9°–20° per family (for example 10° for a triangle's special angles, and
     9°/17° for the quadrilateral tests).
- Next static steps (Ghidra, full auto-analysis; the library is small enough):
  - follow the `MyScriptShape` entry table to the recogniser;
  - recover the **26 per-point and 43 per-stroke features** (the formulas, not the weights; they say which
    geometry matters);
  - read how each family's parameter vector is used (which value is which tolerance);
  - read the beautifier's snapping rules (`0xfb1`).

## Commands
```sh
mkdir -p firmware/myscript && cp firmware/build/stage/system/system/lib64/libMyScriptShape.so \
  firmware/build/stage/system/system/recognition_assets/resources/shape/shk-diagram.res firmware/myscript/
python3 -I docs/scripts/myscript_res.py firmware/myscript/shk-diagram.res > firmware/myscript/shk_tree.txt
```
