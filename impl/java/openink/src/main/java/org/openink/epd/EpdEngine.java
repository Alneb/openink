package org.openink.epd;

import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import java.io.Closeable;

/**
 * The open e-ink refresh engine (crates/openink-epdc, docs/EPDC-ENGINE.md): callers say what
 * changed, the engine merges, picks waveforms, holds updates while the pen writes and cleans
 * ghosting when the screen is idle. Updates go to a {@link Sink} on this engine's Looper.
 * Every method must be called on that Looper's thread.
 */
public final class EpdEngine implements Closeable {
    static {
        System.loadLibrary("openink_jni");
    }

    /** What a refresh shows; codes match openink-epdc's Intent::code. */
    public enum Intent {
        /** UI and text: GU (16-level partial). */
        UI(0),
        /** Photos, gradients: GC16 partial. */
        IMAGE(1),
        /** Scrolling, dragging, animation: A2. */
        MOTION(2),
        /** The app's repaint of finished ink: DU. */
        INK(3),
        /** Explicit cleanup flash: GC16 full. */
        CLEAN(4);

        final int code;

        Intent(int code) {
            this.code = code;
        }
    }

    /** Why an update was sent (openink-epdc's Reason). */
    public enum Reason { REQUEST, GC_INTERVAL, CLEANUP }

    /** Executes one panel update, e.g. {@code backend::refreshScreen}. */
    public interface Sink {
        void refresh(int left, int top, int width, int height, int mode);
    }

    /** Optional trace of every update with its reason. */
    public interface Listener {
        void onUpdate(int left, int top, int width, int height, UpdateMode mode, Reason reason);
    }

    private static final int RAW = -1, UPDATE_LEN = 6;
    private static final Reason[] REASONS = Reason.values();

    private final Sink sink;
    private final Handler handler;
    private final Runnable pump = this::pump;
    private Listener listener;
    private long handle;

    public EpdEngine(int screenWidth, int screenHeight, Sink sink, Looper looper) {
        this.sink = sink;
        this.handler = new Handler(looper);
        this.handle = nativeCreate(screenWidth, screenHeight);
    }

    public void setListener(Listener l) {
        listener = l;
    }

    /** Refresh the screen rectangle (left, top, right, bottom) showing {@code intent}. */
    public void request(int left, int top, int right, int bottom, Intent intent) {
        nativeRequest(live(), left, top, right, bottom, intent.code, 0, now());
        schedule();
    }

    /** Onyx compatibility: refresh with an explicit EINK_* mode, unchanged by the policy. */
    public void requestRaw(int left, int top, int right, int bottom, UpdateMode mode) {
        nativeRequest(live(), left, top, right, bottom, RAW, mode.toOnyx(), now());
        schedule();
    }

    public void penDown() {
        nativePenDown(live(), now());
        schedule();
    }

    /** Pen lifted; the stroke's screen box (inflated by half the width) is repainted as INK. */
    public void penUp(int left, int top, int right, int bottom) {
        nativePenUp(live(), left, top, right, bottom, now());
        schedule();
    }

    /** Pen lifted without ink to repaint (e.g. an eraser pass that repaints itself). */
    public void penUp() {
        penUp(0, 0, 0, 0);
    }

    /** Stock SET_GC_REFRESH_INTERVAL: every {@code n}th partial update becomes a full GC; 0 = off. */
    public void setGcEvery(int n) {
        nativeSetGcEvery(live(), n);
    }

    /** Stock APPLY_APP_SCOPE_UPDATE: one mode for every non-raw request; null clears it. */
    public void setScopeMode(UpdateMode mode) {
        nativeSetScopeMode(live(), mode == null ? RAW : mode.toOnyx());
    }

    @Override public void close() {
        handler.removeCallbacks(pump);
        long h = handle;
        handle = 0;
        nativeFree(h);
    }

    /** Send everything queued now, skipping the coalescing and pen-up waits (not while the pen touches). */
    public void flush() {
        deliver(nativeFlush(live(), now()));
        schedule();
    }

    private void pump() {
        if (handle == 0) return;
        deliver(nativePoll(handle, now()));
        schedule();
    }

    private void deliver(int[] u) {
        for (int i = 0; i + UPDATE_LEN <= u.length; i += UPDATE_LEN) {
            sink.refresh(u[i], u[i + 1], u[i + 2], u[i + 3], u[i + 4]);
            Listener l = listener;
            if (l != null) l.onUpdate(u[i], u[i + 1], u[i + 2], u[i + 3], UpdateMode.fromOnyx(u[i + 4]), REASONS[u[i + 5]]);
        }
    }

    /** One pending wake-up at the engine's next deadline. */
    private void schedule() {
        handler.removeCallbacks(pump);
        long at = nativeNextDeadline(handle);
        if (at >= 0) handler.postAtTime(pump, at);
    }

    private static long now() {
        return SystemClock.uptimeMillis(); // same clock as Handler.postAtTime
    }

    private long live() {
        if (handle == 0) throw new IllegalStateException("EpdEngine closed");
        if (Looper.myLooper() != handler.getLooper()) throw new IllegalStateException("EpdEngine used off its Looper thread");
        return handle;
    }

    private static native long nativeCreate(int width, int height);
    private static native void nativeRequest(long h, int l, int t, int r, int b, int intent, int rawMode, long nowMs);
    private static native void nativePenDown(long h, long nowMs);
    private static native void nativePenUp(long h, int l, int t, int r, int b, long nowMs);
    private static native int[] nativePoll(long h, long nowMs);
    private static native int[] nativeFlush(long h, long nowMs);
    private static native long nativeNextDeadline(long h);
    private static native void nativeSetGcEvery(long h, int n);
    private static native void nativeSetScopeMode(long h, int mode);
    private static native void nativeFree(long h);
}
