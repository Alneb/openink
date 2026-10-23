package org.openink.oracle;

import com.onyx.android.sdk.pen.NeoPenConfig;
import com.onyx.android.sdk.pen.NeoPenNative;
import com.onyx.android.sdk.pen.NeoPenResult;
import com.onyx.android.sdk.pen.PenInk;
import java.io.BufferedReader;
import java.io.FileReader;
import java.lang.reflect.Field;
import java.util.ArrayList;
import java.util.List;

/**
 * Feeds samples (`x y pressure tiltX tiltY tMs` per line) to libneo: down on the first, one move per sample, up on
 * the last. Prints, per call, `D|M|U nPieces` then one line per piece: its point count and floats.
 * args: PEN_TYPE SAMPLES [field=value ...] (NeoPenConfig fields), [--batch] = all moves in one call.
 */
public final class Main {
    public static void main(String[] a) throws Exception {
        System.load("/system/lib64/libneo_pen.so");
        int type = Integer.parseInt(a[0]);
        List<double[]> s = new ArrayList<>();
        try (BufferedReader r = new BufferedReader(new FileReader(a[1]))) {
            for (String l; (l = r.readLine()) != null; ) {
                l = l.trim();
                if (l.isEmpty() || l.startsWith("#")) continue;
                String[] f = l.split("\\s+");
                s.add(new double[]{d(f, 0), d(f, 1), d(f, 2), -1, d(f, 3), d(f, 4), d(f, 5)});
            }
        }
        NeoPenConfig cfg = new NeoPenConfig();
        cfg.type = type;
        boolean batch = false;
        for (int i = 2; i < a.length; i++) {
            if (a[i].equals("--batch")) { batch = true; continue; }
            String[] kv = a[i].split("=", 2);
            Field fl = NeoPenConfig.class.getField(kv[0]);
            Class<?> t = fl.getType();
            if (t == int.class) fl.setInt(cfg, Integer.parseInt(kv[1]));
            else if (t == boolean.class) fl.setBoolean(cfg, Boolean.parseBoolean(kv[1]));
            else fl.setFloat(cfg, Float.parseFloat(kv[1]));
        }
        NeoPenNative n = new NeoPenNative();
        long pen = n.nativeCreatePen(type, cfg);
        if (pen == 0) throw new IllegalStateException("nativeCreatePen returned 0");
        StringBuilder out = new StringBuilder();
        print(out, "D", n.nativeOnPenDown(pen, s.get(0), false));
        if (batch && s.size() > 2) {
            double[] all = new double[7 * (s.size() - 2)];
            for (int i = 1; i < s.size() - 1; i++) System.arraycopy(s.get(i), 0, all, 7 * (i - 1), 7);
            print(out, "M", n.nativeOnPenMove(pen, all, null, false));
        } else {
            for (int i = 1; i < s.size() - 1; i++) print(out, "M", n.nativeOnPenMove(pen, s.get(i), null, false));
        }
        print(out, "U", n.nativeOnPenUp(pen, s.get(s.size() - 1), false));
        n.nativeDestroyPen(pen);
        System.out.print(out);
    }

    private static double d(String[] f, int i) {
        return i < f.length ? Double.parseDouble(f[i]) : 0;
    }

    private static int bitmaps;

    private static void print(StringBuilder o, String tag, NeoPenResult r) {
        PenInk ink = r == null ? null : r.real;
        if (ink != null && ink.bitmaps != null) {
            for (android.graphics.Bitmap b : ink.bitmaps) {
                if (b == null) continue;
                String f = "/data/local/tmp/neo-bm/" + (bitmaps++) + ".png";
                try (java.io.FileOutputStream fo = new java.io.FileOutputStream(f)) {
                    b.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, fo);
                } catch (java.io.IOException e) {
                    throw new RuntimeException(e);
                }
                o.append("B ").append(f).append(' ').append(b.getWidth()).append(' ').append(b.getHeight()).append('\n');
            }
        }
        int pieces = ink == null || ink.sizes == null ? 0 : ink.sizes.length;
        o.append(tag).append(' ').append(pieces).append('\n');
        if (pieces == 0) return;
        int at = 0;
        for (int sz : ink.sizes) {
            o.append(sz);
            // sz counts points; the float stride per point is inferred from the array length below
            int stride = ink.points.length / Math.max(1, sum(ink.sizes));
            for (int k = 0; k < sz * stride && at < ink.points.length; k++) o.append(' ').append(ink.points[at++]);
            o.append('\n');
        }
    }

    private static int sum(int[] v) {
        int s = 0;
        for (int x : v) s += x;
        return s;
    }
}
