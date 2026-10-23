package org.openink.demo;

import java.io.BufferedInputStream;
import java.io.BufferedOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import org.openink.stroke.PenConfig;
import org.openink.stroke.PenKind;

/**
 * The demo's page on disk, so strokes survive a restart. Big-endian:
 * {@code "OINK" u16 version, i32 strokes, then per stroke: u8 raw, u8 penCode, f32×5 config
 * (width, pressureSens, smooth, velocitySens, tiltScale), u8 tiltEnabled, i32 points,
 * f64×6 per point (x, y, pressure, tiltX, tiltY, timeUs)}. Written to a temp file, synced and
 * renamed, so a crash or power cut mid-save leaves the previous page intact. Strokes without
 * points are not written (load rejects them).
 */
final class PageFile {
    private static final int MAGIC = 0x4f494e4b; // "OINK"
    private static final int VERSION = 1;
    private static final int MAX_STROKES = 1_000_000, MAX_POINTS = 10_000_000;

    private PageFile() {}

    static void save(File f, List<InkCanvas.Stroke> strokes) throws IOException {
        List<InkCanvas.Stroke> kept = new ArrayList<>(strokes.size());
        for (InkCanvas.Stroke s : strokes) if (!s.pts.isEmpty()) kept.add(s);
        File tmp = new File(f.getPath() + ".tmp");
        FileOutputStream fos = new FileOutputStream(tmp);
        try (DataOutputStream out = new DataOutputStream(new BufferedOutputStream(fos))) {
            out.writeInt(MAGIC);
            out.writeShort(VERSION);
            out.writeInt(kept.size());
            for (InkCanvas.Stroke s : kept) {
                out.writeByte(s.raw ? 1 : 0);
                out.writeByte(s.kind.code);
                PenConfig c = s.config;
                for (float v : new float[]{c.width, c.pressureSensitivity, c.smoothLevel, c.velocitySensitivity, c.tiltScale}) out.writeFloat(v);
                out.writeByte(c.tiltEnabled ? 1 : 0);
                out.writeInt(s.pts.size());
                for (double[] p : s.pts) for (double v : p) out.writeDouble(v);
            }
            out.flush();
            // Without this, ext4/f2fs may commit the rename before the data: an empty page after a power cut.
            fos.getFD().sync();
        }
        if (!tmp.renameTo(f)) throw new IOException("rename " + tmp + " → " + f + " failed");
    }

    /** @return the saved strokes, or an empty list when there is no file yet */
    static List<InkCanvas.Stroke> load(File f) throws IOException {
        List<InkCanvas.Stroke> out = new ArrayList<>();
        if (!f.exists()) return out;
        // Identical configs share one object: the canvas reuses a stroke engine per config identity.
        Map<String, PenConfig> configs = new HashMap<>();
        try (DataInputStream in = new DataInputStream(new BufferedInputStream(new FileInputStream(f)))) {
            if (in.readInt() != MAGIC) throw new IOException(f + ": not an openink page");
            int version = in.readUnsignedShort();
            if (version != VERSION) throw new IOException(f + ": page version " + version + ", expected " + VERSION);
            int n = in.readInt();
            if (n < 0 || n > MAX_STROKES) throw new IOException(f + ": implausible stroke count " + n);
            for (int i = 0; i < n; i++) {
                boolean raw = in.readUnsignedByte() != 0;
                PenKind kind;
                try {
                    kind = PenKind.fromCode(in.readUnsignedByte());
                } catch (IllegalArgumentException e) {
                    throw new IOException(f + ": stroke " + i + ": " + e.getMessage(), e);
                }
                float w = in.readFloat(), ps = in.readFloat(), sm = in.readFloat(), vs = in.readFloat(), ts = in.readFloat();
                boolean tilt = in.readUnsignedByte() != 0;
                if (!(w > 0 && w < 1e4)) throw new IOException(f + ": stroke " + i + ": implausible width " + w); // NaN fails too
                String key = kind.code + "/" + w + "/" + ps + "/" + sm + "/" + vs + "/" + ts + "/" + tilt;
                PenConfig c = configs.computeIfAbsent(key, k -> new PenConfig(w, ps, sm, vs, tilt, ts));
                InkCanvas.Stroke s = new InkCanvas.Stroke(raw, kind, c);
                int m = in.readInt();
                if (m < 1 || m > MAX_POINTS) throw new IOException(f + ": stroke " + i + ": implausible point count " + m);
                for (int j = 0; j < m; j++) {
                    double[] p = new double[6];
                    for (int k = 0; k < 6; k++) p[k] = in.readDouble();
                    s.pts.add(p);
                }
                out.add(s);
            }
        }
        return out;
    }
}
