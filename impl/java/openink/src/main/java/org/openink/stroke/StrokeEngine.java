package org.openink.stroke;

import java.io.Closeable;

/**
 * Stroke geometry from libopenink_jni.so (crates/openink-stroke; replaces libneo_pen.so).
 * One engine per pen setting, reused stroke after stroke: down, move*, up. Confined to one
 * thread: use a separate engine for redraws on another thread.
 */
public final class StrokeEngine implements Closeable {
    static {
        System.loadLibrary("openink_jni");
    }

    public final PenKind kind;
    public final PenConfig config;
    private long handle;
    private long downUs;
    private final float[] one = new float[6];

    public StrokeEngine(PenKind kind, PenConfig config) {
        this.kind = kind;
        this.config = config;
        this.handle = nativeCreate(kind.code, config.toArray());
    }

    /** Coordinates in the target canvas's pixels; pressure 0..1; tilt in degrees. */
    public Ink down(float x, float y, float pressure, float tiltX, float tiltY, long timeUs) {
        downUs = timeUs;
        return Ink.decode(kind.output, nativeDown(live(), sample(x, y, pressure, tiltX, tiltY, timeUs)));
    }

    public Ink move(float x, float y, float pressure, float tiltX, float tiltY, long timeUs) {
        return Ink.decode(kind.output, nativeMove(live(), sample(x, y, pressure, tiltX, tiltY, timeUs)));
    }

    public Ink up(float x, float y, float pressure, float tiltX, float tiltY, long timeUs) {
        return Ink.decode(kind.output, nativeUp(live(), sample(x, y, pressure, tiltX, tiltY, timeUs)));
    }

    /** A whole recorded stroke: {@code samples} is {@code [x, y, pressure, tiltX, tiltY, tMs]*}. */
    public Ink render(float[] samples) {
        return Ink.decode(kind.output, nativeRender(live(), samples));
    }

    /** The grain mask for dabs of edge {@code size} px (alpha bytes, row-major). */
    public static byte[] grainMask(int size) {
        return nativeGrainMask(size);
    }

    /** Mask edge for a dab of {@code size} px: must match raster::mask_size. */
    public static int maskSize(float size) {
        return Math.max(2, Math.min(256, Math.round(size)));
    }

    @Override public void close() {
        long h = handle;
        handle = 0;
        nativeFree(h);
    }

    private float[] sample(float x, float y, float p, float tx, float ty, long timeUs) {
        one[0] = x; one[1] = y; one[2] = p; one[3] = tx; one[4] = ty;
        one[5] = (timeUs - downUs) / 1000f; // relative: f32 keeps sub-ms precision
        return one;
    }

    private long live() {
        if (handle == 0) throw new IllegalStateException("StrokeEngine closed");
        return handle;
    }

    private static native long nativeCreate(int kind, float[] config);
    private static native float[] nativeDown(long h, float[] sample);
    private static native float[] nativeMove(long h, float[] samples);
    private static native float[] nativeUp(long h, float[] sample);
    private static native float[] nativeRender(long h, float[] samples);
    private static native void nativeFree(long h);
    private static native byte[] nativeGrainMask(int size);
}
