# SurfaceFlinger's instant ink, decoded — and how the settled stroke can match it

**Why this exists.** Atlas (2026-10-18): *"what I draw is not exactly what then gets settled, something changes"*
and *"some strokes disappear when I write but then appear later when it gets settled"*. On a stock
(locked) Boox the low-latency ink you see while writing is drawn **inside SurfaceFlinger** from
`/dev/input`, by Onyx's painters (`MemoryPainter` and friends, SF-ONYX.md). The ink that stays is drawn
afterwards by the app (knote2 with `libneo_pen`, Atlas/openink with `openink-stroke`). If the two
renderers, their settings or their input points differ, the line visibly changes when it settles.

Everything below is read from the Ghidra decompile of `surfaceflinger` 4.2.1 (`firmware/onyx/sf/decomp/`,
names from its MiniDebugInfo; read through `docs/scripts/safecat.sh`) and from knote2's jadx output.
Ghidra addresses = ELF vaddr + 0x100000.

## 1. Input: from evdev to painter points (`android::TouchReader`, `PenManager`, `DrawReader`)

- **Device scale.** `TouchReader` starts with the primary display size and max pressure 4096 (0x45800000),
  reads the pen's absinfo in `openDevice`, then `PenManager::setRatio(dispW / rawMaxX, dispH / rawMaxY,
  maxPressure)`. Android's InputReader scales by `disp / (max − min + 1)`: the two positions differ by at most
  ~0.1 px at the far edge of this panel (raw X range ~20 000). Not a visible cause.
- **Events** (`PenManager::onEvent(type, code, value)`):
  - `EV_ABS` ABS_X / ABS_Y → `x = raw · ratioX`, `y = raw · ratioY` (floats, no rounding).
  - ABS_PRESSURE (0x18) → raw int; ABS_TILT_X (0x1a), ABS_TILT_Y (0x1b) → ints.
  - `EV_KEY` 0x141 (RUBBER), 0x142 (BRUSH), 0x14a (TOUCH), 0x14b (STYLUS) → flags. 0x152/0x153 latch a
    **pen model** (Onyx-specific codes): 0x152 also resets the pressure filter.
  - `EV_SYN` → the active reader's `onSynEvent`: **one painter point per SYN report**.
- **Pressure** (`PenManager::moveTo/quadTo`):
  - pen model 0x153: `p_i = (p_{i−1}/2 + raw)/1.5` (integer), `n = p_i / max`;
    `n < 0.15 → 4n`, else `min((n − 0.135)^0.2 + 0.17, 1)`.
  - any other model (**this device reports 0x152**, see the getevent trace): `n = raw / max`,
    linear — the same value Android gives `MotionEvent.getPressure()`.
- **Region gate** (`DrawReader::onSynEvent` + `PenManager::inValidRegion`): a point is drawn only if the
  square `[x ± w/2] × [y ± w/2]` (w = SF's stroke width) lies **fully inside** a limit rect (in region mode 1
  the first rect entered is latched for the stroke) and **does not touch** any exclude rect.
  First valid point → `moveTo` (pen-down); valid → `quadTo`; **invalid → `finishQuadTo` (the instant stroke
  ends)**; valid again → a new `moveTo` (a new instant stroke). The app's MotionEvents are not gated.
  → *This is Atlas's "strokes vanish while writing and appear once settled"*: ink near a limit-rect edge
  (within half a stroke width), or crossing an exclude rect (toolbar, page chrome), is never drawn by SF.
- **Painter call**: `moveToImpl(x, y, strokeWidth, size = −1, pressure, tiltX, tiltY, …)`,
  `quadToImpl(x, y, …, size = −1, pressure, tiltX, tiltY, …)`; pen-up (`onDeactivate`) → `finishQuadTo`.

## 2. Styles → painters (`MemoryPainter`)

`MemoryPainter::MemoryPainter` builds the painters in this order; `SET_STROKE_STYLE` (0xff0008) indexes it:

| style | painter | libneo pen type | knote2 pen (shape type) |
|---|---|---|---|
| 0 | `PainterBase` (plain Skia stroke, no libneo) | — | ballpoint / pencil (2, 3, 4) and anything unlisted |
| 1 | `PainterFountainV2` | 6 FountainV2 | fountain (5) |
| 2 | `PainterMarker` | 3 Marker | marker (15) |
| 3 | `PainterNeoBrushPen` | 1 Brush | brush (21) |
| 4 | `PainterCharcoal` | 4 Charcoal | charcoal (22) |
| 5 | `PainterDash` | — | (dashed line) |
| 6 | `PainterCharcoal(…, 5)` | 5 CharcoalV2 | charcoal with texture 2 (22) |
| 7 | `PainterSquare` | 9 Square | calligraphy (60, 61) |
| 8 | `PainterSoftEraser` | — | soft eraser |

knote2's mapping is `ShapeFactory.getStrokeStyle(type, attrs)`.

## 3. The pen each painter builds (the `NeoPenConfig` passed to `neopen_createPen` at every pen-down)

Field offsets as in STROKE.md ("libneo internals"). **W = SF's stroke width** (`SET_STROKE_WIDTH`).

| field | FountainV2 (1) | Marker (2) | NeoBrush (3) | Square (7) | Charcoal (4/6) |
|---|---|---|---|---|---|
| dpi +0x08 | **320** | 320 | 320 | 320 | (Impl, not decoded) |
| displayScale, precision | 1, 1, 1 | 1, 1, 1 | 1, 1, 1 | 1, 1, 1 | — |
| width +0x1c | **W + 3** | W | W | **2·W** | W |
| minWidth +0x20 | **1.0** | 1.0 | 1.0 | 0.001 | 1.0 |
| tiltEnabled +0x28 | 0 | 0 | 0 | 0 | param 0, or property `vendor.onyx.htcon` |
| tiltScale +0x2c | 3 | 3 | 3 | 3 | param 1 (default 3) |
| brushShape / ratio / angle | 0 / 5 / 0 | 0 / 5 / 0 | 0 / 5 / 0 | param 0 / param 1 / param 2 + display rotation (defaults 2 / 10 / 45°) | — |
| brushSpacing +0x40 | 0.25 | 0.25 | 0.25 | 0.25 | — |
| pressureSensitivity +0x44 | **param 0 (default 0.3)** | 0.375 | 0.375 | 0.375 | — |
| velocitySensitivity +0x48 | 0.5 | 0.5 | 0.5 | 0.5 | — |
| velocity amplifier / bounds | 0 | 0 | 0 | 0 | — |
| smoothLevel +0x5c | **param 1 (default 0.2)** | 0.2 | 0.2 | param 3 if ≥ 0, else 0.2 | — |

`SET_STROKE_PARAMETERS` (0x100201) / `GET_STROKE_PARAMETERS` (0x100200) carry the per-style float vector:
FountainV2 `[pressureSensitivity, smoothLevel]`, Square `[shape, ratio, angle, smoothLevel]`, Charcoal
`[tiltEnabled, tiltScale]`. Points go to libneo as `{x, y, pressure, size = −1, 0, 0, altitude, azimuth,
t = 0}` (tilt converted by `neo::MathUtils::AndroidTilt::computeAltitudeAndAzimuth`): `onPenDown` at
moveTo, `onPenMove` per quadTo, `onPenUp` with the last point repeated at finish. The resulting inks go to
`PenSkRender` (polygons for outline pens, dabs for textured pens) into the handwrite buffer.

**PainterBase (style 0)** does not use libneo: a Skia path, `quadTo(mid(prev, cur), cur)` per point — i.e.
straight segments between SYN points — stroked with width **W** (no pressure), drawn after every point;
the path restarts every 5 segments. Paint style, cap, join and antialias come from `SET_PAINTER_STYLE`
(not decoded yet).

**Marker**: ink is drawn into an offscreen bitmap; for a colour that is neither black nor white (and a
global flag set) the colour is pre-tinted by drawing it at alpha 0.75 with blend mode 16 (darken).
**Charcoal**: also through an offscreen bitmap.

## 4. What knote2 does so its settled ink matches

knote2 reads SF's own per-style parameters back (`Device.getStrokeParameters(style)`) and uses them for its
final libneo render: `NoteRenderUtils` takes the pressure sensitivity from `params[0]` and the smoothing
level for the normal/square brush from them, and writes changed values back with `setStrokeParameters`
(square pen: `params[1] = min(width, 10)`, `params[2] = ±45`). The width it sends is
`ShapeUtils.getRenderStrokeWidth(width, type, ctx)`. So in knote2 both renders are libneo with the same
config; the only remaining differences are the input points (evdev vs MotionEvent) and the region gate.

## 5. Why openink/Atlas ink changes when it settles (causes, biggest first)

1. **Different config.** openink's settled fountain uses `PenConfig::knote2_default` (width W, smoothLevel
   0.6, dpi 300 for the speed term, minWidth 0.001) while SF draws `W + 3`, smoothLevel 0.2, dpi 320,
   minWidth 1. The width alone differs by 3 px.
2. **Ballpoint/pencil**: SF draws a constant-width polyline through the raw points (style 0); the settled
   ballpoint is libneo-style smoothed curves, the pencil a textured dab stroke.
3. **Region gate** (missing pieces while writing, see §1).
4. Smoothing/spline: openink-stroke's EMA + Catmull-Rom approximates libneo's `StrokePointProducer` /
   `SplineProducer`; small shape differences remain until those are decoded.

## 6. The fix (openink)

- `openink-stroke::sf` describes SF's live pen per style (`SfStyle`, the configs above, the width rules and
  their inverses) so a caller can (a) set SF's style, width and parameters for the pen it will render, and
  (b) render the settled stroke with exactly the config SF used.
- `PenConfig` gains `dpi` and `min_width` (the speed term used 300 dpi as a constant).
- The region gate is mirrored as a pure function so an app can tell which of its samples SF drew.

Open: decode libneo's smoothing and spline stages for an exact match; PainterBase's paint (cap/join/AA);
PenSkRender's fill; the Charcoal Impl's dpi; Dash and SoftEraser.
