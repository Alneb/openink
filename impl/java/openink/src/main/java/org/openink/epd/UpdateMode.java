package org.openink.epd;

/**
 * A typed e-ink update mode: waveform + flag bits, lossless to and from the Onyx int.
 * Decoding is strict (unknown bits throw), matching openink-core's UpdateMode.
 */
public final class UpdateMode {
    public static final int AUTOMATIC = 0x10, FULL = 0x20, WAIT = 0x40, COMBINE = 0x80, DITHER = 0x100,
            INVERT = 0x200, CONVERT = 0x400, DITHER_Y1 = 0x800, REGAL_D = 0x1000, HANDWRITE_GU = 0x80000,
            SHUTDOWN = 0x500000, DITHER_X = 0x1000000, APPLY_MONO = 0x2000000;
    static final int KNOWN = AUTOMATIC | FULL | WAIT | COMBINE | DITHER | INVERT | CONVERT | DITHER_Y1 | REGAL_D
            | HANDWRITE_GU | SHUTDOWN | DITHER_X | APPLY_MONO;

    public static final UpdateMode DEFAULT = new UpdateMode(Waveform.AUTO, 0);
    public static final UpdateMode DU = new UpdateMode(Waveform.DU, 0);
    public static final UpdateMode GU = new UpdateMode(Waveform.GC16, 0);
    public static final UpdateMode A2 = new UpdateMode(Waveform.A2, 0);
    public static final UpdateMode REGAL = new UpdateMode(Waveform.REGAL, 0);
    /** Full GC16 with wait: the "flash" refresh (UI_GC_MODE). */
    public static final UpdateMode GC = new UpdateMode(Waveform.GC16, FULL | WAIT);
    public static final UpdateMode A2_QUALITY = new UpdateMode(Waveform.A2, DITHER | DITHER_Y1);
    public static final UpdateMode HANDWRITING_REPAINT = new UpdateMode(Waveform.GC16, HANDWRITE_GU);

    public final Waveform waveform;
    public final int flags;

    public UpdateMode(Waveform waveform, int flags) {
        if ((flags & ~KNOWN) != 0) {
            throw new IllegalArgumentException("unknown flag bits 0x" + Integer.toHexString(flags & ~KNOWN));
        }
        this.waveform = waveform;
        this.flags = flags;
    }

    public static UpdateMode fromOnyx(int value) {
        if (value < 0) throw new IllegalArgumentException("negative update mode " + value + " (INVALID_MODE?)");
        return new UpdateMode(Waveform.fromNibble(value & Waveform.MASK), value & ~Waveform.MASK);
    }

    public int toOnyx() {
        return waveform.nibble | flags;
    }

    public UpdateMode with(int extraFlags) {
        return new UpdateMode(waveform, flags | extraFlags);
    }

    @Override public boolean equals(Object o) {
        return o instanceof UpdateMode && ((UpdateMode) o).toOnyx() == toOnyx();
    }

    @Override public int hashCode() {
        return toOnyx();
    }

    @Override public String toString() {
        return "UpdateMode(" + waveform + ", flags=0x" + Integer.toHexString(flags) + ")";
    }
}
