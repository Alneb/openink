package org.openink.pen;

/** Sent with SET_SCREEN_HANDWRITING_PEN_STATE. Start alone does not render ink: follow with RESUME. */
public enum PenCommand {
    QUIT(0), START(1), RESUME(2), PAUSE(3);

    public final int value;

    PenCommand(int value) {
        this.value = value;
    }
}
