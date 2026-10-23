package org.openink.compat.epd;

/**
 * Onyx's {@code com.onyx.android.sdk.api.device.epd.UpdateMode}, same constants in the same order.
 * {@link #onyx} is the EINK_* int SDMDevice.h() sends for it (values from impl/spec/sf_protocol.json).
 */
public enum UpdateMode {
    None(5),                 // UI_DEFAULT_MODE
    DU(1),                   // UI_DU_MODE
    DU4(2312),               // UI_DU4_MODE
    GU(2),                   // UI_GU_MODE
    GU_FAST(1),              // SDMDevice maps GU_FAST to UI_DU_MODE as well
    GC(98),                  // UI_GC_MODE
    GCC(107),                // UI_GCC_MODE
    DEEP_GC(108),            // UI_DEEP_GC_MODE
    ANIMATION(4),            // REGIONAL | NOWAIT | WAVEFORM_ANIM | PARTIAL (all flags 0)
    ANIMATION_QUALITY(2308), // UI_A2_QUALITY_MODE
    ANIMATION_MONO(33554436),// UI_MONO_A2_MODE
    ANIMATION_X(16777220),   // UI_X_A2_MODE
    GC4(3),                  // UI_GC4_MODE
    REGAL(6),                // UI_REGAL_MODE
    REGAL_D(4102),           // REGIONAL | NOWAIT | REAGL_MODE_REAGLD | WAVEFORM_REAGL | PARTIAL
    REGAL_PLUS(9),           // UI_REGAL_PLUS_MODE
    DU_QUALITY(2305),        // UI_DU_QUALITY_MODE
    HAND_WRITING_REPAINT_MODE(524290);

    public final int onyx;

    UpdateMode(int onyx) {
        this.onyx = onyx;
    }

    public static UpdateMode getTypeByName(String name) {
        try {
            return valueOf(name);
        } catch (IllegalArgumentException e) {
            return None;
        }
    }

    /** SDMDevice.k(): the int a view reports back, as a mode (unknown values read as GC, as Onyx does). */
    public static UpdateMode fromOnyx(int v) {
        switch (v) {
            case 1: return DU;
            case 2312: return DU4;
            case 2: return GU;
            case 98: return GC;
            case 107: return GCC;
            case 108: return DEEP_GC;
            default: return GC;
        }
    }
}
