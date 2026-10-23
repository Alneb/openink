package org.openink.backend;

import android.view.View;

/**
 * What the open API needs from the compositor. Implemented today by {@link OnyxFrameworkBackend}
 * (the stock, Onyx-patched SurfaceFlinger); later by our own SF. Raw ints here; typing is the
 * API layer's job.
 */
public interface EpdBackend {
    void refreshScreen(int left, int top, int width, int height, int mode);
    void waitForUpdateFinished();
    void setPenState(int command);
    int getPenState();
    /** Rects are flat {l,t,r,b,...}, relative to {@code view} (or the screen when null); never mutated. */
    void setRegionLimit(View view, int[] rects);
    void setRegionExclude(View view, int[] rects);
    void setRegionMode(int mode);
    void setStrokeWidth(float width);
    void setStrokeColor(int color);
    void setStrokeStyle(int style);
    /** Per-style painter parameters (docs/SF-INSTANT-INK.md §3), e.g. FountainV2 {pressureSensitivity, smoothLevel}. */
    void setStrokeParameters(int style, float[] params);
    float[] getStrokeParameters(int style);
    void setBrushRawDrawingEnabled(boolean enabled);
    void setEraserRawDrawingEnabled(boolean enabled, int arg);
    void enablePost(int enable);
    void penUp();
    void handwritingRepaint(View view, int left, int top, int right, int bottom);
}
