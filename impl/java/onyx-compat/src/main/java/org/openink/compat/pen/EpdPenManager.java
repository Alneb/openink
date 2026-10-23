package org.openink.compat.pen;

import android.view.View;
import org.openink.compat.epd.EpdController;
import org.openink.compat.epd.UpdateMode;

/**
 * Onyx's {@code com.onyx.android.sdk.pen.EpdPenManager}: SurfaceFlinger's handwriting pen states
 * over {@link EpdController}. The state values are {@link org.openink.pen.PenCommand}'s codes.
 */
public class EpdPenManager {
    public static final int PEN_STOP = 0, PEN_START = 1, PEN_DRAWING = 2, PEN_PAUSE = 3, PEN_ERASING = 4;

    private View hostView;

    public static void moveTo(float x, float y, float width) {
        EpdController.moveTo(x, y, width);
    }

    public static void quadTo(float x, float y, UpdateMode mode) {
        EpdController.quadTo(x, y, mode);
    }

    public static void setStrokeColor(int color) {
        EpdController.setStrokeColor(color);
    }

    public void setStrokeStyle(int style) {
        EpdController.setStrokeStyle(style);
    }

    public EpdPenManager setHostView(View view) {
        hostView = view;
        return this;
    }

    public void startDrawing() {
        EpdController.setScreenHandWritingPenState(hostView, PEN_START);
    }

    public void resumeDrawing() {
        EpdController.setScreenHandWritingPenState(hostView, PEN_DRAWING);
    }

    public void pauseDrawing() {
        EpdController.setScreenHandWritingPenState(hostView, PEN_PAUSE);
    }

    public void quitDrawing() {
        EpdController.setScreenHandWritingPenState(hostView, PEN_STOP);
        hostView = null;
    }
}
