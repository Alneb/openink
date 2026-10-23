package org.openink.shape;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/**
 * A recognition result: {@link Status#SHAPE} with candidates (best first), {@link Status#SCRATCH_OUT} with
 * the hull of the gesture (erase what it covers), or {@link Status#REJECTED} (leave the ink alone).
 * Decodes the float[] layout documented in crates/openink-shape/src/wire.rs.
 */
public final class Shape {
    public enum Status { REJECTED, SCRATCH_OUT, SHAPE }

    /** A line or arc end ornament; wire codes 0, 1, 2 (wire.rs). */
    public enum Head {
        NONE, OPEN, CLOSED;

        static Head of(float code) {
            int c = (int) code;
            return c >= 0 && c < values().length ? values()[c] : NONE;
        }
    }

    /** A straight segment ({@code arc == false}: x1 y1 x2 y2) or an elliptic arc (cx cy rx ry phi start sweep). */
    public static final class Primitive {
        public final boolean arc;
        /** Line: {x1, y1, x2, y2}. Arc: {cx, cy, rx, ry, phi, start, sweep} (radians). */
        public final float[] p;
        public final Head headAtStart, headAtEnd;
        /** Any head (open or closed) at that end. */
        public final boolean arrowAtStart, arrowAtEnd;

        Primitive(boolean arc, float[] p, Head headAtStart, Head headAtEnd) {
            this.arc = arc;
            this.p = p;
            this.headAtStart = headAtStart;
            this.headAtEnd = headAtEnd;
            this.arrowAtStart = headAtStart != Head.NONE;
            this.arrowAtEnd = headAtEnd != Head.NONE;
        }
    }

    public static final class Candidate {
        public final ShapeClass shapeClass;
        /** Lower is better. */
        public final float cost;
        public final List<Primitive> primitives;

        Candidate(ShapeClass c, float cost, List<Primitive> prims) {
            this.shapeClass = c;
            this.cost = cost;
            this.primitives = Collections.unmodifiableList(prims);
        }
    }

    public final Status status;
    public final List<Candidate> candidates;
    /** Scratch-out hull: x0, y0, x1, y1, ... (empty otherwise). */
    public final float[] hull;

    private Shape(Status s, List<Candidate> c, float[] hull) {
        this.status = s;
        this.candidates = Collections.unmodifiableList(c);
        this.hull = hull;
    }

    public Candidate best() {
        return candidates.isEmpty() ? null : candidates.get(0);
    }

    /** Decode the wire array (public so host checks can build results without the native library). */
    public static Shape decode(float[] v) {
        int[] at = {1};
        switch ((int) v[0]) {
            case 0:
                return new Shape(Status.REJECTED, new ArrayList<>(), new float[0]);
            case 1: {
                int n = (int) v[at[0]++];
                float[] hull = new float[2 * n];
                System.arraycopy(v, at[0], hull, 0, 2 * n);
                return new Shape(Status.SCRATCH_OUT, new ArrayList<>(), hull);
            }
            case 2: {
                int count = (int) v[at[0]++];
                List<Candidate> cands = new ArrayList<>(count);
                for (int i = 0; i < count; i++) {
                    ShapeClass c = ShapeClass.of((int) v[at[0]++]);
                    at[0]++; // knote2 type: also on ShapeClass
                    float cost = v[at[0]++];
                    int np = (int) v[at[0]++];
                    List<Primitive> prims = new ArrayList<>(np);
                    for (int k = 0; k < np; k++) {
                        boolean arc = v[at[0]++] == 1f;
                        float[] p = new float[arc ? 7 : 4];
                        System.arraycopy(v, at[0], p, 0, p.length);
                        at[0] += p.length;
                        Head d1 = Head.of(v[at[0]++]), d2 = Head.of(v[at[0]++]);
                        prims.add(new Primitive(arc, p, d1, d2));
                    }
                    cands.add(new Candidate(c, cost, prims));
                }
                return new Shape(Status.SHAPE, cands, new float[0]);
            }
            default:
                throw new IllegalArgumentException("unknown shape status " + v[0]);
        }
    }
}
