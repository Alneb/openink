package com.onyx.android.sdk.pen;

/** Stub: libneo's result, built through the static `create(PenInk, PenInk)` (real ink, prediction). */
public final class NeoPenResult {
    public final PenInk real, prediction;

    public NeoPenResult(PenInk real, PenInk prediction) {
        this.real = real;
        this.prediction = prediction;
    }

    public static NeoPenResult create(PenInk real, PenInk prediction) {
        return new NeoPenResult(real, prediction);
    }
}
