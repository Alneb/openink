package org.openink.demo;

import android.content.Context;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.PorterDuff;
import android.graphics.Rect;
import android.graphics.RectF;
import android.os.SystemClock;
import android.util.Log;
import android.view.MotionEvent;
import android.view.View;
import java.util.ArrayList;
import java.util.List;
import org.openink.pen.PenReader;
import org.openink.pen.MapCalibrator;
import org.openink.pen.PenAxes;
import org.openink.pen.ScreenMap;
import org.openink.shape.Shape;
import org.openink.shape.ShapeRecognizer;
import org.openink.stroke.Ink;
import org.openink.stroke.InkPainter;
import org.openink.stroke.PenConfig;
import org.openink.stroke.PenKind;
import org.openink.stroke.StrokeEngine;

/**
 * The drawing surface. Strokes come from our own evdev reader (PenReader, reader thread) in raw
 * digitizer units and are shaped by our stroke engine (openink-stroke, the libneo_pen
 * replacement); Android's stylus MotionEvents only fit the raw → screen map (pen-down/up pairs
 * matched by time), or draw when the reader could not open the device.
 */
final class InkCanvas extends View implements PenReader.Listener {
    static final String TAG = "openink";

    /** One stroke as recorded: points in digitizer units if {@code raw}, else view pixels. */
    static final class Stroke {
        final boolean raw;
        final PenKind kind;
        final PenConfig config;
        final List<double[]> pts = new ArrayList<>(); // {x, y, pressure, tiltX, tiltY, timeUs}: double keeps µs
        /** View-pixel bounds for eraser hit tests, valid for the map they were computed with. */
        RectF viewBounds;
        ScreenMap boundsMap;
        Stroke(boolean raw, PenKind kind, PenConfig config) { this.raw = raw; this.kind = kind; this.config = config; }
    }

    /**
     * One undoable action: a stroke that appeared, or strokes removed. Each removal keeps the index
     * it had at the moment it was removed; undo reinserts in reverse order, the exact inverse.
     */
    private static final class Edit {
        final Stroke added;
        final List<Stroke> removed = new ArrayList<>();
        final List<Integer> at = new ArrayList<>();
        Edit(Stroke added) { this.added = added; }
        void remember(Stroke s, int index) { removed.add(s); at.add(index); }
    }

    private static final int UNDO_DEPTH = 100;

    /** Eraser reach beyond a stroke's half width, view px (about 1 mm at 300 dpi). */
    private static final float ERASER_RADIUS_PX = 12;

    private final Runnable onStrokeDone;
    private final float dpi;
    private final InkPainter painter = new InkPainter(Color.BLACK);
    private final Paint layerPaint = new Paint();
    // Guarded by `this`: written by the reader thread, read by the UI thread.
    private final List<Stroke> strokes = new ArrayList<>();
    private PenKind kind = PenKind.FOUNTAIN;
    private PenConfig config;
    private Stroke current;
    /** Live engine (whichever thread draws the stroke in progress) and redraw engine. */
    private StrokeEngine live, redraw;
    private Bitmap page, layer;
    private Canvas pageCanvas, layerCanvas;
    private boolean layerInUse;
    /** What the live translucent stroke has drawn into the layer so far (view px). */
    private final RectF layerDirty = new RectF();
    /** Raw → screen map, fitted from contacts; null until the reader opens (MotionEvents draw). */
    private MapCalibrator calib;
    private final int[] onScreen = new int[2];
    private boolean readerLive;
    /** The live engine is between down() and up(). */
    private boolean liveActive;
    /** Erase mode. */
    private boolean eraseMode;
    private boolean snapShapes = true;
    /** The current contact erases (erase mode, or a pen's eraser end), and its last view point. */
    private boolean erasingContact, eraseRaw;
    private float[] eraseLast;
    /** Undo history, newest last; the erasing contact in progress collects into `erasing`. */
    private final java.util.ArrayDeque<Edit> history = new java.util.ArrayDeque<>();
    private Edit erasing;
    // Status, read by the activity. Raw values only: formatting per event (480 Hz) makes garbage
    // on the pen thread; lastText() formats on demand. Fields may tear across one event: status only.
    volatile long events;
    private volatile int lastAction = -1, lastX, lastY, lastTiltX, lastTiltY;
    private volatile float lastPressure;

    InkCanvas(Context c, Runnable onStrokeDone) {
        super(c);
        this.onStrokeDone = onStrokeDone;
        dpi = c.getResources().getDisplayMetrics().densityDpi;
        config = PenConfig.knote2Default(kind, dpi);
        layerPaint.setAlpha(PenKind.TRANSLUCENT_ALPHA);
        setBackgroundColor(Color.WHITE);
    }

    synchronized PenKind pen() {
        return kind;
    }

    synchronized PenConfig penConfig() {
        return config;
    }

    /** Hold-to-snap: a stroke finished with the pen at rest becomes a clean shape. */
    synchronized void setSnapShapes(boolean on) {
        snapShapes = on;
    }

    synchronized boolean snapShapes() {
        return snapShapes;
    }

    /** The pen for the next stroke; the stroke in progress keeps its own. */
    synchronized void setPen(PenKind k, PenConfig c) {
        kind = k;
        config = c;
    }

    synchronized void setEraseMode(boolean on) {
        eraseMode = on;
    }

    synchronized boolean eraseMode() {
        return eraseMode;
    }

    /** Width for SurfaceFlinger's instant ink: the pen's nominal width. */
    synchronized float strokeWidthPx() {
        return Math.max(1f, Math.min(config.width, 12f));
    }

    /** The reader opened: strokes come from evdev from now on. */
    synchronized void readerStarted(PenAxes axes) {
        readerLive = true;
        android.util.DisplayMetrics dm = getResources().getDisplayMetrics();
        // Keep a fitted map across pause/resume: the digitizer and panel did not move.
        if (calib == null) calib = new MapCalibrator(axes, dm.widthPixels, dm.heightPixels);
    }

    synchronized String mapInfo() {
        return calib == null ? "no map (MotionEvents draw)" : calib.map().toString();
    }

    synchronized void clear() {
        flushErasing();
        Edit e = new Edit(null);
        for (int i = strokes.size() - 1; i >= 0; i--) { // from the end, so each index is current
            Stroke st = strokes.remove(i);
            if (st != current) e.remember(st, i); // the abandoned live stroke is not restored
        }
        if (!e.removed.isEmpty()) record(e);
        current = null;
        if (liveActive) { // abandon the stroke in progress: its later moves/up are ignored
            live.close();
            live = null;
            liveActive = false;
        }
        if (page != null) page.eraseColor(Color.TRANSPARENT);
        if (layer != null) layer.eraseColor(Color.TRANSPARENT);
        layerDirty.setEmpty();
        layerInUse = false;
        postInvalidate();
    }

    /** Revert the newest action. @return false when there is nothing to undo */
    synchronized boolean undo() {
        flushErasing();
        Edit e = history.pollLast();
        if (e == null) return false;
        if (e.added != null) strokes.remove(e.added);
        for (int i = e.removed.size() - 1; i >= 0; i--) {
            strokes.add(Math.min(e.at.get(i), strokes.size()), e.removed.get(i));
        }
        Log.i(TAG, "undo: " + (e.added != null ? "stroke removed" : e.removed.size() + " stroke(s) restored") + ", " + strokes.size() + " now");
        redrawAll();
        return true;
    }

    /** The finished strokes, for saving (a finished stroke's points never change again). */
    synchronized List<Stroke> finishedStrokes() {
        List<Stroke> out = new ArrayList<>(strokes);
        out.remove(current);
        return out;
    }

    /** Replace the page with saved strokes (no undo history across a restart). */
    synchronized void restore(List<Stroke> saved) {
        strokes.clear();
        strokes.addAll(saved);
        history.clear();
        redrawAll();
    }

    synchronized boolean canUndo() {
        return !history.isEmpty();
    }

    /**
     * An erasing contact still in progress records what it removed so far as its own edit, so a
     * clear/undo arriving mid-contact sees a history whose indices match the current list.
     */
    private void flushErasing() {
        if (erasing == null || erasing.removed.isEmpty()) return;
        record(erasing);
        erasing = new Edit(null);
    }

    private void record(Edit e) {
        history.addLast(e);
        while (history.size() > UNDO_DEPTH) history.removeFirst();
    }

    @Override protected synchronized void onSizeChanged(int w, int h, int ow, int oh) {
        if (w == 0 || h == 0) return;
        page = Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888);
        pageCanvas = new Canvas(page);
        layer = Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888);
        layerCanvas = new Canvas(layer);
        getLocationOnScreen(onScreen);
        redrawAll();
    }

    @Override protected synchronized void onDraw(Canvas c) {
        if (page != null) c.drawBitmap(page, 0, 0, null);
        if (layerInUse) c.drawBitmap(layer, 0, 0, layerPaint);
    }

    // ---- evdev path (reader thread) ----

    @Override public void onPen(int action, int tool, int x, int y, float pressure, int tiltX, int tiltY,
                                boolean side, long timeUs) {
        events++;
        long now = SystemClock.uptimeMillis();
        synchronized (this) {
            if (calib != null) calib.rawContact(action, x, y, now);
            try {
                switch (action) {
                    case PenReader.DOWN: begin(true, x, y, pressure, tiltX, tiltY, timeUs, tool == PenReader.TOOL_ERASER); break;
                    case PenReader.MOVE: extend(x, y, pressure, tiltX, tiltY, timeUs); break;
                    case PenReader.UP:
                    case PenReader.FORCED_UP: end(x, y, pressure, tiltX, tiltY, timeUs); break;
                    default: break;
                }
            } catch (RuntimeException e) {
                // JNI reports a listener exception only to stderr, which Android drops: log it here.
                Log.e(TAG, "pen event " + name(action) + " failed", e);
            }
        }
        if (action != PenReader.HOVER) {
            lastX = x; lastY = y; lastPressure = pressure; lastTiltX = tiltX; lastTiltY = tiltY;
            lastAction = action;
        }
    }

    /** The last non-hover pen event, for the status panel. */
    String lastText() {
        if (lastAction < 0) return "none yet";
        return name(lastAction) + " raw " + lastX + "," + lastY
            + String.format(java.util.Locale.ROOT, " p=%.2f tilt %d,%d", lastPressure, lastTiltX, lastTiltY);
    }

    private static String name(int a) {
        String[] n = {"hover-enter", "hover", "down", "move", "up", "hover-leave", "forced-up"};
        return a >= 0 && a < n.length ? n[a] : "action " + a;
    }

    // ---- MotionEvent path (UI thread): calibration, or drawing when evdev is unavailable ----

    @Override public boolean onTouchEvent(MotionEvent e) {
        int tool = e.getToolType(0);
        if (tool != MotionEvent.TOOL_TYPE_STYLUS && tool != MotionEvent.TOOL_TYPE_ERASER) return false;
        int a = e.getActionMasked();
        synchronized (this) {
            if (a == MotionEvent.ACTION_DOWN) pairWith(PenReader.DOWN, e);
            if (a == MotionEvent.ACTION_UP) pairWith(PenReader.UP, e);
            if (!readerLive) drawFromMotion(e, a);
        }
        return true;
    }

    private void drawFromMotion(MotionEvent e, int a) {
        if (a == MotionEvent.ACTION_DOWN) {
            begin(false, e.getX(), e.getY(), e.getPressure(), 0, 0, e.getEventTime() * 1000,
                e.getToolType(0) == MotionEvent.TOOL_TYPE_ERASER);
            return;
        }
        if (current == null && !erasingContact) return;
        for (int h = 0; h < e.getHistorySize(); h++) {
            extend(e.getHistoricalX(h), e.getHistoricalY(h), e.getHistoricalPressure(h), 0, 0, e.getHistoricalEventTime(h) * 1000);
        }
        if (a == MotionEvent.ACTION_UP || a == MotionEvent.ACTION_CANCEL) end(e.getX(), e.getY(), e.getPressure(), 0, 0, e.getEventTime() * 1000);
        else extend(e.getX(), e.getY(), e.getPressure(), 0, 0, e.getEventTime() * 1000);
    }

    /** Feed the calibration; a full redraw holds the lock the pen thread needs: only when the map moved. */
    private void pairWith(int action, MotionEvent e) {
        if (calib != null && calib.screenContact(action, e.getRawX(), e.getRawY(), e.getEventTime())) redrawAll();
    }

    // ---- live stroke (callers hold `this`) ----

    private void begin(boolean raw, float x, float y, float p, float tx, float ty, long timeUs, boolean eraserEnd) {
        erasingContact = eraseMode || eraserEnd;
        if (erasingContact) {
            eraseRaw = raw;
            eraseLast = null;
            erasing = new Edit(null);
            eraseTo(x, y);
            return;
        }
        current = new Stroke(raw, kind, config);
        strokes.add(current);
        // A down while the engine still holds a stroke means an up was lost: start a fresh engine
        // rather than letting every later down() fail.
        if (liveActive) Log.w(TAG, "pen-down without pen-up: resetting the stroke engine");
        if (live == null || liveActive || live.kind != kind || live.config != config) {
            if (live != null) live.close();
            live = new StrokeEngine(kind, config);
        }
        current.pts.add(new double[]{x, y, p, tx, ty, timeUs});
        if (!drawable(current)) return;
        layerInUse = kind.output == PenKind.Output.TRANSLUCENT;
        liveActive = true;
        paint(live.down(viewX(current, x, y), viewY(current, x, y), p, tx, ty, timeUs));
    }

    private void extend(float x, float y, float p, float tx, float ty, long timeUs) {
        if (erasingContact) {
            eraseTo(x, y);
            return;
        }
        if (current == null) return;
        current.pts.add(new double[]{x, y, p, tx, ty, timeUs});
        if (drawable(current) && liveActive) paint(live.move(viewX(current, x, y), viewY(current, x, y), p, tx, ty, timeUs));
    }

    private void end(float x, float y, float p, float tx, float ty, long timeUs) {
        if (erasingContact) {
            eraseTo(x, y);
            erasingContact = false;
            if (erasing != null && !erasing.removed.isEmpty()) record(erasing);
            erasing = null;
            post(onStrokeDone);
            return;
        }
        if (current == null) return;
        current.pts.add(new double[]{x, y, p, tx, ty, timeUs});
        if (drawable(current) && liveActive) {
            liveActive = false;
            paint(live.up(viewX(current, x, y), viewY(current, x, y), p, tx, ty, timeUs));
            commitLayer();
        }
        Stroke done = current;
        current = null;
        if (!snapShapes || snap(done)) record(new Edit(done));
        post(onStrokeDone);
    }

    // ---- hold-to-snap shapes (callers hold `this`) ----

    /**
     * If the stroke ended with the pen at rest, recognise it: a shape replaces the stroke's points (so save and
     * undo keep working), a scratch-out erases what it covers, anything else stays as drawn.
     * @return false when the stroke was consumed (a scratch-out) and must not be recorded as added
     */
    private boolean snap(Stroke s) {
        if (!drawable(s)) return true;
        int n = s.pts.size();
        float[] vx = new float[n], vy = new float[n];
        double[] t = new double[n];
        for (int i = 0; i < n; i++) {
            double[] p = s.pts.get(i);
            vx[i] = viewX(s, (float) p[0], (float) p[1]);
            vy[i] = viewY(s, (float) p[0], (float) p[1]);
            t[i] = p[5];
        }
        int k = ShapeSnap.holdStart(vx, vy, t, ShapeSnap.HOLD_BOX_PX, ShapeSnap.HOLD_US);
        if (k < 0) return true;
        // Recognise in the stroke's own coordinates (raw or view), without the resting tail.
        double t0 = s.pts.get(0)[5];
        float[] xyt = new float[3 * (k + 1)];
        double[] pressures = new double[k + 1];
        for (int i = 0; i <= k; i++) {
            double[] p = s.pts.get(i);
            xyt[3 * i] = (float) p[0];
            xyt[3 * i + 1] = (float) p[1];
            xyt[3 * i + 2] = (float) ((p[5] - t0) / 1000);
            pressures[i] = p[2];
        }
        long startNs = System.nanoTime();
        Shape shape = ShapeRecognizer.recognize(new float[][]{xyt});
        Log.i(TAG, "snap: " + shape.status + (shape.best() != null ? " " + shape.best().shapeClass : "")
            + " in " + (System.nanoTime() - startNs) / 1000 + " µs");
        RectF dirty = inkBounds(s);
        switch (shape.status) {
            case SHAPE: {
                java.util.Arrays.sort(pressures);
                List<double[]> outline = ShapeSnap.trace(shape.best(), pressures[pressures.length / 2], t0, s.pts.get(k)[5]);
                if (outline.size() < 2) return true;
                s.pts.clear();
                s.pts.addAll(outline);
                s.viewBounds = null;
                dirty.union(inkBounds(s));
                redrawRegion(dirty);
                return true;
            }
            case SCRATCH_OUT: {
                strokes.remove(s);
                float[] hull = new float[shape.hull.length];
                for (int i = 0; i < hull.length; i += 2) {
                    hull[i] = viewX(s, shape.hull[i], shape.hull[i + 1]);
                    hull[i + 1] = viewY(s, shape.hull[i], shape.hull[i + 1]);
                }
                Edit e = new Edit(null);
                for (int i = 0; i < strokes.size(); ) {
                    Stroke st = strokes.get(i);
                    if (drawable(st) && covered(st, hull)) {
                        strokes.remove(i);
                        e.remember(st, i);
                        dirty.union(inkBounds(st));
                    } else {
                        i++;
                    }
                }
                if (!e.removed.isEmpty()) record(e);
                Log.i(TAG, "scratch-out erased " + e.removed.size() + " stroke(s)");
                redrawRegion(dirty);
                return false;
            }
            default:
                return true;
        }
    }

    private boolean covered(Stroke st, float[] hull) {
        int in = 0;
        for (double[] p : st.pts) {
            if (ShapeSnap.inside(hull, viewX(st, (float) p[0], (float) p[1]), viewY(st, (float) p[0], (float) p[1]))) in++;
        }
        return in >= ShapeSnap.SCRATCH_COVER * st.pts.size();
    }

    // ---- stroke eraser (callers hold `this`) ----

    /** Remove every stroke the eraser path touches since its last point; redraw once if any went. */
    private void eraseTo(float x, float y) {
        if (eraseRaw && calib == null) return;
        float vx = eraseRaw ? calib.map().screenX((int) x, (int) y) - onScreen[0] : x;
        float vy = eraseRaw ? calib.map().screenY((int) x, (int) y) - onScreen[1] : y;
        float ax = eraseLast == null ? vx : eraseLast[0], ay = eraseLast == null ? vy : eraseLast[1];
        eraseLast = new float[]{vx, vy};
        int gone = 0;
        RectF dirty = new RectF();
        for (int i = 0; i < strokes.size(); ) {
            Stroke st = strokes.get(i);
            if (st != current && drawable(st) && touches(st, ax, ay, vx, vy)) {
                strokes.remove(i);
                if (erasing != null) erasing.remember(st, i);
                dirty.union(inkBounds(st));
                gone++;
            } else {
                i++;
            }
        }
        if (gone > 0) {
            Log.i(TAG, "erased " + gone + " stroke(s), " + strokes.size() + " left");
            redrawRegion(dirty);
        }
    }

    private boolean touches(Stroke s, float ax, float ay, float bx, float by) {
        float reach = ERASER_RADIUS_PX + s.config.width / 2;
        RectF b = viewBounds(s);
        if (Math.max(ax, bx) < b.left - reach || Math.min(ax, bx) > b.right + reach
            || Math.max(ay, by) < b.top - reach || Math.min(ay, by) > b.bottom + reach) return false;
        for (double[] p : s.pts) {
            float px = viewX(s, (float) p[0], (float) p[1]), py = viewY(s, (float) p[0], (float) p[1]);
            if (segmentDistance(px, py, ax, ay, bx, by) <= reach) return true;
        }
        return false;
    }

    /** Cached per map: raw strokes move when the map is refitted. */
    private RectF viewBounds(Stroke s) {
        ScreenMap m = s.raw ? calib.map() : null;
        if (s.viewBounds != null && s.boundsMap == m) return s.viewBounds;
        RectF b = null;
        for (double[] p : s.pts) {
            float px = viewX(s, (float) p[0], (float) p[1]), py = viewY(s, (float) p[0], (float) p[1]);
            if (b == null) b = new RectF(px, py, px, py); else b.union(px, py);
        }
        s.viewBounds = b == null ? new RectF() : b;
        s.boundsMap = m;
        return s.viewBounds;
    }

    /** Where a stroke's ink can reach: its point bounds grown by the pen width (view px). */
    private RectF inkBounds(Stroke s) {
        RectF b = new RectF(viewBounds(s));
        float pad = s.config.width + 2;
        b.inset(-pad, -pad);
        return b;
    }

    private static float segmentDistance(float px, float py, float ax, float ay, float bx, float by) {
        float dx = bx - ax, dy = by - ay, len2 = dx * dx + dy * dy;
        float t = len2 == 0 ? 0 : Math.max(0, Math.min(1, ((px - ax) * dx + (py - ay) * dy) / len2));
        return (float) Math.hypot(px - (ax + t * dx), py - (ay + t * dy));
    }

    private boolean drawable(Stroke s) {
        return pageCanvas != null && !(s.raw && calib == null);
    }

    /** New ink of the live stroke: the page, or the translucent stroke's layer. */
    private void paint(Ink ink) {
        if (ink.bounds == null) return;
        painter.draw(layerInUse ? layerCanvas : pageCanvas, ink);
        RectF b = ink.bounds;
        if (layerInUse) layerDirty.union(b);
        postInvalidate((int) b.left - 1, (int) b.top - 1, (int) Math.ceil(b.right) + 1, (int) Math.ceil(b.bottom) + 1);
    }

    /** A finished translucent stroke goes onto the page at the layer alpha, once. */
    private void commitLayer() {
        if (!layerInUse) return;
        Rect r = compositeLayer(layerDirty);
        layerDirty.setEmpty();
        layerInUse = false;
        if (r != null) postInvalidate(r.left, r.top, r.right, r.bottom);
    }

    /**
     * Layer → page at the marker alpha, then clear: only within {@code b}. A full-screen blit per
     * marker stroke is 18 MB of pixels on this panel. @return the area touched, or null
     */
    private Rect compositeLayer(RectF b) {
        if (b.isEmpty()) return null;
        Rect r = new Rect();
        b.roundOut(r);
        r.inset(-2, -2); // antialiased edges reach past the polygon bounds
        if (!r.intersect(0, 0, layer.getWidth(), layer.getHeight())) return null;
        pageCanvas.drawBitmap(layer, r, r, layerPaint);
        layerCanvas.save();
        layerCanvas.clipRect(r);
        layerCanvas.drawColor(Color.TRANSPARENT, PorterDuff.Mode.CLEAR);
        layerCanvas.restore();
        return r;
    }

    // ---- redraw (callers hold `this`) ----

    /**
     * Repaint only {@code area}: clear it and re-render the strokes that reach into it, clipped.
     * An eraser hit costs the strokes nearby instead of the whole page.
     */
    private void redrawRegion(RectF area) {
        if (page == null || area.isEmpty()) return;
        Rect r = new Rect();
        area.roundOut(r);
        if (!r.intersect(0, 0, page.getWidth(), page.getHeight())) return;
        RectF rf = new RectF(r);
        pageCanvas.save();
        pageCanvas.clipRect(r);
        pageCanvas.drawColor(Color.TRANSPARENT, PorterDuff.Mode.CLEAR);
        int n = 0;
        for (Stroke s : strokes) {
            if (s == current || !drawable(s) || !RectF.intersects(inkBounds(s), rf)) continue;
            Ink ink = render(s);
            if (s.kind.output == PenKind.Output.TRANSLUCENT) {
                painter.draw(layerCanvas, ink);
                if (ink.bounds != null) compositeLayer(ink.bounds); // page clip limits the blit
            } else {
                painter.draw(pageCanvas, ink);
            }
            n++;
        }
        pageCanvas.restore();
        Log.i(TAG, "redrew " + n + " of " + strokes.size() + " strokes in " + r.toShortString());
        postInvalidate(r.left, r.top, r.right, r.bottom);
    }

    /**
     * Repaint every stroke from its recorded points (new size or map). The stroke in progress is
     * re-rendered too, so its early ink survives a refit; the live engine then continues it.
     */
    private void redrawAll() {
        if (page == null) return;
        page.eraseColor(Color.TRANSPARENT);
        layer.eraseColor(Color.TRANSPARENT);
        layerDirty.setEmpty();
        for (Stroke s : strokes) {
            if (s == current || !drawable(s)) continue;
            Ink ink = render(s);
            if (s.kind.output == PenKind.Output.TRANSLUCENT) {
                painter.draw(layerCanvas, ink);
                if (ink.bounds != null) compositeLayer(ink.bounds);
            } else {
                painter.draw(pageCanvas, ink);
            }
        }
        if (current != null && liveActive && drawable(current)) {
            // layerInUse already says where the live stroke goes; the layer stays uncomposited.
            Ink ink = render(current);
            painter.draw(layerInUse ? layerCanvas : pageCanvas, ink);
            if (layerInUse && ink.bounds != null) layerDirty.union(ink.bounds);
        }
        postInvalidate();
    }

    /** A recorded stroke through the redraw engine, in view pixels. */
    private Ink render(Stroke s) {
        if (redraw == null || redraw.kind != s.kind || redraw.config != s.config) {
            if (redraw != null) redraw.close();
            redraw = new StrokeEngine(s.kind, s.config);
        }
        float[] flat = new float[s.pts.size() * 6];
        double t0 = s.pts.get(0)[5];
        for (int i = 0; i < s.pts.size(); i++) {
            double[] p = s.pts.get(i);
            flat[6 * i] = viewX(s, (float) p[0], (float) p[1]);
            flat[6 * i + 1] = viewY(s, (float) p[0], (float) p[1]);
            flat[6 * i + 2] = (float) p[2];
            flat[6 * i + 3] = (float) p[3];
            flat[6 * i + 4] = (float) p[4];
            flat[6 * i + 5] = (float) ((p[5] - t0) / 1000);
        }
        return redraw.render(flat);
    }

    private float viewX(Stroke s, float x, float y) {
        return s.raw ? calib.map().screenX((int) x, (int) y) - onScreen[0] : x;
    }

    private float viewY(Stroke s, float x, float y) {
        return s.raw ? calib.map().screenY((int) x, (int) y) - onScreen[1] : y;
    }
}
