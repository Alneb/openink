# Refactor of `impl/` (October 2026): smaller modules, one copy of each helper

A behaviour-preserving cleanup of the Rust crates and the `onyx-compat` Java layer. No API, wire format, weight
file or tuning constant changed. This file records what moved, why, and how "behaviour-preserving" was checked.

## What changed

### `openink-shape`: the recogniser split by shape family
`src/recognize.rs` (770 lines: scoring, polygons, conics and arrows in one file) became a module directory:

| file | holds |
|---|---|
| `recognize/mod.rs` | `Recognizer`, the cost model (`scored`, reject cost), `Ctx` / `Cand`, the family dispatch, helpers every family uses (`snap_line`, `lines`, `centroid`), the shared tolerances (`ANGLE_TOL`, `AXIS_SNAP`, `SPECIAL_BONUS`) |
| `recognize/polygon.rs` | side merging, polylines, polygons (chamfers, gentle corners), triangle and quadrilateral specialisation and idealisation; `RIGHT_TOL`, `RIGHT_PAR_TOL`, `LEN_TOL`, `CHAMFER`, `GENTLE_TURN` |
| `recognize/conic.rs` | ellipse / circle and arc candidates; `CIRCLE_RATIO`, `ARC_CIRCLE_BONUS` |
| `recognize/arrow.rs` | shafts, open (barb) and closed (triangle) heads; the head constants |

The public paths are unchanged (`openink_shape::recognize::{Recognizer, RESIDUAL_WEIGHT, REJECT_MARGIN, step_for}`),
so Atlas's vendored copy needs no code change, only a re-sync. Each unit test moved next to the code it tests.

Inside `arrow.rs`, `closed_head` (a head drawn as its own stroke) and `is_triangle_head` (a head drawn without lifting
the pen) both repeated the same test: small next to the shaft, going round most of its hull, hull close to a
triangle. That test is now one function, `small_closed_triangle`, returning the hull and the box diagonal each
caller needs. In `polygon.rs`, the equilateral-triangle idealisation now calls `orient()` instead of an inline copy.

The two Atlas regression test files (`tests/atlas_boxes.rs`, `tests/atlas_more.rs`) share Atlas's `wobble` generator
from `tests/common/mod.rs`, so the generator that must match Atlas's exists once.

### `openink-stroke`
- `Ink` lost three one-line wrappers (`empty_fill`, `empty`, `grow_by`); `Ink::new` and `Ink::grow` are
  `pub(crate)` and called directly by `brush.rs` and `marker.rs`.
- The uniform Catmull-Rom basis was written out twice (`catmull_rom` for positions, `cr1` for the width / alpha
  channels). `catmull_rom` now runs `cr1` on each channel. Its doc comment also lost a stale first line ("the chord
  as the end tangent", which described an older spline).
- `ActiveStroke::moved`: the brush and marker arms repeated the same "each finite sample" loop; now `each_finite`.

### `openink-jni`
New private module `jvm.rs`: `read_floats`, `read_ints`, `write_floats`, `write_ints`, `throw` and the exception class
names. Before, every entry-point file had its own copies: three `throw` functions, the float/int array reading in
`hwr.rs`, `shape.rs` and `stroke.rs`, and the array writing in four places. The two recognisers' identical "read
`xyt` + `lens`, split into strokes, else IllegalArgumentException" block is `shape::read_ink`, used by `hwr.rs` too.

### `onyx-compat` (Java)
`TouchHelper.sfQuiet` was a character-for-character copy of `RawInputReader.sf` (same package, same log tag).
`RawInputReader.sf` is package-private now and `TouchHelper` calls it, as it already did for `RawInputReader.copy`.

## How it was checked
1. `impl/check.sh` (all five stages: Rust tests, clippy `-D warnings`, the Java host checks, the Android cross-build
   + demo APK, the onyx-compat / demo checks) passed before and after, with the same 115 Rust tests.
2. A bit-exact fingerprint, run before and after: every pen of `openink-stroke` rendering one 400-sample
   pressure/tilt stroke both whole (`Stroke::render`) and incrementally (`down`, `moved` in chunks of 3, `up`),
   every output float compared by bit pattern; and `Recognizer::builtin()` on 1500 synthetic examples
   (`synth::example`, seed 12345), the full `Recognition` compared.
   - **Shape: identical** on all 1500.
   - **Stroke: identical for the five outline pens.** The pencil and charcoal dabs differ by at most 2·10⁻⁶ px
     (one float rounding step, same dab count and order): the old `cr1` evaluated `x·t·t` where the position
     spline evaluated `x·(t·t)`, and with one basis both now use the latter. That is far below the 0.6–2.5 %
     tolerance the pens are held to against libneo on the device (STROKE.md).

## What to watch out for
- Atlas vendors `openink-shape` and `openink-stroke` by copying `src/` whole (`scripts/dev/sync-openink.sh`:
  `rm -rf` then `cp -r`), so a re-sync replaces `src/recognize.rs` with `src/recognize/` cleanly. Its `--check` mode
  will report the vendored copy as different until that re-sync.
- The `include_bytes!` path of the shipped weights moved one level (`../../weights/shape.oisw`), as `mod.rs` sits one
  folder deeper.
