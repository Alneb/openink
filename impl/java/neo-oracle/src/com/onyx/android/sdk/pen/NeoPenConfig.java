package com.onyx.android.sdk.pen;

/** Stub: the field names and types libneo's nativeCreatePen reads (defaults as knote2's class has them). */
public final class NeoPenConfig {
    public int type = 1;
    public int color = 0xff000000;
    public float width = 3f, minWidth = 0.001f, tiltScale = 3f, maxTouchPressure = 1f, dpi = 320f;
    public float displayScaleX = 1f, displayScaleY = 1f, scalePrecision = 1f;
    public float brushSpacing = 0.25f, brushRatio = 5f, brushAngle;
    public int brushShape, rotateAngle;
    public float pressureSensitivity = 0.3f, velocitySensitivity = 0.5f, velocityAmplifier;
    public float velocityIgnoreThreshold, velocityLowerBound, velocityUpperBound;
    public float smoothLevel = 0.6f;
    public boolean fastMode, tiltEnabled, directionEnabled;
}
