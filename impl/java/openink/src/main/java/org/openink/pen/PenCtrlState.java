package org.openink.pen;

/** Returned by GET_PEN_STATE (OnyxPenStateUtil.CTRL_*). A different vocabulary from PenCommand. */
public enum PenCtrlState {
    STOP, RUNNING, PAUSED, OTHER;

    public static PenCtrlState fromReply(int v) {
        switch (v) {
            case 0: return STOP;
            case 2: return RUNNING;
            case 4: return PAUSED;
            default: return OTHER;
        }
    }

    /** OnyxPenStateUtil.isPenDrawState(). */
    public boolean isDrawing() {
        return this != STOP && this != PAUSED;
    }
}
