package org.openink.pen;

/** SET_STROKE_STYLE values (Onyx EpdController stroke styles). */
public enum StrokeStyle {
    PENCIL(0), BRUSH(1), MARKER(2), NEO_BRUSH(3), CHARCOAL(4);

    public final int value;

    StrokeStyle(int value) {
        this.value = value;
    }
}
