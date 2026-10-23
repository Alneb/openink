package org.openink.shape;

/** Host check: Shape.decode reads the layout openink_shape::wire::encode writes (same arrays as its tests). */
public final class ShapeWireCheck {
    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    public static void main(String[] args) {
        Shape s = Shape.decode(new float[] {2, 1, 18, 28, 0.5f, 1, 0, 1, 2, 3, 4, 0, 1});
        check(s.status == Shape.Status.SHAPE && s.candidates.size() == 1, "shape status");
        Shape.Candidate c = s.best();
        check(c.shapeClass == ShapeClass.ARROW && c.shapeClass.knote2Type == 28 && c.cost == 0.5f, "arrow candidate");
        Shape.Primitive p = c.primitives.get(0);
        check(!p.arc && p.p[0] == 1 && p.p[3] == 4 && !p.arrowAtStart && p.arrowAtEnd, "line primitive");
        check(p.headAtStart == Shape.Head.NONE && p.headAtEnd == Shape.Head.OPEN, "open head");
        Shape.Primitive closed = Shape.decode(new float[] {2, 1, 18, 28, 0.5f, 1, 0, 1, 2, 3, 4, 2, 0}).best().primitives.get(0);
        check(closed.arrowAtStart && closed.headAtStart == Shape.Head.CLOSED && !closed.arrowAtEnd, "closed head");
        Shape arc = Shape.decode(new float[] {2, 1, 15, 0, 0.1f, 1, 1, 5, 6, 2, 2, 0, 0, 6.2831855f, 0, 0});
        check(arc.best().shapeClass == ShapeClass.CIRCLE && arc.best().primitives.get(0).arc, "circle arc");
        check(arc.best().primitives.get(0).p[6] == 6.2831855f, "arc sweep");
        check(Shape.decode(new float[] {0}).status == Shape.Status.REJECTED, "rejected");
        Shape so = Shape.decode(new float[] {1, 1, 5, 6});
        check(so.status == Shape.Status.SCRATCH_OUT && so.hull.length == 2 && so.hull[1] == 6, "scratch-out hull");
        for (ShapeClass k : ShapeClass.values()) check(ShapeClass.of(k.code) == k, "code round trip " + k);
        System.out.println("ShapeWireCheck: OK (" + ShapeClass.values().length + " classes)");
    }
}
