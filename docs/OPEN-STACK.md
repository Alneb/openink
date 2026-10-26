# Open stack — open-source reimplementation of the e-ink display, stylus and API layers

**Request (user, 2026-10-17):** *"could you do a reimplementation of the drivers for the e-ink
display and the stylus, and all the apis, open source, such that they work exactly as the one here
but open source?"* + *"it would be great to use rust + java"*.

**Decisions (scoping questions, same day):**
| question | answer | consequence |
|---|---|---|
| scope | **full stack in sequence** | plan all four layers; build top-down, each layer tested against the stock layers below it |
| compatibility | **clean API + compat shim** | an idiomatic Rust core and a new Java API (`org.openink`); a separate thin `onyx-compat` layer maps the Onyx names onto it |
| location | **subfolder here** (`impl/`) | this repo stays the spec (RE docs + `impl/spec/*.json`); the code sits beside it |
| goal | **both, in order** | first: power the author's own stylus app without the closed Onyx SDK; then: replace the Onyx binaries in the rebuilt firmware ([REBUILD.md](REBUILD.md)) |
| ink quality | **the same experience as the stock notes app, as much functionality as possible** | low latency alone is not enough: the target is **knote2 parity**: pressure/tilt-shaped strokes like `libneo_pen` (pencil, brush, fountain, marker, charcoal, ballpoint, square), eraser end + side-button erase, pen-up refresh, and the knote2 storage formats ([KNOTE2.md](KNOTE2.md)). So `openink-stroke` moves from "later" to right after the first device test |
| license | *"licence mit"* | `impl/` is **MIT** (`impl/LICENSE`, workspace `license = "MIT"`); a future L4 kernel driver must still be GPL-2.0 (it links against the GPL kernel) |

## The stock stack, and what replaces each layer
```
 layer                stock (closed)                                   open replacement (impl/)                testable on the LOCKED device?
 ───────────────────  ───────────────────────────────────────────────  ──────────────────────────────────────  ─────────────────────────────
 L1 Java API          android.onyx.ViewUpdateHelper (framework.jar)    java/openink        org.openink.*      yes: talks to the stock SF
                      Onyx SDK in apps (EpdController, TouchHelper,    java/onyx-compat    Onyx-named facade  yes
                      RawInputReader, EpdPenManager)
 L2 native libs       libonyx_pen_touch_reader.so (evdev → pen events) crates/openink-pen + openink-jni      yes: the app process reads
                      libonyx_epd_listener.so (FIFO /dev/onyx/listener) crates/openink-jni (listener)          /dev/input itself (SDK does)
                      libneo_pen.so (stroke rendering, 3.8 MB)         crates/openink-stroke (built)          yes (pure computation)
 L3 compositor        Onyx-patched SurfaceFlinger (≈120 private codes, crates/openink-ebc (ABI + packets)    NO: needs flashing
                      instant ink into the handwrite buffer)           + an SF patch set (C++ shim → Rust)
 L4 kernel            /dev/ebc + lfe5u TCON driver (4.19, C, GPL)      C driver (4.19 has no Rust-for-Linux)  NO: needs a custom boot.img
 L5 FPGA              LFCPNX-100 bitstream (waveforms inside)          **kept stock**: reflash/fuse hazards (EBC-ABI.md)
```
Sources for every layer: [ONYX-EPD-PEN.md](ONYX-EPD-PEN.md) (L1/L2, SF protocol, pen state
machine), [EBC-ABI.md](EBC-ABI.md) (L3/L4: ioctls, `struct ebc_update`, handwrite packets, TCON
registers), [KERNEL.md](KERNEL.md).

## Why this order (top-down)
- L1 and L2 run inside an **app process** on the stock firmware: the Onyx SDK already reads
  `/dev/input/event*` from the app and calls `ViewUpdateHelper` by reflection, so ours can too.
  They can be swapped in and compared side by side with the closed versions while the bootloader
  is locked. That is also the author's first goal (the stylus app).
- L3 and L4 can only run on a flashed device. They are built against the same specs, with host
  tests (ABI layouts, packet encoders, state machines), but proven only after unlock.
- Every layer is replaced while the layers below stay stock. A difference in behaviour is then
  pinned to one layer.

## Single source of truth: `impl/spec/`
Hand-copied magic numbers drift. The spec is generated from the firmware and both languages are
checked against it:
- `impl/spec/sf_protocol.json`: `docs/scripts/extract_sf_protocol.py` over the decompiled
  `ViewUpdateHelper.java`. **112 transaction codes**, 84 constants (EINK_* bits, UI_* modes,
  schemes, pen-state helpers), **112 transacting methods** with their ordered Parcel writes and
  reply reads. 4 codes have no Java caller (RUN_SH, SEND_UPDATE_INFO, SHOW_SCREEN, TRANSFER_EPDC).
  Codes are classified by **range** (0xFF00xx, 0x100000–0x100FFF), not magnitude, since the UI_X_*
  mode values are large too.
- Rust tests load this JSON and check every named UI_* mode decodes into a typed `UpdateMode` and
  re-encodes to the same integer; Java tests do the same for the compat shim.

## Rust crates (`impl/crates/`)
| crate | role | OS deps |
|---|---|---|
| `openink-core` | typed model: `Waveform` enum, `UpdateMode` (waveform + flags, lossless to/from the Onyx int), `PenState`, SF `Code` table, regions | none (host-testable) |
| `openink-pen` | evdev decoding: raw `input_event` → `PenSample` + `PenAction` state machine (down/move/up/hover, rubber, side button), pressure normalisation, limit/exclude regions; device discovery **by capabilities** (ABS_X/Y/PRESSURE plus any pen-type tool key, `PEN_TOOLS`: this digitiser reports BTN_TOOL_BRUSH, never BTN_TOOL_PEN), not by name; `pen-dump` trace recorder | Linux (`/dev/input`), the decoder itself is pure |
| `openink-stroke` | the knote2 pen set, replacing `libneo_pen.so` ([STROKE.md](STROKE.md)) | none |
| `openink-epdc` | the refresh engine: queue, merging, waveform choice, ghost ledger, pen-aware holding ([EPDC-ENGINE.md](EPDC-ENGINE.md)) | none |
| `openink-shape` | shape recogniser + beautifier, replacing MyScript Shape ([SHAPE-ENGINE.md](SHAPE-ENGINE.md)) | none (weights embedded) |
| `openink-hwr` | handwriting recogniser, replacing MyScript HWR ([HWR-ENGINE.md](HWR-ENGINE.md)) | none (weights embedded) |
| `openink-nn` | the `.oisw` weight format and the layers both recognisers run (dense, GRU, BiGRU) | none |
| `openink-sf` | Onyx's private SurfaceFlinger requests as typed data, checked against the spec (a data layer, no Binder client) | none |
| `openink-ebc` | L3/L4 contract: `#[repr(C)] EbcUpdate` (0x28 B, asserted at compile time), ioctl numbers, handwrite packet encoder (`0x55aa55aa`, cmd 0x21, 16-px x alignment, ≤ 65 500 B chunks) | Linux ioctl, only on a flashed device |
| `openink-jni` | `cdylib` for Android: JNI for `org.openink.pen.PenReader`, `stroke.StrokeEngine`, `epd.EpdEngine`, `shape.ShapeRecognizer`, `hwr.HandwritingRecognizer`; shared marshalling in `jvm.rs` | Android NDK |

Not built: a replacement for `libonyx_epd_listener.so` (the reader of SF's event FIFO `/dev/onyx/listener`,
behind `EpdEventListener`). Nothing in the open stack needs those events yet.

Invalid states are made unrepresentable: waveforms are an exhaustive enum (an unknown nibble is a
decode error, not a silent default), pen states are an enum, `EbcUpdate`'s size is a const assert.

## Java (`impl/java/`)
- `openink`: `org.openink.epd.Epd` (refresh, update modes, waiting) and `org.openink.pen.InkSession`
  (open / enable / pause / close, regions, stroke params). Backends behind one interface:
  `OnyxFrameworkBackend` (reflection on `android.onyx.ViewUpdateHelper`, the stock path) and later
  `OpenSfBackend` (our SF).
- `onyx-compat`: the Onyx-shaped facade built on `openink`, so code written against the Onyx SDK ports by changing
  imports. **Built**: `TouchHelper`, `RawInputCallback`, `RawInputReader`, `TouchPoint`, `TouchPointList`,
  `EpdPenManager` (`org.openink.compat.pen`) and `EpdController` (`org.openink.compat.epd`): 204 of the SDK's 211
  public names ([API-COVERAGE.md](API-COVERAGE.md), [ONYX-COMPAT.md](ONYX-COMPAT.md)). The raw → screen
  self-calibration is the library's `org.openink.pen.MapCalibrator`.

## Behaviour that must match exactly (from the traces)
- Pen state: 0 quit, 1 start drawing, 2 resume (render on), 3 pause; pen state and enablePost carry
  `Process.myPid()`. **An app that stops at state 1 never gets SF ink**: the SDK always follows
  with `setRawDrawingEnabled(true)` → state 2, `setBrushRawDrawingEnabled(true)`,
  `setEraserRawDrawingEnabled(false, 5)`.
- Close: `setStrokeStyle(0)`, `enablePost(1)`, state 3, state 0.
- Region limit/exclude arrays are view-relative and get `getLocationOnScreen()` added in place:
  call after layout, with a fresh array.
- Pen-up refresh fires after `penUpRefreshTimeMs` (default 500 ms).
- **evdev overflow (`SYN_DROPPED`)**: the decoder sends `ForcedUp` (the stock reader's action 6),
  skips events up to the next `SYN_REPORT`, then the read loop re-reads the device with
  `EVIOCGKEY`/`EVIOCGABS` (`PenDevice::snapshot` → `PenDecoder::resync`). The kernel only sends
  *changes*, so without this a pen held on the glass through an overflow never re-sends
  `BTN_TOUCH=1` and reads as hovering until it is lifted. With it, a held pen gets a fresh `Down`
  and a new stroke starts. Read loops call `PenDevice::decode` (feed + resync), not `feed` alone.
  Trace replays have no device to query, so they keep the old "pen away until the next contact".

## Build
- Rust: `cargo test` on the host for the pure crates. Android: targets `aarch64-linux-android`
  (installed via rustup) with the NDK linker. The NDK is found via `$ANDROID_NDK_HOME`, else
  `android-ndk-r30` under `$OPENINK_TOOLCHAINS` (default `impl/.toolchains`: NDK r30, android.jar API 37), and the build logs which one it used.
- Java: compiled against `android.jar` from the same place.

## Status
**Now:** all ten crates build and `impl/check.sh` passes (115 Rust tests, clippy `-D warnings`, the Java host
checks, the Android build of the demo APK). On the device: the pen reader, instant ink through the whitelisted
`ViewUpdateHelper`, the demo app, and (through Atlas) the stroke and shape engines. Not yet on the device:
`onyx-compat`, the demo's eraser / undo / persistence. Each engine doc ends with what is still open. The entries
below are the build-up in order; their test counts are as of then.

- First stage:
  - spec extracted (`impl/spec/sf_protocol.json`), Rust/Java code tables generated from it;
  - crates `openink-core`, `openink-pen` (+ `pen-dump` recorder), `openink-ebc`, `openink-jni`:
    **16 host tests pass** (`cargo test --workspace`), including spec conformance of all 112 codes
    and 19 named modes;
  - `./build-android.sh` builds `pen-dump` and `libopenink_jni.so` for aarch64 Android 35 with NDK r30;
  - Java `org.openink` (UpdateMode/Waveform, PenCommand/PenCtrlState/StrokeStyle, EpdBackend +
    OnyxFrameworkBackend, InkSession, Epd, PenReader) compiles against android.jar with
    `-Xlint:all`, no warnings; `SpecCheck` passes (19 modes, 112 codes).
- On the device (firmware `2026-08-03_16-13_4.2.1-rel_0803_eaa32b54f`, kernel
  `4.19.157-perf-g77c132eccfab-dirty`): **the digitizer has no `BTN_TOOL_PEN`**.
  `/dev/input/event5` "onyx_emp_Wacom I2C Digitizer" advertises keys 0x141 RUBBER, 0x142 BRUSH,
  0x14a TOUCH, 0x14b STYLUS, 0x14c STYLUS2, and unnamed 0x152/0x153; axes X 0..20832, Y 0..15624,
  PRESSURE 0..4095, DISTANCE 0..255, TILT_X/Y −63..63; `INPUT_PROP_DIRECT`. The first probe required
  BTN_TOOL_PEN and found nothing. Now any pen-type tool key (PEN/BRUSH/PENCIL/AIRBRUSH, `PEN_TOOLS`)
  counts, matching the stock reader's `isBrushDown`. Touch is `/dev/input/event6` "pt_mt" (MT
  0..1859 × 0..2479, a different coordinate space from the pen). `adb shell` is in group `input`
  (1004), so the event nodes are readable; `/dev/ebc` is `crw------- root` and `/dev/onyx` is not
  listable from shell. 17 host tests.
- **First real trace** (116 s, 90 780 events, 23 540 reports, user drawing freely):
  `impl/spec/traces/note-air-5c-2026-10-17-pen.trace.xz` (pen-dump format) and `….getevent.xz`
  (stock `getevent -lt`, recorded at the same time as a cross-check: same event count ±3 for the
  start/stop skew). Facts from it:
  - **~482 Hz**: median report interval 2.073 ms (p95 2.35 ms), the same hovering and touching.
  - Proximity = `BTN_TOOL_BRUSH` 0x142 (20 enter/leave pairs); contact = `BTN_TOUCH` 0x14a (80
    strokes). The unnamed key **0x152 always toggles in the same report as BTN_TOUCH** (a duplicate
    contact flag; the decoder ignores it).
  - Once, range entry and contact arrive in **one report** (BRUSH 1 + TOUCH 1): the decoder emits
    Down without a HoverEnter, which is correct.
  - Pressure while touching: 1…4095, median 2283, never 0. ABS_DISTANCE is 0 while touching.
    Tilt seen: X −2…36, Y −6…53 (range ±63).
  - No RUBBER or STYLUS events: **the author's pen has no eraser end and no side button**. The digitizer still advertises RUBBER,
    STYLUS and STYLUS2 (other Wacom EMR pens have them), so the decoder keeps supporting them. With
    this pen, erasing is a mode chosen in the app, as in knote2.
  - Regression test `crates/openink-pen/tests/replay.rs` replays 3 of the 20 hover sessions
    (`tests/data/note-air-5c-strokes.trace`, 10 686 events, 17 strokes). Its expected counts come
    from the raw key toggles, not from the decoder. 18 host tests in total.
- **Test app on the device** (`impl/java/demo`, `org.openink.demo`, built by `build-demo.sh`
  without Gradle: cargo → javac `-Werror` → d8 → aapt2 → zipalign → apksigner; 252 KB):
  - **Our evdev reader works in an ordinary app** (untrusted_app): `/dev/input/event5` is
    `crw-rw-r-- root input`, label `input_device`; no SELinux denial. So L2 (the pen) runs on the
    stock, locked device without anything closed.
  - **Self-calibration:** pen-down/up from evdev are matched to stylus MotionEvents by time
    (< 250 ms) and fitted with a least-squares affine map (`org.openink.pen.ScreenMap`, host check
    `ScreenMapCheck`). On real strokes: **35 pairs, rms 0.0 px**,
    `sx = 1860 − 0.11904·rawY, sy = 0.11904·rawX` (= 1860/15624 and 2480/20832, i.e. the IDC's
    `touch.orientation = ORIENTATION_270` from `/system/usr/idc/Vendor_2d1f_Product_0217.idc`).
    The unfitted guess `ScreenMap.rotated` already matched.
  - **Instant ink is blocked for third-party apps.** Every `android.onyx.ViewUpdateHelper` method
    is on the hidden-API **blocklist**: logcat `Accessing hidden method
    Landroid/onyx/ViewUpdateHelper;->setScreenHandWritingPenState(I)V (blocked, reflection, denied)`,
    and `getMethod` throws NoSuchMethodException. The Onyx SDK's own bypass (ReflectUtil:
    meta-reflection to `VMRuntime.setHiddenApiExemptions`) fails on Android 15 with
    NoSuchMethodException. `OnyxFrameworkBackend` tries it and reports `Access.DIRECT` / `EXEMPTED`
    or the reason. Only `View.refreshScreen(int)` / `(l,t,w,h,mode)` live on the public `View`.
  - **How Onyx exempts apps** (`ApplicationInfo.isAllowedToUseHiddenApis`, framework 4.2.1), any of:
    1. the package name *contains* `com.onyx` (`ActivityManagerHelper.isOnyxApp`, a substring test);
    2. the package is in the `OnyxSystemConfig` set `hiddenAPIThirdPartyAppWhitelist` (persisted in
       `/onyxconfig/mmkv/onyx_config`);
    3. platform signature.

    Set 2 is filled by an **exported** receiver in `OnyxAlarmHelper`: action
    `com.onyx.action.disable_restriction`, string extra `package` →
    `EInkHelper.addToHiddenApiThirdPartyAppWhiteList(pkg)` plus "allow alarms in run" for that
    package. It writes to onyxconfig, so it was done only with the device owner's explicit OK: whitelist now, Binder
    next (2026-10-17). **Sent** at 03:28:56 for `org.openink.demo`
    and `org.atlas.boox` (`am broadcast -a com.onyx.action.disable_restriction --es package <pkg>`,
    received by system_server's `OnyxAlarmManagerReceiver`). No removal path was found. After a
    process restart the demo reaches the methods `DIRECT`, opens instant ink over 1860×2350, and SF
    reports pen state **2 (RUNNING)**. The exemption is keyed by package name and survives
    `adb install -r`.
  - Onyx's app freeze (`ApplicationFreezeHelper`, "isAutoFreeze:true") **disabled the freshly
    installed app** (`enabled=3`) at the same moment `am start` launched it, which killed it.
    `pm enable org.openink.demo` fixes it. The launcher unfreezes apps on tap.
  - This is also why Atlas for Boox's OnyxInk never engaged (told to the Atlas session, which saw
    no AtlasInk log lines).
- **Atlas for Boox confirmed:** after the whitelist, SF logs `start → running` and one
  `HandlePenTrigger` per stroke for Atlas. Drawing in Excalidraw felt responsive.
- **`openink-sf` (new crate), the pure request layer:** each of the 15 calls the backend uses is a
  typed `Request` (code, ordered `Arg`s, `Reply`), plus `open_sequence` / `close_sequence` as data.
  `tests/spec.rs` checks every request's Parcel shape against `impl/spec/sf_protocol.json`. Rectangles
  are screen-absolute: the helper's `View` only adds `getLocationOnScreen()`. 21 host tests in total.
- **`openink-sf` is a data layer only.** The display integration drives instant ink through the
  whitelisted `ViewUpdateHelper` / `View.refreshScreen` API (`OnyxFrameworkBackend`-style); a direct
  SurfaceFlinger Binder client is out of scope and not needed.
- (Then, in order: `openink-stroke`, the `onyx-compat` shim, `openink-epdc`, `openink-shape`, `openink-hwr`; each
  has its own doc. The long-term aim stays our own compositor on a flashed device, with no Onyx Java.)
- Run the checks — **one command runs every gate** (Rust tests, clippy `-D warnings`, the Java host
  checks SpecCheck + ScreenMapCheck + MapCalibratorCheck, the Android cross-build and the demo APK,
  then PageFileCheck); it stops at the first failure with a non-zero exit (`--no-device-builds`
  skips stages 4–5 on a machine without the NDK/android.jar):
  ```sh
  impl/check.sh
  ```
  The individual commands:
  ```sh
  cd impl && cargo test --workspace                 # Rust, host
  ./build-android.sh -p openink-pen -p openink-jni  # Rust, device (aarch64 Android 35)
  AJ=impl/.toolchains/unpack/android-37.2/android.jar
  javac --release 11 -Xlint:all -cp $AJ -d /tmp/oi $(find java/openink/src/main -name '*.java')
  javac --release 11 -d /tmp/oit java/openink/src/main/java/org/openink/{epd/Waveform,epd/UpdateMode,sf/SfCode}.java \
        java/openink/src/test/java/org/openink/SpecCheck.java && java -cp /tmp/oit org.openink.SpecCheck spec/sf_protocol.json
  ```
- License: **MIT**, in `impl/LICENSE`; a future L4 kernel driver must be
  GPL-2.0 regardless.

## Open
- Which SF codes are permission-checked on the SF side (RUN_SH, SET_VCOM, SHUTDOWN, STOP_SYSTEM).
- `libneo_pen.so` internals: its C++ type info names the whole pipeline and the native config
  clamps were read ([STROKE.md](STROKE.md) "libneo_pen.so internals"). The open
  renderer stays a new design; the width formulas are the next thing to compare.
- L4: request the GPL source for `4.19.157-perf-g77c132eccfab` from Onyx before decompiling more of
  the driver; it is owed under GPL-2.0 and would turn L4 from reconstruction into review.


## Demo app (`impl/java/demo`, org.openink.demo) — features
- Buttons: **Clear**, **Undo** (last stroke / eraser contact / Clear, 100 deep), **Instant ink** on/off,
  **Flash** (GC16 full refresh), **Pen:** cycles knote2's 7 pens, **Erase** (stroke eraser: removes
  every stroke within 12 px + half its width of the eraser path; SF instant ink is paused while
  erasing), **Width −/+** within the pen's knote2 range (SF's instant-ink width follows).
- A pen's eraser end (evdev `BTN_TOOL_RUBBER`, MotionEvent `TOOL_TYPE_ERASER`) erases in any mode;
  the author's pen has none, hence the mode.
- The page persists across restarts in `files/page.oink` (`PageFile`: versioned, atomic,
  unreadable files moved aside); the raw → screen calibration (`MapCalibrator`) is kept across
  resumes.
- Gates: `impl/check.sh` stages 4–5 build the APK (`-Xlint:all -Werror`) and run `PageFileCheck`.
- Not yet tried on the device: eraser, undo, persistence (no device attached in).

## openink-stroke (after "start building the reimplementation")
- This is not a whitelist bypass: it uses Onyx's `ViewUpdateHelper` and the approved whitelist entry. A direct
  SurfaceFlinger client is out of scope.
- Built instead: the stroke renderer, [STROKE.md](STROKE.md). Seven knote2 pens, JNI
  `StrokeEngine`, Java `InkPainter`, demo with pen/width buttons; installed on the device.
- `openink-pen` gained `trace::parse` (one parser for the replay test and the preview).


## Prebuilt packages (`impl/package.sh`)
For apps that build without Gradle (Atlas: javac + d8 + copy `.so`). The output is `impl/target/dist/`:
- `openink.jar`: `org.openink.*`, javac `--release 17` against android.jar, `-Werror`. The consumer runs d8.
- `onyx-compat.jar`: compiled against `openink.jar`.
- `arm64-v8a/libopenink_jni.so`: stripped; stroke, pen, epd and shape.
- `VERSION` (git describe and build time) and `SHA256SUMS`.

Stable packages: `org.openink.stroke`, `org.openink.pen`, `org.openink.shape`. `org.openink.epd` and
`org.openink.backend` are still changing. Atlas (github.com/Alneb/atlas) vendors the stable ones.
