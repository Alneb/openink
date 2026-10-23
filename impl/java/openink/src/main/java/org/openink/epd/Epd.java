package org.openink.epd;

import org.openink.backend.EpdBackend;

/** Screen refresh with typed modes. */
public final class Epd {
    private final EpdBackend sf;

    public Epd(EpdBackend backend) {
        this.sf = backend;
    }

    public void refresh(int left, int top, int width, int height, UpdateMode mode) {
        sf.refreshScreen(left, top, width, height, mode.toOnyx());
    }

    /** Full-screen flash (GC16 full + wait), the classic ghosting cleanup. */
    public void flash(int screenWidth, int screenHeight) {
        refresh(0, 0, screenWidth, screenHeight, UpdateMode.GC);
    }

    public void waitForUpdates() {
        sf.waitForUpdateFinished();
    }
}
