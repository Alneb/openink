package org.openink.compat.epd;

/** Onyx's {@code UpdateScheme}; {@link #code} is what SDMDevice.i() sends (None counts as QUEUE). */
public enum UpdateScheme {
    None(1), SNAPSHOT(0), QUEUE(1), QUEUE_AND_MERGE(2);

    public final int code;

    UpdateScheme(int code) {
        this.code = code;
    }
}
