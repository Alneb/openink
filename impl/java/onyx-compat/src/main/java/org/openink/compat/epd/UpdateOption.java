package org.openink.compat.epd;

/** Onyx's {@code UpdateOption}: the app-scope refresh mode; {@link #code} is BaseDevice's UPDATE_MODE_*. */
public enum UpdateOption {
    NORMAL(0), FAST_QUALITY(1), REGAL(3), FAST(2), FAST_X(4);

    public final int code;

    UpdateOption(int code) {
        this.code = code;
    }

    /** BaseDevice.c(): unknown codes read as NORMAL. */
    public static UpdateOption fromCode(int c) {
        for (UpdateOption o : values()) {
            if (o.code == c) return o;
        }
        return NORMAL;
    }
}
