package org.openink.stroke;

import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.graphics.Matrix;
import android.graphics.Paint;
import android.graphics.Path;
import android.graphics.PorterDuff;
import android.graphics.PorterDuffColorFilter;
import android.util.SparseArray;

/**
 * Draws Ink on a Canvas. Translucent ink is drawn opaque here: the caller draws it into a
 * per-stroke layer and composites that at {@link PenKind#TRANSLUCENT_ALPHA}.
 */
public final class InkPainter {
    private final Paint fill = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint dab = new Paint(Paint.FILTER_BITMAP_FLAG);
    private final Path path = new Path();
    private final Matrix m = new Matrix();
    private final SparseArray<Bitmap> masks = new SparseArray<>();

    public InkPainter(int color) {
        fill.setStyle(Paint.Style.FILL);
        fill.setColor(color);
        dab.setColorFilter(new PorterDuffColorFilter(color | 0xff000000, PorterDuff.Mode.SRC_IN));
    }

    public void draw(Canvas c, Ink ink) {
        if (ink.isEmpty()) return;
        if (ink.output == PenKind.Output.DABS) drawDabs(c, ink.data);
        else drawPolygons(c, ink.data);
    }

    private void drawPolygons(Canvas c, float[] d) {
        path.rewind();
        path.setFillType(Path.FillType.WINDING); // all polygons share one orientation: union
        for (int i = 0; i < d.length; ) {
            int n = (int) d[i++];
            path.moveTo(d[i], d[i + 1]);
            for (int k = 1; k < n; k++) path.lineTo(d[i + 2 * k], d[i + 2 * k + 1]);
            path.close();
            i += 2 * n;
        }
        c.drawPath(path, fill);
    }

    private void drawDabs(Canvas c, float[] d) {
        for (int i = 0; i + 4 < d.length; i += 5) {
            int size = StrokeEngine.maskSize(d[i + 2]);
            Bitmap mask = mask(size);
            m.setTranslate(-size / 2f, -size / 2f);
            m.postRotate((float) Math.toDegrees(d[i + 3]));
            m.postTranslate(d[i], d[i + 1]);
            dab.setAlpha(Math.round(255 * d[i + 4]));
            c.drawBitmap(mask, m, dab);
        }
    }

    private Bitmap mask(int size) {
        Bitmap b = masks.get(size);
        if (b != null) return b;
        byte[] a = StrokeEngine.grainMask(size);
        int[] argb = new int[a.length];
        for (int i = 0; i < a.length; i++) argb[i] = (a[i] & 0xff) << 24; // black, alpha = mask
        b = Bitmap.createBitmap(argb, size, size, Bitmap.Config.ARGB_8888);
        masks.put(size, b);
        return b;
    }
}
