package org.openink.compat.pen;

import static org.openink.compat.pen.RegionGate.Step.BEGIN;
import static org.openink.compat.pen.RegionGate.Step.END_OUT;
import static org.openink.compat.pen.RegionGate.Step.MOVE;
import static org.openink.compat.pen.RegionGate.Step.SKIP;

/** Host check: the native reader's cut of one contact into strokes at the region edge. */
public final class RegionGateCheck {
    public static void main(String[] args) {
        RegionGate g = new RegionGate();
        // in, out, back in, up: two strokes, the first ends out of region, the second normally.
        check(g.point(true) == BEGIN, "down inside opens a stroke");
        check(g.point(true) == MOVE, "inside extends it");
        check(g.point(false) == END_OUT, "leaving ends it, out of region");
        check(g.point(false) == SKIP, "still outside: nothing");
        check(g.point(true) == BEGIN, "re-entering opens a new stroke");
        check(g.up(), "pen up ends the open stroke");

        // Landing outside: the stroke starts where the pen enters; an up while outside reports nothing.
        check(g.point(false) == SKIP, "down outside: nothing");
        check(g.point(true) == BEGIN, "first point inside opens the stroke");
        check(g.point(false) == END_OUT, "and leaving ends it");
        check(!g.up(), "up outside: no second end");
        check(!g.up(), "up while idle: nothing");

        // Region tests pad the point by half the stroke width.
        check(RegionGate.fits(10, 10, 2, 8, 8, 20, 20), "padded point on the edge fits (inclusive)");
        check(!RegionGate.fits(9, 10, 2, 8, 8, 20, 20), "padding crosses the left edge");
        check(RegionGate.overlaps(7, 10, 1, 8, 8, 20, 20), "padding touches an exclude rect");
        check(!RegionGate.overlaps(6, 10, 1, 8, 8, 20, 20), "clear of the exclude rect");
        System.out.println("RegionGateCheck: OK");
    }

    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }
}
