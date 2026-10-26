# onyx-compat — the Onyx SDK pen API over openink


`impl/java/onyx-compat` (package `org.openink.compat.pen`) mirrors the part of Onyx's
`com.onyx.android.sdk.pen` API that drawing apps use, built only on `openink`. An app written
against the Onyx SDK ports by **changing imports**: same class names, same method names and
argument types, same callback order. It needs no Onyx library in the app: pen input comes from our
evdev reader (`libopenink_jni.so`), not from `libonyx_pen_touch_reader.so`.

## Where the API shape came from
Read from knote2's decompiled SDK (`firmware/onyx/pen/knote2_src/sources/com/onyx/android/sdk/`,
through `docs/scripts/safecat.sh`):
- `pen/RawInputCallback.java` — 8 abstract callbacks (`onBeginRawDrawing(boolean, TouchPoint)`,
  `onEndRawDrawing`, `onRawDrawingTouchPointMoveReceived`, `onRawDrawingTouchPointListReceived`,
  and the four `...Erasing...` twins) plus two optional ones, `onPenActive(TouchPoint)` and
  `onPenUpRefresh(RectF)`. It imports `data.note.TouchPoint` and `pen.data.TouchPointList`.
- `pen/TouchHelper.java` — `create(View, RawInputCallback)`, `openRawDrawing`, `closeRawDrawing`,
  `setLimitRect` (3 overloads), `setExcludeRect`, `setStrokeWidth/Color/Style`,
  `setRawDrawingEnabled`, `setRawDrawingRenderEnabled`, `setRawInputReaderEnable`,
  `enableSideBtnErase`, `setPenUpRefreshEnabled`, `setPenUpRefreshTimeMs`, `isRawDrawing*`,
  `onTouchEvent(MotionEvent)`, `getHostView`, ...
- `base/data/TouchPoint.java` — public fields `x, y, pressure, size, tiltX, tiltY, timestamp`
  (what `data.note.TouchPoint` extends).

## What is implemented
| Onyx | openink-compat behaviour |
|---|---|
| `TouchHelper.create(view, cb)` | holds the view + callback; nothing starts yet |
| `openRawDrawing()` | starts `PenReader`; builds a `MapCalibrator` (raw → screen, see below); installs a **non-consuming** touch listener for calibration; opens instant ink after layout. Failures are **logged, not thrown** (tag `openink-compat`), as the Onyx SDK does |
| `closeRawDrawing()` | stops the reader (joins the pen thread), removes listeners, closes the SF session |
| `setLimitRect(...)`, `setExcludeRect(...)` | view-relative rects. Strokes may only **start** inside a limit rect (or anywhere if none) and outside every exclude rect. SF takes the limit at open, so changing it reopens instant ink |
| `setStrokeWidth/Color/Style` | stored, and pushed to SF when the session is open |
| `setRawDrawingRenderEnabled(false/true)` | SF pause / `post()` + resume: the app redraws while off, and its picture is shown over the quick ink when back on |
| `setRawInputReaderEnable(b)` | new strokes are ignored while off (a stroke in progress finishes) |
| `setRawDrawingEnabled(b)` | both of the above |
| `enableSideBtnErase(true)` | a stroke begun with the barrel button held is an **erasing** stroke; the eraser tool end always erases |
| `setPenUpRefreshEnabled/TimeMs` | after the last pen-up, `onPenUpRefresh(dirty)` on the **main thread** after the delay (default **500 ms**, knote2's); a new pen-down cancels it; `dirty` is the union of the strokes since the last refresh, padded by the stroke width |
| `onTouchEvent(e)` | feeds stylus DOWN/UP to the calibrator; returns false. Call it yourself if your app replaces the host view's touch listener |
| `getMaxPressure()` | extra: `TouchPoint.pressure` is in **raw digitizer units** (0..pressureMax, 4095 on this device), as Onyx reports it |

`TouchPoint` coordinates are **host-view pixels**; `timestamp` is milliseconds (from the evdev
event time). `onPenActive` fires on hover-enter.

**Threads.** All `RawInputCallback` methods except `onPenUpRefresh` run on the `openink-pen` reader
thread, so a callback must not touch views directly. Exceptions thrown by a callback are caught and
logged (the JNI layer only prints them to stderr, which Android drops) so one bad callback cannot
kill pen input.

## Shared calibration: `org.openink.pen.MapCalibrator`
The raw → screen map used to live inside the demo's `InkCanvas`. It moved into the library so the
demo and `TouchHelper` share one implementation: each contact is seen by evdev (raw) and by Android
as a stylus MotionEvent (screen px); DOWN/UP pairs within 250 ms are refitted by least squares
(`ScreenMap.fit`), a fit with rms > 25 px drops its newest pair, and 8 pairs under 1 px rms lock the
map. It starts from the `ScreenMap.rotated` guess, which on this device already matches the IDC's
ORIENTATION_270. `screenContact` returns true when a refit moved the page by more than 0.5 px,
which is when a drawing app should redraw.

Moving it surfaced a demo bug: every resume rebuilt the map from the guess but kept the "locked"
flag, so after a pause/resume a locked map stayed on the guess forever. The demo now keeps its
calibrator across resumes.

## Not mirrored (yet)
- `create(View, int, RawInputCallback)`, `create(View, boolean, RawInputCallback)` and
  `create(View, int, RawInputCallback, boolean)`: the extra flags' meaning is not known, and
  guessing would silently change behaviour.
- The event bus (`register/unregister`, `getEventBusHolder`): Onyx EventBus types, not app API.
  (Finger-touch modes, multi-region mode, `setPostInputEvent`, `setFilterRepeatMovePoint` and the whole
  `EpdController` are implemented now; the full list is in [API-COVERAGE.md](API-COVERAGE.md).)
- The leading boolean of `onBegin/onEnd...` is always `false` (its Onyx meaning is not known).

## Verification status
Compiles with `javac -Xlint:all -Werror` against android.jar as part of the demo build
(`impl/java/demo/build-demo.sh`, so `impl/check.sh` stage 4). **Not yet run on the device** (none
was attached yet): the first on-device test should port a small Onyx-SDK sample and
check the callback order against stock `TouchHelper`.
