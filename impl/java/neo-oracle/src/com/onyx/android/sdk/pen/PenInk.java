package com.onyx.android.sdk.pen;

import android.graphics.Bitmap;

/** Stub: what libneo builds through the static `create([F[I[Landroid/graphics/Bitmap;)`. */
public final class PenInk {
    public final float[] points;
    public final int[] sizes;
    public final Bitmap[] bitmaps;

    public PenInk(float[] points, int[] sizes, Bitmap[] bitmaps) {
        this.points = points;
        this.sizes = sizes;
        this.bitmaps = bitmaps;
    }

    public static PenInk create(float[] points, int[] sizes, Bitmap[] bitmaps) {
        return new PenInk(points, sizes, bitmaps);
    }
}
