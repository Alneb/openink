package org.openink.pen;

import android.view.View;
import org.openink.backend.EpdBackend;

/**
 * Instant ink (SurfaceFlinger draws the stroke straight from evdev) over one view.
 * Encodes the stock SDK's order (docs/ONYX-EPD-PEN.md, "Full SDK open sequence") as a state
 * machine, so out-of-order calls throw instead of silently leaving ink off.
 */
public final class InkSession {
    private enum Phase { CLOSED, OPEN, RENDERING, PAUSED }

    /** TouchHelper's default for setEraserRawDrawingEnabled(false, 5). */
    private static final int ERASER_DEFAULT_ARG = 5;

    private final EpdBackend sf;
    private final View view;
    private Phase phase = Phase.CLOSED;

    public InkSession(EpdBackend backend, View view) {
        this.sf = backend;
        this.view = view;
    }

    /**
     * Enter handwriting mode, limited to {@code limitRects} (flat l,t,r,b, view-relative).
     * Call after layout: the framework adds the view's screen position.
     */
    public void open(int[] limitRects, float strokeWidth) {
        require(Phase.CLOSED, "open");
        if (!view.isLaidOut()) throw new IllegalStateException("open() before layout: region would be misplaced");
        // From the first call on SF may be in handwriting mode: a failure part-way must undo it,
        // or the app is left with SF ink half-on and a session that thinks it is closed.
        phase = Phase.OPEN;
        try {
            sf.setStrokeStyle(StrokeStyle.PENCIL.value);
            sf.setPenState(PenCommand.START.value);
            sf.setRegionLimit(view, limitRects);
            sf.setRegionMode(0); // multi-region (0), as the stock SDK sends; 1 = single (SF-ONYX.md)
            sf.setStrokeWidth(strokeWidth);
            resume();
        } catch (RuntimeException e) {
            try {
                close();
            } catch (RuntimeException undo) {
                e.addSuppressed(undo);
            } finally {
                phase = Phase.CLOSED;
            }
            throw e;
        }
    }

    /** Width of SF's instant ink, e.g. after the user picks another pen. */
    public void setStrokeWidth(float px) {
        if (phase == Phase.CLOSED) throw wrong("setStrokeWidth");
        sf.setStrokeWidth(px);
    }

    /** Render on. The step whose absence leaves an app at "state 1, no ink". */
    public void resume() {
        if (phase != Phase.OPEN && phase != Phase.PAUSED) throw wrong("resume");
        sf.setPenState(PenCommand.RESUME.value);
        sf.setBrushRawDrawingEnabled(true);
        sf.setEraserRawDrawingEnabled(false, ERASER_DEFAULT_ARG);
        phase = Phase.RENDERING;
    }

    /** Render off, e.g. while the app redraws its own content; then {@link #resume()}. */
    public void pause() {
        require(Phase.RENDERING, "pause");
        sf.setPenState(PenCommand.PAUSE.value);
        phase = Phase.PAUSED;
    }

    /**
     * Show the app's own picture over the quick ink (enablePost(1)): call once the finished
     * stroke is in the app's canvas, and after a redraw done while {@link #pause() paused}.
     */
    public void post() {
        if (phase == Phase.CLOSED) throw wrong("post");
        sf.enablePost(1);
    }

    public void setExclude(int[] rects) {
        if (phase == Phase.CLOSED) throw wrong("setExclude");
        sf.setRegionExclude(view, rects);
    }

    public void close() {
        if (phase == Phase.CLOSED) return;
        sf.setStrokeStyle(StrokeStyle.PENCIL.value);
        sf.enablePost(1);
        sf.setPenState(PenCommand.PAUSE.value);
        sf.setPenState(PenCommand.QUIT.value);
        phase = Phase.CLOSED;
    }

    /** ARGB colour of SF's instant ink. */
    public void setStrokeColor(int argb) {
        if (phase == Phase.CLOSED) throw wrong("setStrokeColor");
        sf.setStrokeColor(argb);
    }

    /**
     * Make SF's instant ink the pen the app will render when the stroke settles (style, width, parameters
     * from {@link org.openink.stroke.LivePen}); render the settled stroke with {@code pen.settled}.
     */
    public void usePen(org.openink.stroke.LivePen pen) {
        if (phase == Phase.CLOSED) throw wrong("usePen");
        sf.setStrokeStyle(pen.sfStyle);
        sf.setStrokeWidth(pen.sfWidth);
        if (pen.sfParams.length > 0) sf.setStrokeParameters(pen.sfStyle, pen.sfParams);
    }

    /** One of {@link StrokeStyle}'s values; raw int so Onyx-style callers can pass theirs through. */
    public void setStrokeStyle(int style) {
        if (phase == Phase.CLOSED) throw wrong("setStrokeStyle");
        sf.setStrokeStyle(style);
    }

    public boolean isOpen() {
        return phase != Phase.CLOSED;
    }

    public boolean isRendering() {
        return phase == Phase.RENDERING;
    }

    private void require(Phase p, String op) {
        if (phase != p) throw wrong(op);
    }

    private IllegalStateException wrong(String op) {
        return new IllegalStateException(op + "() in phase " + phase);
    }
}
