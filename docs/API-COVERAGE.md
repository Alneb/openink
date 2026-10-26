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
  Point filtering follows SF's `PenManager::inValidRegion` (every point in a limit rect, latched to the
  starting rect in single-region mode, never in an exclude rect). Pen-down/up also feed the refresh
  engine, which holds explicit refreshes while the pen writes.
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
