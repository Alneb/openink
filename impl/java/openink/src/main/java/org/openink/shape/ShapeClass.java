package org.openink.shape;

/**
 * Recognised shape classes. {@code code} must match openink_shape::wire::class_code; {@code knote2Type}
 * is knote2's ShapeFactory id (40 = SHAPE_UNIVERSAL, its vertex-list shape; -1 = none).
 */
public enum ShapeClass {
    LINE(0, 7), POLYLINE(1, 31), POLYGON(2, 17),
    TRIANGLE(3, 8), ISOSCELES_TRIANGLE(4, 8), RIGHT_TRIANGLE(5, 12), RIGHT_ISOSCELES_TRIANGLE(6, 10),
    EQUILATERAL_TRIANGLE(7, 8),
    QUADRILATERAL(8, 40), TRAPEZOID(9, 26), PARALLELOGRAM(10, 40), RHOMBUS(11, 18), RECTANGLE(12, 1), SQUARE(13, 1),
    ELLIPSE(14, 0), CIRCLE(15, 0), ARC_OF_ELLIPSE(16, 39), ARC_OF_CIRCLE(17, 39),
    ARROW(18, 28), DOUBLE_ARROW(19, 40), POLYLINE_ARROW(20, 40), POLYLINE_DOUBLE_ARROW(21, 40),
    CURVED_ARROW(22, 40), CURVED_DOUBLE_ARROW(23, 40),
    SCRATCH_OUT(24, -1);

    public final int code;
    public final int knote2Type;

    ShapeClass(int code, int knote2Type) {
        this.code = code;
        this.knote2Type = knote2Type;
    }

    public static ShapeClass of(int code) {
        for (ShapeClass c : values()) if (c.code == code) return c;
        throw new IllegalArgumentException("unknown shape class code " + code);
    }
}
