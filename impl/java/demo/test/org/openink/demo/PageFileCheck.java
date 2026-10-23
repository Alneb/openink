package org.openink.demo;

import java.io.File;
import java.io.RandomAccessFile;
import java.util.ArrayList;
import java.util.List;
import org.openink.stroke.PenConfig;
import org.openink.stroke.PenKind;

/** Host check: PageFile round-trips strokes exactly, shares configs, rejects damaged files. */
public final class PageFileCheck {
    public static void main(String[] args) throws Exception {
        int failures = 0;
        File f = File.createTempFile("page", ".oink");
        PenConfig c = PenConfig.knote2Default(PenKind.MARKER, 300);
        List<InkCanvas.Stroke> in = new ArrayList<>();
        for (int i = 0; i < 3; i++) {
            InkCanvas.Stroke s = new InkCanvas.Stroke(i != 1, i == 2 ? PenKind.PENCIL : PenKind.MARKER, c);
            for (int j = 0; j < 5 + i; j++) s.pts.add(new double[]{j * 1.5, j * 2.25, 0.4, -12, 33, 1_000_000_000_123L + j});
            in.add(s);
        }
        PageFile.save(f, in);
        List<InkCanvas.Stroke> out = PageFile.load(f);
        if (out.size() != 3) { System.out.println("FAIL count " + out.size()); failures++; }
        for (int i = 0; i < Math.min(3, out.size()); i++) {
            InkCanvas.Stroke a = in.get(i), b = out.get(i);
            boolean same = a.raw == b.raw && a.kind == b.kind && a.pts.size() == b.pts.size()
                && b.config.width == c.width && b.config.tiltEnabled == c.tiltEnabled;
            for (int j = 0; same && j < a.pts.size(); j++) same = java.util.Arrays.equals(a.pts.get(j), b.pts.get(j));
            if (!same) { System.out.println("FAIL stroke " + i + " differs"); failures++; }
        }
        if (out.size() == 3 && out.get(0).config != out.get(1).config) { System.out.println("FAIL configs not shared"); failures++; }
        try (RandomAccessFile r = new RandomAccessFile(f, "rw")) { r.setLength(r.length() - 7); } // truncate
        try {
            PageFile.load(f);
            System.out.println("FAIL truncated file loaded");
            failures++;
        } catch (java.io.IOException expected) { /* ok */ }
        if (!PageFile.load(new File(f.getPath() + ".none")).isEmpty()) { System.out.println("FAIL missing file not empty"); failures++; }
        // A stroke without points is skipped on save, not written as an unloadable record.
        List<InkCanvas.Stroke> withEmpty = new ArrayList<>(in);
        withEmpty.add(1, new InkCanvas.Stroke(true, PenKind.BALLPOINT, c));
        PageFile.save(f, withEmpty);
        if (PageFile.load(f).size() != 3) { System.out.println("FAIL empty stroke not skipped"); failures++; }
        // A NaN width (damaged config) is rejected rather than handed to the stroke engine.
        List<InkCanvas.Stroke> bad = new ArrayList<>();
        InkCanvas.Stroke nanWidth = new InkCanvas.Stroke(true, PenKind.BALLPOINT, c.withWidth(Float.NaN));
        nanWidth.pts.add(new double[]{1, 2, 0.5, 0, 0, 0});
        bad.add(nanWidth);
        PageFile.save(f, bad);
        try {
            PageFile.load(f);
            System.out.println("FAIL NaN width loaded");
            failures++;
        } catch (java.io.IOException expected) { /* ok */ }
        if (new File(f.getPath() + ".tmp").exists()) { System.out.println("FAIL temp file left behind"); failures++; }
        f.delete();
        System.out.println("PageFileCheck: " + failures + " failures");
        if (failures > 0) System.exit(1);
    }
}
