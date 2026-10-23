package org.openink.stroke;

/** The knote2 pen set. Codes and defaults must match crates/openink-stroke/src/pen.rs. */
public enum PenKind {
    BALLPOINT(0, Output.FILL, 7.2f, 0.1f, 2f),
    FOUNTAIN(1, Output.FILL, 7.2f, 0.1f, 2f),
    BRUSH(2, Output.FILL, 8.4f, 0.1f, 2f),
    MARKER(3, Output.TRANSLUCENT, 48f, 0.5f, 8f),
    SQUARE(4, Output.FILL, 11.81f, 0.1f, 2f),
    PENCIL(5, Output.DABS, 6f, 0.1f, 2f),
    CHARCOAL(6, Output.DABS, 3.6f, 0.1f, 2f);

    /** How the ink reaches the canvas (see Ink). */
    public enum Output { FILL, TRANSLUCENT, DABS }

    /** Layer alpha of translucent pens: knote2 composites the marker at 128. */
    public static final int TRANSLUCENT_ALPHA = 128;

    public final int code;
    public final Output output;
    /** knote2 PenConstant default, px at 300 dpi. */
    public final float defaultWidthPx300;
    public final float minWidthMm, maxWidthMm;

    PenKind(int code, Output output, float defaultWidthPx300, float minWidthMm, float maxWidthMm) {
        this.code = code;
        this.output = output;
        this.defaultWidthPx300 = defaultWidthPx300;
        this.minWidthMm = minWidthMm;
        this.maxWidthMm = maxWidthMm;
    }

    /** The pen with this wire/storage code. @throws IllegalArgumentException for an unknown code */
    public static PenKind fromCode(int code) {
        for (PenKind k : values()) if (k.code == code) return k;
        throw new IllegalArgumentException("unknown pen code " + code);
    }

    public float defaultWidthPx(float dpi) {
        return defaultWidthPx300 * dpi / 300f;
    }
}
