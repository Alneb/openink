# Onyx e-ink (EPD) and pen stack — BOOX Note Air 5C, firmware 4.2.1

How the Onyx-specific display refresh and stylus paths are wired, reconstructed from the shipped
firmware (static analysis only: jadx 1.5.1 on `framework.jar`, `readelf`/`nm` on the native libs).

## Where the files came from
Extracted with `7z e -y -o<dir> <img> <path>` (note: `-o` takes the dir **attached**, no space —
`-o dir` fails with "Too short switch") into `firmware/onyx/` (gitignored):

| file | source image / path | role |
|---|---|---|
| `epd/framework.jar` (47 MB) | system `system/framework/` | Android framework + Onyx `android.onyx.*` packages |
| `epd/services.jar` (22 MB) | system `system/framework/` | system_server (not yet decompiled) |
| `epd/libonyx_epd_listener.so` | system `system/lib64/` | JNI reader for the SurfaceFlinger → app event FIFO |
| `epd/libonyx_neo_dither.so` | system `system/lib64/` | JNI bitmap dithering (`android.onyx.utils.DitherUtils`) |
| `epd/init.onyx.rc` | vendor `etc/init/hw/` | device nodes / sysfs permissions / Onyx services |
| `pen/knote2-release.apk` | system `system/app/knote2-release/` | Notes app |
| `pen/libonyx_pen_touch_reader.so` | system `system/lib64/` | native raw stylus reader |
| `pen/tscalibration-release.apk` | system `system/app/` | touch/pen calibration app |
| `pen/wacom_flash` | vendor `bin/` | Wacom digitizer firmware flasher |

Decompiled source: `firmware/onyx/epd/framework_src/sources/` (7578 classes, 33 jadx errors — normal).

## Architecture in one picture
```
 app / Onyx SDK                       framework.jar (android.onyx.*)             SurfaceFlinger (Onyx-modified)
 ───────────────                      ─────────────────────────────              ──────────────────────────────
 View / knote2  ──► ViewUpdateHelper.refreshScreen / moveTo / quadTo ──Binder──►  custom transact codes
                     (Parcel w/ token "android.ui.ISurfaceComposer",             (0xFF00xx, 0x1000xx/0x1001xx)
                      ServiceManager.getService("SurfaceFlinger"))                    │
                                                                                     ▼
                                                                              /dev/ebc (e-ink controller)
 EpdEventListener.Callback  ◄── libonyx_epd_listener.so ◄── FIFO /dev/onyx/listener ◄─┘ (epdc update,
                                 (readEpdEventLoop)                                    activity state, input)

 stylus: /dev/input/event%d ──► libonyx_pen_touch_reader.so (PenManager + readers) ──► Java RawInputReader
         (bypasses the Android input pipeline for latency)                            ──► ViewUpdateHelper.quadTo …
```
**Key finding:** e-ink refresh scheduling *and* low-latency pen ink are implemented inside Onyx's
SurfaceFlinger, driven by ~120 private Binder transaction codes. The Java side is a thin Parcel
marshaller (`ViewUpdateHelper.transactData`). The next layer to reverse is therefore
`system/bin/surfaceflinger` (+ whatever it links for the EPDC), not the apps.

## `init.onyx.rc` (vendor) — hardware surface
- `on boot`: creates `/dev/onyx/` and the FIFO `/dev/onyx/listener` (mode 666) — the SF→app channel.
- sysfs switches made world-writable (0666): `/sys/onyx_misc/{captp_disable, stylus_disable,
  key_control, hall_ctl, wacom_pm, onyx_keyboard_wakeup, kb_touchpad_enable}`,
  `/sys/onyx_misc/onyx_active_pen/{usi_enable, usi_haptic_type, usi_haptic_strength}` (USI pen
  support + haptics), front-light `/sys/class/backlight/{warm,white,onyx_bl_br,onyx_bl_ct}/brightness`,
  battery `charge_control_{start,end}_threshold`, `rtc0/wakealarm`; read-only PCB / sound-config version.
- `on post-fs-data`: `/sys/wacom_ts/{caldata,rawdata}` 0666 (digitizer calibration).
- `on fs`: `/onyxconfig` partition (ext4, `by-name/onyxconfig`) + `/onyxconfig/mmkv` (MMKV
  key-value store — matches `libmmkv-onyx.so`, `BaseMMKV`/`SystemMMKV` classes), `init.onyxconfig.sh`.
- `on charger`: `chmod 0666 /dev/ebc` — **the e-ink controller device node is `/dev/ebc`**.
- Property hooks: `vendor.onyx.{blue,red}_led`, `vendor.onyx_dump_enable` (toggles Qualcomm
  `download_mode`), `vendor.onyx.run_diag_mdlog`.
- Services: factory audio/mic tests (`mm-audio-ftm`, `sndTinycap.sh`), `TouchMisc.sh`,
  `init.onyx.sh`, `tpCalibrate.sh`, RPMB provisioning (`init.rpmb.sh 0/1/2`), keybox
  install/erase, WLAN FTM, diag mdlog.
- Also `rm /data/local/assets/update.zip` on `sys.boot_completed=1` (OTA staging cleanup).

## `android.onyx.ViewUpdateHelper` — the refresh API
`android/onyx/ViewUpdateHelper.java` (1551 lines). Every call builds a Parcel with interface token
`android.ui.ISurfaceComposer` and `transact()`s it to the `SurfaceFlinger` service.

### Update-mode bit layout (`EINK_*`)
| bits | meaning |
|---|---|
| `0x0F` waveform | INIT 0, DU 1, GC16 2, GC4 3, ANIM(A2) 4, AUTO 5, REAGL 6, DU4 8, REAGL_PLUS 9, GCC16 11, DEEP_GC16 12 |
| `0x10` | auto mode (automatic vs regional) |
| `0x20` | full (vs partial) update |
| `0x40` | wait for completion |
| `0x80` | combine updates |
| `0x100` | dither; `0x800` dither color Y1 vs Y4 |
| `0x200` | invert |
| `0x400` | convert |
| `0x1000` | REAGL-D |
| `0x80000` | `EPDC_FLAG_HANDWRITE_GU` |
| `0x500000` | shutdown flag |
| `0x1000000` | "X" / Onyx auto (dither-X) |
| `0x2000000` | apply mono / Onyx GC |

Named UI modes are compositions: `UI_GC_MODE=98` (0x62 = GC16|FULL|WAIT), `UI_DEFAULT_MODE=5`
(AUTO), `UI_DU_MODE=1`, `UI_GU_MODE=2`, `UI_A2_PERFORMANCE_MODE=4`, `UI_A2_QUALITY_MODE=2308`
(0x904), `UI_DU_QUALITY_MODE=2305`, `UI_DU4_MODE=2312`, `UI_REGAL_MODE=6`, `UI_REGAL_PLUS_MODE=9`,
`UI_GCC_MODE=107`, `UI_DEEP_GC_MODE=108`, `UI_X_A2_MODE`/`UI_X_DU_MODE` (+0x1000000),
`UI_MONO_A2_MODE` (+0x2000000), `UI_GC_SHUTDOWN_MODE`/`UI_REGAL_SHUTDOWN_MODE` (+0x500000),
`HAND_WRITING_REPAINT_MODE=524290` (0x80002). Display schemes: NORMAL 1, KEYBOARD 2,
SCRIBBLE 3, APPLICATION_ANIMATION 4, SYSTEM_ANIMATION 5.

### Private SurfaceFlinger transaction codes
Range A `0xFF0001`+ (16711681…) — core EPD + pen drawing:
REFRESH_SCREEN 0xFF0001, SHOW_SCREEN, APPLY_APP_SCOPE_UPDATE, SEND_UPDATE_INFO,
SET_STROKE_{COLOR,WIDTH,STYLE}, SET_PAINTER_STYLE, **MOVE_TO 0xFF000A, LINE_TO 0xFF000B**,
ENABLE_POST, SET_SCREEN_HANDWRITING_{PEN_STATE,REGION_LIMIT,REGION_EXCLUDE}, APPLY_GAMMA_CORRECTION,
**QUAD_TO 0xFF0010**, START_STROKE/ADD_STROKE_POINT/FINISH_STROKE, REPAINT_EVERY_THING(_WITH_MODE),
SET_APPLICATION_DISPLAY_SCHEME, SCREENSHOT, WAIT_FOR_UPDATE_FINISHED, ENABLE_BIG_PEN,
ENABLE_ONYX_TPD, GET_TOUCH_{WIDTH,HEIGHT}, SUPPORT_REGAL, STOP_SYSTEM, SHOW_IMAGE,
CHANGE_ORIENTATION, GET_EPD_{WIDTH,HEIGHT}, SET_TRIGGER, SET_UPD_LIST_SIZE, APP_DIE, APPLY_GC_ONCE,
**SET_VCOM 0xFF0028**, ENABLE_REGAL, MAP_{EPD_TO_VIEW,VIEW_TO_EPD,FROM_RAW_TOUCH_POINT,TO_RAW_TOUCH_POINT},
APPLY_{SATURATION,BRIGHTNESS,MONO_LEVEL}, SET_GRAYSCALE_MODE, DEBOUNCER, DEBOUNCE_INC_REFRESH,
APPLY/CLEAR_TRANSIENT_UPDATE, PEN_UP 0xFF0068, SET_DEBOUNCER_TRANSIENT_UPDATE_MODE,
BYPASS_UNTIL_ROTATE_LAYER_OFF.

Range B `0x100029`+ (1048617…) — newer additions: MERGE_DISPLAY_UPDATE(_BY_COUNT),
GET_MAX_TOUCH_PRESSURE, SET_TRIGGER_BY_PEN_RECT, SET_SCREEN_HANDWRITING_REGION_MODE, BYPASS,
COLOR_FILTER, RUN_SH 0x10002F, CLEAR_APP_SCOPE_UPDATE, IS_PEN_STATE_VALID, APPLY_SF_DEBUG,
GET_PEN_STATE, CALIBRATION_RESET, COLLECT_PEN_{MOVE,UP}, HANDWRITING_REPAINT,
MARK/IS_OEC_SERVICE_READY, IS_IN_FAST_MODE, USE_GC_FOR_NEW_SURFACE, SET_GC_REFRESH_INTERVAL,
GET_COLOR_TYPE, FILL_WHITE_ON_WAKEUP, SET/GET_EPD_TURBO, BYPASS_UNTIL_LAYER_{ON,OFF}, WHITE_CLEAR,
TRANSFER_EPDC, SCREENSHOT_GRANT_PERMISSION, SET_ANTI_FLICKER, ENABLE_DITHER, ENABLE_COLOR_CU,
APPLY_SATURATION_{MIN_VALUE,FACTOR}, TOGGLE_DREAM, ENABLE_BW_MODE, SET_ENHANCE_STRATEGY,
SET_DITHER_THRESHOLD, ENABLE_ENHANCE, SET_CAPTURE_EXCLUDE_LAYERS, POKE_USER_ACTIVITY,
APPLY_COLOR_PARAMETER, SHUTDOWN, GET_NIGHT_MODE_SUPPORT, SET_ENABLE_NIGHT_MODE, APPLY_NOISE_STRENGTH,
APPLY_DITHER_FILTER_TOLERANCE, ENABLE_COLOR_ADJUST, SET_AUTO_SYNC_BUF_ENABLE (0x1000xx);
SET_ENABLE_PEN_SIDE_BUTTON, SET_ERASER/BRUSH_RAW_DRAWING_ENABLED (0x1001xx), GET/SET_STROKE_PARAMETERS
(0x1002xx), GET_EPD_TO_VIEW_MATRIX (0x1003xx), SAVE_PEN_ATTACHED_FB (0x1004xx).

Open question for the SurfaceFlinger pass: which of these codes are permission-checked on the SF
side (e.g. `RUN_SH`, `SET_VCOM`, `SHUTDOWN`, `STOP_SYSTEM`) — the Java helper does none.

## SF → app event channel
`android.onyx.optimization.EpdEventListener` starts `libonyx_epd_listener.so`
(`nativeStart(Constant.PIPE_FILE_PATH)` → `EpdListener::readEpdEventLoop`), which reads the FIFO
`/dev/onyx/listener` and calls back `onEpdEvent(String)`; lines are parsed into `PipeMessage`s and
dispatched as `epdcUpdate`, `nativeSurfaceUpdate`, `onActivityStateChanged`, input events and
`onEACScreenNoteStateUpdate`. The EAC* classes (`EACScrollRefreshManager`, `EACAppConfig` v1/v2,
`TabletEACRefreshImpl`, `EpdcUpdateDebounceWithDelay`) are the per-app "E-ink App Center"
optimisation layer built on top.

`libonyx_neo_dither.so`: `DitherUtils.dither/ditherColor` → `imgfilter::ImageFilter`,
`ImageColorFilter`, `ImageFilterRGB565` in-place filters on Android bitmaps (`libjnigraphics`).

## Pen: `libonyx_pen_touch_reader.so`
JNI for `com.onyx.android.sdk.pen.RawInputReader` (Onyx SDK, bundled in apps like knote2; the
framework has its own `android.onyx.inputreader.RawInputReader`):
`nativeRawReader / nativeRawClose / nativeIsValid / nativeDebug / nativePausePen / nativeSetPenState /
nativeSetStrokeWidth / nativeSetLimitRegion / nativeSetExcludeRegion / nativeSetRegionMode /
nativeEnableSideBtnErase`.

Internals (C++ symbols):
- `TouchReader` — `findDevice` / `openDevice("/dev/input/event%d")`, `readTouchEventLoop`, an
  `eventfd` for wakeups (`notifyFd`). Reads evdev directly, **bypassing InputFlinger** for latency.
- `PenManager` — state machine fed by `onAbsEvent/onKeyEvent/onSyncEvent`; `normalizePressure`,
  `setMaxPressure`, limit/exclude region tests (`inLimitRegion`, `inExcludeRegion`, `quadOutRegion`),
  smoothing (`moveTo`/`quadTo`/`finishQuadTo`), side-button / rubber detection, `report` to Java.
- Readers (strategy objects swapped by `changeReader`): `DrawReader`, `EraseReader`,
  `SideEraseReader`, `BtnReader`, all derived from `BaseReader` (`isPenUp`, `isRubberUp`,
  `isBrushDown`, `isBtnUp`).
- `TouchConsumer` + `EventQueue` — consumer thread decoupling read and report.

So the ink path is: evdev → PenManager (filter/smooth) → Java callback → `ViewUpdateHelper.quadTo`
→ SurfaceFlinger draws straight into the EPD framebuffer in handwriting-GU mode, and the app
re-renders properly later (`HANDWRITING_REPAINT`).

## Pen SDK as shipped in knote2 (`firmware/onyx/pen/knote2_src/`, 21596 classes, 43 jadx errors)
knote2 bundles the Onyx SDK (`com.onyx.android.sdk.*`); its native libs are **not** in the APK
(`lib/` empty) — they come from `system/lib64` (`libonyx_pen_touch_reader.so`, `libneo_pen.so`).

- **`api.device.epd.EpdController`** → `Device.currentDevice()` → per-SoC backend classes
  `RK31XXDevice`, `RK32XXDevice`, `RK33XXDevice`, `IMX7Device`, **`SDMDevice`** (Snapdragon — this
  device). `SDMDevice` resolves ~230 `android.onyx.ViewUpdateHelper` methods/constants **by
  reflection** (`ReflectUtil.classForName("android.onyx.ViewUpdateHelper")`, `getMethodSafely`,
  `getStaticIntFieldSafely("EINK_ONYX_AUTO_MASK")` …), so the app works across framework versions.
  Stroke styles: PENCIL 0, BRUSH 1, MARKER 2, NEO_BRUSH 3, CHARCOAL 4.
- **`EpdPenManager`** — thin: `moveTo/quadTo/setStrokeColor/setStrokeStyle` → EpdController;
  `setScreenHandWritingPenState(view, n)` with **0 = quit, 1 = start drawing, 2 = resume, 3 = pause**.
- **`RawInputReader`** (Java side of `libonyx_pen_touch_reader.so`) — native calls back
  `onTouchPointReceived(x, y, pressure, size, ?, erasing, shortcutDrawing, shortcutErasing, action,
  timestamp)` (JNI `(FFIIIZZZIJ)V`); action codes: 0 down, 1 move, 2 up, 3 the pen left the valid
  region (the stroke ends there, `PenManager::quadOutRegion`; re-entering starts a new stroke), 5
  hover/active, 6 forced release (`pausePen`, "EpdShapeHandler pen force release"). `shortcutDrawing`
  is always false (the native queue pushes a constant); `shortcutErasing` marks side-button erasing
  (reader 3, `SideEraseReader`). Details and the port: [API-COVERAGE.md](API-COVERAGE.md).
  Delivered to `RawInputCallback`: `onBeginRawDrawing/Erasing`, `onRawDrawingTouchPointMoveReceived`, `onRawDrawingTouchPointListReceived`, `onEndRawDrawing`,
  `onPenActive`, `onPenUpRefresh(RectF)`. `TouchHelper` / `RawInputManager` are the public facade.
- **`NeoPenNative`** → `libneo_pen.so` (3.8 MB, internals hidden; exports only JNI + spdlog/fmt):
  `nativeCreatePen(penType, NeoPenConfig)`, `nativeOnPenDown/Move(point, prediction)/Up` →
  `NeoPenResult`, `nativeSetBitmapColor`, `nativeDestroyPen`. Pen types: BRUSH 1, FOUNTAIN 2,
  MARKER 3, CHARCOAL 4, PENCIL 7, BALLPOINT 8, SQUARE 9; brush shapes circle/ellipse/rectangle;
  config has tilt/direction, velocity bounds/amplifier, `fastMode`. Note `nativeOnPenMove` takes a
  **prediction** array — point prediction is part of the latency strategy.
  Java renderers per pen: `NeoFountainPen(V2)`, `NeoMarkerPen(V2)`, `NeoCharcoalPen(V2)`,
  `NeoPencilPen`, `NeoBallpointPen`, `NeoBrushPen`, `NeoSquarePen` + `*Wrapper`s.

So knote2 runs **two** ink layers: the instant SF-side stroke (EpdController.quadTo, crude, in
handwriting-GU waveform) and the high-quality app-side render (libneo_pen into a bitmap) that
replaces it on pen-up (`onPenUpRefresh`).

## Instant-ink protocol without the SDK (traced for peer "Atlas", 2026-10-16)
- All targets are **static** `android.onyx.ViewUpdateHelper` methods (SDMDevice passes a view as
  the invoke receiver, ignored): `setScreenHandWritingPenState(int)` (+ sends `Process.myPid()`),
  `setScreenHandWritingRegionLimit/Exclude(View, int[] flat l,t,r,b)` (view-relative, location
  added; null view = screen), `setScreenHandWritingRegionMode(int)` **0 multi / 1 single** (corrected in from SF's `PenManager::inValidRegion`; see SF-ONYX.md),
  `setStrokeWidth/Color/Style`, `setPainterStyle`, `setBrushRawDrawingEnabled(bool)`,
  `setEraserRawDrawingEnabled(bool,int)` (TouchHelper defaults true / (false,5)),
  `setEnablePenSideButton`, `moveTo/lineTo/quadTo(View, x, y, width|mode[, pressure])`, `penUp()`,
  `enablePost(int)`, `handwritingRepaint(View,l,t,r,b[,bool])`.
- **SurfaceFlinger draws the instant ink itself from evdev** once pen state = 1 inside the limit
  region: the SDK's `SFTouchRender` never calls quadTo per point. open = `setStrokeStyle(0)`,
  RawInputReader start, pen state 1, region limit, stroke width; render on/off = state 2/3; close = 0.
- Pen-up: `RawInputReader` fires `onPenUpRefresh(RectF)` after `penUpRefreshTimeMs` (default
  **500 ms**). Onyx's `ScribbleTestActivity`: draw bitmap into the SurfaceView canvas →
  `enablePost(1)` → `unlockCanvasAndPost` (app layer replaces SF ink). Programmatic content
  changes: state 3 → draw/post → state 2.
- The framework's own `android.onyx.inputreader.RawInputReader` loads `libonyx_touch_reader.so`,
  **absent** in 4.2.1 → dead code. None of the Onyx/MyScript libs are in
  `system/etc/public.libraries.txt` → third-party apps cannot load them from `/system/lib64`
  (linker namespaces) — the SDK AAR ships its own copies.

### Full SDK open sequence (second Atlas question: "ink not engaging", 2026-10-16)
- `TouchHelper.openRawDrawing()` → SFTouchRender: `setStrokeStyle(0)` → native reader start →
  **pen state 1**. RawInputReader setters push `setScreenHandWritingRegionLimit(View,int[])`,
  `setScreenHandWritingRegionMode(0)` (multi-region) and `setStrokeWidth`.
- `TouchHelper.setRawDrawingEnabled(true)` (every SDK app calls it after open) → **pen state 2**
  (render on) → `setBrushRawDrawingEnabled(true)` → `setEraserRawDrawingEnabled(false, 5)`.
  An app that stops at state 1 never gets SF ink: that is the likely Atlas bug.
- `closeRawDrawing()` → `setStrokeStyle(0)`, `enablePost(1)`, state 3, state 0.
- `enablePost(0)` (= SDMDevice `enterScribbleMode`) is used only by the app-rendered
  `AppTouchRender`, never by SFTouchRender.
- `setScreenHandWritingRegionLimit/Exclude(View,int[])` add `view.getLocationOnScreen()` to the
  array **in place**: call them after attach/layout, with a fresh array.
- No package whitelist in framework.jar/services.jar. Pen-state consumers: `OnyxPenStateUtil`
  (0/4 = not drawing), `SystemGesturesPointerEventListener` (stylus edge gestures off while
  drawing), the EAC bypass/scroll managers. Pen state and enablePost carry `Process.myPid()`.
- Debug: `ViewUpdateHelper.getPenState()` / `isValidPenState()`; SF logcat strings
  "### startHandwriting", "Set limit region: (%d %d) - (%d %d)", "### not in handwriting schema.",
  "Should not call when scribble is disabled.".

`wacom_flash` (vendor/bin): stock Wacom I2C-HID flasher — Intel-HEX parser, `flash_erase_all`,
MPU type `wez01`, boot-loader version query. Nothing Onyx-specific.

## E-ink optimisations framework-wide (EAC = E-ink App Center)
Traced for a peer session's question "how do Onyx apps avoid ghosting".
- **Scrolling** — every `View` has `beforeScroll/afterScroll` → `EInkHelper` → `ScrollHelper`.
  Enter: per-app `scrollingRefreshMode` 2/4 → `applyTransientUpdate(EACUtils.toEpdMode(m))`.
  Exit (after `Settings.Global.SCROLL_REFRESH_DELAY`, default 1000 ms, max 2000): mode 0 +
  `deviceExtraConfig.isGcAfterScrolling` → **`applyGCOnce()` + `repaintEverything()`** (Onyx's
  canonical clean refresh); modes 2/4 → `clearTransientUpdate(gcAfterScrolling)`.
  `EACUtils.toEpdMode`: 1→2305 DU-quality, 2→2308 A2-quality, 3→6 REAGL, 4→UI_X_A2, 5→9 REAGL+, else 5 AUTO.
- **WebView** (framework-patched): `autoA2ForScrolling=true` by default — drag ≥ 5 px
  (`MIN_A2_TRIGGER_DISTANCE`) → beforeScroll, UP/CANCEL → afterScroll.
  `mCSSInjectEnabled=true` by default → `EACCSSUtil.injectCSS` on **network URLs only**, when the
  per-app `EACCSSConfig` is on: `evaluateJavascript` adds `<style id="onyx-eac-injected-style">`
  generated in Java (forced gray-level text color, weight 900, font-size %, white form controls,
  no link underline, hover #666, borders #555).
- **Per-app config**: `EACRefreshConfig` {updateMode, refreshModeIndex/alias, supportRegal,
  useGCForNewSurface, **gcInterval = 20** ("full refresh every N"), turbo, antiFlicker, animation};
  applied to SF via `applyAppScopeUpdate(pkg.hashCode(), enable, …)`. Stored in system **MMKV**
  (`SystemMMKV`, id `ONYX_CONFIG`, multi-process) at `<systemConfigPrefix>mmkv` = `/onyxconfig/mmkv`.
- **`IOECService`** (system_server binder, `EInkHelper.getService()`) — ~75 methods: refresh
  (gcInterval, scrollingRefreshMode, appScopeRefreshMode, turbo, antiFlicker), color/CFA
  (colorMode, saturation, brightness, contrast, mono level, dither threshold, enhance, bw mode),
  themes, **process control** (auto-freeze apps, disallowed auto-start, custom OOM-adj, pre-granted
  permissions, full-PM-access packages) and **`applyAppNetworkingConfigsToService` /
  `getAppNetworkingConfigs`** (per-app network policy — relevant to TELEMETRY / rebuild).
  Server side (permission checks) lives in `services.jar` — not yet decompiled.
- No manifest meta-data / intent lets a third-party app declare its own default mode.

## SurfaceFlinger internals (first pass: `strings`, `firmware/onyx/sf/sf_strings.txt`)
REA `search_strings` timed out (330 s) on the 12.5 MB stripped binary; plain `strings` used instead.
- Classes: `EpdcManager`, `HWEpdcManager` / `SWEpdcManager` (hardware vs software EPDC backends —
  this device uses the software one, see KERNEL.md), `EpdcSchemaManager`, `EpdcWrapper`
  (`addEpdc`, `addEpdcList`, `mergeByMode`, `getBatch` over `hwc_epdc_llist`).
- Flow strings: `processOnyxRequests %x` (the transact dispatcher), `### sf epdc region received:
  %d %d %d %d mode: %d`, `commitEpdc`, `refresh screen (%d, %d - %d, %d) waveform_mode %d flags 0x%x
  marker %d`, `onyx_epdc_screenRefresh(): error! SET_EBC_SEND_UPDATE retval`, `### warning: commit
  epdc in handwriting mode.`, `pen control state: %s, penTriggered: %d, current epdc schema: %d,
  androidDrawing: %d`.
- Devices: opens **`/dev/ebc`** (ioctl `SET_EBC_SEND_UPDATE`) and **`/dev/input/event%d`** itself
  (pen-triggered drawing), reads `/sys/onyx_misc/cytp_lo_filter`, property `vendor.onyx.htcon`.

## Next steps
0. **Done:** the kernel side of `/dev/ebc` (FPGA TCON, ioctl table, update struct,
   schemes) is in [EBC-ABI.md](EBC-ABI.md). This doc's "software EPDC" wording is superseded
   there: the Note Air 5C uses a Lattice FPGA TCON.
1. `system/bin/surfaceflinger`: locate the `onTransact` switch for the codes above and the `/dev/ebc`
   ioctl interface (waveform/mode struct layout).
2. `services.jar`: the system_server side (EAC config service `OECService`, refresh policies).
3. Kernel side of `/dev/ebc` (driver is in the 4.19 kernel image — `kernel.config` for the symbol).
