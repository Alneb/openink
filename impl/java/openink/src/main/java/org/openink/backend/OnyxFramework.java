package org.openink.backend;

import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.util.Arrays;
import java.util.concurrent.ConcurrentHashMap;

/**
 * Calls into the stock Onyx framework by reflection, the way the Onyx SDK's SDMDevice does: one
 * named method per call, resolved once and cached. The bindings each SDK method uses are listed by
 * docs/scripts/sdk_bindings.py. A method the framework does not have (or hides from this app)
 * throws {@link Missing} naming it, never a silent no-op.
 */
public final class OnyxFramework {
    public static final String VIEW_UPDATE_HELPER = "android.onyx.ViewUpdateHelper";
    public static final String EINK_HELPER = "android.onyx.optimization.EInkHelper";
    public static final String DEVICE_CONTROLLER = "android.onyx.hardware.DeviceController";

    /** The framework lacks the method, or the hidden-API filter blocks it for this app. */
    public static final class Missing extends UnsupportedOperationException {
        private static final long serialVersionUID = 1L;

        Missing(String what, Throwable cause) {
            super(what + " is not reachable on this framework", cause);
        }
    }

    private static final ConcurrentHashMap<String, Method> CACHE = new ConcurrentHashMap<>();

    private OnyxFramework() {}

    /** A static method of the class named {@code cls}. */
    public static Object callStatic(String cls, String name, Class<?>[] sig, Object... args) {
        return invoke(resolve(cls, null, name, sig), null, args);
    }

    /** An instance method on {@code target} (Onyx's additions to View, InputManager, WebView). */
    public static Object callOn(Object target, String name, Class<?>[] sig, Object... args) {
        return invoke(resolve(null, target.getClass(), name, sig), target, args);
    }

    /** {@link #callStatic} on ViewUpdateHelper, the class almost every EpdController call reaches. */
    public static Object helper(String name, Class<?>[] sig, Object... args) {
        return callStatic(VIEW_UPDATE_HELPER, name, sig, args);
    }

    private static Method resolve(String cls, Class<?> on, String name, Class<?>[] sig) {
        String key = (cls != null ? cls : on.getName()) + "#" + name + Arrays.toString(sig);
        Method m = CACHE.get(key);
        if (m != null) return m;
        try {
            Class<?> c = cls != null ? Class.forName(cls) : on;
            m = c.getMethod(name, sig);
        } catch (ReflectiveOperationException e) {
            throw new Missing(key, e);
        }
        CACHE.put(key, m);
        return m;
    }

    private static Object invoke(Method m, Object target, Object[] args) {
        try {
            return m.invoke(target, args);
        } catch (IllegalAccessException e) {
            throw new Missing(m.toString(), e);
        } catch (InvocationTargetException e) {
            throw new IllegalStateException(m.getName() + " threw", e.getCause());
        }
    }
}
