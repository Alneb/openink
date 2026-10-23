package org.openink.compat.pen;

import android.graphics.RectF;

/**
 * Onyx's {@code com.onyx.android.sdk.pen.RawInputCallback}, same methods. Called on the
 * "openink-pen" thread (not the UI thread), except {@link #onPenUpRefresh}, which runs on the main
 * thread. The leading boolean of begin/end is Onyx's flag; openink always passes false.
 */
public abstract class RawInputCallback {
    public abstract void onBeginRawDrawing(boolean b, TouchPoint p);

    public abstract void onEndRawDrawing(boolean b, TouchPoint p);

    public abstract void onRawDrawingTouchPointMoveReceived(TouchPoint p);

    /** The whole stroke, after {@link #onEndRawDrawing}. */
    public abstract void onRawDrawingTouchPointListReceived(TouchPointList list);

    public abstract void onBeginRawErasing(boolean b, TouchPoint p);

    public abstract void onEndRawErasing(boolean b, TouchPoint p);

    public abstract void onRawErasingTouchPointMoveReceived(TouchPoint p);

    public abstract void onRawErasingTouchPointListReceived(TouchPointList list);

    /** The pen came into range. */
    public void onPenActive(TouchPoint p) {}

    /** {@link TouchHelper#setPenUpRefreshTimeMs} after the last pen-up: redraw {@code dirty} now. */
    public void onPenUpRefresh(RectF dirty) {}
}
