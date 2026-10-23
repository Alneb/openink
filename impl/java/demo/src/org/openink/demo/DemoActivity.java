package org.openink.demo;

import android.app.Activity;
import android.graphics.Color;
import android.graphics.Typeface;
import android.os.Bundle;
import android.util.Log;
import android.view.View;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.TextView;
import java.io.IOException;
import org.openink.backend.OnyxFrameworkBackend;
import org.openink.epd.Epd;
import org.openink.pen.InkSession;
import org.openink.pen.PenAxes;
import org.openink.pen.PenCtrlState;
import org.openink.pen.PenReader;
import org.openink.stroke.PenConfig;
import org.openink.stroke.PenKind;

/**
 * openink test app: instant ink through the stock SurfaceFlinger (InkSession), the lasting
 * stroke from our evdev reader (PenReader), and a status panel saying which parts work.
 * Every failure is shown and logged (tag "openink"), never swallowed.
 */
public final class DemoActivity extends Activity {
    private static final String TAG = InkCanvas.TAG;
    private static final long STATUS_MS = 500;
    /** Saves run one at a time, in order: two quick pauses must not race on the temp file. */
    private static final java.util.concurrent.ExecutorService SAVER =
        java.util.concurrent.Executors.newSingleThreadExecutor(r -> new Thread(r, "openink-save"));

    private InkCanvas canvas;
    private TextView status;
    private Button inkButton, penButton, eraseButton, shapeButton;
    private OnyxFrameworkBackend backend;
    private InkSession session;
    private Epd epd;
    private PenReader reader;
    private String readerInfo = "not started", inkInfo = "not started";
    private long lastEvents, lastStatusAt;
    private String loggedPen = "";
    private boolean inkWanted = true, resumed;
    /** One instance, so a quick pause/resume can cancel the pending tick instead of doubling the loop. */
    private final Runnable statusTick = this::updateStatus;

    @Override protected void onCreate(Bundle b) {
        super.onCreate(b);
        canvas = new InkCanvas(this, this::strokeDone);
        loadPage();
        status = new TextView(this);
        status.setTypeface(Typeface.MONOSPACE);
        status.setTextSize(11);
        status.setTextColor(Color.BLACK);
        status.setPadding(16, 8, 16, 8);

        LinearLayout buttons = new LinearLayout(this);
        buttons.addView(button("Clear", v -> clear()));
        buttons.addView(button("Undo", v -> undo()));
        inkButton = button("Instant ink: on", v -> toggleInk());
        buttons.addView(inkButton);
        buttons.addView(button("Flash", v -> flash()));
        penButton = button("Pen: " + canvas.pen().name().toLowerCase(java.util.Locale.ROOT), v -> nextPen());
        buttons.addView(penButton);
        eraseButton = button("Erase: off", v -> toggleErase());
        buttons.addView(eraseButton);
        shapeButton = button("Shapes: on", v -> toggleShapes());
        buttons.addView(shapeButton);
        buttons.addView(button("Width -", v -> scaleWidth(1 / 1.25f)));
        buttons.addView(button("Width +", v -> scaleWidth(1.25f)));

        LinearLayout root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        root.setBackgroundColor(Color.WHITE);
        root.addView(status);
        root.addView(buttons);
        root.addView(canvas, new LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, 0, 1));
        setContentView(root);

        try {
            backend = OnyxFrameworkBackend.open();
            epd = new Epd(backend);
            session = new InkSession(backend, canvas);
            Log.i(TAG, "backend: ViewUpdateHelper reached " + backend.access);
        } catch (UnsupportedOperationException e) {
            inkInfo = "unavailable: " + e.getMessage();
            Log.w(TAG, "backend", e);
        }
    }

    private Button button(String label, View.OnClickListener l) {
        Button btn = new Button(this);
        btn.setText(label);
        btn.setAllCaps(false);
        btn.setOnClickListener(l);
        return btn;
    }

    @Override protected void onResume() {
        super.onResume();
        resumed = true;
        startReader();
        canvas.post(this::openInk); // after layout: the region is view-relative
        canvas.removeCallbacks(statusTick);
        canvas.postDelayed(statusTick, STATUS_MS);
    }

    @Override protected void onPause() {
        resumed = false;
        savePage();
        closeInk();
        if (reader != null) {
            reader.close();
            reader = null;
        }
        super.onPause();
    }

    private java.io.File pageFile() {
        return new java.io.File(getFilesDir(), "page.oink");
    }

    /** The page from the last run. An unreadable file is moved aside (kept), never overwritten. */
    private void loadPage() {
        java.io.File f = pageFile();
        try {
            java.util.List<InkCanvas.Stroke> saved = PageFile.load(f);
            canvas.restore(saved);
            Log.i(TAG, "page: " + saved.size() + " strokes loaded from " + f);
        } catch (IOException | RuntimeException e) {
            java.io.File aside = new java.io.File(f.getPath() + ".unreadable-" + System.currentTimeMillis());
            boolean moved = f.renameTo(aside);
            Log.e(TAG, "page load failed; " + (moved ? "kept as " + aside : "could not move it aside"), e);
        }
    }

    /** Snapshot now, write on a worker thread: onPause must not block on storage. */
    private void savePage() {
        java.util.List<InkCanvas.Stroke> snapshot = canvas.finishedStrokes();
        java.io.File f = pageFile();
        SAVER.execute(() -> {
            try {
                PageFile.save(f, snapshot);
                Log.i(TAG, "page: " + snapshot.size() + " strokes saved to " + f);
            } catch (IOException e) {
                Log.e(TAG, "page save failed", e);
            }
        });
    }

    private void startReader() {
        try {
            reader = PenReader.start(canvas);
            readerInfo = reader.deviceInfo();
            PenAxes axes = reader.axes();
            canvas.readerStarted(axes);
            Log.i(TAG, "reader: " + readerInfo);
        } catch (IOException | UnsatisfiedLinkError | RuntimeException e) {
            readerInfo = "FAILED (" + e + "); drawing from MotionEvents";
            Log.e(TAG, "reader", e);
        }
    }

    private void openInk() {
        if (session == null || !inkWanted || session.isOpen()) return;
        try {
            session.open(new int[]{0, 0, canvas.getWidth(), canvas.getHeight()}, canvas.strokeWidthPx());
            inkInfo = "open (" + backend.access + ")";
            syncInkWithEraseMode(); // reopened while erasing: keep SF ink off
            Log.i(TAG, "instant ink on, region " + canvas.getWidth() + "x" + canvas.getHeight());
        } catch (RuntimeException e) {
            inkInfo = "open FAILED: " + e;
            Log.e(TAG, "ink open", e);
        }
    }

    private void closeInk() {
        if (session == null) return;
        try {
            session.close();
            inkInfo = inkWanted ? "closed (paused app)" : "off";
        } catch (RuntimeException e) {
            inkInfo = "close FAILED: " + e;
            Log.e(TAG, "ink close", e);
        }
    }

    /** The finished stroke is in the canvas: show it over the quick ink. */
    private void strokeDone() {
        if (session == null || !session.isRendering()) return;
        try {
            session.post();
        } catch (RuntimeException e) {
            inkInfo = "post FAILED: " + e;
            Log.e(TAG, "ink post", e);
        }
    }

    private void clear() {
        canvas.clear();
        if (session == null || !session.isRendering()) {
            flash();
            return;
        }
        // SF keeps its own quick ink: pause it, let the cleared canvas reach the screen, resume.
        session.pause();
        canvas.postDelayed(() -> {
            if (!resumed) return; // the app paused meanwhile: onPause closed the session
            if (!session.isOpen()) { // instant ink was switched off meanwhile
                flash();
                return;
            }
            try {
                session.post();
                flash();
                if (!canvas.eraseMode()) session.resume(); // erase may have been switched on meanwhile
            } catch (RuntimeException e) {
                inkInfo = "clear FAILED: " + e;
                Log.e(TAG, "ink clear", e);
            }
        }, 150);
    }

    /** Revert the last stroke, erase or clear; the redrawn page goes over SF's quick ink. */
    private void undo() {
        if (canvas.undo()) strokeDone();
    }

    private void flash() {
        if (epd == null) return;
        View root = canvas.getRootView();
        try {
            epd.flash(root.getWidth(), root.getHeight());
        } catch (RuntimeException e) {
            Log.e(TAG, "flash", e);
        }
    }

    /** Cycle knote2's pens, each with its knote2 default config. */
    private void nextPen() {
        PenKind[] all = PenKind.values();
        PenKind k = all[(canvas.pen().ordinal() + 1) % all.length];
        canvas.setPen(k, PenConfig.knote2Default(k, getResources().getDisplayMetrics().densityDpi));
        penButton.setText("Pen: " + k.name().toLowerCase(java.util.Locale.ROOT));
        Log.i(TAG, "pen " + k + " width " + canvas.penConfig().width);
        syncInkWidth();
    }

    /** SF's instant ink should match the pen the canvas will draw with. */
    private void syncInkWidth() {
        if (session == null || !session.isOpen()) return; // paused (erasing) too: the width outlives the pause
        try {
            session.setStrokeWidth(canvas.strokeWidthPx());
        } catch (RuntimeException e) {
            inkInfo = "width FAILED: " + e;
            Log.e(TAG, "ink width", e);
        }
    }

    /** Width within the pen's knote2 slider range (mm). */
    private void scaleWidth(float f) {
        PenKind k = canvas.pen();
        float pxPerMm = getResources().getDisplayMetrics().densityDpi / 25.4f;
        float w = Math.max(k.minWidthMm * pxPerMm, Math.min(k.maxWidthMm * pxPerMm, canvas.penConfig().width * f));
        canvas.setPen(k, canvas.penConfig().withWidth(w));
        Log.i(TAG, String.format(java.util.Locale.ROOT, "pen %s width %.1f px (%.2f mm)", k, w, w / pxPerMm));
        syncInkWidth();
    }

    /**
     * Stroke eraser on/off. SF's instant ink would draw black along the eraser path, so it is
     * paused while erasing; turning erase off shows the edited page and resumes it.
     */
    private void toggleErase() {
        boolean on = !canvas.eraseMode();
        canvas.setEraseMode(on);
        eraseButton.setText(on ? "Erase: ON" : "Erase: off");
        syncInkWithEraseMode();
    }

    private void syncInkWithEraseMode() {
        if (session == null || !session.isOpen()) return;
        try {
            if (canvas.eraseMode() && session.isRendering()) {
                session.pause();
            } else if (!canvas.eraseMode() && !session.isRendering()) {
                session.post();
                session.resume();
            }
        } catch (RuntimeException e) {
            inkInfo = "erase toggle FAILED: " + e;
            Log.e(TAG, "ink erase toggle", e);
        }
    }

    /** Hold-to-snap shapes on/off (rest the pen at the end of a stroke to snap it). */
    private void toggleShapes() {
        boolean on = !canvas.snapShapes();
        canvas.setSnapShapes(on);
        shapeButton.setText(on ? "Shapes: on" : "Shapes: off");
    }

    private void toggleInk() {
        inkWanted = !inkWanted;
        inkButton.setText(inkWanted ? "Instant ink: on" : "Instant ink: off");
        if (inkWanted) openInk(); else closeInk();
    }

    private void updateStatus() {
        if (!resumed) return;
        long now = android.os.SystemClock.uptimeMillis();
        long ev = canvas.events;
        double rate = lastStatusAt == 0 ? 0 : (ev - lastEvents) * 1000.0 / (now - lastStatusAt);
        lastEvents = ev;
        lastStatusAt = now;
        String pen = "-";
        if (backend != null) {
            try {
                int raw = backend.getPenState();
                pen = raw + " (" + PenCtrlState.fromReply(raw) + ")";
            } catch (RuntimeException e) {
                pen = "getPenState FAILED: " + e.getMessage();
            }
        }
        if (!pen.equals(loggedPen)) {
            Log.i(TAG, "SF pen state " + pen + " (ink " + inkInfo + ")");
            loggedPen = pen;
        }
        status.setText(String.format(java.util.Locale.ROOT,
            "evdev  %s%n       %d pen events (%.0f/s), last %s%nink    %s, SF pen state %s%nmap    %s%npen    %s %.1f px",
            readerInfo, ev, rate, canvas.lastText(), inkInfo, pen, canvas.mapInfo(), canvas.pen(), canvas.penConfig().width));
        canvas.postDelayed(statusTick, STATUS_MS);
    }
}
