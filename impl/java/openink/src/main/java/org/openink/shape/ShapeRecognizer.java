package org.openink.shape;

/**
 * Shape recognition from libopenink_jni.so (crates/openink-shape: our own trained classifier + segmenter,
 * replacing MyScript Shape). Stateless and thread-safe; the model loads once on first use.
 */
public final class ShapeRecognizer {
    static {
        System.loadLibrary("openink_jni");
    }

    private ShapeRecognizer() {}

    /** {@code strokes[i]} is {@code [x, y, tMs]*} for stroke i, in drawing order; any coordinate unit. */
    public static Shape recognize(float[][] strokes) {
        int total = 0;
        int[] lens = new int[strokes.length];
        for (int i = 0; i < strokes.length; i++) {
            if (strokes[i].length % 3 != 0) throw new IllegalArgumentException("stroke " + i + " is not [x, y, t]*");
            lens[i] = strokes[i].length / 3;
            total += strokes[i].length;
        }
        float[] xyt = new float[total];
        int at = 0;
        for (float[] s : strokes) {
            System.arraycopy(s, 0, xyt, at, s.length);
            at += s.length;
        }
        return Shape.decode(nativeRecognize(xyt, lens));
    }

    private static native float[] nativeRecognize(float[] xyt, int[] lens);
}
