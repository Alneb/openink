package android.util;

/** Host-only stand-in for android.util.Log, so library classes that log run in host checks. */
public final class Log {
    private Log() {}

    public static int i(String tag, String msg) {
        return 0;
    }

    public static int w(String tag, String msg) {
        System.out.println("  [log w] " + msg);
        return 0;
    }

    public static int e(String tag, String msg, Throwable t) {
        System.out.println("  [log e] " + msg + ": " + t);
        return 0;
    }
}
