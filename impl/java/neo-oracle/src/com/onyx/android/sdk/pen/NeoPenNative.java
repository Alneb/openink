package com.onyx.android.sdk.pen;

/** Stub: the class whose JNI symbols libneo exports (Java_com_onyx_android_sdk_pen_NeoPenNative_*). */
public final class NeoPenNative {
    public native long nativeCreatePen(int penType, NeoPenConfig config);
    public native void nativeDestroyPen(long pen);
    /** Points are double[7] each: x, y, pressure, size, tiltX, tiltY, timestamp. */
    public native NeoPenResult nativeOnPenDown(long pen, double[] point, boolean repaint);
    public native NeoPenResult nativeOnPenMove(long pen, double[] points, double[] prediction, boolean repaint);
    public native NeoPenResult nativeOnPenUp(long pen, double[] point, boolean repaint);
}
