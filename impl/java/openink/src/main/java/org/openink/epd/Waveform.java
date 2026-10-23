package org.openink.epd;

/** Waveform nibble of an update mode (EINK_WAVEFORM_MODE_*). Mirrors openink-core's Waveform. */
public enum Waveform {
    INIT(0), DU(1), GC16(2), GC4(3), A2(4), AUTO(5), REGAL(6), DU4(8), REGAL_PLUS(9), GCC16(11), DEEP_GC16(12);

    public static final int MASK = 0xF;
    public final int nibble;

    Waveform(int nibble) {
        this.nibble = nibble;
    }

    /** @throws IllegalArgumentException for the undefined nibbles 7, 10, 13-15 */
    public static Waveform fromNibble(int n) {
        for (Waveform w : values()) {
            if (w.nibble == n) return w;
        }
        throw new IllegalArgumentException("undefined waveform nibble " + n);
    }
}
