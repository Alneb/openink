package org.openink;

import org.openink.pen.ScreenMap;

/** Host check for ScreenMap.fit: recovers a known rotation + flip, and refuses degenerate input. */
public final class ScreenMapCheck {
    public static void main(String[] args) {
        int failures = 0;
        // A truth map: the 270° panel (raw 20832×15624 → screen 1860×2480), screen x reversed.
        double[][] raw = {{0, 0}, {20832, 0}, {0, 15624}, {10000, 7000}, {3000, 12000}};
        double[][] scr = new double[raw.length][];
        for (int i = 0; i < raw.length; i++) {
            scr[i] = new double[]{1860 - raw[i][1] * 1860 / 15624, raw[i][0] * 2480 / 20832};
        }
        ScreenMap m = ScreenMap.fit(raw, scr);
        if (m == null || m.rmsPx > 1e-6) { System.out.println("FAIL exact fit: " + m); failures++; }
        else if (Math.abs(m.screenX(10416, 7812) - 930) > 1e-3 || Math.abs(m.screenY(10416, 7812) - 1240) > 1e-3) {
            System.out.println("FAIL centre maps to " + m.screenX(10416, 7812) + "," + m.screenY(10416, 7812));
            failures++;
        }
        // The unfitted guess must be the same orientation as this truth.
        ScreenMap g = ScreenMap.rotated(20832, 15624, 1860, 2480);
        if (Math.abs(g.screenX(0, 0) - 1860) > 1e-3 || Math.abs(g.screenY(20832, 0) - 2480) > 1e-3) {
            System.out.println("FAIL guess: " + g);
            failures++;
        }
        if (ScreenMap.fit(new double[][]{{0, 0}, {1, 1}, {2, 2}}, new double[][]{{0, 0}, {1, 1}, {2, 2}}) != null) {
            System.out.println("FAIL collinear points accepted");
            failures++;
        }
        System.out.println("ScreenMapCheck: " + m + "; " + failures + " failures");
        if (failures > 0) System.exit(1);
    }
}
