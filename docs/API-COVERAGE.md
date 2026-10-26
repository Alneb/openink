# Onyx SDK API coverage — what onyx-compat reimplements

Goal: *"reimplement 100% of the functions in this whole stack"*. This file
is the checklist for the app-facing layer (L1): every public method name of the Onyx SDK classes
apps call, from knote2's bundled SDK (`firmware/onyx/pen/knote2_src`),
compared by name with `impl/java/onyx-compat`. Regenerate the table with
`python3 -I docs/scripts/sdk_coverage.py`. Names only: a covered name still needs its behavior checked on the device.

Scope rule: instant-ink and display calls go through the system's own `android.onyx.ViewUpdateHelper`
(`OnyxFrameworkBackend`, with the app's whitelist entry) and the public `View.refreshScreen`. The
whole display integration is built on that whitelisted API; a direct SurfaceFlinger Binder client is
out of scope and not needed (the sanctioned path already drives instant ink on a stock device, and a
flashed device uses our own compositor).

| class | covered | missing |
|---|---|---|
| TouchHelper | 37/40 | `getEventBusHolder`, `register`, `unregister` |
| TouchPointList | 22/22 | — |
| RawInputCallback | 10/10 | — |
| EpdPenManager | 9/9 | — |
| RawInputReader | 26/30 | `getEventBusHolder`, `nativeRawReader`, `onNext`, `run` |
| EpdController | 100/100 | — |

**204/211 public names.** The 7 left are not app API: Onyx's internal event bus
(`register`, `unregister`, `getEventBusHolder`: Onyx EventBus types) and the native reader's internals
(`run`, `onNext`, `nativeRawReader`).

Notes:
- `RawInputReader`'s single-letter names (`J`, `Q`, `i`, `k`, `r`, `w`) are obfuscated internals, not API.
- `TouchPointList.toTinyPointList` / `detachPointList` return knote2's storage type, now ported as
  `TinyPoint` (tilt packed into `size`); `pointsTo/FromByteArray` is knote2's 24 B/point big-endian blob
  (host check `TouchPointListCheck`).
- **RawInputReader is now the pen core**: TouchHelper delegates input to it, as Onyx's does.
  Point filtering follows the native reader's `PenManager::inValidRegion` (libonyx_pen_touch_reader):
  every point, padded by half the stroke width, fits a limit rect (in single-region mode the first one
  the contact hit) and touches no exclude rect. Pen-down/up also feed the refresh engine, which holds
  explicit refreshes while the pen writes.
- **One contact can make several strokes** (`RegionGate`, from the native `BaseReader::onEvent`): leaving
  the valid region ends the stroke (native action 3, `onEnd…(outLimitRegion = true, …)`, the outside
  point is not added to the list), re-entering starts a new one, and a pen that lands outside starts
  its stroke where it enters. A pen-up while outside reports nothing more. Host check `RegionGateCheck`.
- **The callbacks' leading booleans** (named in the framework's copy of the reader and in knote2's
  Kotlin metadata): `onBeginRawDrawing(shortcutDrawing)` is always `false`, because the native reader
  hard-codes that slot (`TouchConsumer::onTouchEvent` pushes a constant `false`).
  `onBeginRawErasing(shortcutErasing)` is `true` when the side (barrel) button made the pen erase
  (native reader 3, `SideEraseReader`), `false` for the eraser end. `onEnd…(outLimitRegion)` is the
  action-3 case above.
- **`TouchHelper.create(View, int renderSet, cb[, boolean touchListenerEnabled])` and
  `create(View, boolean, cb)`** (true = set 2, false = set 1). Render-set bits: 2 `SFTouchRender` (the
  native reader plus SurfaceFlinger ink), 1 `AppTouchRender` and 4 `AppPenTouchRender` (the host view's
  MotionEvents, with ink pushed through `EpdController.moveTo/quadTo`). The SDK's default is 2 when the
  device has a stylus. openink: with bit 2, the evdev reader; without it, the view's stylus MotionEvents
  (batched history included) through the same region gate. Instant ink comes from `InkSession` either
  way, since the SDK's app renders differ only in which ink they draw themselves.
- `TouchHelper.setFilterRepeatMovePoint` forwards to `setPenUpRefreshEnabled` in the SDK (an Onyx bug);
  mirrored on purpose, since apps already live with it.
- **EpdController: 100/100**, `org.openink.compat.epd`: refreshes through the open engine
  (EPDC-ENGINE.md), the rest through `OnyxFramework` to the exact framework method SDMDevice reaches
  (`docs/scripts/sdk_bindings.py`). The SDK's `UpdateMode` / `UpdateOption` / `UpdateScheme` are ported with their
  wire ints. Names are covered; behaviour is still to be checked on the device.
- EpdController (100 names) was the big block: refresh/update modes, A2/regal/turbo, colour
  (gamma, saturation, filter, night mode, dither), CTP/EMTP power and disable regions, and
  handwriting (`setScreenHandWriting*`, stroke style/width/colour, `startStroke`/`lineTo`/`penUp`).
  Each maps onto a `ViewUpdateHelper` call already listed in `impl/spec/sf_protocol.json`.

## Plan (order)
1. ~~TouchHelper's remaining 21~~, ~~EpdPenManager~~, ~~RawInputReader~~, ~~EpdController~~.
2. Device checks of the behaviour behind each name, starting with the refresh path through the engine.
