# openink-stroke — the open stroke renderer (replaces `libneo_pen.so`)

Goal: *"i want same experience as the default notes app"*, *"the more
functionality the better"*. knote2 draws a crude instant stroke through SurfaceFlinger and, on
pen-up, replaces it with a high-quality render from `libneo_pen.so`. This crate is that second
layer, open: `impl/crates/openink-stroke` (Rust, pure, host-tested), JNI in
`impl/crates/openink-jni/src/stroke.rs`, Java API in `impl/java/openink/.../stroke/`.

## What was taken from the firmware (knote2 jadx, read through safecat)
`libneo_pen.so` exports only 7 JNI functions, but its C++ type info names the whole internal design. See **libneo_pen.so internals** below (read with rea/Ghidra). The **contract** around it is visible from Java too. Shape recognition (circle/rectangle snapping) is a different system, see [SHAPE-RECOGNITION.md](SHAPE-RECOGNITION.md):

| knote2 class | fact used |
|---|---|
| `NeoPenConfig` | pen type ids BRUSH 1, FOUNTAIN 2, MARKER 3, CHARCOAL 4, CHARCOAL_V2 5, FOUNTAIN_V2 6, PENCIL 7, BALLPOINT 8, SQUARE 9; brush shapes circle/ellipse/rectangle; defaults width 3, minWidth 0.001, tiltScale 3 (TILT_SCALE_VALUE 5), maxTouchPressure 1, dpi 320, brushSpacing 0.25, brushRatio 5, pressureSensitivity 0.3, velocitySensitivity 0.5, smoothLevel 0.6; `getPrecision(scale)` 1/4/8 |
| `NeoPenResult` / `PenInk` | each call returns *real ink* and *prediction ink*: `float[] points`, `int[] pointSizeArray`, `Bitmap[]` |
| `PenPathResult.buildFromInkArray` | outline pens: points are x,y pairs, `pointSizeArray` splits them into closed polygons, drawn with `drawPath` |
| `PenPointResult` | brush/fountain fast mode: `(x, y, size)` triples drawn as lines of width `size` |
| `NeoPencilPen` + `PenBrushResult` | pencil: 5 floats per dab `(x, y, size, angle_rad, alpha)`; angle quantised to 36 steps of 10°; mask = the `pencil` drawable scaled to `size`, ALPHA_8, drawn without filtering |
| `NeoMarkerPenWrapper.drawStroke` | marker drawn opaque into an offscreen bitmap, composited at **alpha 128** (self-overlaps do not darken) |
| `NeoSquarePen.defaultPenConfig` | rectangle nib, ratio 10, angle 45°, no direction |
| `FountainShapes.createNeoPenV2` | width + 3/createScale, tilt off, pressure /= `EpdController.MAX_TOUCH_PRESSURE` |
| `NotePenInfoUtils` | width slider 0.1–2.0 mm (marker 0.5–8 mm), step 0.05 mm; default sensitivity 0.3, smoothing 0.6; sliders 0–10 |
| `PenConstant` | default widths px: 7.2 (pens), brush 8.4, marker 48, charcoal 3.6, pencil 6.0 (device config may override in mm) |
| `ShapeFactory` | shape types: 2/3/4 plain pencil-path pens, 5 fountain, 15 marker, 21 neo-brush, 22 charcoal (texture 2 = pencil v2), 47 square, 60/61 calligraphy |

The Onyx `pencil` texture is **not** used: the grain mask is generated (`raster::grain_mask`, value
noise at ~1.5 px cells inside a soft disc), so the repo stays MIT-clean.

## libneo_pen.so internals (rea + Ghidra 12.1.4)
Binary: `firmware/onyx/pen/libneo_pen.so`, arm64, sha256 `4e902c14…5f5e`. 2 501 functions,
`.text` 519 KB, `.rodata` 2.9 MB (mostly brush bitmaps/tables), 1 912 strings. Source files named in
log calls: `neopen_jni.cpp`, `neo_pen_export.cpp`, `JNIUtils.cpp`. Bundled open libraries:
**Clipper2** (polygon union, Boost licence), **spdlog** + **fmt** (logging).

**JNI surface** (`com.onyx.android.sdk.pen.NeoPenNative`): `nativeCreatePen`, `nativeDestroyPen`,
`nativeOnPenDown`, `nativeOnPenMove`, `nativeOnPenUp`, `nativeSetBitmapColor`, `nativeSetLogLevel`.

**Config read by `nativeCreatePen`** (fields of the Java `NeoPenConfig`, with the clamps the
native side applies):

| field | clamp in native | Java default |
|---|---|---|
| fastMode, dpi, displayScaleX/Y, scalePrecision | — | false, —, 1, 1, 1 |
| color | ARGB → internal ARGB8888 | — |
| width | must be ≥ 0, else "invalid pen width" and no pen | 3 |
| minWidth | — | 0.001 |
| rotateAngle (int), tiltEnabled, tiltScale, directionEnabled | — | 0, —, 3, false |
| brushShape (int), brushRatio, brushAngle | — | —, 5, — |
| brushSpacing | [0.1, 1] | 0.25 |
| pressureSensitivity | [0, 1] | 0.3 |
| velocitySensitivity | [0, 1] | 0.5 |
| velocityAmplifier | [0, 1] | 0 |
| velocityIgnoreThreshold | ≤ velocityLowerBound (logged and lowered if larger) | 0 |
| velocityLowerBound, velocityUpperBound | [0, 50] | 0, 0 |
| smoothLevel | [0, 1] | 0.6 |

Our `PenConfig` has no velocity bounds, amplifier or ignore threshold yet. With knote2's
defaults of 0 they are probably inactive unless a pen sets them; to check per pen.

**Pen factory** (`neopen_createPen`): Java pen type 1…9 → one factory each (anonymous namespace):
`BrushPenFactory`, `FountainPenFactory`, `MarkerPenFactory`, `CharcoalPenFactory`,
`CharcoalPenV2Factory`, `FountainPenV2Factory`, `PencilPenFactory`, `BallpointPenFactory`,
`SquarePenFactory`. Any other type → "unknow pen type". Pen classes (`neo::pen`): `NeoBrushPen`,
`NeoFountainPen`, `NeoFountainPenV2`, `NeoMarkerPen`, `NeoCharcoalPenV2`, `NeoPencilPen`,
`NeoBallpointPen`, `NeoSquarePen`, base `NeoPen`.

**Pipeline** (from the `StrokeProcessor<In, Out>` template instances, a chain of stages):
1. `TouchEvent` → `StrokePointBlob` (input points; `StrokePointProducer`).
2. `StrokePointBlob` → `StrokeSpline` (`SplineProducer`), → `StrokePointBlob` again via
   `SplineInterpolator`, `SplineDistanceInterpolator` (even spacing) or
   `SplineCurvatureInterpolator` (denser where it bends).
3. Width per point: `StrokePointCalculator` with `BallpointPenCalculator`,
   `DirectionalWidthCalculator` (direction/nib angle: fountain, brush), `FixedWidthCalculator`
   (marker) and `SquareCalculator` (square nib); `PointCalculator` is the base.
4. Output: outline pens → polygons merged by `PolygonMerger` (Clipper2 `Clipper64` union);
   textured pens → `Brush` / `VectorBrush` / `BrushApplier` stamping a mask: `BitmapMask` (pencil
   texture), `CircleMask`, `GaussCircleMask` (soft dab, charcoal). `NeoStrokeRenderer` /
   `StrokeRenderer` draw into the bitmap.

**What this means for openink-stroke:** the overall structure matches ours: smoothing, then
a spline, resampling, a width per point, then polygons for outline pens and stamped dabs for
textured pens. Two differences are worth trying:
- knote2 resamples by **curvature** (more points in tight bends). We cut fixed ~1.5 px pieces.
- knote2 **unions the pieces into one polygon** with Clipper2. We rely on nonzero-winding fill
  of overlapping convex pieces. The result looks the same, but the output is bigger.

### Width formulas (decoded; ported into `stroke.rs::node_for`)
How they were found: the class name strings → their type-info → the vtables. The vtables are
filled by `R_AARCH64_RELATIVE` relocations, which Ghidra does not cross-reference, so a small
script resolved them. Each calculator's vtable lies inside its pen's block of type-info, which
gives the pen → calculator map. Ghidra addresses are the file's vaddr + 0x100000. Config offsets
follow the struct `nativeCreatePen` fills: dpi +8, displayScale +0xc/+0x10, width +0x1c,
minWidth +0x20, tiltEnabled +0x28, directionEnabled +0x30, pressureSensitivity +0x44,
velocitySensitivity +0x48, velocityAmplifier +0x4c, velocityIgnoreThreshold +0x50,
velocityLowerBound +0x54, velocityUpperBound +0x58.

| pen (knote2 type) | calculator (Ghidra addr) | per-point output |
|---|---|---|
| ballpoint (8) | `BallpointPenCalculator` (0x43c970) | constant half-width W/2, no pressure, no speed |
| marker (3) | `FixedWidthCalculator` (0x459c0c) | constant W/2 |
| square (9) | `FixedWidthCalculator`; with `directionEnabled`, `DirectionalWidthCalculator` (0x45983c) | constant nib W |
| fountain V2 (6) | `SquareCalculator` (0x4529ac) → `FountainPenWidthCalculator` (0x453d90) | x, y, width, nib angle (unwrapped), then two tilt terms `1 + 0.3·|cos|`, `w·0.3·|cos|` when tilt is on |
| pencil (7) | `PointCalculator` (0x457f80) → fountain width | x, y, width, `0.6·min(p,1)` |
| charcoal V2 (5) | `PointCalculator` (0x44e330) | x, y, `min(p,1)`, size `1.2·W + 5`, × `⌊(3·θ^2.2 + 1)·5⌋/5` with tilt (θ from vertical, rad) |
| brush (1) | own pipeline, decoded 2026-10-18 (see "Brush" below) | x, y, width (raw positions) |
| fountain V1, charcoal V1 | no named calculator, not decoded | — |

**FountainPenWidthCalculator**, with W = width, p = pressure, s = pressureSensitivity,
vs = velocitySensitivity, a = velocityAmplifier:
```
w   = max(W · p^(2s), minWidth)
v   = hypot(dx/displayScaleX, dy/displayScaleY) · 160/dpi      // travel since the previous point
f   = 1
if vs > 0:   f = 1 − 0.6·vs·atan(0.06·v')   with v' = v, or v − upperBound when upperBound > 0
             (and f = 1 when v ≤ upperBound)
if a > 0 and lowerBound > 0 and ignore < v ≤ lowerBound:  f *= (a + 1) − v·a/lowerBound
width = clamp(w · f, minWidth, W·(1 + a))
```
At knote2's defaults (s 0.3, vs 0.5, a = bounds = 0): `W · p^0.6 · (1 − 0.3·atan(0.06·v))`.

**DirectionalWidthCalculator** (square pen with direction; ported only in this doc): the first
point waits until the pen has moved 0.5 px from the down point. Half-width
`r = R·0.6 + R·0.4·sinθ + |R·0.4·cosθ|` (θ = atan2(dy, dx) of the motion, so moving up is thinnest).
It is then smoothed with the previous outputs by a speed-dependent kernel: v ≥ 5 → [⅔, ⅓];
0.5 ≤ v < 5 → [½, ⅓, ⅙]; v < 0.5 → [10, 8, 6, 4, 2]/30. A kernel longer than the history falls
back to a shorter one. The history is cleared at pen-up.

**What `node_for` does now:**
- Ballpoint, marker and square are constant.
- Fountain is the formula above (diameter, halved to a half-width).
- Pencil's dab size is the formula above. Its opacity stays openink's curve: libneo's `0.6·p`
  rendered far too faint as alpha, so how the brush stage maps it is not decoded yet.
- Charcoal is `1.2·W + 5` with alpha p and the stepped tilt factor.
- Brush keeps the earlier openink model.
- Not ported: the velocity bounds and amplifier (inactive at knote2's defaults, and not in
  `PenConfig`), the direction mode, and the fountain V2 tilt terms (knote2 turns tilt off for it).
- Speed is libneo's: travel per sample in 160-dpi px. This panel's 300 dpi is a constant
  (`PX_TO_160DPI`).
- Visible effect: fountain lines are finer at ordinary pressure (≈ 0.57·W at the median 0.39),
  and charcoal is bolder.

Next reads, if wanted: the brush applier (pencil/charcoal opacity), NeoBrushPen, and
`SplineCurvatureInterpolator`.


## Design (new)
- `Stroke::down(sample) -> (ActiveStroke, Ink)`, `ActiveStroke::moved(&[Sample]) -> Ink`,
  `ActiveStroke::up(self, sample) -> Ink`: a typestate, so move-after-up cannot compile. Each call
  returns only *new* ink; earlier ink never changes (a live view draws it once). `Stroke::render`
  = down + moves + up, and a test proves incremental == whole.
- Smoothing (`filter.rs`): libneo's FIR SmoothFilter (see "Smoothing and spline, decoded" below; the earlier
  EMA is replaced).
- Curve: libneo's uniform Catmull-Rom with one point of look-ahead; pieces cut to ~1.5 px.
- Outline pens emit one convex polygon per piece (tapered capsule = hull of two discs; the square
  nib = hull of the nib rectangle at both ends); all share one orientation, so nonzero-winding
  fill of the whole list is their union.
- Width model: the original hand-tuned model was replaced by the decoded libneo formulas (see "Width formulas" above); only the brush still uses it.
- (Original, superseded) Width model (`stroke.rs::node_for`), tuned so the author's **median pressure 0.39** (p10 0.16,
  p90 0.64, from the recorded trace) gives about the nominal width:
  ballpoint `w/2·lerp(.8,1,p)`; fountain `w/2·(1−a+1.4a·p^.8)`, `a=.25+s`, thinned up to 35 %
  by speed; brush `a=.6+s`, `1.6a·p^1.1`, thinned up to 45 %; marker constant; square nib length
  `w·lerp(.75,1,p)`; pencil size `w·lerp(.7,1,p)·tilt`, alpha `.35+.8p^1.1`; charcoal size
  `w·lerp(.6,1,p)·tilt`, alpha `.35+.7p`. Speed normalised 0.3–4 px/ms. Tilt widens dabs only
  beyond 30° (ordinary grip), up to `tiltScale` at 60°; the trace's max tilt was 39°.
- Dabs every 0.25·size (charcoal 0.18), carried across pieces; each dab gets a fixed
  pseudo-random 10°-step rotation.

## Smoothing and spline, decoded from libneo and checked against it (2026-10-18)

**The oracle.** `impl/java/neo-oracle/` runs the device's own `/system/lib64/libneo_pen.so` (sha256 `4e902c14…`,
the same binary) through `app_process` with our stub classes for the names its JNI code looks up, and prints the
ink per call (`bash impl/java/neo-oracle/run.sh PEN_TYPE samples.txt field=value…`). Each fountain piece is the
hull of two near-octagon discs (10 points), so node centres and radii can be read back exactly.

**SmoothFilter** (`neo::pen::SmoothFilter`; vtable found through its type-info and the RELATIVE relocations):
- An **FIR low-pass** over the last N samples of every point channel: N = ⌊50 · smoothLevel⌋ (from the
  constructor's caller: `fmul s0, smoothLevel, #50.0; fcvtzu`), N < 3 → 2, at most 50.
- Weights: a fixed 15-tap kernel (`_INIT_2`: −6e-6, −0.000139, −0.000185, 0.000414, 0.002357, 0.003357,
  −0.003135, −0.023928, −0.042909, −0.017858, 0.096525, 0.254692, 0.347072, 0.268810, 0.114933; a windowed
  low-pass, newest tap last), resampled linearly to N taps (t_i = i·14/(N−1)), times 15/N, normalised to sum 1.
  (Ghidra's decompile shows both lerp terms reading the same entry; the oracle says it interpolates.)
- The history starts filled with copies of the first sample; one filtered point per input.
- Pen-up pushes the up point max(4, ⌊4N/15⌋) times, so the line runs out to it (the kernel's side lobes make
  it overshoot slightly first, which is what the oracle shows).
- Verified: every node of a 60-sample irregular line at smoothLevel 0.3, 0.6 and 1.0 within ~1e-4 px.
- The old openink model (EMA with alpha 1 − 0.85·smoothLevel and a 0.75 px minimum step) is gone.

**SplineProducer**: uniform **Catmull-Rom** in matrix form, `[1, t, t², t³] · M · [p0, p1, p2, p3]` with
M = [[0, −½, 1, −½], [1, 0, −5/2, 3/2], [0, ½, 2, −3/2], [0, 0, −½, ½]] (`_INIT_3`). The segment p1 → p2 waits
for p3 (one point of look-ahead: the first two moves return no ink), the first point is duplicated as p0, and
pen-up duplicates the last as p3. openink-stroke now does the same (it had a chord end tangent, no look-ahead).

**Widths** (confirmed on the oracle): radius = W·p^(2s)/2 · (1 − 0.6·vs·atan(0.06·v)), v = distance between
consecutive nodes × 160/dpi, with dpi from the config (SF's painters use 320).

**SplineCurvatureInterpolator, x/y part (ported, checked):** recursive midpoint bisection of each segment's
t-range: split while the curve's midpoint is more than tol from the chord (squared distance to the clamped
projection, vs tol²) or more than 10·tol from the chord midpoint in x or y; tol = 0.15 / clamp(scalePrecision/2,
1, 16) = 0.15 px (pen width does not change it; scalePrecision 8 doubles the pieces). The model reproduces the
oracle's pieces per segment on 16 arcs (2°–45° per sample, radius 30 and 120) and the exact t values
(0.125, 0.25, 0.5, 0.75, 0.875 on a 45° arc). Every channel is splined (pressure too), and inserted nodes get the
segment's speed (libneo's radii stay constant along a steady arc). openink-stroke now draws one piece per
interval instead of fixed 1.5 px pieces. Still open: the pressure-channel bisection (`FUN_0044abd4`), the extra
quarter points of channel 4 (`FUN_004498a4`, threshold 0.06) and the accumulated-turn test (`FUN_0044973c`).

**Stage 4, the other channels (in progress).** `FUN_0044abd4` splits an interval when a channel's spline midpoint
differs from the average of its ends by more than tol (0.15); `FUN_004421b0` looks a channel id up in the point's
channel list (x = 0, y = 1; ids 3 and 4 are the two extra channels, not yet mapped). Oracle on a straight line,
6 px steps, smoothLevel 0:
- pressure 0.2/0.9 in runs of three: a t = 0.5 node only in the segments *inside* the 0.9 run (radius 4.61 there
  vs 4.44 at the knots); none across the steps or inside the 0.2 run, although the raw pressure deviation is the
  same 0.044 everywhere. So the tested channel is not raw pressure: most likely the width, or pressure scaled.
- tiltX 0/40 in runs of three: nodes at t = 0.25/0.75 (and 0.5) in every segment of the 40° runs: the channel-4
  quarter-point rule (|Δ| > 0.06, probably the azimuth/altitude from `computeAltitudeAndAzimuth`).
Resolved for the width: the calculators run before the spline, so channel 3 is the **width** (a diameter for
outline pens): the per-move piece counts 1, 2, 2 per 3-sample run match a width-midpoint test at 0.15, and the
inserted node's radius (4.61) is the spline of the knot widths (4.60), not the width of the splined pressure.
Ported: inserted nodes take the width spline, and the bisection also splits on it. Channel 4 (the nib angle, only
with tilt) is not ported yet. The accumulated-turn test (`FUN_0044973c`) is inactive by default: its threshold
(interpolator +0x2c) starts at −1 and the test only runs when it is ≥ 0, so nothing to port for the stock pens.

**End-to-end check on the author's real strokes** (`examples/stroke-dump.rs` vs the oracle, fountain, width 10,
smoothLevel 0.6, dpi 320; rasterised at 4×, `xor / union`): 3.3–3.8 % with true circular caps → **0.8–1.4 %**
after switching outline pieces to libneo's shape, the hull of two axis-aligned regular octagons
(`geom::octagon_capsule`); total ink area within 0.3 %. The rest is at stroke ends and the open stage-4 items.

## Brush (NeoBrushPen, type 1), decoded and ported (2026-10-18)

Found through the relocations: type-info `N3neo3pen11NeoBrushPenE` → vtable 0x4a0368 (down `FUN_00436ed8`, move
`FUN_00437004`, up `FUN_004371e0`); `BrushPenFactory::create` (`FUN_00434bc4`) builds `NeoBrushPen::Impl`
(`FUN_00436dc8`, 0xd8 bytes): a copy of the config with **width doubled** (Impl+0x24 = W2 = 2W), and a block of
constants: fixed time step (+0x68 = true, +0x70 = 3), prev-weight k 0.5 (+0x78), down factor 1 (+0x7c), pool rate
0.01 / 16 (+0x80/+0x84), slower/faster ratios 0.95 / 1.1 (+0x88/+0x8c), ink 0.02, 1.2, exponent 0.5
(+0x90..+0x98), pressure exponent 0.5 (+0xa4). It **does not use smoothLevel, the spline, dpi or the
sensitivities**: positions are the raw samples (the oracle confirms all of this).

- **Down:** width = clamp(W2·√p, minWidth, W2 + 3); ink term 0; one point.
- **Resting** (until a sample is > 3 px from the down point): each sample adds W2·0.01·3/16 to the width (ink pools;
  0.0375 at W 10), sets ink = clamp(width − W2·√p, −0.8·W2, 3) and re-emits the down point. The first sample past
  3 px emits nothing; the segment to an accepted point is drawn when the next one arrives.
- **Move** (`FUN_00437864`, points a, b, c = the last three accepted): v1 = |ab|/3, v2 = |bc|/3,
  r = ((v2 + k·v1)/(k + 1))/v1. If r < 0.95 (slowing) ink += ((1 − e^−W2)·W2·0.02 + 1.2·√r)/2; if r > 1.1
  (speeding) ink −= the same /3; ink clamped to [−0.8·W2, 3]. Width at c = clamp(ink + W2·√((k·p_b + p_c)/(k + 1)),
  minWidth, W2 + 3).
- **Pieces** (`FUN_00437bc8`): b → c in ⌊|Δwidth|⌋ + 1 equal steps, width linear.
- **Up:** the pending segment, then the up point: if v2/v1 ≤ 1.1 it tapers to clamp(W2·√p_up), else the move rule.
- **Drawing** (knote2 `PenUtils.drawStrokeByPointSize`): a round-capped line from each output point to the next,
  as wide as the later point.

Port: `openink-stroke/src/brush.rs` (`BrushStroke`; `ActiveStroke` is now an enum of the spline pipeline and the
brush). Tests reproduce the oracle's numbers (14.1796 pooling, 14.8695 = 14.1421 + 0.7274 after slowing, 18.2393,
19.7011). On the author's six recorded strokes (W 10, oracle vs ours, `xor / union` at 4×): **0.6–1.4 %** on five
(was 8–28 % with the fountain-style model), 6.9 % on a 1.5 px dot; the rest is the 1.5 px-chord tessellation of the
round caps (≈ 0.7 % less area).

## Marker, square and ballpoint against the device (2026-10-18)

Every outline pen now checked end-to-end against libneo on the author's recorded strokes (W 10, `xor / union` at 4×):

| pen | before | after | what changed |
|---|---|---|---|
| ballpoint (8) | 0.02–0.17 % | (unchanged) | already libneo's: smoothed spline, octagon capsules, W/2 |
| marker (3) | no comparison (libneo returns points, not polygons) | **1.2–2.5 %** | own pipeline `marker.rs` |
| square (9, shape 2, ratio 10, angle 45) | 38–39 % | **0.03–0.25 %** | nib length W/2, exact thickness, angle sign |
| brush (1) | 8–28 % | **0.6–1.4 %** | own pipeline `brush.rs` (above) |
| fountain V2 (6) | 3.3–3.8 % | 0.8–1.4 % | octagon capsules (earlier) |

**Marker** (`NeoMarkerPen`; ctor `FUN_00437f74` keeps only the width; down `FUN_00438030`, move `FUN_00438110`,
up `FUN_00438224`): no smoothing, no spline; each raw sample is joined to the previous one by a line of width
W·0.8 (p ≤ 0.3), W·0.9 (p ≤ 0.6) or W, drawn round-capped by knote2 (`PenPointResult`), translucent.

**Square** (libneo hexagons = hull of the nib rectangle at two positions; measured at W 10 and 20): the nib is a
segment of length **W/2** at 45° rising to the right on screen, thickness length/ratio (0.5 px at W 10; openink had a
1 px floor and the opposite angle sign). SF's Square painter sends 2·strokeWidth as the libneo width, so its live
nib is strokeWidth long; `sf::live_pen` already settles with the same width.

## Pencil dabs: where the opacity goes (2026-10-18)

The oracle shows libneo's pencil (type 7) returns **dabs, not bitmaps**: `x, y, size, angle, 0.6·p` per dab (W 6:
size 2.28 / 3.96 / 5.63 at p 0.2 / 0.5 / 0.9 — the fountain width, as ported). The stamping is knote2's:
`NeoPencilPen.a(PenInk)` packs each dab into bytes (size → size/W·255, angle → 36 steps of 10°, alpha →
0.6p·255) and `PenBrushResult.a` draws the `pencil` drawable rotated by angle·10°, sized
max(2, size_byte/255·W·scale), with paint alpha `coerceIn(((alpha_byte & 255) / 255) · paintAlpha · k, 4, 255)`.
In the shipped bytecode that division is **integer** (jadx prints `/ 255`, next to `/ 255.0f` for the size), so it is
0 for every alpha byte below 255 (0.6·p never reaches 1) and each dab is painted at the floor alpha **4/255**; the
visible grey is the overlap of many dabs (spacing 0.25 of the size) and the panel's rendering. Not ported: openink
keeps its own opacity curve until a knote2 pencil stroke can be captured on the device and compared.

**Pencil dab placement and angles, decoded and ported (2026-10-18).** `NeoPencilPen` chains point calculator →
FIR (`FUN_00442f0c`) → spline → `SplineDistanceInterpolator` (`FUN_00451010`, spacing = brushSpacing 0.25): per
spline segment n + 1 candidates at t = j/n with n = ⌊2.5·(d / min knot size) / 0.25⌋ + 1; a candidate becomes a dab
when it is ≥ 0.25 · the mean of its and the last dab's size from the last dab (t = 0 only before the first dab).
Angles (`FUN_004580e0`): `java.util.Random` (`FUN_00470cb8`/`FUN_00470d34` = constructor/`nextInt`), seeded at
pen-down with 13·⌊1024·p⌋ (+ 7·a + 11·b of two point fields that are 0 for pen input), `nextInt(360)` degrees per dab
(prediction dabs use (17x + 13y) mod 360). Ported in `stroke.rs` (`PencilDabs`, `JavaRandom`). Against the device on
the author's strokes: s2 100/100 dabs, max position error 0.001 px, all angles equal; s5 116/116 (0.09 px); s4 156/157.
Opacity stays openink's curve (above).

**Stage order fixed (2026-10-18): calculators on raw samples, then the FIR.** libneo's chain is calculator → FIR →
spline → interpolator (pencil `FUN_00457524`, charcoal V2 `FUN_0044c8ec`): the width/alpha are computed from the
raw sample (speed = raw travel) and then filtered with the same FIR as the positions. openink computed them from the
already-filtered sample. `SplineStroke` now runs a second `Smoother` over (size, alpha). Effect: pencil dabs
**identical** to the device on s2/s4/s5 (100/100, 157/157, 116/116; position ≤ 0.001 px, size 0.000); charcoal V2
tiles 51/52, 58/66, 54/58 identical (were 44, 19, 27; spacing 0.1 and libneo's tilt angle acos(cos tx·cos ty) also
ported); fountain 0.72 / 1.31 / 1.83 % (were 0.76 / 1.40 / 1.85 %).

## Charcoal V2 drawing, decoded (`neo_charcoal_pen_v2.cpp` drawStroke, `FUN_0044cfac`)

The oracle (with bitmap dumping) shows libneo's charcoal ink as one RGBA tile per dab (22×22 at width 3.6), every
pixel fully on (alpha 255) or off: a hard **binary speckle**, dense in the middle, scattered at the edge. Per dab:
- S = ⌊dab size⌋. A shape bitmap S×S (cached per S) from libneo's `CircleMask` (vtable 0x4a1238): alpha 255
  inside an inner ellipse of 0.6·S, ramping to 0 at the outer circle of diameter S (`FUN_0044e96c`).
- A texture is chosen from the pen's list by a 0..1 channel: index = min(⌊v·(n−1)⌋, n−1); it is tiled in
  **screen space** (x, y mod texture size), so the grain stays put on the page.
- Per pixel, a = combine(texture alpha, shape alpha) (`FUN_00470920`); a = 255 → set; else set when a random
  integer in [1, 10000] ≤ a/255 · 10000 (`FUN_00470824`): stochastic dithering, not blending.
- The tile's top-left is (x − S/2, y − S/2) in integer pixels.
The textures are Onyx's art embedded in libneo's .rodata and are not copied into this repo; openink keeps its own
grain and adopts the binary dithering, which is what gives charcoal its look.

## Wire format (JNI, `org.openink.stroke.StrokeEngine`)
config `float[6]` width, pressureSensitivity, smoothLevel, velocitySensitivity, tiltEnabled,
tiltScale · samples `float[6n]` x, y, pressure, tiltX, tiltY, tMs-since-down · ink `float[4+k]`
bounds (NaN = empty) then data. Pen codes 0–6 in `PenKind` order (Rust `PenKind::code`).

## Verify
- `cargo test -p openink-stroke` (12 tests: geometry areas/orientation, bounds, tap, pressure
  ordering, incremental == whole, dab spacing, sanitising, rasterizer exactness, marker no-darken,
  grain mask).
- `cargo run --release -p openink-stroke --example stroke-preview -- "" out.pgm` renders the real
  trace with every pen (panels top→bottom in `PenKind::ALL` order).
- Device: `impl/java/demo/build-demo.sh --install`, "Pen:" cycles pens, "Width −/+" within the
  knote2 slider range.

## Open
- Prediction ink (libneo returns a predicted tail; we do not predict yet).
- Eraser.
- On-device side-by-side tuning against knote2.
- Rendering knote2 notes from storage ([KNOTE2.md](KNOTE2.md)) with these pens.
