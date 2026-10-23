package org.openink.demo;

import java.util.ArrayList;
import java.util.List;
import org.openink.shape.Shape;

/**
 * Hold-to-snap helpers (knote2's gesture: finish a stroke by resting the pen, and it becomes a clean shape).
 * Pure functions on point lists, so they run in host checks without Android.
 */
final class ShapeSnap {
    /** knote2's hold (docs/SHAPE-RECOGNITION.md): every point within ±2 view px in x and y of the anchor for > 500 ms. */
    static final float HOLD_BOX_PX = 2f;
    static final long HOLD_US = 500_000;
    /** A stroke is scratched out when this share of its points lies inside the gesture's hull. */
    static final float SCRATCH_COVER = 0.6f;
    /** Arrow barbs: length relative to the decorated primitive, and half-angle. */
    static final double BARB_FRACTION = 0.15, BARB_ANGLE = Math.toRadians(25);

    private ShapeSnap() {}

    /**
     * Index where the final rest begins (view-px points {@code x, y}, times in µs), or -1 if the pen did not
     * stay within ±{@code box} px (x and y) of the last point for more than {@code holdUs}.
     */
    static int holdStart(float[] x, float[] y, double[] tUs, float box, long holdUs) {
        int n = x.length;
        if (n < 2) return -1;
        int k = n - 1;
        while (k > 0 && Math.abs(x[k - 1] - x[n - 1]) <= box && Math.abs(y[k - 1] - y[n - 1]) <= box) k--;
        return tUs[n - 1] - tUs[k] > holdUs && k > 0 ? k : -1;
    }

    /**
     * The candidate's outline as stroke points {x, y, pressure, 0, 0, tUs}, in the coordinate space the ink
     * was recognised in. Arrow heads are drawn in the same pen-down path: open tip → barb → tip → barb → tip,
     * closed tip → barb → barb → tip.
     */
    static List<double[]> trace(Shape.Candidate c, double pressure, double t0Us, double t1Us) {
        List<float[]> xy = new ArrayList<>();
        for (Shape.Primitive p : c.primitives) {
            List<float[]> part = sample(p);
            if (p.arrowAtStart) part.addAll(0, barbs(part, true, p.headAtStart == Shape.Head.CLOSED));
            if (p.arrowAtEnd) part.addAll(barbs(part, false, p.headAtEnd == Shape.Head.CLOSED));
            if (!xy.isEmpty() && !part.isEmpty() && near(xy.get(xy.size() - 1), part.get(0))) part.remove(0);
            xy.addAll(part);
        }
        List<double[]> out = new ArrayList<>(xy.size());
        for (int i = 0; i < xy.size(); i++) {
            double t = xy.size() == 1 ? t0Us : t0Us + (t1Us - t0Us) * i / (xy.size() - 1);
            out.add(new double[]{xy.get(i)[0], xy.get(i)[1], pressure, 0, 0, t});
        }
        return out;
    }

    /** Lines: 24 segments; arcs: 64 per full turn (at least 8). */
    private static List<float[]> sample(Shape.Primitive p) {
        List<float[]> out = new ArrayList<>();
        if (!p.arc) {
            for (int i = 0; i <= 24; i++) {
                float t = i / 24f;
                out.add(new float[]{p.p[0] + (p.p[2] - p.p[0]) * t, p.p[1] + (p.p[3] - p.p[1]) * t});
            }
            return out;
        }
        float cx = p.p[0], cy = p.p[1], rx = p.p[2], ry = p.p[3], phi = p.p[4], start = p.p[5], sweep = p.p[6];
        int n = Math.max(8, (int) Math.ceil(Math.abs(sweep) / (2 * Math.PI) * 64));
        double cs = Math.cos(phi), sn = Math.sin(phi);
        for (int i = 0; i <= n; i++) {
            double t = start + sweep * i / n, ex = rx * Math.cos(t), ey = ry * Math.sin(t);
            out.add(new float[]{(float) (cx + ex * cs - ey * sn), (float) (cy + ex * sn + ey * cs)});
        }
        return out;
    }

    /** The head at one end of a sampled primitive, as a path that starts and ends at the tip (closed: a triangle). */
    private static List<float[]> barbs(List<float[]> pts, boolean atStart, boolean closed) {
        List<float[]> out = new ArrayList<>();
        if (pts.size() < 2) return out;
        float[] tip = atStart ? pts.get(0) : pts.get(pts.size() - 1);
        // Direction from the tip back along the primitive, a few samples in (robust on arcs).
        float[] in = atStart ? pts.get(Math.min(3, pts.size() - 1)) : pts.get(Math.max(0, pts.size() - 4));
        double len = 0;
        for (int i = 1; i < pts.size(); i++) len += Math.hypot(pts.get(i)[0] - pts.get(i - 1)[0], pts.get(i)[1] - pts.get(i - 1)[1]);
        double back = Math.atan2(in[1] - tip[1], in[0] - tip[0]), barb = BARB_FRACTION * len;
        for (int s = -1; s <= 1; s += 2) {
            double a = back + s * BARB_ANGLE;
            out.add(new float[]{(float) (tip[0] + barb * Math.cos(a)), (float) (tip[1] + barb * Math.sin(a))});
            if (!closed || s == 1) out.add(tip.clone());
        }
        if (atStart) {
            // Prepended: the path must end at the tip so the primitive continues from it.
            out.add(0, tip.clone());
            out.remove(out.size() - 1);
        }
        return out;
    }

    private static boolean near(float[] a, float[] b) {
        return Math.abs(a[0] - b[0]) < 1e-3f && Math.abs(a[1] - b[1]) < 1e-3f;
    }

    /** Even-odd point-in-polygon; {@code poly} is x0, y0, x1, y1, ... */
    static boolean inside(float[] poly, float x, float y) {
        boolean in = false;
        int n = poly.length / 2;
        for (int i = 0, j = n - 1; i < n; j = i++) {
            float xi = poly[2 * i], yi = poly[2 * i + 1], xj = poly[2 * j], yj = poly[2 * j + 1];
            if ((yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi) in = !in;
        }
        return in;
    }
}
