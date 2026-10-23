package org.openink.pen;

/** Raw axis ranges of the stylus digitizer (evdev absinfo min/max), inclusive. */
public final class PenAxes {
    public final int xMin, xMax, yMin, yMax, pressureMin, pressureMax;

    PenAxes(int xMin, int xMax, int yMin, int yMax, int pressureMin, int pressureMax) {
        this.xMin = xMin; this.xMax = xMax; this.yMin = yMin; this.yMax = yMax;
        this.pressureMin = pressureMin; this.pressureMax = pressureMax;
    }

    /** From the JNI layout {xMin, xMax, yMin, yMax, pressureMin, pressureMax}. */
    static PenAxes fromArray(int[] a) {
        if (a == null || a.length != 6) throw new IllegalStateException("bad axes from native: " + (a == null ? "null" : a.length + " values"));
        return new PenAxes(a[0], a[1], a[2], a[3], a[4], a[5]);
    }

    @Override public String toString() {
        return "x " + xMin + ".." + xMax + " y " + yMin + ".." + yMax + " pressure " + pressureMin + ".." + pressureMax;
    }
}
