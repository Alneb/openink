package org.openink.compat.pen;

import android.graphics.Rect;
import android.graphics.RectF;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import android.util.DisplayMetrics;
import android.util.Log;
import android.view.View;
import android.view.ViewTreeObserver;
import java.io.IOException;
import java.lang.ref.WeakReference;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import org.openink.compat.epd.EpdController;
import org.openink.pen.MapCalibrator;
import org.openink.pen.PenAxes;
import org.openink.pen.PenReader;
import org.openink.pen.ScreenMap;

/**
 * Onyx's {@code com.onyx.android.sdk.pen.RawInputReader} over openink: the stylus from our evdev
 * reader (no libonyx_pen_touch_reader.so), mapped to host-view pixels, assembled into strokes and
 * delivered to a {@link RawInputCallback}. Point filtering follows SurfaceFlinger's
 * {@code PenManager::inValidRegion}: every point must lie in a limit rect (in single-region mode,
 * in the one the stroke started in) and outside every exclude rect.
 *
 * <p>Pen-down/up also go to the open refresh engine ({@link EpdController#engine()}), which holds
 * explicit refreshes while the pen writes.
 *
 * <p>Not mirrored: {@code getEventBusHolder} (Onyx's internal event bus) and the native internals
 * ({@code nativeRawReader}, {@code run}, {@code onNext}).
 */
public class RawInputReader implements PenReader.Listener {
    private static final String TAG = "openink-compat";
    /** PenConstant.DEFAULT_STROKE_WIDTH and the SDK's other defaults. */
    static final float DEFAULT_STROKE_WIDTH = 3f;
    static final int DEFAULT_PEN_UP_REFRESH_MS = 500;
    private static volatile boolean debug;

    /** Whether region/width setters are also pushed to SurfaceFlinger, as the SDK's class does. */
    private final boolean syncSurfaceFlinger;
    private final Handler main = new Handler(Looper.getMainLooper());
    private final Runnable penUpRefresh = this::firePenUpRefresh;
    private final ViewTreeObserver.OnGlobalLayoutListener layoutListener = this::trackViewPosition;

    // Settings: UI thread writes, the pen thread reads.
    private volatile WeakReference<View> host = new WeakReference<>(null);
    private volatile RawInputCallback callback;
    private volatile List<Rect> limit = Collections.emptyList(), exclude = Collections.emptyList();
    private volatile boolean singleRegion, drawing, sideButtonErases, filterRepeatMovePoint = true, postInputEvent;
    private volatile boolean penUpRefreshEnabled = true, penInput = true;
    private volatile int penUpRefreshMs = DEFAULT_PEN_UP_REFRESH_MS, strokeColor = 0xff000000;
    private volatile float strokeWidth = DEFAULT_STROKE_WIDTH;

    // Runtime.
    private PenReader reader;
    private volatile MapCalibrator calib;
    private volatile PenAxes axes;
    private volatile int viewLeft, viewTop;

    // The stroke in progress (pen thread, or the caller of onTouchPointReceived).
    private TouchPointList stroke;
    private volatile boolean erasing;
    private int strokeRegion = -1;
    private TouchPoint last;
    private final RectF dirty = new RectF(); // handed to the main thread for the pen-up refresh

    public RawInputReader() {
        this(true);
    }

    /** TouchHelper passes false: its InkSession owns the SurfaceFlinger side. */
    RawInputReader(boolean syncSurfaceFlinger) {
        this.syncSurfaceFlinger = syncSurfaceFlinger;
    }

    public static void debugLog(boolean on) {
        debug = on;
    }

    // ---- lifecycle (UI thread) ----

    /** Open the pen device (if needed); input flows after {@link #resume()}. */
    public void start() {
        drawing = false;
        if (reader != null) return;
        View v = getHostView();
        try {
            reader = PenReader.start(this);
            axes = reader.axes();
        } catch (IOException | RuntimeException | UnsatisfiedLinkError e) {
            Log.e(TAG, "RawInputReader.start: no pen input", e);
            reader = null;
            return;
        }
        if (v != null) {
            DisplayMetrics dm = v.getResources().getDisplayMetrics();
            if (calib == null) calib = new MapCalibrator(axes, dm.widthPixels, dm.heightPixels);
            v.getViewTreeObserver().addOnGlobalLayoutListener(layoutListener);
            trackViewPosition();
        }
        if (debug) Log.d(TAG, "RawInputReader start: " + reader.deviceInfo());
    }

    public void resume() {
        drawing = true;
    }

    public void pause() {
        drawing = false;
    }

    /** Close the device and reset to the SDK's defaults (callback, width, colour). */
    public void quit() {
        if (reader != null) {
            reader.close(); // joins the pen thread: nothing below races with it
            reader = null;
        }
        stroke = null;
        main.removeCallbacks(penUpRefresh);
        synchronized (dirty) {
            dirty.setEmpty();
        }
        View v = getHostView();
        if (v != null) v.getViewTreeObserver().removeOnGlobalLayoutListener(layoutListener);
        callback = null;
        drawing = false;
        strokeWidth = DEFAULT_STROKE_WIDTH;
        strokeColor = 0xff000000;
    }

    public boolean isFdValid() {
        return reader != null;
    }

    public void printTouchInfo() {
        Log.i(TAG, "RawInputReader: " + (reader == null ? "closed" : reader.deviceInfo()) + ", drawing=" + drawing
                + ", limit=" + limit + ", exclude=" + exclude + ", single=" + singleRegion + ", map=" + (calib == null ? null : calib.map()));
    }

    // ---- settings ----

    public void setHostView(View view) {
        host = new WeakReference<>(view);
    }

    public View getHostView() {
        return host.get();
    }

    public void setRawInputCallback(RawInputCallback cb) {
        callback = cb;
    }

    public void setLimitRect(Rect rect) {
        if (rect != null) setLimitRect(Collections.singletonList(rect));
    }

    /** Host-view pixels; null or empty leaves the current limit (as the SDK does). */
    public void setLimitRect(List<Rect> rects) {
        if (rects == null || rects.isEmpty()) return;
        limit = copy(rects);
        if (syncSurfaceFlinger) sf(() -> EpdController.setScreenHandWritingRegionLimit(getHostView(), rects.toArray(new Rect[0])));
    }

    public void setExcludeRect(List<Rect> rects) {
        if (rects == null || rects.isEmpty()) return;
        exclude = copy(rects);
        if (syncSurfaceFlinger) sf(() -> EpdController.setScreenHandWritingRegionExclude(getHostView(), rects.toArray(new Rect[0])));
    }

    /** Replace both lists, empty allowed (TouchHelper's semantics). */
    void setRegions(List<Rect> limits, List<Rect> excludes) {
        limit = copy(limits);
        exclude = copy(excludes);
    }

    /** A stroke stays in the limit rect it started in (SF region mode 1). */
    public void setSingleRegionMode() {
        singleRegion = true;
        if (syncSurfaceFlinger) sf(() -> EpdController.setScreenHandWritingRegionMode(getHostView(), 1));
    }

    /** A stroke may cross into any limit rect (SF region mode 0, the default). */
    public void setMultiRegionMode() {
        singleRegion = false;
        if (syncSurfaceFlinger) sf(() -> EpdController.setScreenHandWritingRegionMode(getHostView(), 0));
    }

    /** Off: the stylus is ignored (TouchHelper.onlyEnableFingerTouch); finger points still arrive. */
    void setPenInput(boolean on) {
        penInput = on;
    }

    boolean isSingleRegionMode() {
        return singleRegion;
    }

    public void setStrokeWidth(float w) {
        strokeWidth = w;
        if (syncSurfaceFlinger) sf(() -> EpdController.setStrokeWidth(w));
    }

    public void setStrokeColor(int argb) {
        strokeColor = argb;
    }

    int getStrokeColor() {
        return strokeColor;
    }

    float getStrokeWidth() {
        return strokeWidth;
    }

    /** Side (barrel) button held: the stroke erases. */
    public void enableSideBtnErase(boolean on) {
        sideButtonErases = on;
    }

    public boolean isErasing() {
        return erasing;
    }

    /** Drop move points at the same position as the previous one (default on). */
    public void setFilterRepeatMovePoint(boolean on) {
        filterRepeatMovePoint = on;
    }

    public boolean isFilterRepeatMovePoint() {
        return filterRepeatMovePoint;
    }

    public void setPenUpRefreshEnabled(boolean on) {
        penUpRefreshEnabled = on;
        if (!on) main.removeCallbacks(penUpRefresh);
    }

    public boolean isPenUpRefreshEnabled() {
        return penUpRefreshEnabled;
    }

    public void setPenUpRefreshTimeMs(int ms) {
        penUpRefreshMs = Math.max(0, ms);
    }

    /** Kept for API parity: the SDK's flag for also posting input events; no effect here. */
    public void setPostInputEvent(boolean on) {
        postInputEvent = on;
    }

    /** The points of the stroke in progress, handed over; collection restarts empty. */
    public TouchPointList detachTouchPointList() {
        TouchPointList s = stroke;
        if (s != null) stroke = new TouchPointList();
        return s;
    }

    /** {@link TouchPoint#pressure} runs 0..this (raw digitizer units); 0 before start. */
    float maxPressure() {
        PenAxes a = axes;
        return a == null ? 0 : a.pressureMax;
    }

    MapCalibrator calibrator() {
        return calib;
    }

    // ---- input ----

    /** From our evdev reader (pen thread): raw units → host-view pixels, then {@link #handle}. */
    @Override public void onPen(int action, int tool, int x, int y, float pressure, int tiltX, int tiltY,
                                boolean side, long timeUs) {
        MapCalibrator c = calib;
        PenAxes a = axes;
        if (c == null || a == null || !penInput) return;
        c.rawContact(action, x, y, SystemClock.uptimeMillis());
        ScreenMap m = c.map();
        TouchPoint p = new TouchPoint(m.screenX(x, y) - viewLeft, m.screenY(x, y) - viewTop,
                pressure * a.pressureMax, 0, tiltX, tiltY, timeUs / 1000);
        handle(action, p, tool == PenReader.TOOL_ERASER || (side && sideButtonErases));
    }

    /**
     * The SDK's native entry, kept public: a point already in host-view pixels. {@code action} uses
     * the native codes: 0 down, 1 move, 2 up, 3 up (eraser), 5 hover, 6 forced release.
     */
    public void onTouchPointReceived(float x, float y, int pressure, int tiltX, int tiltY, boolean erase,
                                     boolean unused1, boolean unused2, int action, long timestamp) {
        TouchPoint p = new TouchPoint(x, y, pressure, 0, tiltX, tiltY, timestamp);
        int a;
        switch (action) {
            case 0: a = PenReader.DOWN; break;
            case 1: a = PenReader.MOVE; break;
            case 2: case 3: a = PenReader.UP; break;
            case 5: a = PenReader.HOVER_ENTER; break;
            case 6: a = PenReader.FORCED_UP; break;
            default: return;
        }
        handle(a, p, erase);
    }

    private void handle(int action, TouchPoint p, boolean erase) {
        RawInputCallback cb = callback;
        if (cb == null) return;
        try {
            switch (action) {
                case PenReader.HOVER_ENTER: cb.onPenActive(p); break;
                case PenReader.DOWN: begin(cb, p, erase); break;
                case PenReader.MOVE: move(cb, p); break;
                case PenReader.UP:
                case PenReader.FORCED_UP: end(cb, p); break;
                default: break;
            }
        } catch (RuntimeException e) {
            // The JNI side only prints listener exceptions to stderr, which Android drops.
            Log.e(TAG, "RawInputCallback threw", e);
        }
    }

    private void begin(RawInputCallback cb, TouchPoint p, boolean erase) {
        if (!drawing) return;
        int region = limitIndex(p);
        if (region == -2 || excluded(p)) return;
        main.removeCallbacks(penUpRefresh);
        stroke = new TouchPointList();
        stroke.add(p);
        strokeRegion = region;
        last = p;
        erasing = erase;
        main.post(() -> engine(true));
        if (erase) cb.onBeginRawErasing(false, p); else cb.onBeginRawDrawing(false, p);
    }

    private void move(RawInputCallback cb, TouchPoint p) {
        if (stroke == null || !valid(p)) return;
        if (filterRepeatMovePoint && last != null && last.x == p.x && last.y == p.y) return;
        last = p;
        stroke.add(p);
        if (erasing) cb.onRawErasingTouchPointMoveReceived(p); else cb.onRawDrawingTouchPointMoveReceived(p);
    }

    private void end(RawInputCallback cb, TouchPoint p) {
        if (stroke == null) return;
        TouchPointList done = stroke;
        stroke = null;
        done.add(p);
        boolean wasErasing = erasing;
        erasing = false;
        if (wasErasing) {
            cb.onEndRawErasing(false, p);
            cb.onRawErasingTouchPointListReceived(done);
        } else {
            cb.onEndRawDrawing(false, p);
            cb.onRawDrawingTouchPointListReceived(done);
        }
        RectF b = TouchPointList.getBoundingRect(done.getPoints());
        b.inset(-strokeWidth - 1, -strokeWidth - 1); // ink reaches past the centre line; a tap has zero area
        synchronized (dirty) {
            dirty.union(b);
        }
        main.post(() -> engine(false));
        main.removeCallbacks(penUpRefresh);
        main.postDelayed(penUpRefresh, penUpRefreshMs);
    }

    /** Index of the limit rect containing p; -1 when there is no limit (whole view); -2 when outside all. */
    private int limitIndex(TouchPoint p) {
        List<Rect> lim = limit;
        if (lim.isEmpty()) return -1;
        for (int i = 0; i < lim.size(); i++) if (lim.get(i).contains((int) p.x, (int) p.y)) return i;
        return -2;
    }

    private boolean excluded(TouchPoint p) {
        for (Rect r : exclude) if (r.contains((int) p.x, (int) p.y)) return true;
        return false;
    }

    /** SF's rule per point: inside a limit rect (the starting one in single-region mode), not excluded. */
    private boolean valid(TouchPoint p) {
        int i = limitIndex(p);
        if (i == -2 || excluded(p)) return false;
        return !singleRegion || strokeRegion < 0 || i == strokeRegion;
    }

    // ---- main thread ----

    /** Tell the refresh engine; the app's own pen-up redraw follows through onPenUpRefresh. */
    private void engine(boolean down) {
        try {
            if (down) EpdController.engine().penDown(); else EpdController.engine().penUp();
        } catch (RuntimeException | UnsatisfiedLinkError e) {
            if (debug) Log.d(TAG, "refresh engine unavailable", e);
        }
    }

    private void firePenUpRefresh() {
        RectF r;
        synchronized (dirty) {
            r = new RectF(dirty);
            dirty.setEmpty();
        }
        RawInputCallback cb = callback;
        if (cb != null && penUpRefreshEnabled && !r.isEmpty()) cb.onPenUpRefresh(r);
    }

    void trackViewPosition() {
        View v = getHostView();
        if (v == null) return;
        int[] at = new int[2];
        v.getLocationOnScreen(at);
        viewLeft = at[0];
        viewTop = at[1];
    }

    /** A SurfaceFlinger push that may be unavailable (framework missing or blocked): log, never throw. */
    private static void sf(Runnable r) {
        try {
            r.run();
        } catch (RuntimeException e) {
            Log.w(TAG, "SurfaceFlinger setting not applied: " + e.getMessage());
        }
    }

    static List<Rect> copy(List<Rect> rects) {
        if (rects == null || rects.isEmpty()) return Collections.emptyList();
        List<Rect> out = new ArrayList<>(rects.size());
        for (Rect r : rects) out.add(new Rect(r));
        return Collections.unmodifiableList(out);
    }
}
