package org.openink.compat.pen;

import android.graphics.Rect;
import android.util.Log;
import android.view.MotionEvent;
import android.view.View;
import android.view.ViewTreeObserver;
import java.util.Collections;
import java.util.List;
import org.openink.backend.OnyxFramework;
import org.openink.backend.OnyxFrameworkBackend;
import org.openink.compat.epd.EpdController;
import org.openink.pen.InkSession;
import org.openink.pen.MapCalibrator;
import org.openink.pen.PenReader;
import org.openink.pen.StrokeStyle;

/**
 * Onyx's {@code com.onyx.android.sdk.pen.TouchHelper} over openink: code written against the Onyx
 * SDK ports by changing imports. Input comes from {@link RawInputReader} (our evdev reader, no
 * Onyx library); instant ink goes through the stock SurfaceFlinger ({@link InkSession}) when the
 * framework lets this app reach it, and is simply absent otherwise: the callbacks still arrive, so
 * the app draws the stroke itself.
 *
 * <p>Not mirrored: the event bus ({@code register}, {@code unregister}, {@code getEventBusHolder},
 * Onyx-internal types) and the {@code create(View, int|boolean, ...)} render-set flags.
 */
public final class TouchHelper {
    private static final String TAG = "openink-compat";
    /** TouchHelper's default for setEraserRawDrawingEnabled(false, 5). */
    private static final int ERASER_DEFAULT_ARG = 5;

    private View view;
    private RawInputCallback callback;
    private final RawInputReader reader = new RawInputReader(false); // InkSession owns the SF side
    private final ViewTreeObserver.OnScrollChangedListener scrollListener = this::onHostMoved;

    // Settings (UI thread).
    private List<Rect> limit = Collections.emptyList(), exclude = Collections.emptyList();
    private int strokeStyle = StrokeStyle.PENCIL.value;
    private boolean created, inputEnabled = true, renderEnabled = true;
    private boolean touchListenerEnabled = true, scrollListenerEnabled;
    private boolean fingerTouch, onlyFingerTouch, fingerPressure;
    private float fingerTouchPressure = 1f;

    private InkSession session;

    private TouchHelper(View view, RawInputCallback callback) {
        bind(view, callback);
    }

    public static TouchHelper create(View view, RawInputCallback callback) {
        if (view == null) throw new IllegalArgumentException("hostView should not be null!");
        return new TouchHelper(view, callback);
    }

    public View getHostView() {
        return view;
    }

    /** Move to another host view (closed first if open; reopen with {@link #openRawDrawing}). */
    public TouchHelper bindHostView(View v, RawInputCallback cb) {
        if (v == null) throw new IllegalArgumentException("hostView should not be null!");
        if (created) closeRawDrawing();
        session = null; // the session is tied to its view
        bind(v, cb);
        return this;
    }

    private void bind(View v, RawInputCallback cb) {
        view = v;
        callback = cb;
        reader.setHostView(v);
        reader.setRawInputCallback(cb);
    }

    public TouchHelper debugLog(boolean on) {
        RawInputReader.debugLog(on);
        return this;
    }

    public void printTouchInfo() {
        reader.printTouchInfo();
    }

    // ---- lifecycle (UI thread) ----

    /** Start pen input (and instant ink, when available). Failures are logged, not thrown, as in the Onyx SDK. */
    public TouchHelper openRawDrawing() {
        if (created) return this;
        reader.setRawInputCallback(callback); // quit() cleared it
        reader.setRegions(limit, exclude);
        reader.start();
        if (!reader.isFdValid()) return this;
        created = true;
        reader.setPenInput(!onlyFingerTouch);
        if (inputEnabled) reader.resume();
        if (touchListenerEnabled) view.setOnTouchListener((v, e) -> onTouchEvent(e));
        if (scrollListenerEnabled) view.getViewTreeObserver().addOnScrollChangedListener(scrollListener);
        if (view.isLaidOut()) openInk(); else view.post(this::openInk);
        return this;
    }

    public void closeRawDrawing() {
        if (!created) return;
        created = false;
        reader.quit();
        view.setOnTouchListener(null);
        view.getViewTreeObserver().removeOnScrollChangedListener(scrollListener);
        if (session != null) closeInk();
    }

    public void restartRawDrawing() {
        closeRawDrawing();
        openRawDrawing();
    }

    public boolean isRawDrawingCreated() {
        return created;
    }

    public boolean isRawDrawingInputEnabled() {
        return inputEnabled;
    }

    public boolean isRawDrawingRenderEnabled() {
        return renderEnabled;
    }

    // ---- settings ----

    public TouchHelper setRawDrawingEnabled(boolean enabled) {
        setRawInputReaderEnable(enabled);
        return setRawDrawingRenderEnabled(enabled);
    }

    /** Both switches at once, only once created (the SDK's guard). */
    public TouchHelper forceSetRawDrawingEnabled(boolean enabled) {
        return created ? setRawDrawingEnabled(enabled) : this;
    }

    public TouchHelper setRawInputReaderEnable(boolean enabled) {
        inputEnabled = enabled;
        if (!created) return this;
        if (enabled) reader.resume(); else reader.pause();
        return this;
    }

    /** Off while the app redraws its own content; on again shows that content over the quick ink. */
    public TouchHelper setRawDrawingRenderEnabled(boolean enabled) {
        renderEnabled = enabled;
        if (session == null || !session.isOpen()) return this;
        try {
            if (enabled && !session.isRendering()) {
                session.post();
                session.resume();
            } else if (!enabled && session.isRendering()) {
                session.pause();
            }
        } catch (RuntimeException e) {
            Log.e(TAG, "setRawDrawingRenderEnabled(" + enabled + ")", e);
        }
        return this;
    }

    public TouchHelper setStrokeWidth(float px) {
        reader.setStrokeWidth(px);
        if (session != null && session.isOpen()) session.setStrokeWidth(px);
        return this;
    }

    public TouchHelper setStrokeColor(int argb) {
        reader.setStrokeColor(argb);
        if (session != null && session.isOpen()) session.setStrokeColor(argb);
        return this;
    }

    public TouchHelper setStrokeStyle(int style) {
        strokeStyle = style;
        if (session != null && session.isOpen()) session.setStrokeStyle(style);
        return this;
    }

    /** SurfaceFlinger draws instant ink for the pen tip. */
    public TouchHelper setBrushRawDrawingEnabled(boolean on) {
        RawInputReader.sf(() -> OnyxFramework.helper("setBrushRawDrawingEnabled", new Class<?>[]{boolean.class}, on));
        return this;
    }

    /** SurfaceFlinger erases instant ink under the eraser; {@code arg} as the SDK passes it (default 5). */
    public TouchHelper setEraserRawDrawingEnabled(boolean on, int arg) {
        RawInputReader.sf(() -> OnyxFramework.helper("setEraserRawDrawingEnabled", new Class<?>[]{boolean.class, int.class}, on, arg));
        return this;
    }

    public void resetPenDefaultRawDrawing() {
        setBrushRawDrawingEnabled(true);
        setEraserRawDrawingEnabled(false, ERASER_DEFAULT_ARG);
    }

    /** Where strokes may start, in host-view pixels; empty = the whole view. */
    public TouchHelper setLimitRect(List<Rect> limits) {
        return setLimitRect(limits, exclude);
    }

    public TouchHelper setLimitRect(Rect limitRect, List<Rect> excludes) {
        return setLimitRect(Collections.singletonList(limitRect), excludes);
    }

    public TouchHelper setLimitRect(List<Rect> limits, List<Rect> excludes) {
        limit = RawInputReader.copy(limits);
        exclude = RawInputReader.copy(excludes);
        reader.setRegions(limit, exclude);
        if (session != null && session.isOpen()) {
            // SF takes the limit at open: reopen to move it.
            closeInk();
            openInk();
        }
        return this;
    }

    public TouchHelper setExcludeRect(List<Rect> excludes) {
        exclude = RawInputReader.copy(excludes);
        reader.setRegions(limit, exclude);
        if (session != null && session.isOpen()) session.setExclude(flat(exclude));
        return this;
    }

    public TouchHelper setSingleRegionMode() {
        reader.setSingleRegionMode();
        RawInputReader.sf(() -> EpdController.setScreenHandWritingRegionMode(view, 1));
        return this;
    }

    public TouchHelper setMultiRegionMode() {
        reader.setMultiRegionMode();
        RawInputReader.sf(() -> EpdController.setScreenHandWritingRegionMode(view, 0));
        return this;
    }

    /** Side (barrel) button held: the stroke erases. */
    public void enableSideBtnErase(boolean enabled) {
        reader.enableSideBtnErase(enabled);
    }

    public void setPenUpRefreshEnabled(boolean enabled) {
        reader.setPenUpRefreshEnabled(enabled);
    }

    public void setPenUpRefreshTimeMs(int ms) {
        reader.setPenUpRefreshTimeMs(ms);
    }

    /** The SDK forwards this to setPenUpRefreshEnabled (an Onyx bug apps already live with): mirrored. */
    public void setFilterRepeatMovePoint(boolean on) {
        reader.setPenUpRefreshEnabled(on);
    }

    public void setPostInputEvent(boolean on) {
        reader.setPostInputEvent(on);
    }

    /** Install our touch listener on open (default); off if the app forwards {@link #onTouchEvent} itself. */
    public void setTouchListenerEnabled(boolean on) {
        touchListenerEnabled = on;
        if (created) view.setOnTouchListener(on ? (v, e) -> onTouchEvent(e) : null);
    }

    /** Follow the host view when it scrolls (re-reads its screen position). */
    public TouchHelper setHostViewScrollListenerEnabled(boolean on) {
        scrollListenerEnabled = on;
        ViewTreeObserver o = view.getViewTreeObserver();
        o.removeOnScrollChangedListener(scrollListener);
        if (on && created) o.addOnScrollChangedListener(scrollListener);
        return this;
    }

    /** Finger touches draw strokes too. */
    public void enableFingerTouch(boolean on) {
        fingerTouch = on;
    }

    /** Only finger touches draw: the stylus is ignored. */
    public void onlyEnableFingerTouch(boolean on) {
        onlyFingerTouch = on;
        if (on) fingerTouch = true;
        reader.setPenInput(!on);
    }

    /** Finger strokes take the touch's own pressure instead of {@link #setFingerTouchPressure}. */
    public void enableFingerTouchPressure(boolean on) {
        fingerPressure = on;
    }

    /** Pressure of finger strokes, 0..1 of the pen's maximum. */
    public void setFingerTouchPressure(float p) {
        fingerTouchPressure = p;
    }

    /** {@link TouchPoint#pressure} runs 0..this (raw digitizer units, as in the Onyx SDK); 0 before open. */
    public float getMaxPressure() {
        return reader.maxPressure();
    }

    /**
     * Stylus MotionEvents refine the raw → view map; finger MotionEvents draw when enabled.
     * Installed as the host view's touch listener by {@link #openRawDrawing}; call it yourself if
     * you replace that listener. Consumes only finger events it turns into strokes.
     */
    public boolean onTouchEvent(MotionEvent e) {
        int tool = e.getToolType(0);
        if (tool == MotionEvent.TOOL_TYPE_FINGER) return finger(e);
        MapCalibrator c = reader.calibrator();
        if (c == null || (tool != MotionEvent.TOOL_TYPE_STYLUS && tool != MotionEvent.TOOL_TYPE_ERASER)) return false;
        int a = e.getActionMasked();
        if (a == MotionEvent.ACTION_DOWN) c.screenContact(PenReader.DOWN, e.getRawX(), e.getRawY(), e.getEventTime());
        if (a == MotionEvent.ACTION_UP) c.screenContact(PenReader.UP, e.getRawX(), e.getRawY(), e.getEventTime());
        return false;
    }

    private boolean finger(MotionEvent e) {
        if (!fingerTouch || !created) return false;
        int code;
        switch (e.getActionMasked()) {
            case MotionEvent.ACTION_DOWN: code = 0; break;
            case MotionEvent.ACTION_MOVE: code = 1; break;
            case MotionEvent.ACTION_UP: code = 2; break;
            case MotionEvent.ACTION_CANCEL: code = 6; break;
            default: return false;
        }
        float max = Math.max(1f, reader.maxPressure());
        int p = Math.round(max * (fingerPressure ? e.getPressure() : fingerTouchPressure));
        reader.onTouchPointReceived(e.getX(), e.getY(), p, 0, 0, false, false, false, code, e.getEventTime());
        return true;
    }

    // ---- instant ink (UI thread) ----

    private void openInk() {
        if (!created) return; // closed before the post ran
        try {
            if (session == null) session = new InkSession(OnyxFrameworkBackend.open(), view);
            if (session.isOpen()) return;
            reader.trackViewPosition();
            int[] region = limit.isEmpty() ? new int[]{0, 0, view.getWidth(), view.getHeight()} : flat(limit);
            session.open(region, reader.getStrokeWidth());
            session.setStrokeColor(reader.getStrokeColor());
            session.setStrokeStyle(strokeStyle);
            if (!exclude.isEmpty()) session.setExclude(flat(exclude));
            if (reader.isSingleRegionMode()) RawInputReader.sf(() -> EpdController.setScreenHandWritingRegionMode(view, 1));
            if (!renderEnabled) session.pause();
        } catch (UnsupportedOperationException e) {
            Log.w(TAG, "no instant ink (stock SurfaceFlinger unreachable): " + e.getMessage());
        } catch (RuntimeException e) {
            Log.e(TAG, "instant ink open", e);
        }
    }

    private void closeInk() {
        try {
            session.close();
        } catch (RuntimeException e) {
            Log.e(TAG, "instant ink close", e);
        }
    }

    private void onHostMoved() {
        reader.trackViewPosition();
    }

    private static int[] flat(List<Rect> rects) {
        int[] out = new int[rects.size() * 4];
        for (int i = 0; i < rects.size(); i++) {
            Rect r = rects.get(i);
            out[4 * i] = r.left;
            out[4 * i + 1] = r.top;
            out[4 * i + 2] = r.right;
            out[4 * i + 3] = r.bottom;
        }
        return out;
    }
}
