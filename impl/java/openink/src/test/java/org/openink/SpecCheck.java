package org.openink;

import java.nio.file.Files;
import java.nio.file.Paths;
import java.util.regex.Matcher;
import java.util.regex.Pattern;
import org.openink.epd.UpdateMode;
import org.openink.sf.SfCode;

/** Host check of the Java layer against impl/spec/sf_protocol.json. Usage: SpecCheck <spec.json> */
public final class SpecCheck {
    public static void main(String[] args) throws Exception {
        String spec = new String(Files.readAllBytes(Paths.get(args[0])), "UTF-8");
        int failures = 0, modes = 0, codes = 0;
        Matcher m = Pattern.compile("\"((?:UI_\\w+|HAND_WRITING_REPAINT)_MODE)\": (\\d+)").matcher(spec);
        while (m.find()) {
            int v = Integer.parseInt(m.group(2));
            if (UpdateMode.fromOnyx(v).toOnyx() != v) {
                System.out.println("FAIL mode " + m.group(1) + "=" + v);
                failures++;
            }
            modes++;
        }
        String codeBlock = spec.substring(spec.indexOf("\"codes\": {"), spec.indexOf("}", spec.indexOf("\"codes\": {")));
        Matcher c = Pattern.compile("\"(\\w+)\": (\\d+)").matcher(codeBlock);
        while (c.find()) {
            if (SfCode.valueOf(c.group(1)).code != Integer.parseInt(c.group(2))) {
                System.out.println("FAIL code " + c.group(1));
                failures++;
            }
            codes++;
        }
        if (codes != SfCode.values().length) {
            System.out.println("FAIL SfCode has " + SfCode.values().length + " entries, spec " + codes);
            failures++;
        }
        System.out.println("[spec-check] " + modes + " modes, " + codes + " codes, " + failures + " failures");
        if (failures > 0 || modes < 19 || codes < 100) System.exit(1);
    }
}
