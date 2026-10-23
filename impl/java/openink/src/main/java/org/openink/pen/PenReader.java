package org.openink.pen;

import java.io.Closeable;
import java.io.IOException;

/**
 * Raw stylus events from evdev via libopenink_jni.so (replaces libonyx_pen_touch_reader.so).
 * Callbacks arrive on the "openink-pen" thread, in raw device coordinates.
 */
public final class PenReader implements Closeable {
    // Wire codes: must match action_code/tool_code in crates/openink-jni.
    public static final int HOVER_ENTER = 0, HOVER = 1, DOWN = 2, MOVE = 3, UP = 4, HOVER_LEAVE = 5, FORCED_UP = 6;
    public static final int TOOL_PEN = 0, TOOL_ERASER = 1;

    public interface Listener {
        void onPen(int action, int tool, int x, int y, float pressure, int tiltX, int tiltY,
                   boolean sideButton, long timeUs);
    }

    static {
        System.loadLibrary("openink_jni");
    }

    private long handle;

    private PenReader(long handle) {
        this.handle = handle;
    }

    /** @throws IOException when no stylus is found, naming each device skipped and why */
    public static PenReader start(Listener listener) throws IOException {
        return new PenReader(nativeStart(listener));
    }

    /** Device path, name and axis ranges, for mapping raw coordinates to the screen. */
    public synchronized String deviceInfo() {
        if (handle == 0) throw new IllegalStateException("closed");
        return nativeInfo(handle);
    }

    /** The digitizer's axis ranges, in the raw units onPen reports. */
    public synchronized PenAxes axes() {
        if (handle == 0) throw new IllegalStateException("closed");
        return PenAxes.fromArray(nativeAxes(handle));
    }

    @Override public synchronized void close() {
        if (handle == 0) return;
        long h = handle;
        handle = 0;
        nativeStop(h);
    }

    private static native long nativeStart(Listener listener) throws IOException;
    private static native String nativeInfo(long handle);
    private static native int[] nativeAxes(long handle);
    private static native void nativeStop(long handle);
}
