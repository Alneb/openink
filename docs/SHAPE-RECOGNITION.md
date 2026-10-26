# Shape recognition in knote2 — how "draw a circle, hold, it snaps" works

How knote2 processes strokes, and its circle and square recognition. Read from the knote2 jadx sources
(`firmware/onyx/pen/knote2_src`).

## Two different things were being called "hidden"
- **Stroke rendering** (how a pen-down…pen-up becomes ink: width, taper, pencil grain): done in
  `libneo_pen.so`. Its Java side shows only the *contract* (config fields, input samples, output
  polygons/dabs; table in [STROKE.md](STROKE.md)); the geometry itself is compiled ARM code with
  only JNI symbols exported. "Hidden" there means *not readable as source*. It could be
  disassembled (Ghidra), but openink-stroke reimplements the visible behavior instead, which also
  keeps the repo free of Onyx code.
- **Shape recognition** (circle/rectangle/line snapping) is a separate system, described below.
  Its trigger logic is plain Java and fully visible; the recognizer itself is **MyScript**.

## The pipeline
1. **Hold detection — Java, visible.** `com.android.onyxsdk.rxgesture.pen.PenMoveReader`
   watches the pen's MotionEvents during a stroke:
   - anchor = a move point; while each later point stays within **±2 px in x and y** of the
     anchor, it is "still";
   - still for **> 500 ms** (`timestamp − anchor.timestamp > 500`) → `PenStatus.HOLDING`,
     `holdCount++`, posted as `PenStatusChangeEvent`;
   - a move beyond 2 px while holding → back to `MOVING` ("onReleaseHold").
2. **Trigger — Java, visible.** `HWRContextLessEventHandler.onPenHolding`: if the setting
   `NoteSetting.isRecognizeGraphical()` is on and `holdCount == 1` (first hold of this
   stroke), run `StartRecognizeDiagramAction`.
3. **Packaging — Java, visible.** `StartRecognizeDiagramAction` turns the stroke's collected
   points (`NoteHWRBundle` rec point list) into a `Shape` with the active pen's drawing args and
   posts `HWRAddContextLessStrokeEvent` with **content type `"Diagram"`** and the box info.
4. **Recognition — MyScript, closed.** The HWR service (MyScript iink engine, `libMyScript*.so`,
   bundled `system/recognition_assets/conf/diagram…`; see [HWR-OFFLINE.md](HWR-OFFLINE.md)) does
   the actual shape fitting and returns **JIIX** (MyScript's JSON) with `Diagram` / `Node` /
   `Edge` items.
5. **Resolving — Java, visible.** `com.onyx.android.sdk.hwr.onyxms.ShapeRecognize.resolveJIIXBean`
   dispatches by JIIX type: `Diagram`, `Node`/`Edge` → `HWRDiagramResolver`; `Text`, `Math`,
   `Raw Content` → their resolvers. The resolvers build ordinary knote2 `Shape`s (circle,
   rectangle, line…), which `StrokeShapeRecRequest.handleHWROutput` scales by the page's
   normalize scale, gives the stroke width, and adds to the page through the command manager
   (so it is undoable). Then the hand-drawn stroke is replaced.

The same recognizer also serves the Text / Math / Diagram conversion pens
(`TextHWRHandler`, `MathHWRHandler`, `DiagramHWRHandler` → `StrokeShapeRecAction`).

## Consequences for openink
- Steps 1–3 and 5 are simple and fully documented above; only step 4 (the fitting) is the closed
  MyScript engine. It works offline once its activation check is patched out (patch P1, kept private), but it
  is licensed, closed code.
- An open replacement for step 4 is very doable without machine learning: classify one stroke by
  closure (end-to-start gap vs. length), corner count (Douglas–Peucker / curvature), and residual
  of least-squares fits (line, circle, ellipse, polygon), then snap to the best fit under a
  threshold. Steps 1–2 port directly onto the openink pen stream (we already have evdev points at
  ~480 Hz with timestamps).
- Not started; offered to the author as a possible next feature.
