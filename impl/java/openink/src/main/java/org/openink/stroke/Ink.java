package org.openink.stroke;

import android.graphics.RectF;

/**
 * New ink from one engine call. {@code data}: for FILL/TRANSLUCENT a polygon list
 * {@code [n, x0, y0, ..., x(n-1), y(n-1)]*}; for DABS {@code [x, y, size, angleRad, alpha]*}.
 */
public final class Ink {
    public final PenKind.Output output;
    public final float[] data;
    /** Bounds in the samples' coordinates; null when empty. */
    public final RectF bounds;

    private Ink(PenKind.Output output, float[] data, RectF bounds) {
        this.output = output;
        this.data = data;
        this.bounds = bounds;
    }

    /** Decode the JNI form: 4 bound floats (NaN when empty), then data. */
    static Ink decode(PenKind.Output output, float[] wire) {
        if (wire == null || wire.length < 4) throw new IllegalStateException("native returned no ink (allocation failed?)");
        float[] data = new float[wire.length - 4];
        System.arraycopy(wire, 4, data, 0, data.length);
        RectF b = Float.isNaN(wire[0]) ? null : new RectF(wire[0], wire[1], wire[2], wire[3]);
        return new Ink(output, data, b);
    }

    public boolean isEmpty() {
        return data.length == 0;
    }
}
