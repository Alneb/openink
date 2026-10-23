package org.openink.compat.pen;

import java.util.List;

/** Host check (android.jar on the classpath, no Android calls): knote2's point formats round-trip. */
public final class TouchPointListCheck {
    public static void main(String[] args) {
        TouchPointList l = new TouchPointList();
        l.add(new TouchPoint(1.5f, 2.5f, 0.75f, 3f, 10, -5, 1000));
        l.add(new TouchPoint(4f, 8f, 0.25f, 0f, 63, 63, 1016));

        byte[] b = TouchPointList.pointsToByteArray(l);
        check(b.length == 48, "24 B per point, got " + b.length);
        check(b[0] == 0x3f && b[1] == (byte) 0xc0, "big-endian float 1.5 first");
        TouchPointList back = TouchPointList.pointsFromByteArray(b);
        check(back.size() == 2, "two points back");
        TouchPoint p = back.get(1);
        check(p.x == 4f && p.y == 8f && p.pressure == 0.25f && p.timestamp == 1016, "fields survive: " + p);

        List<TinyPoint> t = l.toTinyPointList(4095f);
        check(t.get(0).getTime() == 0 && t.get(1).getTime() == 16, "times relative to the first point");
        check(t.get(0).getPressure() == (short) (0.75f * 4095f), "pressure scaled");
        check(t.get(0).getSize() == (short) ((10 & 255) | (-5 << 8)), "tilt packed into size");
        check(l.detachPointList().size() == 2 && l.size() == 0, "detach empties in place");
        System.out.println("TouchPointListCheck: OK");
    }

    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }
}
