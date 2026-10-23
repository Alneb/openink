package org.openink.demo;

import java.util.List;
import org.openink.shape.Shape;

/** Host check: hold detection, shape outlines (arrow barbs, closed circle) and the scratch-out hull test. */
public final class ShapeSnapCheck {
    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    public static void main(String[] args) {
        // A stroke moving right for 10 samples, then resting 600 ms.
        float[] x = new float[16], y = new float[16];
        double[] t = new double[16];
        for (int i = 0; i < 16; i++) {
            x[i] = i < 10 ? i * 10 : 90 + (i % 2) * 1.5f; // rest jitter inside ±2 px
            t[i] = i < 10 ? i * 5_000 : 45_000 + (i - 9) * 100_000;
        }
        check(ShapeSnap.holdStart(x, y, t, ShapeSnap.HOLD_BOX_PX, ShapeSnap.HOLD_US) == 9, "hold starts where the pen stopped");
        check(ShapeSnap.holdStart(x, y, t, ShapeSnap.HOLD_BOX_PX, 2_000_000) == -1, "too short a rest is no hold");

        // Arrow: line (0,0)→(100,0) with a head at the end: the path must finish at the tip.
        Shape arrow = Shape.decode(new float[]{2, 1, 18, 28, 0.5f, 1, 0, 0, 0, 100, 0, 0, 1});
        List<double[]> a = ShapeSnap.trace(arrow.best(), 0.5, 0, 1000);
        double[] last = a.get(a.size() - 1);
        check(Math.abs(last[0] - 100) < 1e-3 && Math.abs(last[1]) < 1e-3, "arrow path ends at the tip");
        boolean barbBehind = false;
        for (double[] p : a) barbBehind |= p[0] < 99 && Math.abs(p[1]) > 5;
        check(barbBehind, "arrow has barbs behind the tip");
        check(a.get(0)[5] == 0 && last[5] == 1000 && a.get(0)[2] == 0.5, "times span the stroke, pressure kept");

        // Full circle r=10 at (50,50): closed, every point on the circle.
        Shape circle = Shape.decode(new float[]{2, 1, 15, 0, 0.1f, 1, 1, 50, 50, 10, 10, 0, 0, 6.2831855f, 0, 0});
        List<double[]> c = ShapeSnap.trace(circle.best(), 0.5, 0, 1000);
        for (double[] p : c) check(Math.abs(Math.hypot(p[0] - 50, p[1] - 50) - 10) < 1e-3, "on the circle");
        check(Math.hypot(c.get(0)[0] - c.get(c.size() - 1)[0], c.get(0)[1] - c.get(c.size() - 1)[1]) < 1e-3, "closed");

        float[] sq = {0, 0, 10, 0, 10, 10, 0, 10};
        check(ShapeSnap.inside(sq, 5, 5) && !ShapeSnap.inside(sq, 15, 5), "point in polygon");
        System.out.println("ShapeSnapCheck: OK");
    }
}
