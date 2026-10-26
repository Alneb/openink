package org.openink.compat.pen;

/**
 * How libonyx_pen_touch_reader cuts one pen contact into strokes at the edge of the valid region
 * ({@code BaseReader::onEvent}, {@code DrawReader::onDeactivate}): leaving the region ends the stroke
 * with the "out of limit region" flag, coming back starts a new one, and a pen that lands outside
 * starts its stroke where it first enters. No Android types, so the host checks can run it.
 */
final class RegionGate {
    /** What one pen-down point means for the callback. */
    enum Step {
        /** The point opens a stroke (onBegin…). */
        BEGIN,
        /** The point extends the open stroke. */
        MOVE,
        /** The point left the region: the open stroke ends with {@code outLimitRegion = true}. */
        END_OUT,
        /** Outside the region with no stroke open: nothing is reported. */
        SKIP
    }

    private enum State { IDLE, INSIDE, OUTSIDE }

    private State state = State.IDLE;

    /** One point while the pen is down; {@code valid} is the region test for it. */
    Step point(boolean valid) {
        if (state == State.INSIDE) {
            if (valid) return Step.MOVE;
            state = State.OUTSIDE;
            return Step.END_OUT;
        }
        state = valid ? State.INSIDE : State.OUTSIDE;
        return valid ? Step.BEGIN : Step.SKIP;
    }

    /** Pen up (or a forced release): true when a stroke is open and ends normally. */
    boolean up() {
        boolean open = state == State.INSIDE;
        state = State.IDLE;
        return open;
    }

    /**
     * The native limit-rect test: the point padded by half the stroke width lies wholly inside the
     * rect (edges inclusive). {@link #overlaps} is the exclude-rect test for the same padded point.
     */
    static boolean fits(float x, float y, float half, int left, int top, int right, int bottom) {
        return x - half >= left && x + half <= right && y - half >= top && y + half <= bottom;
    }

    static boolean overlaps(float x, float y, float half, int left, int top, int right, int bottom) {
        return x + half >= left && x - half <= right && y + half >= top && y - half <= bottom;
    }
}
