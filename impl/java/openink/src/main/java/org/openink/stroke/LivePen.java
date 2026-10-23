package org.openink.stroke;

/**
 * A pen choice for a stock Boox, both halves (crates/openink-stroke/src/sf.rs, docs/SF-INSTANT-INK.md):
 * what SurfaceFlinger's instant ink is told ({@link org.openink.pen.InkSession#usePen}) and the config to
 * render the settled stroke with, so the line does not change when it settles.
 */
public final class LivePen {
    static {
        System.loadLibrary("openink_jni");
    }

    /** SET_STROKE_STYLE value (0 plain, 1 FountainV2, 2 marker, 3 brush, 6 charcoal V2, 7 square). */
    public final int sfStyle;
    public final float sfWidth;
    /** SET_STROKE_PARAMETERS for sfStyle; empty = leave SF's own values. */
    public final float[] sfParams;
    /** Render the settled stroke with this (same pen kind, same samples). */
    public final PenConfig settled;
    /** False when the settled render can only approximate SF's (the pencil: SF draws a plain line). */
    public final boolean exact;

    private LivePen(int sfStyle, float sfWidth, float[] sfParams, PenConfig settled, boolean exact) {
        this.sfStyle = sfStyle;
        this.sfWidth = sfWidth;
        this.sfParams = sfParams;
        this.settled = settled;
        this.exact = exact;
    }

    public static LivePen of(PenKind kind, PenConfig want) {
        return decode(nativeLivePen(kind.code, want.toArray()));
    }

    /** Wire: style, sfWidth, exact (0/1), n, params[n], settled config float[8] (openink-jni stroke.rs). */
    static LivePen decode(float[] v) {
        int n = (int) v[3];
        float[] params = new float[n];
        System.arraycopy(v, 4, params, 0, n);
        int c = 4 + n;
        PenConfig settled = new PenConfig(v[c], v[c + 1], v[c + 2], v[c + 3], v[c + 4] != 0, v[c + 5], v[c + 6], v[c + 7]);
        return new LivePen((int) v[0], v[1], params, settled, v[2] != 0);
    }

    private static native float[] nativeLivePen(int kind, float[] config);
}
