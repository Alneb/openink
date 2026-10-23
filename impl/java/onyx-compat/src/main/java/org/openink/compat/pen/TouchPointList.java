package org.openink.compat.pen;

import android.graphics.Matrix;
import android.graphics.PointF;
import android.graphics.RectF;
import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;

/**
 * Onyx's {@code com.onyx.android.sdk.pen.data.TouchPointList}. Mirrors its semantics where they
 * are observable: {@link #clear()} swaps in a new list (a caller holding {@link #getPoints()}
 * keeps its points), and the transforms edit the points in place. The storage helpers
 * ({@link #toTinyPointList}, {@link #detachPointList}, {@link #pointsToByteArray}) use knote2's formats.
 */
public class TouchPointList implements Iterable<TouchPoint>, Cloneable {
    /** Above this many points {@link #getRenderPoints()} subsamples (Onyx's threshold). */
    private static final int RENDER_POINT_LIMIT = 20_000;

    private List<TouchPoint> points;

    public TouchPointList() {
        points = new ArrayList<>();
    }

    public TouchPointList(int capacity) {
        points = new ArrayList<>(capacity);
    }

    public void add(TouchPoint p) {
        points.add(p);
    }

    public void add(int index, TouchPoint p) {
        points.add(index, p);
    }

    public void addAll(TouchPointList other) {
        if (other != null) points.addAll(other.points);
    }

    public TouchPoint get(int i) {
        return points.get(i);
    }

    public TouchPoint first() {
        return points.isEmpty() ? null : points.get(0);
    }

    public TouchPoint last() {
        return points.isEmpty() ? null : points.get(points.size() - 1);
    }

    public final List<TouchPoint> getPoints() {
        return points;
    }

    public TouchPointList setPoints(List<TouchPoint> list) {
        points = list;
        return this;
    }

    /** All points, or an evenly spaced subset of at most about 20 000 for very long strokes. */
    public final List<TouchPoint> getRenderPoints() {
        int n = points.size();
        if (n <= RENDER_POINT_LIMIT) return points;
        int step = (n + RENDER_POINT_LIMIT - 1) / RENDER_POINT_LIMIT;
        List<TouchPoint> out = new ArrayList<>(n / step + 1);
        for (int i = 0; i < n; i += step) out.add(points.get(i));
        return out;
    }

    public int size() {
        return points.size();
    }

    public boolean isEmpty() {
        return points.isEmpty();
    }

    public void clear() {
        points = new ArrayList<>();
    }

    @Override public Iterator<TouchPoint> iterator() {
        return points.iterator();
    }

    /** A deep copy: the points are copied too. */
    @Override public TouchPointList clone() {
        TouchPointList c = new TouchPointList(points.size());
        for (TouchPoint p : points) c.points.add(p.copy());
        return c;
    }

    /** Map every point through {@code m}, in place; null or identity does nothing. */
    public TouchPointList applyMatrix(Matrix m) {
        if (m == null || m.isIdentity()) return this;
        float[] xy = new float[2];
        for (TouchPoint p : points) {
            xy[0] = p.x;
            xy[1] = p.y;
            m.mapPoints(xy);
            p.x = xy[0];
            p.y = xy[1];
        }
        return this;
    }

    public TouchPointList cloneMatrixPoints(Matrix m) {
        return clone().applyMatrix(m);
    }

    /** Mirror across x = {@code at}/2 (X axis) or y = {@code at}/2 (Y axis): x → at − x. */
    public void mirrorAllPoints(MirrorType type, float at) {
        for (TouchPoint p : points) {
            if (type == MirrorType.XAxisMirror) p.x = at - p.x; else p.y = at - p.y;
        }
    }

    /** Rotate by {@code degrees} around {@code pivot}. */
    public void rotateAllPoints(float degrees, PointF pivot) {
        Matrix m = new Matrix();
        m.setRotate(degrees, pivot.x, pivot.y);
        applyMatrix(m);
    }

    public void scaleAllPoints(float factor) {
        for (TouchPoint p : points) {
            p.x *= factor;
            p.y *= factor;
        }
    }

    public void translateAllPoints(float dx, float dy) {
        for (TouchPoint p : points) {
            p.x += dx;
            p.y += dy;
        }
    }

    /** Bounds of the points' positions; an empty rect for no points. */
    public static RectF getBoundingRect(List<TouchPoint> list) {
        RectF r = new RectF();
        if (list == null || list.isEmpty()) return r;
        r.set(list.get(0).x, list.get(0).y, list.get(0).x, list.get(0).y);
        for (TouchPoint p : list) r.union(p.x, p.y);
        return r;
    }

    public void removeAll(List<TouchPoint> remove) {
        points.removeAll(remove);
    }

    /** knote2's stored form; pressure scaled by {@code maxPressure}, times relative to the first point. */
    public List<TinyPoint> toTinyPointList(float maxPressure) {
        List<TinyPoint> out = new ArrayList<>(points.size());
        if (points.isEmpty()) return out;
        long t0 = points.get(0).timestamp;
        for (TouchPoint p : points) out.add(TinyPoint.of(p, t0, maxPressure));
        return out;
    }

    public List<TinyPoint> toTinyPointList() {
        return toTinyPointList(1f);
    }

    /** The points as TinyPoints, then this list emptied in place (unlike {@link #clear()}). */
    public List<TinyPoint> detachPointList() {
        List<TinyPoint> out = toTinyPointList();
        points.clear();
        return out;
    }

    /** Null, or equal to the identity matrix. */
    public static boolean isEmptyMatrix(Matrix m) {
        return m == null || m.isIdentity();
    }

    /** 24 B per point, big-endian: x, y, pressure, size (float), timestamp (long); tilt is not stored. */
    public static byte[] pointsToByteArray(TouchPointList list) {
        if (list == null) return new byte[0];
        java.nio.ByteBuffer b = java.nio.ByteBuffer.allocate(24 * list.points.size());
        for (TouchPoint p : list.points) b.putFloat(p.x).putFloat(p.y).putFloat(p.pressure).putFloat(p.size).putLong(p.timestamp);
        return b.array();
    }

    public static TouchPointList pointsFromByteArray(byte[] blob) {
        TouchPointList out = new TouchPointList(100);
        if (blob == null) return out;
        java.nio.ByteBuffer b = java.nio.ByteBuffer.wrap(blob);
        for (int i = 0; i < blob.length / 24; i++) {
            out.add(new TouchPoint(b.getFloat(), b.getFloat(), b.getFloat(), b.getFloat(), b.getLong()));
        }
        return out;
    }
}
