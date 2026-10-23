#!/usr/bin/env python3
"""Network/telemetry triage of APKs without decompiling.

Per APK: manifest facts (package, sharedUserId, INTERNET, boot receivers, exported services),
third-party SDK markers (by class-descriptor prefix) and network endpoints (URL / host strings)
read straight from the dex string tables. Output is a Markdown report on stdout.

Usage: apk_net_triage.py APK... > report.md
Pipe terminal views through docs/scripts/safecat.sh (project rule).
"""
import re
import struct
import sys
import zipfile
from collections import Counter

# Shape-based first: any descriptor under these prefixes counts. Names → what it is.
SDK_PREFIXES = {
    "Lcom/umeng/": "Umeng analytics/push",
    "Lcom/tencent/bugly/": "Tencent Bugly crash reporting",
    "Lcom/tencent/beacon/": "Tencent Beacon analytics",
    "Lcom/tencent/mm/": "WeChat SDK",
    "Lcom/tencent/tauth/": "QQ connect",
    "Lcom/tencent/mmkv/": "MMKV (local KV store)",
    "Lcom/tencent/android/tpush/": "Tencent push (XG)",
    "Lcom/google/firebase/analytics/": "Firebase Analytics",
    "Lcom/google/android/gms/measurement/": "Google measurement (Analytics)",
    "Lcom/google/firebase/crashlytics/": "Firebase Crashlytics",
    "Lcom/google/firebase/messaging/": "Firebase Cloud Messaging",
    "Lcom/google/android/gms/ads/": "Google Ads",
    "Lcom/crashlytics/": "Crashlytics (legacy)",
    "Lio/sentry/": "Sentry",
    "Lcom/sensorsdata/": "SensorsData analytics",
    "Lcom/baidu/mobstat/": "Baidu Mobstat",
    "Lcom/baidu/": "Baidu SDK (other)",
    "Lcom/alibaba/sdk/android/oss/": "Alibaba OSS storage",
    "Lcom/alibaba/sdk/android/man/": "Alibaba Mobile Analytics",
    "Lcom/aliyun/": "Aliyun SDK",
    "Lcom/alipay/": "Alipay",
    "Lcom/igexin/": "Getui push",
    "Lcn/jpush/": "JPush",
    "Lcn/jiguang/": "Jiguang (JPush/JAnalytics)",
    "Lcom/xiaomi/push/": "Xiaomi push",
    "Lcom/xiaomi/mipush/": "Xiaomi push",
    "Lcom/huawei/hms/": "Huawei HMS",
    "Lcom/amplitude/": "Amplitude",
    "Lcom/mixpanel/": "Mixpanel",
    "Lcom/flurry/": "Flurry",
    "Lcom/appsflyer/": "AppsFlyer",
    "Lcom/adjust/sdk/": "Adjust",
    "Lcom/facebook/appevents/": "Facebook app events",
    "Lcom/facebook/": "Facebook SDK (other)",
    "Lcom/microsoft/appcenter/": "MS AppCenter",
    "Lcom/microsoft/identity/": "MS identity (AAD)",
    "Lcom/dropbox/": "Dropbox SDK",
    "Lcom/jd/": "JD.com SDK",
    "Lcom/iflytek/": "iFlytek (speech)",
    "Lcom/microsoft/cognitiveservices/": "MS Cognitive Services (speech)",
    "Lcom/openai/": "OpenAI client",
    "Lio/agora/": "Agora RTC",
    "Lcom/qiniu/": "Qiniu storage",
    "Lcom/amazonaws/": "AWS SDK",
    "Lcom/evernote/": "Evernote SDK",
    "Lcom/google/api/services/drive/": "Google Drive API",
    "Lcom/github/scribejava/": "ScribeJava OAuth",
    "Lokhttp3/": "OkHttp",
    "Lretrofit2/": "Retrofit",
    "Lorg/java_websocket/": "Java-WebSocket",
    "Lio/socket/": "socket.io",
    "Lorg/eclipse/paho/": "MQTT (Paho)",
    "Lcom/hivemq/": "MQTT (HiveMQ)",
    "Lcom/onyx/android/sdk/data/ClusterHost": "Onyx cloud region map",
    "Lcom/onyx/android/sdk/data/CloudManager": "Onyx CloudManager",
    "Lcom/onyx/android/sdk/statistics/": "Onyx statistics",
    "Lcom/onyx/common/statistics/": "Onyx statistics (common)",
}

URL_RE = re.compile(r"(?:https?|wss?|ftp|mqtt|tcp)://[^\s\"'<>{}|\\^`]+")
# Bare hosts: a dotted name ending in a common TLD; filters out package names by requiring
# the TLD to be last and the first label not to look like a Java package root.
HOST_RE = re.compile(r"^(?:[a-z0-9-]+\.)+(?:com|cn|net|org|io|ai|app|cloud|vn|de|uk|co)$", re.I)
JAVA_ROOTS = {"com", "org", "net", "io", "android", "java", "javax", "kotlin", "cn", "androidx"}
IPV4_RE = re.compile(r"\b(?:\d{1,3}\.){3}\d{1,3}(?::\d+)?\b")


def uleb(buf, off):
    val = shift = 0
    while True:
        b = buf[off]
        off += 1
        val |= (b & 0x7F) << shift
        if not b & 0x80:
            return val, off
        shift += 7


def dex_strings(data):
    """All strings in a dex string table (MUTF-8 decoded leniently)."""
    n, ids_off = struct.unpack_from("<II", data, 0x38)
    out = []
    for i in range(n):
        (off,) = struct.unpack_from("<I", data, ids_off + 4 * i)
        _, start = uleb(data, off)
        end = data.index(b"\0", start)
        out.append(data[start:end].decode("utf-8", "replace"))
    return out


# --- minimal Android binary XML (AXML) reader: enough for manifest attributes -------------
def axml_elements(buf):
    """Yield (tag, {attr: value}) for each start element of a binary XML."""
    pos = 8
    strings = []
    while pos < len(buf):
        ctype, hsize, csize = struct.unpack_from("<HHI", buf, pos)
        if ctype == 0x0001:  # string pool
            count, _, flags, s_off = struct.unpack_from("<IIII", buf, pos + 8)
            utf8 = flags & 0x100
            base = pos + s_off
            for i in range(count):
                (o,) = struct.unpack_from("<I", buf, pos + hsize + 4 * i)
                p = base + o
                if utf8:
                    _, p = _len8(buf, p)
                    blen, p = _len8(buf, p)
                    strings.append(buf[p:p + blen].decode("utf-8", "replace"))
                else:
                    (ln,) = struct.unpack_from("<H", buf, p)
                    p += 2
                    if ln & 0x8000:
                        (lo,) = struct.unpack_from("<H", buf, p)
                        ln = ((ln & 0x7FFF) << 16) | lo
                        p += 2
                    strings.append(buf[p:p + 2 * ln].decode("utf-16le", "replace"))
        elif ctype == 0x0102:  # start element
            name_i, = struct.unpack_from("<I", buf, pos + 20)
            a_start, a_size, a_count = struct.unpack_from("<HHH", buf, pos + 24)
            attrs = {}
            for k in range(a_count):
                ap = pos + 16 + a_start + k * a_size
                _, an, araw, _, _, dtype, dval = struct.unpack_from("<IIIHBBI", buf, ap)
                key = strings[an] if an < len(strings) else str(an)
                if araw != 0xFFFFFFFF:
                    attrs[key] = strings[araw]
                elif dtype == 0x12:
                    attrs[key] = "true" if dval else "false"
                else:
                    attrs[key] = dval
            yield strings[name_i], attrs
        pos += csize


def _len8(buf, p):
    ln = buf[p]
    p += 1
    if ln & 0x80:
        ln = ((ln & 0x7F) << 8) | buf[p]
        p += 1
    return ln, p


def manifest_facts(z):
    facts = {"perms": set(), "boot": [], "services": [], "providers": [], "attrs": {}}
    current = None
    for tag, a in axml_elements(z.read("AndroidManifest.xml")):
        if tag == "manifest":
            facts["attrs"] = {k: a.get(k) for k in ("package", "sharedUserId")}
        elif tag == "uses-permission":
            facts["perms"].add(a.get("name"))
        elif tag in ("receiver", "service", "provider", "activity"):
            current = (tag, a.get("name"), a.get("exported"))
            if tag == "service":
                facts["services"].append(f"{a.get('name')} (exported={a.get('exported')})")
            elif tag == "provider":
                facts["providers"].append(a.get("name"))
        elif tag == "action" and current and current[0] == "receiver":
            act = a.get("name", "")
            if any(s in act for s in ("BOOT_COMPLETED", "CONNECTIVITY_CHANGE", "STATE_CHANGE")):
                facts["boot"].append(f"{current[1]} ← {act.rsplit('.', 1)[-1]}")
    return facts


def triage(path):
    z = zipfile.ZipFile(path)
    strings = []
    for name in sorted(n for n in z.namelist() if re.fullmatch(r"classes\d*\.dex", n)):
        strings.extend(dex_strings(z.read(name)))
    strings = set(strings)
    sdks = Counter()
    for s in strings:
        if s.startswith("L") and s.endswith(";"):
            for pre, label in SDK_PREFIXES.items():
                if s.startswith(pre):
                    sdks[label] += 1
    # A more specific label claims its prefix; drop the generic "(other)" duplicate counts.
    urls, hosts, ips = set(), set(), set()
    for s in strings:
        if len(s) > 400:
            continue
        urls.update(URL_RE.findall(s))
        if HOST_RE.match(s) and s.split(".", 1)[0].lower() not in JAVA_ROOTS:
            hosts.add(s.lower())
        ips.update(m for m in IPV4_RE.findall(s) if not m.startswith(("0.", "1.0", "2.0")))
    return manifest_facts(z), sdks, urls, hosts, ips


def report(path):
    m, sdks, urls, hosts, ips = triage(path)
    name = path.rsplit("/", 1)[-1]
    print(f"## {name}\n")
    print(f"- package `{m['attrs'].get('package')}` · sharedUserId `{m['attrs'].get('sharedUserId')}`"
          f" · INTERNET: {'yes' if 'android.permission.INTERNET' in m['perms'] else 'no'}")
    if m["boot"]:
        print("- boot/network receivers: " + "; ".join(sorted(set(m["boot"]))))
    if m["services"]:
        print(f"- services ({len(m['services'])}): " + "; ".join(m["services"][:15]))
    print("- SDKs: " + (", ".join(f"{k} ({v})" for k, v in sdks.most_common()) or "none matched"))
    interesting = sorted(u for u in urls if not re.search(
        r"schemas\.android|w3\.org|apache\.org|xmlpull|ns\.adobe|purl\.org|xml\.org|"
        r"example\.(com|org)|localhost|127\.0\.0\.1", u))
    print(f"- URLs ({len(interesting)}):")
    for u in interesting[:200]:
        print(f"  - `{u}`")
    if hosts:
        print(f"- bare hosts: " + ", ".join(f"`{h}`" for h in sorted(hosts)[:80]))
    if ips:
        print(f"- IPs: " + ", ".join(f"`{i}`" for i in sorted(ips)[:40]))
    print()


if __name__ == "__main__":
    print("# APK network/telemetry triage (generated by docs/scripts/apk_net_triage.py)\n")
    for p in sys.argv[1:]:
        try:
            report(p)
        except Exception as e:  # never silently skip an APK
            print(f"## {p}\n\n- **triage failed:** {type(e).__name__}: {e}\n", flush=True)
            print(f"triage failed for {p}: {e}", file=sys.stderr)
