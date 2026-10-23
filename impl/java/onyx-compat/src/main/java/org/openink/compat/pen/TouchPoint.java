package org.openink.compat.pen;

/**
 * Onyx's {@code com.onyx.android.sdk.data.note.TouchPoint}: public fields, same names. Coordinates
 * are host-view pixels; {@code pressure} is in raw digitizer units (0..{@link TouchHelper#getMaxPressure()}),
 * as Onyx reports it; {@code timestamp} is milliseconds.
 */
public class TouchPoint {
    public float x, y, pressure, size;
    public int tiltX, tiltY;
    public long timestamp;

    public TouchPoint() {}

    public TouchPoint(float x, float y) {
        this.x = x;
        this.y = y;
    }

    public TouchPoint(float x, float y, float pressure, float size, long timestamp) {
        this(x, y, pressure, size, 0, 0, timestamp);
    }

    public TouchPoint(float x, float y, float pressure, float size, int tiltX, int tiltY, long timestamp) {
        this.x = x;
        this.y = y;
        this.pressure = pressure;
        this.size = size;
        this.tiltX = tiltX;
        this.tiltY = tiltY;
        this.timestamp = timestamp;
    }

    public TouchPoint(TouchPoint o) {
        this(o.x, o.y, o.pressure, o.size, o.tiltX, o.tiltY, o.timestamp);
    }

    public float getX() { return x; }
    public float getY() { return y; }
    public float getPressure() { return pressure; }
    public float getSize() { return size; }
    public int getTiltX() { return tiltX; }
    public int getTiltY() { return tiltY; }
    public long getTimestamp() { return timestamp; }
    public void setTimestamp(long t) { timestamp = t; }

    public void set(TouchPoint o) {
        x = o.x; y = o.y; pressure = o.pressure; size = o.size; tiltX = o.tiltX; tiltY = o.tiltY; timestamp = o.timestamp;
    }

    public void offset(float dx, float dy) {
        x += dx;
        y += dy;
    }

    public TouchPoint copy() {
        return new TouchPoint(this);
    }

    @Override public String toString() {
        return "TouchPoint(" + x + ", " + y + ", p=" + pressure + ", tilt " + tiltX + "," + tiltY + ", t=" + timestamp + ")";
    }
}
