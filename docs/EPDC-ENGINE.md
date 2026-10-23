# The open e-ink refresh engine (`openink-epdc`)


## What it replaces, and why it looks like this
Stock firmware decides every panel refresh inside SurfaceFlinger's Onyx layer
([SF-ONYX.md](SF-ONYX.md)). The pieces are:
- `EpdcManager`, the update queue and merging;
- `UpdateEntry` / `FixedUpdateEntryManager` / `UpdateEntryDetector`, the per-app and per-layer modes and the
  GC interval;
- the schemas (`NormalSchema`, `HandwritingSchema`, …), the pen-state machine.

The goal is a **100% reimplementation of the drawing + e-ink stack, working on a
locked device, with our own refresh policy** (keep the protocol for compatibility, redesign the policy).

On a locked device an app cannot open `/dev/ebc` (SELinux: only surfaceflinger and charger), and
the display integration goes only through the whitelisted `ViewUpdateHelper` / `View.refreshScreen` API
(the API-COVERAGE.md scope rule). So the engine:
- is **pure and clock-free** (`crates/openink-epdc`): requests and pen events come in with the caller's
  time, `poll(now)` returns the updates due, and `next_deadline()` says when to poll again. No threads and no
  callbacks, so every policy rule is a host test;
- runs **in the app** on a stock device: `org.openink.epd.EpdEngine` (Java) polls it on one Looper and hands
  each update to a `Sink`, e.g. `ViewUpdateHelper.refreshScreen(l, t, w, h, mode)`;
- is the same code that will drive `/dev/ebc` from our own compositor on a flashed device (a different `Sink`).

## Inputs
- `request(rect, Intent, now)`, where `Intent` says what changed:

  | intent | mode | why |
  |---|---|---|
  | `Ui` | GU (GC16 partial) | text and UI: 16 levels, no flash |
  | `Image` | GC16 partial | photos and gradients |
  | `Motion` | A2 | scrolling, dragging, animation: fastest, ghosts most |
  | `Ink` | DU | the app's repaint of finished ink: 2-level, fast, no flash |
  | `Clean` | GC16 full | an explicit cleanup flash |
  | `Raw(mode)` | the given `EINK_*` mode | Onyx-SDK compatibility (`EpdController.refreshScreen`) |

- `pen_down(now)` and `pen_up(stroke_box, now)`: the stroke box (inflated by half the width) is queued as
  `Ink`.
- `set_gc_every(n)` (stock `SET_GC_REFRESH_INTERVAL`) and `set_scope_mode(mode)` (stock `APPLY_APP_SCOPE_UPDATE`).

## Policy (defaults in `Policy::default`, every value a named field)
1. **Coalescing**: a burst is collected for `coalesce_ms` = 25 ms after its first request, then sent together.
2. **Merging without reordering pixels**: a new request joins an older pending one of the *same mode* when
   they are within `merge_gap` = 16 px. If a later pending update of a different mode overlaps in between, the
   merge stops there. A newer request that fully covers an older one with an equal or better waveform
   (quality rank: full GC > GC16/Regal partial > GC4/DU4 > DU > A2) absorbs it.
3. **Writing holds the panel**: from `pen_down` to `pen_up` nothing is sent, because a refresh would overdraw the
   live instant ink. Stock SF logs `### warning: commit epdc in handwriting mode.` for the same case. After
   `pen_up` the engine waits `pen_up_repaint_ms` = 500 ms (knote2's pen-up refresh time) for the next stroke. A
   new `pen_down` keeps holding, so a written word gets one repaint, not one per stroke. A request deferred
   longer than `max_defer_ms` = 3 s goes out at the next pen-up without the wait, so a long writing session
   does not starve the UI.
4. **Ghost ledger**: the screen is split into `tile` = 64 px cells. Each update adds its waveform's ghost cost
   to the cells it covers, counted in A2 frames: A2 1.0, DU 0.5, DU4 0.4, GC4 0.3, Regal 0.15, GC16/GU partial 0.1.
   A full GC-class update (or INIT) resets its cells.
5. **Cleanup**: when a cell reaches `ghost_threshold` = 6.0, and the screen has been quiet for
   `idle_cleanup_ms` = 1.5 s, and the pen is not writing (and is past its pen-up wait), one GC16 full update is
   sent over the bounding box of the cells at or above the threshold. If that box covers at least
   `full_screen_fraction` = 50 % of the screen, the whole screen is flashed instead.
6. **GC interval** (compat): with `gc_every = n`, every nth partial update is promoted to a full GC16 (`Reason::GcInterval`).
7. **App-scope mode** (compat): replaces the mode of every request except `Raw` and `Clean`.
8. `flush(now)` sends everything queued without the coalescing or pen-up waits
   (`EpdController.waitForUpdateFinished`), still never under the pen.

Every update carries a `Reason` (`Request`, `GcInterval`, `Cleanup`), so a log explains each flash.

## On the stock device: who owns the refresh
SurfaceFlinger refreshes the panel by itself on every frame (`NormalSchema::output`), so on a locked device
an in-app engine adds refreshes on top of the stock ones. The firmware has the switch to hand the decision
over: `BYPASS(n)` (whitelisted as `ViewUpdateHelper.byPass(int)`, exposed as `EpdController.byPass`) sets
`EpdcManager`'s bypass counter (field 0x2c). Each frame's `bypassUpdate()` decrements it and returns
`UpdateEntry::disableUpdateEntry` while it is above zero, which skips that frame's panel update; with the
counter at zero it returns `nullUpdateEntry` (no override). A negative n subtracts. Decoded from
`processOnyxRequests` case 0x10002d and `EpdcManager::bypassUpdate` @0x607218 (see SF-ONYX.md).
_Not yet wired_: the plan is for `EpdEngine` to call `byPass(1)` before an app frame it will refresh itself.
That needs a device test, because the frame accounting has to match.

## Java (`org.openink.epd.EpdEngine`)
- One per process, on one Looper; every method checks the thread and throws off it.
- `request(l, t, r, b, Intent)`, `requestRaw(l, t, r, b, UpdateMode)`, `penDown()`, `penUp(l, t, r, b)`,
  `flush()`, `setGcEvery(n)`, `setScopeMode(mode)`, `setListener(...)` (traces each update with its reason).
- Scheduling: after every call it posts one `pump` at `nextDeadline` (`Handler.postAtTime` on
  `SystemClock.uptimeMillis`, the same clock the engine is fed).
- JNI wire formats in `crates/openink-jni/src/epdc.rs`: intent codes 0..4, `RAW` = -1 plus the Onyx int, and a
  poll result of `int[6 n]` = l, t, w, h, mode, reason.

## onyx-compat on top
`org.openink.compat.epd.EpdController` (all 100 public names of Onyx's class): explicit refreshes go through
the engine as `Raw` (the app keeps its waveform; the engine adds timing, merging, pen holding and cleanup).
State that SurfaceFlinger keeps passes straight through to the same framework method SDMDevice uses
(`org.openink.backend.OnyxFramework`, bindings from `docs/scripts/sdk_bindings.py`).

## Tests
`crates/openink-epdc/tests/engine.rs`, 11 scenarios: coalescing, merge order across modes, absorption, pen
holding with two strokes merged into one DU repaint, no starvation, ghost cleanup tile-aligned after idle, no
cleanup while writing, GC interval, scope mode vs raw, clipping, flush. Unit tests cover geometry, the ledger
and the intent/wire codes.

## Open
- Wire `byPass` so the engine owns the refresh on a stock device (needs a device test).
- Tune the ghost costs and threshold on the panel (they are a first proposal, not measurements).
- A pixel classifier (text vs image vs motion) when the caller passes a buffer instead of an intent.
- The flashed-device `Sink` (`/dev/ebc` through `openink-ebc`), once our compositor exists.
