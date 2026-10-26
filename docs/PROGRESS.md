# Progress report — what is done, what is missing, in numbers (2026-10-26)

How far openink is, in one place: the open e-ink and stylus stack for the BOOX Note Air 5C (see
[OPEN-STACK.md](OPEN-STACK.md) for the layers L1–L5). Every row is a concrete deliverable with a state and the
evidence behind it, and every percentage below is **counted from those rows**, never guessed.

## How to read it

**Evidence levels** (strongest last), as each row is known:

| level | meaning |
|---|---|
| **none** | nothing exists yet |
| **static** | read from the firmware (decompiled / disassembled); not exercised |
| **built** | code exists in `impl/` and its host tests pass (`impl/check.sh`) |
| **oracle** | compared number by number against the original library running on the device (`impl/java/neo-oracle`) |
| **device** | seen working on the Note Air 5C itself |
| **n/a** | turned out not to be needed (the stock software does not use it); counted as done, with the proof |

**Scoring:** a deliverable counts 1 when done, 0.5 when partly done (the row says which half), 0 when missing.
A layer's percentage is its points ÷ its deliverables. The totals are weighted by deliverable count, so a layer
with more pieces weighs more.

## Headline numbers

| measure | value |
|---|---|
| Open stack, app-process half (L1 + L2: runs on a locked device) | **33 / 42 deliverables = 79 %** |
| Open stack, system half (L3 compositor + L4 kernel driver) | **3 / 10 = 30 %** (specs and ABI only; no code that runs on the device) |
| knote2 feature parity (what a user of the stock notes app sees) | **9.5 / 11 = 86 %** built, **3 / 11** seen on the device |
| Rust code | 10 crates, 11 113 lines, 117 tests (`impl/check.sh`: all 5 stages pass) |
| Java code | 56 files, 5 166 lines (`org.openink` API, `onyx-compat`, demo app, oracle) |
| Onyx SDK API reimplemented (`onyx-compat`) | **204 / 211** public names (96.7 %); the 7 left are Onyx-internal |
| Stroke pens matching libneo (outline area `xor / union`, 4× raster) | **7 / 7** pens ported; ballpoint 0.02–0.17 %, square 0.03–0.25 %, brush 0.6–1.4 %, fountain 0.8–1.4 %, marker 1.2–2.5 % |
| Dab pens matching libneo | pencil dabs identical on 3 / 3 recorded strokes (373 / 373 dabs); charcoal V2 tiles 163 / 176 identical |
| Shape recogniser (replaces MyScript Shape) | 0.890 exact class, 0.941 family (shape-eval, 4000 synthetic, seed 3) |
| Handwriting recogniser (replaces MyScript HWR) | CER 0.138, word accuracy 52 % (handwritten notes, physics vocabulary) |
| knote2 storage formats | 14 / 14 protobuf schemas recovered ([formats/knote2-proto](formats/knote2-proto/)); 0 real notes decoded end to end |
| The one blocker | the **locked bootloader**: the system half (a compositor and a kernel driver) cannot run on a locked device, so the compositor, the driver and the device checks of the kernel ABI wait for it |

## 1. Open stack, layer by layer

### L1 — Java API (`org.openink`, `onyx-compat`): 5.5 / 9

| deliverable | state | evidence |
|---|---|---|
| `org.openink` API (pen, stroke, shape, HWR, refresh) | done | device (demo app) |
| `onyx-compat`: Onyx `TouchHelper` / `RawInputReader` / `EpdController` / `EpdPenManager` / `TouchPointList` / `RawInputCallback` | done: 204 / 211 names | built ([API-COVERAGE.md](API-COVERAGE.md)) |
| `onyx-compat`: the 7 names left (`getEventBusHolder`, `register`, `unregister`, `nativeRawReader`, `onNext`, `run`) | n/a: Onyx-internal event-bus plumbing, not app API | static |
| `onyx-compat` run on the device with a real Onyx-SDK app | missing | none |
| 3 `create(...)` overloads | missing | static |
| meaning of `onBeginRawDrawing`'s leading boolean | missing | static |
| JNI layer (`openink-jni`: stroke, shape, HWR, pen) | done | built + device |
| Java host checks + Android cross-build in the gate | done | built |
| demo app (pens, width, eraser, undo, persistence) | half: runs on the device; eraser / undo / persistence not tried there | device (partly) |

### L2 — native, in the app process: 27.5 / 33

**Pen input (`openink-pen`): 4 / 4**

| deliverable | state | evidence |
|---|---|---|
| raw digitiser reader (evdev, pressure, tilt, hover, side button / eraser decode) | done | device (trace replay, demo, peer project Atlas) |
| pen discovery without `BTN_TOOL_PEN` (the digitiser has none) | done | device |
| recorded traces + replay tests | done | built |
| eraser end / side button | done in the decoder; untested on hardware (the test pen has neither) | built |

**Stroke renderer (`openink-stroke`, replaces `libneo_pen.so`): 15 / 17**

| deliverable | state | evidence |
|---|---|---|
| SmoothFilter (FIR, 15-tap kernel resampled to N taps) | done, nodes within 1e-4 px | oracle |
| uniform Catmull-Rom spline over every channel | done | oracle |
| SplineCurvatureInterpolator: x/y bisection | done (exact t values reproduced) | oracle |
| … width-channel bisection (channel 3) | done | oracle |
| … azimuth channel quarter points (channel 4, `FUN_004498a4`) | **done 2026-10-26**: azimuth from tilt (`FUN_00470184`), unwrapped as `FUN_004529ac` does, quarter nodes where it moves > 0.06 rad; fountain only | built (host tests); oracle comparison pending (needs the device) |
| … accumulated-turn test (`FUN_0044973c`) | n/a: its threshold starts at −1 and the test only runs when ≥ 0, so it is off for every stock pen | static |
| ballpoint | done, 0.02–0.17 % | oracle |
| fountain V2 | done, 0.8–1.4 % | oracle |
| square | done, 0.03–0.25 % | oracle |
| brush (own pipeline) | done, 0.6–1.4 % (6.9 % on a 1.5 px dot) | oracle |
| marker (own pipeline) | done, 1.2–2.5 % | oracle |
| pencil dab placement and angles | done, identical | oracle |
| pencil opacity | half: openink keeps its own curve; knote2's is an integer-division quirk (every dab at alpha 4/255) that needs one captured knote2 pencil stroke to confirm | static |
| charcoal V2 tiles (binary dithering, own grain texture) | done, 163 / 176 tiles identical | oracle |
| fountain V1 (pen type 2) and charcoal V1 (type 4) | missing: knote2 still routes old strokes to them (`NeoPenUtils` type 2; `CharcoalPathRender` for any charcoal type ≠ 5), so only older notes need them | static |
| predicted ink tail | n/a: knote2 passes `null` as the prediction at every call site (`TouchData.predictPoint` is never set; every `toTouchData` / `onTouchMove` caller passes `null`), so libneo's prediction ink is always empty and stock knote2 draws no predicted tail | static (2026-10-26) |
| Clipper2-style union of pieces into one polygon | half: nonzero-winding fill of overlapping pieces looks the same; output is bigger | built |

**Refresh engine (`openink-epdc`): 2 / 4**

| deliverable | state | evidence |
|---|---|---|
| engine: pen-up refresh (500 ms hold, one repaint per word), region merging, waveform choice | done, 17 tests | built |
| live (instant) ink through the whitelisted `ViewUpdateHelper` | done | device |
| `byPass` wiring so the engine owns refresh on a stock device | missing | none |
| ghost costs tuned on the panel | missing (the costs are a first guess) | none |

**E-ink event listener: 0 / 1**

| deliverable | state | evidence |
|---|---|---|
| reader for SF's event FIFO (`/dev/onyx/listener`, `libonyx_epd_listener.so`) | missing; only needed if an app uses `EpdEventListener` callbacks | static |

**Shapes (`openink-shape`, replaces MyScript Shape): 3.5 / 4**

| deliverable | state | evidence |
|---|---|---|
| recogniser (polygons, conics, arrows, open and closed heads) + snapping on a 500 ms hold | done; 0.890 exact / 0.941 family | built + device (Atlas) |
| trained weights shipped (`weights/shape.oisw`) | done | built |
| Atlas regression tests from real strokes | done | built |
| known misses: double-line shafts (⇒), crops with neighbouring scribbles, Atlas's "stops drawing shapes after many" (not reproduced) | half | built |

**Handwriting (`openink-hwr` + `openink-nn`, replaces MyScript HWR): 3 / 3 delivered, quality open**

| deliverable | state | evidence |
|---|---|---|
| recogniser + beam search + lexicon + 4-gram LM, pure Rust inference | done | built |
| trained model shipped (`ft4-ds-best`) | done: CER 0.138, word 52 % | built (452 hand-labelled words) |
| math recognition | n/a: decided not to build | — |

Not counted as a deliverable but the weakest number in the stack: **word accuracy 52 %** (about every second
word wrong). Line-level recognition and a "recognise selection" action in the demo are not started.

### L3 — compositor (Onyx SurfaceFlinger layer): 1.5 / 5

| deliverable | state | evidence |
|---|---|---|
| SF's Onyx layer decompiled and named (915 functions, [SF-ONYX.md](SF-ONYX.md)) | done | static |
| the 112 transaction codes as data, `openink-sf` (15 typed requests) | half: typed requests cover 15 of 112 codes | built |
| which codes SF permission-checks (RUN_SH, SET_VCOM, SHUTDOWN, STOP_SYSTEM) | missing | none |
| the compositor itself | missing; needs an unlocked device to run at all | none |
| live-ink painters (`PainterBase`, libneo painters) understood | done in [SF-INSTANT-INK.md](SF-INSTANT-INK.md), but not reimplemented: counted 0 here | static |

### L4 — kernel (`/dev/ebc`, TCON driver): 1.5 / 5

| deliverable | state | evidence |
|---|---|---|
| ioctl ABI, update struct, register map, handwrite DSI channel, built-in bitstreams | half: decoded, `openink-ebc` holds the ABI and packets, never exercised | static + built |
| device-side checks of the decoded ABI | missing | none |
| GPL source request for `4.19.157-perf-g77c132eccfab` | missing (not yet asked) | none |
| open driver | missing | none |
| TCON firmware | n/a: kept stock by design (reflash / fuse hazards) | — |

## 2. knote2 feature parity (what the user sees): 9.5 / 11

| knote2 feature | built | on the device |
|---|---|---|
| 7 pens with pressure / tilt | yes (oracle-matched) | yes (demo) |
| live (instant) ink while drawing | yes | yes |
| pen-up refresh | yes | — |
| stroke eraser (12 px + half width) | yes | not tried |
| undo (100 deep), page persistence (`page.oink`) | yes | not tried |
| eraser end / side button | yes (decoder) | untestable (pen has neither) |
| shape snapping (hold 500 ms) | yes | yes (Atlas) |
| handwriting → text | yes (52 % words) | — |
| predicted ink tail | n/a (knote2 never draws one, see L2) | — |
| reading real knote2 notes | half: formats decoded (Room DBs, point files, 14 / 14 protos, KSync); 0 real notes decoded (needs root or an export) | no |
| `.note` export container, rendering notes from storage | no | no |

## 3. Docs state

Fixed on 2026-10-26 (they contradicted the code or each other): OPEN-STACK.md crates table (4 of 10 crates, a
nonexistent EPD listener, `BTN_TOOL_PEN` discovery, "16 host tests"); HWR-ENGINE.md "untrained placeholder";
ONYX-COMPAT.md "not mirrored"; SHAPE-ENGINE.md closed-head item; STROKE.md prediction and channel-4 items. 0 dangling links between the docs.

Claims that stay static-only and are marked so in their docs: the whole compat layer
([ONYX-COMPAT.md](ONYX-COMPAT.md)), ghost costs ([EPDC-ENGINE.md](EPDC-ENGINE.md)), knote2 formats
([KNOTE2.md](KNOTE2.md), from the APK, not a real note), the kernel ABI ([EBC-ABI.md](EBC-ABI.md),
[KERNEL.md](KERNEL.md)).

## 4. What to do next, in order

| # | item | needs | moves |
|---|---|---|---|
| 1 | device session: run an Onyx-SDK sample through `onyx-compat`; try eraser / undo / persistence; capture one knote2 pencil stroke (opacity); export one real note; oracle-check the channel-4 port with a tilted stroke | the tablet over USB | L1 +1, L2 +1, parity +1.5 |
| 2 | `byPass` wiring + ghost tuning | the tablet | L2 +2 |
| 3 | fountain V1 / charcoal V1 (older notes) | static work, then the oracle | L2 +1 |
| 4 | e-ink event listener (only if an app needs it) | static work | L2 +1 |
| 5 | SF permission checks (which codes an app may send) | static work | L3 +1 |
| 6 | HWR word accuracy (more labelled words, line context) | labelled handwriting | quality (lowest priority) |
| 7 | the compositor and the kernel driver | an unlocked bootloader | system half |

## How the numbers were counted

- Lines: `find impl/crates -name '*.rs' | xargs cat | wc -l`; tests: the count `impl/check.sh` reports (2026-10-26).
- Pen parity: [STROKE.md](STROKE.md) tables (recorded strokes, W 10, rasterised at 4×, `xor / union`).
- API names: [API-COVERAGE.md](API-COVERAGE.md). Shape: [SHAPE-ENGINE.md](SHAPE-ENGINE.md) (shape-eval). HWR:
  [HWR-ENGINE.md](HWR-ENGINE.md).
- Prediction and V1 pens: knote2's `NeoPenRender`, `NeoNativePen`, `NoteTouchPointKt`, `TouchData`, `NeoPenUtils`,
  `CharcoalPathRender`. Channel 4: libneo `FUN_00448bf0` → `FUN_004498a4`, `FUN_004529ac`, `FUN_00470184`, and the
  JNI entry `nativeOnPenMove`.
