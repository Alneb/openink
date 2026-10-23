package org.openink.pen;

/** Host check for MapCalibrator: pairing by time, refit, bad-pair rejection, locking. */
public final class MapCalibratorCheck {
    private static int failures;

    private static void check(boolean ok, String what) {
        if (!ok) {
            System.out.println("FAIL " + what);
            failures++;
        }
    }

    public static void main(String[] args) {
        PenAxes axes = PenAxes.fromArray(new int[]{0, 20832, 0, 15624, 0, 4095});
        // Truth: the 270° panel, but 12 px off the guess in x (a mounting offset the fit must find).
        double sx = 1860.0 / 15624, sy = 2480.0 / 20832;
        java.util.function.BiFunction<Integer, Integer, float[]> truth =
            (x, y) -> new float[]{(float) (1860 - y * sx + 12), (float) (x * sy)};

        MapCalibrator c = new MapCalibrator(axes, 1860, 2480);
        check(c.map().pairs == 0 && !c.isLocked(), "starts from the unfitted guess");

        // A screen contact with no raw contact nearby in time is ignored.
        check(!c.screenContact(PenReader.DOWN, 100, 100, 5_000), "unpaired contact changes nothing");

        int[][] pts = {{1000, 1000}, {19000, 2000}, {3000, 14000}, {17000, 13000}, {10000, 8000}, {5000, 6000}, {15000, 4000}, {8000, 12000}};
        long t = 10_000;
        boolean redrawSeen = false;
        for (int[] p : pts) {
            float[] s = truth.apply(p[0], p[1]);
            c.rawContact(PenReader.DOWN, p[0], p[1], t);
            redrawSeen |= c.screenContact(PenReader.DOWN, s[0], s[1], t + 30); // within the pair window
            t += 1_000;
        }
        check(redrawSeen, "the 12 px correction asks for a redraw");
        check(c.isLocked(), "8 exact pairs lock the map (pairs " + c.map().pairs + ", " + c.map() + ")");
        float[] mid = truth.apply(10416, 7812);
        check(Math.abs(c.map().screenX(10416, 7812) - mid[0]) < 0.01 && Math.abs(c.map().screenY(10416, 7812) - mid[1]) < 0.01,
            "locked map matches the truth at the centre");

        // Locked: a wild pair is ignored entirely.
        ScreenMap before = c.map();
        c.rawContact(PenReader.UP, 500, 500, t);
        check(!c.screenContact(PenReader.UP, 9999, -9999, t + 10) && c.map() == before, "a locked map ignores new pairs");

        // Unlocked: a mismatched pair (wrong contact) is rejected and the previous fit kept.
        MapCalibrator d = new MapCalibrator(axes, 1860, 2480);
        t = 50_000;
        for (int i = 0; i < 4; i++) {
            float[] s = truth.apply(pts[i][0], pts[i][1]);
            d.rawContact(PenReader.DOWN, pts[i][0], pts[i][1], t);
            d.screenContact(PenReader.DOWN, s[0], s[1], t);
            t += 1_000;
        }
        ScreenMap good = d.map();
        d.rawContact(PenReader.DOWN, 1000, 1000, t);
        d.screenContact(PenReader.DOWN, 1500, 200, t); // far from where (1000,1000) lands
        check(d.map() == good && d.map().pairs == 4, "a bad pair is dropped, the fit kept (" + d.map() + ")");

        // A quick double tap: both raw downs are in the window; the screen down pairs with the nearer one.
        MapCalibrator f = new MapCalibrator(axes, 1860, 2480);
        t = 90_000;
        for (int i = 0; i < 4; i++) {
            float[] s = truth.apply(pts[i][0], pts[i][1]);
            f.rawContact(PenReader.DOWN, pts[i][0], pts[i][1], t);
            f.screenContact(PenReader.DOWN, s[0], s[1], t);
            t += 1_000;
        }
        f.rawContact(PenReader.DOWN, pts[4][0], pts[4][1], t);
        f.rawContact(PenReader.DOWN, pts[5][0], pts[5][1], t + 150);
        float[] s4 = truth.apply(pts[4][0], pts[4][1]);
        f.screenContact(PenReader.DOWN, s4[0], s4[1], t + 20);
        check(f.map().pairs == 5, "pairs with the nearest raw contact in time (" + f.map() + ")");

        // Only DOWN/UP feed pairing; a MOVE never pairs.
        MapCalibrator e = new MapCalibrator(axes, 1860, 2480);
        e.rawContact(PenReader.MOVE, 1000, 1000, 1);
        check(!e.screenContact(PenReader.MOVE, 1, 1, 1) && e.map().pairs == 0, "moves are not contacts");

        System.out.println("MapCalibratorCheck: " + failures + " failures");
        if (failures > 0) System.exit(1);
    }
}
