package org.openink.hwr;

import java.util.List;

/** Host check: the wire string from crates/openink-jni/src/hwr.rs decodes to the same candidates. */
public final class HwrWireCheck {
    public static void main(String[] args) {
        List<HandwritingRecognizer.Candidate> c = HandwritingRecognizer.decode("-0.5\tab c\n-2\tx");
        check(c.size() == 2, "two candidates");
        check(c.get(0).text.equals("ab c") && c.get(0).logProb == -0.5f, "first keeps its space");
        check(c.get(1).text.equals("x") && c.get(1).logProb == -2f, "second");
        check(HandwritingRecognizer.decode("").isEmpty(), "empty");
        System.out.println("HwrWireCheck: OK");
    }

    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }
}
