package org.openink.hwr;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/**
 * Handwriting recognition from libopenink_jni.so (crates/openink-hwr: our own BiGRU-CTC model, replacing
 * MyScript Text). One call reads one word or short phrase. Stateless and thread-safe; loads on first use.
 */
public final class HandwritingRecognizer {
    /** One reading, best first in {@link #recognize}'s list. */
    public static final class Candidate {
        public final String text;
        /** Natural-log probability under the model. */
        public final float logProb;

        Candidate(String text, float logProb) {
            this.text = text;
            this.logProb = logProb;
        }

        @Override
        public String toString() {
            return text + " (" + logProb + ")";
        }
    }

    private HandwritingRecognizer() {}

    /** {@code strokes[i]} is {@code [x, y, tMs]*} for stroke i, in drawing order; any coordinate unit. */
    public static List<Candidate> recognize(float[][] strokes) {
        int total = 0;
        int[] lens = new int[strokes.length];
        for (int i = 0; i < strokes.length; i++) {
            if (strokes[i].length % 3 != 0) throw new IllegalArgumentException("stroke " + i + " is not [x, y, t]*");
            lens[i] = strokes[i].length / 3;
            total += strokes[i].length;
        }
        float[] xyt = new float[total];
        int at = 0;
        for (float[] s : strokes) {
            System.arraycopy(s, 0, xyt, at, s.length);
            at += s.length;
        }
        Loader.load();
        return decode(nativeRecognize(xyt, lens));
    }

    /** Parses the native wire string: {@code logProb \t text} per line, best first. */
    public static List<Candidate> decode(String wire) {
        if (wire == null || wire.isEmpty()) return Collections.emptyList();
        List<Candidate> out = new ArrayList<>();
        for (String line : wire.split("\n", -1)) {
            int tab = line.indexOf('\t');
            if (tab < 0) throw new IllegalArgumentException("bad candidate line: " + line);
            out.add(new Candidate(line.substring(tab + 1), Float.parseFloat(line.substring(0, tab))));
        }
        return out;
    }

    /** Loads the native library on first recognise, so {@link #decode} runs in host checks without it. */
    private static final class Loader {
        static {
            System.loadLibrary("openink_jni");
        }

        static void load() {}
    }

    private static native String nativeRecognize(float[] xyt, int[] lens);
}
