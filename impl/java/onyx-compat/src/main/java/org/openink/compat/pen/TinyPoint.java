package org.openink.compat.pen;

/**
 * Onyx's {@code com.onyx.android.sdk.geometry.data.TinyPoint}: knote2's compact stored point.
 * {@code time} is ms since the stroke's first point; {@code size} packs the tilt as
 * {@code (tiltX & 255) | (tiltY << 8)} (TouchPoint.tilt2Size).
 */
public final class TinyPoint {
    private float x, y;
    private short pressure, size;
    private int time;

    public TinyPoint() {}

    public TinyPoint(float x, float y, short pressure, short size, int time) {
        this.x = x;
        this.y = y;
        this.pressure = pressure;
        this.size = size;
        this.time = time;
    }

    /** TouchPoint.toTinyPoint: pressure scaled by {@code maxPressure}, time relative to {@code t0}. */
    public static TinyPoint of(TouchPoint p, long t0, float maxPressure) {
        return new TinyPoint(p.x, p.y, (short) (p.pressure * maxPressure), (short) ((p.tiltX & 255) | (p.tiltY << 8)), (int) (p.timestamp - t0));
    }

    public float getX() { return x; }
    public float getY() { return y; }
    public short getPressure() { return pressure; }
    public short getSize() { return size; }
    public int getTime() { return time; }
    public void setX(float v) { x = v; }
    public void setY(float v) { y = v; }
    public void setPressure(short v) { pressure = v; }
    public void setSize(short v) { size = v; }
    public void setTime(int v) { time = v; }

    public TinyPoint copy(float x, float y, short pressure, short size, int time) {
        return new TinyPoint(x, y, pressure, size, time);
    }

    @Override public boolean equals(Object o) {
        if (!(o instanceof TinyPoint)) return false;
        TinyPoint t = (TinyPoint) o;
        return Float.compare(x, t.x) == 0 && Float.compare(y, t.y) == 0 && pressure == t.pressure && size == t.size && time == t.time;
    }

    @Override public int hashCode() {
        return ((((Float.hashCode(x) * 31 + Float.hashCode(y)) * 31 + pressure) * 31 + size) * 31) + time;
    }

    @Override public String toString() {
        return "TinyPoint(x=" + x + ", y=" + y + ", pressure=" + pressure + ", size=" + size + ", time=" + time + ")";
    }
}
