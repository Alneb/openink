package org.openink.compat.epd;

import android.content.Context;
import android.content.Intent;
import android.content.res.Resources;
import android.graphics.Matrix;
import android.graphics.Paint;
import android.graphics.Rect;
import android.graphics.RectF;
import android.os.Handler;
import android.os.Looper;
import android.util.DisplayMetrics;
import android.view.View;
import android.webkit.WebView;
import org.openink.backend.OnyxFramework;
import org.openink.epd.EpdEngine;

/**
 * Onyx's {@code com.onyx.android.sdk.api.device.epd.EpdController}: same names, overloads and
 * results (from knote2's SDK; each binding listed by docs/scripts/sdk_bindings.py).
 *
 * <p>Two kinds of call (docs/API-COVERAGE.md):
 * <ul>
 * <li><b>Explicit refreshes</b> ({@code refreshScreen}, {@code refreshScreenRegion}) go through the
 *     open refresh engine ({@link EpdEngine}): the app's waveform is kept, the engine adds
 *     coalescing, merging, holding while the pen writes and ghost cleanup.</li>
 * <li><b>State SurfaceFlinger keeps</b> (colour, regal, scopes, per-view modes, the handwriting
 *     setup) goes to the same whitelisted framework method SDMDevice uses.</li>
 * </ul>
 * A framework method that is missing throws {@link OnyxFramework.Missing}.
 */
public final class EpdController {
    public static final String CTP_STATUS_CHANGE_ACTION = "ctp.status.change";
    public static final String ENABLE_SYSTEM_CTP_ACTION = "action.enable.all.tp.region";
    public static final String RESET_SYSTEM_CTP_ACTION = "action.disable.tp.exclude.bar.region";
    public static int SCHEME_START = 1, SCHEME_NORMAL = 1, SCHEME_KEYBOARD = 2, SCHEME_SCRIBBLE = 3,
            SCHEME_APPLICATION_ANIMATION = 4, SCHEME_SYSTEM_ANIMATION = 5, SCHEME_END = 5;
    public static final int STROKE_STYLE_PENCIL = 0, STROKE_STYLE_BRUSH = 1, STROKE_STYLE_MARKER = 2,
            STROKE_STYLE_NEO_BRUSH = 3, STROKE_STYLE_CHARCOAL = 4;
    /** Onyx reads it at class load; here a framework without the method gives 0 instead of failing the class. */
    public static final float MAX_TOUCH_PRESSURE = safeMaxTouchPressure();

    private static final Class<?>[] NONE = {}, I = {int.class}, Z = {boolean.class}, F = {float.class},
            ZZ = {boolean.class, boolean.class}, ZI = {boolean.class, int.class}, I5 = {int.class, int.class, int.class, int.class, int.class},
            F6 = {float.class, float.class, float.class, float.class, float.class, float.class},
            MAP = {View.class, float[].class, float[].class}, VIEW_RECTS = {View.class, int[].class};
    private static final boolean isRegalEnabled = true; // Onyx's flag; nothing in the SDK clears it

    private static final Handler MAIN = new Handler(Looper.getMainLooper());
    private static EpdEngine engine; // main thread only

    private EpdController() {}

    // ---- the open refresh engine -------------------------------------------------------------

    /** The process-wide engine (main thread), e.g. for TouchHelper's pen-down/up. */
    public static EpdEngine engine() {
        if (Looper.myLooper() != Looper.getMainLooper()) throw new IllegalStateException("EpdController.engine() off the main thread");
        if (engine == null) {
            DisplayMetrics dm = Resources.getSystem().getDisplayMetrics();
            engine = new EpdEngine(dm.widthPixels, dm.heightPixels,
                    (l, t, w, h, m) -> OnyxFramework.helper("refreshScreen", I5, l, t, w, h, m), Looper.getMainLooper());
        }
        return engine;
    }

    private static void onMain(Runnable r) {
        if (Looper.myLooper() == Looper.getMainLooper()) r.run();
        else MAIN.post(r);
    }

    /** View-relative (left, top, width, height) → a raw engine request in screen pixels. */
    private static void refresh(View view, int left, int top, int width, int height, UpdateMode mode) {
        if (width <= 0 || height <= 0) return;
        int[] at = new int[2];
        view.getLocationOnScreen(at);
        int l = at[0] + left, t = at[1] + top;
        org.openink.epd.UpdateMode m = org.openink.epd.UpdateMode.fromOnyx(mode.onyx);
        onMain(() -> engine().requestRaw(l, t, l + width, t + height, m));
    }

    public static void refreshScreen(View view, UpdateMode mode) {
        refresh(view, 0, 0, view.getWidth(), view.getHeight(), mode);
    }

    public static void refreshScreenRegion(View view, int left, int top, int width, int height, UpdateMode mode) {
        refresh(view, left, top, width, height, mode);
    }

    /** Sends the engine's queue first, then waits for the panel. */
    public static void waitForUpdateFinished() {
        if (Looper.myLooper() == Looper.getMainLooper()) engine().flush();
        OnyxFramework.helper("waitForUpdateFinished", NONE);
    }

    // ---- SurfaceFlinger's own refresh state ---------------------------------------------------

    public static void invalidate(View view, UpdateMode mode) {
        OnyxFramework.callOn(view, "invalidateWithUpdateMode", I, mode.onyx);
    }

    public static void invalidate(View view, int l, int t, int r, int b, UpdateMode mode) {
        OnyxFramework.callOn(view, "invalidateWithUpdateMode", I5, l, t, r, b, mode.onyx);
    }

    public static void postInvalidate(View view, UpdateMode mode) {
        OnyxFramework.callOn(view, "postInvalidateWithUpdateMode", I, mode.onyx);
    }

    public static void repaintEveryThing() {
        OnyxFramework.helper("repaintEverything", NONE);
    }

    public static void repaintEveryThing(UpdateMode mode) {
        OnyxFramework.helper("repaintEverything", I, mode.onyx);
    }

    public static void applyGCOnce() {
        OnyxFramework.helper("applyGCOnce", NONE);
    }

    public static boolean setViewDefaultUpdateMode(View view, UpdateMode mode) {
        OnyxFramework.callOn(view, "setDefaultUpdateMode", I, mode.onyx);
        return true;
    }

    public static UpdateMode getViewDefaultUpdateMode(View view) {
        Object v = OnyxFramework.callOn(view, "getDefaultUpdateMode", NONE);
        return v instanceof Integer ? UpdateMode.fromOnyx((Integer) v) : UpdateMode.GC;
    }

    public static boolean resetViewUpdateMode(View view) {
        OnyxFramework.callOn(view, "resetUpdateMode", NONE);
        return true;
    }

    public static void resetUpdateMode(View view) {
        resetViewUpdateMode(view);
        useFastScheme();
    }

    public static boolean isDeepGcMode(View view) {
        return getViewDefaultUpdateMode(view) == UpdateMode.DEEP_GC;
    }

    /** BaseDevice: no system default on this device. */
    public static UpdateMode getSystemDefaultUpdateMode() {
        return UpdateMode.GU;
    }

    public static boolean setSystemDefaultUpdateMode(UpdateMode mode) {
        return false;
    }

    /** BaseDevice no-op on this device. */
    public static void holdDisplay(boolean hold, UpdateMode mode, int count) {}

    public static void enableA2ForSpecificView(View view) {
        OnyxFramework.callOn(view, "enableA2", NONE);
    }

    public static void disableA2ForSpecificView(View view) {
        OnyxFramework.callOn(view, "disableA2", NONE);
    }

    public static void switchToA2Mode() {
        OnyxFramework.helper("switchToA2Mode", NONE);
    }

    public static boolean applySystemFastMode(boolean on) {
        if (on) OnyxFramework.helper("switchToA2Mode", NONE);
        else OnyxFramework.helper("clearSysScopeUpdate", NONE);
        return true;
    }

    public static boolean inSystemFastMode() {
        return isTrue(OnyxFramework.helper("inSystemFastMode", NONE));
    }

    public static boolean isInFastMode() {
        return isTrue(OnyxFramework.helper("isInFastMode", NONE));
    }

    public static boolean setDisplayScheme(int scheme) {
        OnyxFramework.helper("setDisplayScheme", I, scheme);
        return true;
    }

    public static void useFastScheme() {
        setDisplayScheme(SCHEME_SCRIBBLE);
    }

    public static boolean applyAppScopeUpdate(String tag, boolean enable, boolean clear, UpdateMode mode, int repeat) {
        OnyxFramework.helper("applyAppScopeUpdate", new Class<?>[]{String.class, boolean.class, boolean.class, int.class, int.class},
                tag, enable, clear, mode.onyx, repeat);
        return true;
    }

    public static boolean clearAppScopeUpdate() {
        return clearAppScopeUpdate(true);
    }

    public static boolean clearAppScopeUpdate(boolean repaint) {
        OnyxFramework.helper("clearAppScopeUpdate", Z, repaint);
        return true;
    }

    public static UpdateOption getAppScopeRefreshMode() {
        Object v = eink("getAppScopeRefreshMode", "getSystemRefreshMode", NONE);
        return v instanceof Integer ? UpdateOption.fromCode((Integer) v) : UpdateOption.NORMAL;
    }

    public static boolean setAppScopeRefreshMode(UpdateOption option) {
        eink("setAppScopeRefreshMode", "setSystemRefreshMode", I, option.code);
        return true;
    }

    public static boolean applyTransientUpdate(UpdateMode mode) {
        return OnyxFramework.helper("applyTransientUpdate", I, mode.onyx) != null;
    }

    public static boolean clearTransientUpdate(View view, boolean repaint) {
        OnyxFramework.helper("clearTransientUpdate", Z, repaint);
        if (view != null) view.invalidate();
        return true;
    }

    public static boolean clearTransientUpdate(boolean repaint) {
        return clearTransientUpdate(null, repaint);
    }

    /** {@code n >= 0}: skip SurfaceFlinger's next n frame refreshes; {@code n < 0}: subtract (SF-ONYX.md). */
    public static void byPass(int n) {
        OnyxFramework.helper("byPass", I, n);
    }

    public static boolean enableScreenUpdate(View view, boolean enable) {
        OnyxFramework.helper("enableScreenUpdate", Z, enable);
        return true;
    }

    public static void setUpdListSize(int size) {
        OnyxFramework.helper("setUpdListSize", I, size);
    }

    public static void resetUpdListSize() {
        setUpdListSize(-1);
    }

    public static void useGCForNewSurface(boolean on) {
        OnyxFramework.helper("useGCForNewSurface", Z, on);
    }

    public static void fillWhiteOnWakeup(boolean on, UpdateMode mode) {
        OnyxFramework.helper("fillWhiteOnWakeup", ZI, on, mode.onyx);
    }

    public static void setEpdTurbo(int turbo) {
        OnyxFramework.helper("setEpdTurbo", I, turbo);
    }

    public static void setTrigger(int trigger) {
        OnyxFramework.helper("setTrigger", I, trigger);
    }

    public static void setAutoSyncBufEnable(boolean on) {
        OnyxFramework.helper("setAutoSyncBufEnable", Z, on);
    }

    public static void resetEpdPost() {
        OnyxFramework.helper("resetEpdPost", NONE);
    }

    public static void resetEpd(Context context) {
        byPass(0);
        resetEpdPost();
        clearTransientUpdate(false);
        appResetCTPDisableRegion(context);
    }

    // ---- colour and image ---------------------------------------------------------------------

    public static void applyBrightness(int v) { OnyxFramework.helper("applyBrightness", I, v); }
    public static void applySaturation(int v) { OnyxFramework.helper("applySaturation", I, v); }
    public static void applyMonoLevel(int v) { OnyxFramework.helper("applyMonoLevel", I, v); }
    public static void applyColorFilter(int v) { OnyxFramework.helper("applyColorFilter", I, v); }
    public static void applyGammaCorrection(boolean on, int gamma) { OnyxFramework.helper("applyGammaCorrection", ZI, on, gamma); }
    public static void enableDither(boolean on) { OnyxFramework.helper("enableDither", Z, on); }
    public static void enableColorCU() { OnyxFramework.helper("enableColorCU", Z, true); }
    public static void disableColorCU() { OnyxFramework.helper("enableColorCU", Z, false); }
    public static void enableColorAdjust() { OnyxFramework.helper("enableColorAdjust", Z, true); }
    public static void disableColorAdjust() { OnyxFramework.helper("enableColorAdjust", Z, false); }
    public static void enableNightMode() { enableNightMode(false); }
    public static void enableNightMode(boolean flag) { OnyxFramework.helper("enableNightMode", ZZ, true, flag); }
    public static void disableNightMode() { disableNightMode(false); }
    public static void disableNightMode(boolean flag) { OnyxFramework.helper("enableNightMode", ZZ, false, flag); }
    public static boolean isSupportNightMode() { return isTrue(OnyxFramework.helper("supportNightMode", NONE)); }
    public static void enableRegal() { OnyxFramework.helper("enableRegal", Z, true); }
    public static void disableRegal() { OnyxFramework.helper("enableRegal", Z, false); }
    public static boolean isRegalEnabled() { return isRegalEnabled; }
    public static boolean supportRegal() { return isTrue(OnyxFramework.helper("supportRegal", NONE)) && isRegalEnabled; }

    public static void setUpRegalPlus(View view) {
        enableRegal();
        setViewDefaultUpdateMode(view, UpdateMode.REGAL_PLUS);
    }

    public static void setWebViewContrastOptimize(WebView webView, boolean on) {
        OnyxFramework.callOn(webView, "setCssInjectEnabled", Z, on);
    }

    // ---- handwriting (SurfaceFlinger's instant ink) -------------------------------------------

    public static int getPenState() { return (Integer) OnyxFramework.helper("getPenState", NONE); }
    public static boolean isValidPenState() { return isTrue(OnyxFramework.helper("isValidPenState", NONE)); }
    public static void setScreenHandWritingPenState(View view, int state) { OnyxFramework.helper("setScreenHandWritingPenState", I, state); }
    public static void setScreenHandWritingRegionMode(View view, int mode) { OnyxFramework.helper("setScreenHandWritingRegionMode", I, mode); }

    /** The framework adds the view's screen position to the array in place: always a fresh copy. */
    public static void setScreenHandWritingRegionLimit(View view, int[] rects) {
        OnyxFramework.helper("setScreenHandWritingRegionLimit", VIEW_RECTS, view, rects.clone());
    }

    public static void setScreenHandWritingRegionLimit(View view) {
        if (view != null) setScreenHandWritingRegionLimit(view, 0, 0, view.getRight(), view.getBottom());
    }

    public static void setScreenHandWritingRegionLimit(View view, int l, int t, int r, int b) {
        setScreenHandWritingRegionLimit(view, new int[]{l, t, r, b});
    }

    public static void setScreenHandWritingRegionLimit(View view, Rect[] rects) {
        setScreenHandWritingRegionLimit(view, flatten(rects));
    }

    public static void setScreenHandWritingRegionExclude(View view, int[] rects) {
        OnyxFramework.helper("setScreenHandWritingRegionExclude", VIEW_RECTS, view, rects.clone());
    }

    public static void setScreenHandWritingRegionExclude(View view, Rect[] rects) {
        setScreenHandWritingRegionExclude(view, flatten(rects));
    }

    public static void setStrokeWidth(float w) { OnyxFramework.helper("setStrokeWidth", F, w); }
    public static void setStrokeColor(int c) { OnyxFramework.helper("setStrokeColor", I, c); }
    public static void setStrokeStyle(int s) { OnyxFramework.helper("setStrokeStyle", I, s); }

    public static void setPainterStyle(boolean antiAlias, Paint.Style style, Paint.Join join, Paint.Cap cap) {
        OnyxFramework.helper("setPainterStyle", new Class<?>[]{boolean.class, Paint.Style.class, Paint.Join.class, Paint.Cap.class},
                antiAlias, style, join, cap);
    }

    public static void enablePost(int enable) { OnyxFramework.helper("enablePost", I, enable); }
    public static void enablePost(View view, int enable) { enablePost(enable); }
    public static void enterScribbleMode() { enablePost(0); }
    public static void enterScribbleMode(View view) { enablePost(0); }
    public static void leaveScribbleMode() { enablePost(1); }
    public static void leaveScribbleMode(View view) { enablePost(1); }
    public static void penUp() { OnyxFramework.helper("penUp", NONE); }

    public static void handwritingRepaint(View view, int l, int t, int r, int b) {
        OnyxFramework.helper("handwritingRepaint", new Class<?>[]{View.class, int.class, int.class, int.class, int.class}, view, l, t, r, b);
    }

    public static void handwritingRepaint(View view, Rect r) {
        handwritingRepaint(view, r.left, r.top, r.right, r.bottom);
    }

    public static float startStroke(float a, float b, float c, float d, float e, float f) { return (Float) OnyxFramework.helper("startStroke", F6, a, b, c, d, e, f); }
    public static float addStrokePoint(float a, float b, float c, float d, float e, float f) { return (Float) OnyxFramework.helper("addStrokePoint", F6, a, b, c, d, e, f); }
    public static float finishStroke(float a, float b, float c, float d, float e, float f) { return (Float) OnyxFramework.helper("finishStroke", F6, a, b, c, d, e, f); }

    public static void moveTo(float x, float y, float width) { moveTo(null, x, y, width); }
    public static void moveTo(View v, float x, float y, float width) {
        OnyxFramework.helper("moveTo", new Class<?>[]{View.class, float.class, float.class, float.class}, v, x, y, width);
    }
    public static void moveTo(View v, float x, float y, float width, float pressure) {
        OnyxFramework.helper("moveTo", new Class<?>[]{View.class, float.class, float.class, float.class, float.class}, v, x, y, width, pressure);
    }
    public static void lineTo(float x, float y, UpdateMode m) { lineTo(null, x, y, m); }
    public static void lineTo(View v, float x, float y, UpdateMode m) {
        OnyxFramework.helper("lineTo", new Class<?>[]{View.class, float.class, float.class, int.class}, v, x, y, m.onyx);
    }
    public static void lineTo(View v, float x, float y, UpdateMode m, float pressure) {
        OnyxFramework.helper("lineTo", new Class<?>[]{View.class, float.class, float.class, int.class, float.class}, v, x, y, m.onyx, pressure);
    }
    public static void quadTo(float x, float y, UpdateMode m) { quadTo(null, x, y, m); }
    public static void quadTo(View v, float x, float y, UpdateMode m) {
        OnyxFramework.helper("quadTo", new Class<?>[]{View.class, float.class, float.class, int.class}, v, x, y, m.onyx);
    }
    public static void quadTo(View v, float x, float y, UpdateMode m, float pressure) {
        OnyxFramework.helper("quadTo", new Class<?>[]{View.class, float.class, float.class, int.class, float.class}, v, x, y, m.onyx, pressure);
    }

    // ---- geometry -----------------------------------------------------------------------------

    public static float getEpdWidth() { return (Float) OnyxFramework.helper("getEpdWidth", NONE); }
    public static float getEpdHeight() { return (Float) OnyxFramework.helper("getEpdHeight", NONE); }
    public static float getTouchWidth() { return (Float) OnyxFramework.helper("getTouchWidth", NONE); }
    public static float getTouchHeight() { return (Float) OnyxFramework.helper("getTouchHeight", NONE); }
    public static float getMaxTouchPressure() { return (Float) OnyxFramework.helper("getMaxTouchPressure", NONE); }

    public static void mapToView(View v, float[] in, float[] out) { OnyxFramework.helper("mapToView", MAP, v, in, out); }
    public static void mapToEpd(View v, float[] in, float[] out) { OnyxFramework.helper("mapToEpd", MAP, v, in, out); }
    public static void mapToRawTouchPoint(View v, float[] in, float[] out) { OnyxFramework.helper("mapToRawTouchPoint", MAP, v, in, out); }
    public static void mapFromRawTouchPoint(View v, float[] in, float[] out) { OnyxFramework.helper("mapFromRawTouchPoint", MAP, v, in, out); }

    public static Rect mapToEpd(View v, Rect r) {
        float[] b = new float[2], c = new float[2];
        mapToEpd(v, new float[]{r.left, r.top}, b);
        mapToEpd(v, new float[]{r.right, r.bottom}, c);
        return new Rect((int) Math.min(b[0], c[0]), (int) Math.min(b[1], c[1]), (int) Math.max(b[0], c[0]), (int) Math.max(b[1], c[1]));
    }

    public static RectF mapToRawTouchPoint(View v, RectF r) {
        float[] b = new float[2], c = new float[2];
        mapToRawTouchPoint(v, new float[]{r.left, r.top}, b);
        mapToRawTouchPoint(v, new float[]{r.right, r.bottom}, c);
        return new RectF(Math.min(b[0], c[0]), Math.min(b[1], c[1]), Math.max(b[0], c[0]), Math.max(b[1], c[1]));
    }

    /** BaseDevice: scale (m[9], m[10]) then the 3x3 in m[0..8]; null without the framework matrix. */
    public static Matrix getRawTouchPointToScreenMatrix() {
        Object v = OnyxFramework.helper("getEpdToViewMatrix", NONE);
        if (!(v instanceof float[]) || ((float[]) v).length < 11) return null;
        float[] m = (float[]) v;
        Matrix out = new Matrix();
        out.setScale(m[9], m[10]);
        Matrix values = new Matrix();
        values.setValues(m);
        out.postConcat(values);
        return out;
    }

    // ---- touch panel (CTP) and pen digitizer (EMTP) ---------------------------------------------

    public static boolean isCTPPowerOn() { return isTrue(OnyxFramework.callStatic(OnyxFramework.DEVICE_CONTROLLER, "isCTPPowerOn", NONE)); }
    public static boolean isEMTPPowerOn() { return isTrue(OnyxFramework.callStatic(OnyxFramework.DEVICE_CONTROLLER, "isEMTPPowerOn", NONE)); }
    public static void powerCTP(boolean on) { OnyxFramework.callStatic(OnyxFramework.DEVICE_CONTROLLER, "powerCTP", Z, on); }
    public static void powerEMTP(boolean on) { OnyxFramework.callStatic(OnyxFramework.DEVICE_CONTROLLER, "powerEMTP", Z, on); }

    private static Object input(Context c) {
        return c.getSystemService(Context.INPUT_SERVICE);
    }

    public static void setAppCTPDisableRegion(Context c, int[] disable) { setAppCTPDisableRegion(c, disable, (int[]) null); }
    public static void setAppCTPDisableRegion(Context c, int[] disable, int[] exclude) {
        OnyxFramework.callOn(input(c), "setAppCTPDisableRegion", new Class<?>[]{int[].class, int[].class}, disable, exclude);
    }
    public static void setAppCTPDisableRegion(Context c, Rect[] disable) { setAppCTPDisableRegion(c, flatten(disable), (int[]) null); }
    public static void setAppCTPDisableRegion(Context c, Rect[] disable, Rect[] exclude) {
        setAppCTPDisableRegion(c, flatten(disable), exclude == null ? null : flatten(exclude));
    }
    public static void appResetCTPDisableRegion(Context c) { OnyxFramework.callOn(input(c), "appResetCTPDisableRegion", NONE); }
    public static boolean isCTPDisableRegion(Context c) { return isTrue(OnyxFramework.callOn(input(c), "isCTPDisableRegion", NONE)); }
    public static void dumpCTPInfo(Context c) { OnyxFramework.callOn(input(c), "dumpInfo", NONE); }
    public static void setSystemCTPDisableRegion(Context c) { c.sendBroadcast(new Intent(ENABLE_SYSTEM_CTP_ACTION)); }
    public static void systemResetCTPDisableRegion(Context c) { c.sendBroadcast(new Intent(RESET_SYSTEM_CTP_ACTION)); }

    // ---- helpers ------------------------------------------------------------------------------

    /** EInkHelper's app-scope method, or the system-scope one on frameworks without it (SDMDevice's fallback). */
    private static Object eink(String name, String fallback, Class<?>[] sig, Object... args) {
        try {
            return OnyxFramework.callStatic(OnyxFramework.EINK_HELPER, name, sig, args);
        } catch (OnyxFramework.Missing e) {
            return OnyxFramework.callStatic(OnyxFramework.EINK_HELPER, fallback, sig, args);
        }
    }

    /** Rects → flat l,t,r,b with each normalised (SDMDevice does the same min/max). */
    private static int[] flatten(Rect[] rects) {
        int[] out = new int[rects.length * 4];
        for (int i = 0; i < rects.length; i++) {
            Rect r = rects[i];
            out[4 * i] = Math.min(r.left, r.right);
            out[4 * i + 1] = Math.min(r.top, r.bottom);
            out[4 * i + 2] = Math.max(r.left, r.right);
            out[4 * i + 3] = Math.max(r.top, r.bottom);
        }
        return out;
    }

    private static boolean isTrue(Object o) {
        return o instanceof Boolean && (Boolean) o;
    }

    private static float safeMaxTouchPressure() {
        try {
            return getMaxTouchPressure();
        } catch (RuntimeException e) {
            return 0f;
        }
    }
}
