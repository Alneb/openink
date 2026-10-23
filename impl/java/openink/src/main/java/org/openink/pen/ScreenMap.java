package org.openink.pen;

/**
 * Raw digitizer coordinates → screen pixels, as an affine map
 * {@code sx = a·x + b·y + c, sy = d·x + e·y + f}. Affine covers the axis swap, flips and scale of
 * any panel orientation (the Note Air 5C's IDC says ORIENTATION_270) without a table of cases.
 * Fitted from pairs of (raw, screen) points, e.g. evdev pen-downs matched with MotionEvents.
 */
public final class ScreenMap {
    private final double a, b, c, d, e, f;
    /** RMS residual of the fit in screen pixels; NaN for a map that was not fitted. */
    public final double rmsPx;
    public final int pairs;

    private ScreenMap(double[] sx, double[] sy, double rmsPx, int pairs) {
        this.a = sx[0]; this.b = sx[1]; this.c = sx[2];
        this.d = sy[0]; this.e = sy[1]; this.f = sy[2];
        this.rmsPx = rmsPx;
        this.pairs = pairs;
    }

    /**
     * Unfitted guess for a raw device rotated against the screen: raw x runs along screen y and
     * raw y along screen x, reversed. Only a starting point; {@link #fit} replaces it.
     */
    public static ScreenMap rotated(int rawMaxX, int rawMaxY, int screenW, int screenH) {
        double sxPerY = -(double) screenW / rawMaxY, syPerX = (double) screenH / rawMaxX;
        return new ScreenMap(new double[]{0, sxPerY, screenW}, new double[]{syPerX, 0, 0}, Double.NaN, 0);
    }

    /**
     * Least-squares fit over {@code raw[i] → screen[i]} ({x, y} pairs).
     * @return null when the points cannot determine a map (fewer than 3, or all on one line)
     */
    public static ScreenMap fit(double[][] raw, double[][] screen) {
        int n = raw.length;
        if (n < 3 || screen.length != n) return null;
        // Normal equations M·p = v with M = Σ [x y 1]ᵀ[x y 1], shared by both output axes.
        double[][] m = new double[3][3];
        double[] vx = new double[3], vy = new double[3];
        for (int i = 0; i < n; i++) {
            double[] r = {raw[i][0], raw[i][1], 1};
            for (int j = 0; j < 3; j++) {
                for (int k = 0; k < 3; k++) m[j][k] += r[j] * r[k];
                vx[j] += r[j] * screen[i][0];
                vy[j] += r[j] * screen[i][1];
            }
        }
        double[] px = solve3(m, vx), py = solve3(m, vy);
        if (px == null || py == null) return null;
        double sq = 0;
        for (int i = 0; i < n; i++) {
            double ex = px[0] * raw[i][0] + px[1] * raw[i][1] + px[2] - screen[i][0];
            double ey = py[0] * raw[i][0] + py[1] * raw[i][1] + py[2] - screen[i][1];
            sq += ex * ex + ey * ey;
        }
        return new ScreenMap(px, py, Math.sqrt(sq / n), n);
    }

    public float screenX(int x, int y) {
        return (float) (a * x + b * y + c);
    }

    public float screenY(int x, int y) {
        return (float) (d * x + e * y + f);
    }

    /** Screen pixels per raw unit along the larger axis; for converting widths. */
    public double scale() {
        return Math.max(Math.hypot(a, d), Math.hypot(b, e));
    }

    @Override public String toString() {
        return String.format(java.util.Locale.ROOT, "sx=%.5f·x%+.5f·y%+.1f sy=%.5f·x%+.5f·y%+.1f (%s)",
            a, b, c, d, e, f, pairs == 0 ? "guess" : pairs + " pairs, rms " + String.format(java.util.Locale.ROOT, "%.1f px", rmsPx));
    }

    /** Cramer's rule; null when singular (collinear points). */
    private static double[] solve3(double[][] m, double[] v) {
        double det = det3(m);
        if (Math.abs(det) < 1e-9 * Math.max(1, Math.abs(m[0][0] * m[1][1] * m[2][2]))) return null;
        double[] out = new double[3];
        for (int col = 0; col < 3; col++) {
            double[][] t = new double[3][];
            for (int r = 0; r < 3; r++) {
                t[r] = m[r].clone();
                t[r][col] = v[r];
            }
            out[col] = det3(t) / det;
        }
        return out;
    }

    private static double det3(double[][] m) {
        return m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
             - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
             + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    }
}
