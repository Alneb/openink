package org.openink.stroke;

/** Per-stroke settings (immutable). Rust clamps every field, so no value here can break geometry. */
public final class PenConfig {
    public final float width, pressureSensitivity, smoothLevel, velocitySensitivity, tiltScale;
    public final boolean tiltEnabled;
    /** NeoPenConfig.dpi (the speed term's scale) and minWidth; SurfaceFlinger's live pens use 320 and 1. */
    public final float dpi, minWidth;

    public PenConfig(float width, float pressureSensitivity, float smoothLevel, float velocitySensitivity,
                     boolean tiltEnabled, float tiltScale, float dpi, float minWidth) {
        this.width = width;
        this.pressureSensitivity = pressureSensitivity;
        this.smoothLevel = smoothLevel;
        this.velocitySensitivity = velocitySensitivity;
        this.tiltEnabled = tiltEnabled;
        this.tiltScale = tiltScale;
        this.dpi = dpi;
        this.minWidth = minWidth;
    }

    /** The earlier six-field form: dpi 300 (this panel), minWidth 0.001 (knote2's NeoPenConfig). */
    public PenConfig(float width, float pressureSensitivity, float smoothLevel, float velocitySensitivity,
                     boolean tiltEnabled, float tiltScale) {
        this(width, pressureSensitivity, smoothLevel, velocitySensitivity, tiltEnabled, tiltScale, 300f, 0.001f);
    }

    /** knote2's defaults (NeoPenConfig, NotePenInfoUtils) for {@code kind} at {@code dpi}. */
    public static PenConfig knote2Default(PenKind kind, float dpi) {
        return new PenConfig(kind.defaultWidthPx(dpi), 0.3f, 0.6f, 0.5f, kind.output == PenKind.Output.DABS, 3f, dpi, 0.001f);
    }

    public PenConfig withWidth(float px) {
        return new PenConfig(px, pressureSensitivity, smoothLevel, velocitySensitivity, tiltEnabled, tiltScale, dpi, minWidth);
    }

    /** The JNI wire format (crates/openink-jni/src/stroke.rs). */
    float[] toArray() {
        return new float[]{width, pressureSensitivity, smoothLevel, velocitySensitivity, tiltEnabled ? 1 : 0, tiltScale, dpi, minWidth};
    }
}
