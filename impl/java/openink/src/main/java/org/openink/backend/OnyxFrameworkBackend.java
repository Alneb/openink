package org.openink.backend;

import android.view.View;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * Backend over the stock android.onyx.ViewUpdateHelper, reached by reflection as Onyx's own
 * SDMDevice does (ServiceManager is hidden API; this class is not). All methods are resolved up
 * front: a missing one fails {@link #open()} with the full list instead of silently no-oping.
 */
public final class OnyxFrameworkBackend implements EpdBackend {
    private static final String HELPER = "android.onyx.ViewUpdateHelper";

    /** How the methods were reached; the hidden-API filter decides which one a process needs. */
    public enum Access {
        /** Plain reflection (the filter lets the class through). */
        DIRECT,
        /** After exempting {@code Landroid/onyx/} from the filter, as the Onyx SDK's ReflectUtil does. */
        EXEMPTED
    }

    private final Map<String, Method> methods;
    public final Access access;

    private OnyxFrameworkBackend(Map<String, Method> methods, Access access) {
        this.methods = methods;
        this.access = access;
    }

    /** @throws UnsupportedOperationException when not on an Onyx framework, naming what is missing */
    public static OnyxFrameworkBackend open() {
        Class<?> c;
        try {
            c = Class.forName(HELPER);
        } catch (ClassNotFoundException e) {
            throw new UnsupportedOperationException(HELPER + " not found: not an Onyx framework", e);
        }
        List<String> missing = new ArrayList<>();
        Map<String, Method> found = resolve(c, missing);
        if (missing.isEmpty()) return new OnyxFrameworkBackend(found, Access.DIRECT);
        // Android 15 blocks these for ordinary apps ("Accessing hidden method ... denied" in logcat).
        String exemptError = exemptOnyxFromHiddenApiFilter();
        missing.clear();
        found = resolve(c, missing);
        if (missing.isEmpty()) return new OnyxFrameworkBackend(found, Access.EXEMPTED);
        throw new UnsupportedOperationException(HELPER + " lacks " + missing + " (hidden-API filter; exemption: "
            + (exemptError == null ? "applied, no effect" : exemptError) + ")");
    }

    private static Map<String, Method> resolve(Class<?> c, List<String> missing) {
        Object[][] sigs = {
            {"refreshScreen", new Class<?>[]{int.class, int.class, int.class, int.class, int.class}},
            {"waitForUpdateFinished", new Class<?>[]{}},
            {"setScreenHandWritingPenState", new Class<?>[]{int.class}},
            {"getPenState", new Class<?>[]{}},
            {"setScreenHandWritingRegionLimit", new Class<?>[]{View.class, int[].class}},
            {"setScreenHandWritingRegionExclude", new Class<?>[]{View.class, int[].class}},
            {"setScreenHandWritingRegionMode", new Class<?>[]{int.class}},
            {"setStrokeWidth", new Class<?>[]{float.class}},
            {"setStrokeColor", new Class<?>[]{int.class}},
            {"setStrokeStyle", new Class<?>[]{int.class}},
            {"setStrokeParameters", new Class<?>[]{int.class, float[].class}},
            {"getStrokeParameters", new Class<?>[]{int.class}},
            {"setBrushRawDrawingEnabled", new Class<?>[]{boolean.class}},
            {"setEraserRawDrawingEnabled", new Class<?>[]{boolean.class, int.class}},
            {"enablePost", new Class<?>[]{int.class}},
            {"penUp", new Class<?>[]{}},
            {"handwritingRepaint", new Class<?>[]{View.class, int.class, int.class, int.class, int.class, boolean.class}},
        };
        Map<String, Method> found = new HashMap<>();
        for (Object[] s : sigs) {
            String name = (String) s[0];
            try {
                found.put(name, c.getMethod(name, (Class<?>[]) s[1]));
            } catch (NoSuchMethodException e) {
                missing.add(name);
            }
        }
        return found;
    }

    /**
     * {@code VMRuntime.getRuntime().setHiddenApiExemptions({"Landroid/onyx/"})}, called through
     * Class's own reflection methods so the lookup is attributed to the boot class path (the Onyx
     * SDK's trick, ReflectUtil, which exempts everything with "L"; we only exempt Onyx's classes).
     * @return null on success, else why it failed
     */
    private static String exemptOnyxFromHiddenApiFilter() {
        try {
            Method forName = Class.class.getDeclaredMethod("forName", String.class);
            Method getDeclared = Class.class.getDeclaredMethod("getDeclaredMethod", String.class, Class[].class);
            Class<?> vmRuntime = (Class<?>) forName.invoke(null, "dalvik.system.VMRuntime");
            Method getRuntime = (Method) getDeclared.invoke(vmRuntime, "getRuntime", (Object) null);
            Method setExemptions = (Method) getDeclared.invoke(vmRuntime, "setHiddenApiExemptions", (Object) new Class<?>[]{String[].class});
            setExemptions.invoke(getRuntime.invoke(null), new Object[]{new String[]{"Landroid/onyx/"}});
            return null;
        } catch (ReflectiveOperationException | RuntimeException e) {
            Throwable t = e instanceof InvocationTargetException ? e.getCause() : e;
            return t.getClass().getSimpleName() + ": " + t.getMessage();
        }
    }

    private Object call(String name, Object... args) {
        try {
            return methods.get(name).invoke(null, args); // all targets are static
        } catch (IllegalAccessException e) {
            throw new IllegalStateException(name, e);
        } catch (InvocationTargetException e) {
            throw new IllegalStateException(name + " threw", e.getCause());
        }
    }

    @Override public void refreshScreen(int l, int t, int w, int h, int mode) { call("refreshScreen", l, t, w, h, mode); }
    @Override public void waitForUpdateFinished() { call("waitForUpdateFinished"); }
    @Override public void setPenState(int command) { call("setScreenHandWritingPenState", command); }
    @Override public int getPenState() { return (Integer) call("getPenState"); }
    // The framework adds the view's screen location to the array in place: always pass a copy.
    @Override public void setRegionLimit(View v, int[] r) { call("setScreenHandWritingRegionLimit", v, r.clone()); }
    @Override public void setRegionExclude(View v, int[] r) { call("setScreenHandWritingRegionExclude", v, r.clone()); }
    @Override public void setRegionMode(int mode) { call("setScreenHandWritingRegionMode", mode); }
    @Override public void setStrokeWidth(float w) { call("setStrokeWidth", w); }
    @Override public void setStrokeColor(int color) { call("setStrokeColor", color); }
    @Override public void setStrokeStyle(int style) { call("setStrokeStyle", style); }
    @Override public void setStrokeParameters(int style, float[] p) { call("setStrokeParameters", style, p.clone()); }
    @Override public float[] getStrokeParameters(int style) {
        Object r = call("getStrokeParameters", style);
        return r instanceof float[] ? (float[]) r : new float[0];
    }
    @Override public void setBrushRawDrawingEnabled(boolean e) { call("setBrushRawDrawingEnabled", e); }
    @Override public void setEraserRawDrawingEnabled(boolean e, int arg) { call("setEraserRawDrawingEnabled", e, arg); }
    @Override public void enablePost(int enable) { call("enablePost", enable); }
    @Override public void penUp() { call("penUp"); }
    @Override public void handwritingRepaint(View v, int l, int t, int r, int b) { call("handwritingRepaint", v, l, t, r, b, false); }
}
