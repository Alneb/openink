package org.openink.pen;

import android.util.Log;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.List;

/**
 * Self-calibrating raw → screen map. Every contact is seen twice: by our evdev reader (raw units)
 * and by Android as a stylus MotionEvent (screen pixels). Pen-downs/ups of both that happen at the
 * same moment are paired and refitted into a {@link ScreenMap}; once enough pairs agree, the map
 * locks. Starts from the {@link ScreenMap#rotated} guess. Thread-safe: raw contacts come from the
 * pen thread, screen contacts from the UI thread.
 */
public final class MapCalibrator {
    private static final String TAG = "openink";
    /** Pairs closer in time than this are the same pen-down/up. */
    private static final long PAIR_WINDOW_MS = 250;
    /** A fit worse than this is a mismatched pair, not a map; keep the previous map. */
    private static final double MAX_FIT_RMS_PX = 25;
    /** Once this many pairs fit within LOCK_RMS_PX the map is final: no more refits. */
    private static final int LOCK_PAIRS = 8;
    private static final double LOCK_RMS_PX = 1;
    /** A refit that moves no screen corner more than this does not need a redraw. */
    private static final double REDRAW_SHIFT_PX = 0.5;
    private static final int RAW_HISTORY = 8;

    private final int rawMaxX, rawMaxY;
    private final ArrayDeque<long[]> rawContacts = new ArrayDeque<>(); // {action, x, y, uptimeMs}
    private final List<double[]> pairRaw = new ArrayList<>(), pairScreen = new ArrayList<>();
    private ScreenMap map;
    private boolean locked;

    public MapCalibrator(PenAxes axes, int screenW, int screenH) {
        this.rawMaxX = axes.xMax;
        this.rawMaxY = axes.yMax;
        this.map = ScreenMap.rotated(rawMaxX, rawMaxY, screenW, screenH);
        Log.i(TAG, "map (unfitted guess): " + map);
    }

    public synchronized ScreenMap map() {
        return map;
    }

    public synchronized boolean isLocked() {
        return locked;
    }

    /** An evdev {@link PenReader#DOWN} or {@link PenReader#UP} (pen thread). Other actions are ignored. */
    public synchronized void rawContact(int action, int x, int y, long uptimeMs) {
        if (action != PenReader.DOWN && action != PenReader.UP) return;
        rawContacts.addLast(new long[]{action, x, y, uptimeMs});
        while (rawContacts.size() > RAW_HISTORY) rawContacts.removeFirst();
    }

    /**
     * The matching stylus MotionEvent contact, in screen pixels ({@code getRawX/Y}) at
     * {@code getEventTime()}; {@code action} is {@link PenReader#DOWN} or {@link PenReader#UP}.
     * @return true when the map moved enough that drawn strokes should be redrawn
     */
    public synchronized boolean screenContact(int action, float screenX, float screenY, long eventTimeMs) {
        long[] best = null;
        for (long[] r : rawContacts) {
            long dt = Math.abs(r[3] - eventTimeMs);
            if (r[0] == action && dt < PAIR_WINDOW_MS && (best == null || dt < Math.abs(best[3] - eventTimeMs))) best = r;
        }
        if (best == null || locked) return false;
        rawContacts.remove(best);
        pairRaw.add(new double[]{best[1], best[2]});
        pairScreen.add(new double[]{screenX, screenY});
        ScreenMap fit = ScreenMap.fit(pairRaw.toArray(new double[0][]), pairScreen.toArray(new double[0][]));
        if (fit == null) return false;
        if (fit.rmsPx > MAX_FIT_RMS_PX) {
            // A bad pair poisons every later fit: drop the newest and keep the old map.
            Log.w(TAG, "fit rejected (" + fit + "), dropping the last pair");
            pairRaw.remove(pairRaw.size() - 1);
            pairScreen.remove(pairScreen.size() - 1);
            return false;
        }
        ScreenMap old = map;
        map = fit;
        if (pairRaw.size() >= LOCK_PAIRS && fit.rmsPx < LOCK_RMS_PX) {
            locked = true;
            Log.i(TAG, "map locked: " + map);
        } else {
            Log.i(TAG, "map fitted: " + map);
        }
        return shift(old, fit) > REDRAW_SHIFT_PX;
    }

    /** Largest screen displacement between two maps, over the digitizer's corners. */
    private double shift(ScreenMap a, ScreenMap b) {
        double worst = 0;
        for (int[] c : new int[][]{{0, 0}, {rawMaxX, 0}, {0, rawMaxY}, {rawMaxX, rawMaxY}}) {
            worst = Math.max(worst, Math.hypot(a.screenX(c[0], c[1]) - b.screenX(c[0], c[1]), a.screenY(c[0], c[1]) - b.screenY(c[0], c[1])));
        }
        return worst;
    }
}
