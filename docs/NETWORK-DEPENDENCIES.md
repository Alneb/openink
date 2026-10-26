# Network dependencies & telemetry — BOOX Note Air 5C, firmware 4.2.1

**Goal (2026-10-16):** a rebuilt image that depends on **no vendor server at all**, only the
classic ones (DNS, NTP, connectivity check). So this is an inventory of *every* outbound
dependency: who calls it, when, what it sends, and what breaks without it. Telemetry is the subset
that sends device/user data without a user action.

Static analysis only (jadx). Sources (gitignored): `firmware/onyx/telemetry/kcb-release_src/`
(launcher, 33 772 classes, decompiled per-dex with `-Xmx4g` after two OOM kills),
`firmware/onyx/pen/knote2_src/`, `firmware/onyx/epd/framework_src/`,
`firmware/onyx/telemetry/OnyxOtaService_src/`.

## Server map (`com.onyx.android.sdk.data.ClusterHost`, shared SDK)
| region | main / push | data | log | material | sync (Couchbase WSS) |
|---|---|---|---|---|---|
| CN | `send2boox.com` | `data.boox.com` | `log.boox.com` | `client.send2boox.com` | `wss://cb.send2boox.com/neocloud` |
| US ("American") | `push.boox.com` | `en-data.boox.com` | `en-lon.boox.com` | `en-client.boox.com` | `wss://cb.boox.com/neocloud` |
| EU | `eur.boox.com` | `en-data.boox.com` | `en-data.boox.com` | `eu-client.boox.com` | `wss://eurcb.boox.com/neocloud` |
| test/dev | `dev.send2boox.com`, plain-HTTP/WS `59.41.197.202:90` / `:4984` | | | | `cb-dev.send2boox.com` |
| Vietnam | US cluster + shop `http://ebookvn.akishop.vn` | | | | |
Also seen: `index1..5.boox.com` / `index1..5.send2boox.com` (server-info index, `OnyxIndexService`
`serverInfos`), `www.onyx-international.com`.

## Automatic traffic — launcher `kcb-release` (`com.onyx.reader.main.ui.MainActivity`)
`kcb-release` (system/priv-app) **is the home launcher / library**. On **every Wi-Fi connect**
(`onNetworkChangeEvent` → `isWifiConnected`) it runs, in order:

| method | actions | what goes out |
|---|---|---|
| `r` | `CheckCloudOTAAction` | firmware update check (`firmware/update`) |
| `l` | `CheckCaptivePortalAction` | HTTP probe (Onyx-configurable captive-portal URL) |
| `f` | `SyncTimeBySNTPAction` | SNTP (server list from Onyx, see `UpdateNTPServerAction`) |
| `n` | `FetchDevicePushMessageAction`, `EnableGoogleSuiteAction`, `CheckGMSAction` | **MAC** → device push messages; GMS state |
| `H1` | `UpdateDeviceLockStateAction`, `UpdateNTPServerAction`, `UpdateCaptivePortalAction` | **MAC** + lock state; fetch NTP + captive-portal config |
| `k` | `CheckAppUpdateAction` | Onyx app-store update check (`apks/batchQuery` …) |
| `G1`/`F1`/`H` | `UpdateDeviceConfigsAction`, `UpdateDeviceSerialNumberAction`, `FetchCoreFeaturesAction` | `Build.MODEL` → device configs / feature flags; **MAC + CPU serial** → serial number lookup |
| `U1` | `UploadImeiAction`, `ShowOtaReminderAction` | **MAC, CPU serial, IMEI1/2** → `devices/updateIMEI` (token `ff210891-…`), once (DeviceConfig flag + MMKV `key_upload_imei`) |
| `p` | `CheckGMSAction` | Google services check |
| `c` | **`ActiveDeviceAction`** | `POST api/1/device/detail/activate` {**serialNumber, systemVersion (fingerprint), deviceModel**, HMAC-SHA256 signature with hard-coded key `boox-device-activation-secret-key`}; non-zh-CN locales get a "regional usage notice" dialog |

Other startup actions in MainActivity: `ForceUpgradeStateCheckAction`, `DexOptCheckAction`,
`DataMigrationAction`, `MergeLocalStatisticsDataAction`, `HandleBetaFeatureChangeAction`.

**Device identity used as the key everywhere: the Wi-Fi MAC** (`findByMac`, `getByCpu`,
`updateInstallationIdByMac`, `statistics/person/{mac}`), plus CPU serial and serial number.

### Event statistics — present but disabled in this build
`com.onyx.common.statistics.StatisticsEventHandler` subscribes to ~45 UI events (open app
`pkg/activity`, open document, create/save/export note, JD-shop browsing/cart/top-up …) and calls
`StatisticsManager.onEvent`. The backend is chosen by `StatisticsType`; this build hard-codes
**`StatisticsType.Empty` → `EmptyStatistics` (no-op)**. `OnyxStatistics` exists (→ `statistics/*`
endpoints) — dormant code that a server flag/update could enable; remove for the rebuild.

## Onyx REST API surface (Retrofit interfaces in the SDK)
`OnyxOTAService` (firmware/update, downloadFirmware, TpFirmwares/getUpdate, apks*, betaGroups*,
deviceconfigs/query) · `OnyxStatisticsService` (statistics, readingInfo, readTimeInfo, person/{mac},
annotationList, update) · `OnyxLogService` (encrypt/key, **reports** = user feedback: name, email,
desc, complainInfo, firmware, zip of logs) · `OnyxDeviceInfoService` (devicessn/getByCpu) ·
`DeviceService` (getSN, updateIMEI, activate) · `ContentService` v2 (users/devices by MAC, bind,
auth/local, core/feature, noteLab/features, libraries, messages, installationId) ·
`DeviceProtectService` (devices, canReset, resetPass) · `CloudAccountService` (signin/up,
qrcode, devices/action/state, lock/state, logout, token refresh …) · `OnyxSyncService`
(noteGroups*, filedatas upload, aliyun/sts, notes/version — **cloud note sync**) ·
`EACCloudService` (appConf* — cloud per-app e-ink configs) · `OnyxPushService` · `OnyxBackupService`
(appDatas) · `OnyxHWRResourceService` (**myScriptModels/check, myscripts, myScriptActives** —
handwriting recognition licensing) · `OnyxGetCloudFontsService` · `OnyxDictionaryService` ·
`OnyxBookStoreService` · `OnyxCertificatesService` (certificates, config/stss) ·
`OnyxDeviceCertsService` · `OnyxHardwareService` (bl) · `OnyxProductionTestService` ·
`OnyxStatementUpdateCheckService` (privacy/agreement updatedAt) · `ScreenSaversService`/
`DreamCloudService` (screensavers, mottos, poetries) · `FeedbackService` (faqs) ·
`OnyxHomeworkService`, `OnyxMeetingService`, `OnyxGroupService`, `OnyxConsumerService`.

## Third parties
- **No analytics SDK** (no Umeng/Bugly/Firebase-Analytics/Crashlytics/Sentry) in knote2 or kcb.
  Firebase = core components only.
- kcb bundles **JD.com SDK** (`com.jd.*`, 1538 classes: open API, TDE encryption, JD Read
  comics JNI) for the JD bookshop; plus Yuewen (`sdk.yuewen`), ZhiShu shop.
- Cloud-storage integrations (user-initiated): Google Drive, OneDrive/OneNote (Microsoft AAD/Live),
  Dropbox, Evernote, Notion, Readwise, Zotero, Youdao Note, Baidu Pan, Aliyun Drive, Alibaba OSS.
- Share SDKs: Tencent WeChat (`com.tencent.mm`), many OAuth providers (ScribeJava catalog).
- Google Mobile Services (GmsCore, Phonesky, GSF) in `product` — own traffic, separate decision.

## Qualcomm platform (vendor partition) — XTRA GNSS assistance
- The device ships the full Qualcomm location stack:
  - `vendor/etc/permissions/android.hardware.location.gps.xml` declares GPS;
  - `loc_launcher` is started from `init.qcom.rc`, plus `android.hardware.gnss@2.1-service-qti`;
  - `xtra-daemon` and `lowi-server` are started via `izat.conf`.
- `gps.conf` has **no active lines**, so built-in defaults apply. `xtra-daemon` has compiled-in
  servers:
  - `https://path{1,2,3}.xtracloud.net/xtra3grcej.bin` and `…/xtra3Mgrbeji.bin` (satellite orbit
    assistance data, cached in `/data/vendor/location/xtra/xtra.sqlite`);
  - its own SNTP: `time.xtracloud.net` / `nts.xtracloud.net`.

  These are **Qualcomm servers**, not on the allowed list (DNS/NTP/connectivity check only).
- `izat.conf`: `NLP_MODE = 1`, `OSNLP_PACKAGE = com.google.android.gms` (network location through
  GMS when present), `GTP_PRIVACY_VERSION_URL = https://info.izatcloud.net/privacy/version.html`,
  `SUPL_WIFI=BASIC`.
- Trigger: XTRA downloads when a GNSS session asks for assistance or the cached data expires
  (days); the SNTP fetch goes with it. With location off, expect little or no traffic.
  To confirm on a device (`/data/vendor/location/xtra/` timestamps, DNS log).
- Mitigation candidate **P4 (config only, vendor image)**: in `izat.conf` set
  `PROCESS_STATE=DISABLED` in the `PROCESS_NAME=xtra-daemon` stanza (and optionally
  `IZAT_FEATURE_MASK`/`NLP_MODE = 0`).
  - Consequence: GPS still works but cold fixes are slower (no orbit assistance: minutes instead of
    seconds). GNSS time comes from satellites.
  - Alternatively, point `XTRA_SERVER_1..3` in `gps.conf` at a self-hosted mirror.
  - vendor is in vbmeta: needs the same rebuild/unlock as everything else.
- `system/bin`/`vendor/bin` hold no other compiled-in vendor hosts in the binaries checked
  (`loc_launcher`, `lowi-server`).

### AI assistant
- `com.onyx.aiassistant` (ai-assistant app: floating service, quick-settings tile) and the shared
  client `com.onyx.android.sdk.aiassistant.client` (kreader2, knote2, dict, …) are thin UIs. The
  backend is **`com.onyx.android.ksync.service.KAssistantService`** in ksync (AIDL
  `IAIAssistantService`).
- All of it is **Onyx cloud** (cluster data URL; no local model). Retrofit routes:
  - conversations: `GET/POST/DELETE service/ai/conversation`, `PUT …/{id}`, `POST …/{id}/interrupt`,
    `GET/DELETE …/{id}/record[/{id}]`;
  - `POST service/ai/reading` (whole-document analysis);
  - meetings: `POST service/ai/meetings/minutes`, `GET …/queryMinutes`, `GET …/options`;
  - `GET service/ai/modelConfig` (model list) and `GET ai/prompt/list`.

  Quota-based (reading quota, login tips).
- Triggers: the service is bound **lazily** by AI actions (the AI/translate panel on a text
  selection in kreader2, the assistant UI, `AssistantQuotaReceiver`). Once created, its
  `onNetworkConnectChanged(true)` runs `u()` on **every network connect**: `updateQuotaInfo(true)`
  ×2 + `AssistantLoadAIModelListAction().setForceLoad()` → `LoadAIModelListRequest` to the Onyx
  cluster, with **no login check**. On login change it also re-queries pending reading analyses.
  → Dormant until the user touches an AI feature; afterwards it phones home anonymously on each
  Wi-Fi connect while bound.
- Offline: the feature can't work without the Onyx backend.
  - **R4** remove `ai-assistant`: loses the assistant UI, tile and floating button. The kreader2
    selection panel's AI entry still binds ksync. **Decided 2026-10-17**: removed in the
    rebuilt image (the rebuild tooling is kept private).
  - **Patched (P3 rule 4):** the network listener is a no-op, so the reconnect-triggered fetch
    is gone. Pointing the assistant at a
    local LLM would mean re-implementing the `service/ai/*` API: a separate project.

### Qualcomm Wi-Fi positioning (xtwifi) and CNE (native sweep)
- **`xtwifi-client`**: `PROCESS_STATE=ENABLED` in `izat.conf` (a PREMIUM_FEATURE, mask 0xd03, so
  whether it actually runs depends on loc_launcher's feature evaluation; check on a device).
  `xtwifi.conf`: `XT_SERVER_ROOT_URL(_V3) = https://gtp1.izatcloud.net:443/uds/v2|v3`, and
  `gtpma1.izatcloud.net`. This is Qualcomm's cloud Wi-Fi/cell positioning ("GTP"): it sends
  observed access points to Qualcomm for a position, with a 5 MB local cache.
  `persist.vendor.overlay.izat.optin=rro` (system build.prop) presets the IZAT opt-in through a
  resource overlay. `xtwifi-inet-agent` is DISABLED.
- **CNE** (`vendor.cnd`, `class main`; `persist.vendor.cne.feature=1` in system build.prop;
  `CneApp`). Its WQE library `libwqe.so` has
  `http://cne.qualcomm.com/cne/v1/bqe/traffic` (bandwidth probe download),
  `https://cne-post.qualcomm.com/cne/v1/bqe/findings` (**crowd-sourced Wi-Fi quality reports**),
  `http://cne.qualcomm.com/cne/v1/icd` and `https://cne-ssl.qualcomm.com/cne/v1/icd/wifi-data`
  (internet-connectivity detection). `etc/cne/mwqem.conf`: `mwqem_http_url:www.qualcomm.com`
  (latency probe). Whether BQE/ICD run with feature=1 on a Wi-Fi-only device: check on a device
  (DNS log).
- Mitigation candidates (config only, part of **P4**):
  - `persist.vendor.cne.feature=0` (or drop the `vendor.cnd` service). Loses CNE's Wi-Fi
    quality/"smart network" switching, which matters little without cellular.
  - `izat.conf`: `PROCESS_STATE=DISABLED` for `xtwifi-client` (and `xtra-daemon`, above). Loses
    network-assisted positioning. Satellite GPS still works if the hardware has it.
  - Set the izat opt-in to off.

### Speech SDKs shipped as system libraries (native sweep)
| lib (system/lib64) | vendor | hosts in the binary | loaded by |
|---|---|---|---|
| `libspeechengine.so` + `libsscronet.so` (ByteDance's Cronet) | ByteDance Volcano Engine | `pc-mon.snssdk.com`, `pc-mon-va.byteoversea.com` (SDK monitoring) | kreader2, ksync |
| `libBaiduSpeechSDK.so` | Baidu | `vop.baidu.com`, `vse.baidu.com`, `upl.baidu.com`, `mse.baidu.com`, `openapi.baidu.com` | ksync |
| `libneonui_shared.so` | Alibaba Cloud NUI | `nls-wave.aliyuncs.com` | not loaded by name by any bundled app (dependency of another lib? open) |
- kreader2 read-aloud (`TtsManager.h()`) picks `VolcTtsProvider` (`wss://openspeech.bytedance.com`
  `/api/v1/tts/ws_binary`, cluster `volcano_tts`, license `authenticate_type=pre_bind` with a
  bundled APP_ID/TOKEN), `BaiduTtsProvider`, or `SysTtsProvider` (Android TTS).
- Default (`TtsPackageName`): Chinese locale → `baidu`; otherwise **`com.google.android.tts` if
  installed**, else `baidu`. So with GMS removed, a non-Chinese locale would **fall back to Baidu
  cloud TTS**. Volc is used only when chosen in settings.
- Offline plan: install an offline TTS engine (e.g. RHVoice, eSpeak NG or a sherpa-onnx TTS) and
  select it as the system engine; `SysTtsProvider` then speaks locally. A one-line patch candidate
  (later): make `TtsPackageName.a()` fall back to the system default engine instead of `baidu`.
- `libddijkplayer.so` / `libddijkffmpeg.so` / `libebk-engine.so` / `libcishusafe.so`: igetshop only
  (removed by R1).

## Framework / system_server
- `services.jar` and `framework.jar` contain **no Onyx or vendor endpoints**: every `http(s)://`
  literal is stock AOSP (linkify patterns for flickr/picasa/youtube, XML namespaces). Platform
  defaults still to read on a device: `settings get global captive_portal_https_url` /
  `ntp_server` (`OnyxNtpServer` picks the NTP server by locale, see below).

## Not network
- `OnyxOtaService` (priv-app) — **no network**: decrypts a downloaded package with
  `RsaUtil.nativeDecryptFile` (`libota_jni.so`) and installs it.
- `framework.jar` Onyx code: only an unused `android.onyx.utils.RESTClient`; `OnyxNtpServer`
  picks global/CN/user NTP server by locale.

## Framework hook relevant to the rebuild
`IOECService.applyAppNetworkingConfigsToService / getAppNetworkingConfigs` — Onyx already has a
per-app networking policy in system_server (services.jar, not yet analysed) — could be a native
place to deny network to Onyx apps, or a mechanism to remove.

## Handwriting recognition (must keep offline — user decision)
- Engine is **fully local**: MyScript iink SDK (`com.myscript.iink` in knote2) + `system/lib64/
  libMyScript{2D,Analyzer,Document,Engine,Gesture,Ink,MLOrt,Math,Prediction,Shape,Text}.so` +
  `system/recognition_assets/{conf,resources}` (en_US, zh_CN, math, diagram, raw-content).
- Server gate = **one boolean**: `CheckMyscriptEnableRequest` returns early if
  `RecognizerMMKVHelper.isHWRVerified()`; otherwise (Wi-Fi required) asks
  `checkMyScriptEnable{model, fingerprint}` on the international data host and on `true` calls
  `setHWRVerifyStatus()` — never re-checked. Offline fix: patch `isHWRVerified()` → true (or seed
  the MMKV key).
- Telemetry: `MyScriptActivesRequest` (via `HWRReportEngineActiveAction`) once posts
  `myScriptActives{model, **MAC**, **Android ID**, fingerprint}` → remove.
- Open alternative to study later: Google's on-device ML Kit digital ink is not open; open options
  are thin (e.g. Tesseract is print-only) — keep MyScript, cut its network calls.

## Google services (user asked for GrapheneOS-style sandboxed Play)
GrapheneOS's sandboxed Google Play is not an app you drop in: it is a **compatibility layer in the
OS framework** (`gmscompat` hooks across `frameworks/base` + the GmsCompat app + Play installed as
ordinary unprivileged apps). Here the framework is a **prebuilt, Onyx-modified `framework.jar` /
`services.jar`** (no source), so porting means re-applying GrapheneOS's framework patches as smali
against Onyx's binaries — large, fragile across OTAs.
**Decided 2026-10-17:** remove the privileged Google stack, and port gmscompat. The plan, inventory,
measurements and stages are kept in private notes. Stage 0 for the locked device
today: disabling the Google packages over adb (`pm disable-user`, reversible). No Onyx app calls GMS
APIs; their `gms`/`firebase` classes are transitive library code.

## All bundled apps — static triage
Method: `docs/scripts/apk_net_triage.py` reads every dex string table + the binary manifest (no
decompile): package, sharedUserId, INTERNET, boot/connectivity receivers, SDK markers by class
prefix, URL/host/IP strings. A second pass subtracted the strings present in ≥ 8 apps (the shared
Onyx SDK) to show what each app adds. Raw output: `firmware/onyx/apps/triage.md`, `uniq.txt`
(gitignored). **Static presence ≠ a call**: every finding below still needs its trigger traced.

### The shared Onyx SDK (in every `com.onyx.*` app)
- Onyx cloud: `ClusterHost` region map, `data.boox.com`, `en-data.boox.com`, test hosts
  `59.41.197.202`, `192.168.*`, `oa.o-in.me`, `cloud.o-in.me` (WebDAV), Alibaba OSS + HTTPDNS
  (`203.107.1.1`), CN NTP pool, ScribeJava OAuth catalog (dozens of dead providers).
- **"Facebook SDK" hits are Fresco** (`imagepipeline`, `drawee`, `soloader`): an image library, not
  telemetry.
- **Tencent MTA analytics** (`com.tencent.wxop.stat`, reports to `pingma.qq.com/mstat/report`)
  rides inside the old WeChat open SDK: `WXApiImplV10.registerApp` → `initMta` →
  `StatService.startStatService` (period 60 min) + `onForeground_WX`/`onBackground_WX` events.
  It starts **only when an app registers with WeChat**: in knote2, `WeChatManager` has no outside
  callers, so it is dormant there. Check per app; the safe fix is stubbing `initMta`.

### Per app (what the app adds beyond the shared SDK)
| app | uid | auto-start | notable network / telemetry |
|---|---|---|---|
| **igetshop** (Dedao/luojilab store) | own | CONNECTIVITY receiver | **Tencent Bugly** crash reporting, **Alibaba MAN** analytics (`adash.man.aliyuncs.com`), iFlytek speech + `log.iflytek.com`, luojilab click/log hosts (`click.luojilab.com/nalog`, `logs.luojilab.com`), JD API, dozens of dev/test hosts → **remove the app** |
| **ksync** (sync + AI services hub) | **system** | WorkManager boot/network | Couchbase sync gateway `ws://120.78.79.5:4984/neocloud`; Baidu speech/TTS/OCR (`vop/tsn/upl.baidu.com`, `aip.baidubce.com`), ByteDance speech (`openspeech.bytedance.com`), Tencent OCR, Google Vision/Translate, MS Translator, DoH hosts (Cloudflare, Quad9); **Google ML Kit + datatransport (Firebase logging)** |
| knote2 | own | WorkManager | ML Kit digital ink + datatransport; translate APIs; (P1 done) |
| kreader2 | own | — | ML Kit text recognition + datatransport; Onyx statistics (27 classes); fonts from `onyx-static.oss-cn-shenzhen.aliyuncs.com/static-fonts/EU/`; Baidu translate |
| dict | own | — | Baidu/Google/MS translate, Wiktionary/Baidu Baike/web-search links (user-initiated) |
| kmail | own | BOOT (calendar reminders) | mail/CalDAV, Google Calendar API, DingTalk calendar (user accounts) |
| Gallery | own | — | `data.onyx-international.cn` API hosts (old domain); Google VR/Play links |
| app-market | own | `AppCheckUpdateService` (exported) | Onyx app store → **remove or stub update check** |
| ai-assistant | own | floating + tile service | Onyx cloud via shared SDK (assistant backend) |
| ProductionTest | own | **BOOT** receiver | factory test app; `oa.o-in.me:9058/api/` (internal), `data.boox.com` → **remove** |
| NeoBrowser (Chromium) | own | — | Chrome's own: FCM/`firebaseinstallations`, crash `clients2.google.com/cr/report`, connectivity checks, Google account/sync URLs |
| Clock, Music, VoiceRecorder | own | Clock: BOOT | Simple Mobile Tools forks; only store/social links |
| Calculator, LatinIME, Kime, floatingbutton, tscalibration | own / tscalibration **system** | LatinIME BOOT (icon only) | nothing beyond the shared SDK |
| Settings, SystemUI (system_ext) | system / systemui | AOSP | no extra hosts (GSI list `dl.google.com`, AOSP links) |

### ksync — what starts by itself (decompiled, `firmware/onyx/apps/ksync/src`)
`KSyncApplication.onCreate` (main process): local config/DB init, `ClusterManager.
checkSaveObservable` (**local only**: writes the cluster list to system config),
`KSyncBundle.initCouchbase`, account load → `UpdateUserInfoAction` (when an account is loaded),
`TokenInvalidMonitorAction`, `CloudStorageManager.initCloudNote`, **`MlKit.initialize`**.
On every network connect: `RefreshTokenAction` (**gated: `AccountDataBundle.hasLogin()` and
Wi-Fi**) → reset OSS + `ensureSyncGatewayExist` + `resumeReplicator` (note sync). On disconnect:
`CheckNoteRecordAction` (local). → Without an Onyx account, ksync's own startup does no Onyx
server traffic found so far; speech/OCR/translate are service calls made on user action.
ksync is `android.uid.system` like kcb → firewall can't isolate it; patch if anything is found.

### Google ML Kit (knote2, kreader2, ksync)
- Bundled `com.google.android.datatransport` **CCT backend** with the Firebase logging endpoints
  (obfuscated with `StringMerger` in `CCTDestination`): ML Kit's usage/latency logging. It sends
  over plain HTTP, **without Play services** → counts as vendor telemetry. Fix options: stub
  `CctTransportBackend.send` (smali), or block the logging hosts in DNS.
- **Digital ink** = Onyx's second HWR engine (`com.onyx.android.sdk.note.recognizer.onyxml`:
  `MLMixedRecognizer`, `MLModelManager`): `RemoteModelManager.download(...)` fetches per-language
  models **from Google** on demand → offline HWR for those languages needs the models pre-seeded
  (or rely on MyScript, which is bundled for en_US/zh_CN).
- Text recognition (`OnyxOfflineOCRProvider`) uses the bundled ML Kit model → offline.

## Triggers traced (step 2)
- **app-market** `AppCheckUpdateService`: its handler runs `CheckAppUpdateAction` only when a
  client binds and sends a message. The client is kcb's `CheckAppUpdateAction` (`k`, stubbed by
  P2), so after P2 nothing triggers it. Removal R3 remains an option.
- **NeoBrowser** = an upstream **Chromium** build (`org.chromium.chrome`, `libchrome.so` +
  crashpad). **No Onyx code in the APK.** The e-ink tuning it gets comes from the framework/SF
  per-app (EAC) config, keyed by package, which works for any browser. Google endpoints in
  `libchrome.so`: variations seed (`clientservices.googleapis.com/chrome-variations/seed`), UMA
  `/uma/v2` + UKM (`clients4.google.com/ukm`), opt-in via the first-run ToS/UMA screen; crash
  upload `clients2.google.com/cr/report` + crashpad; domain reliability
  (`clients2.google.com/domainreliability/upload`); component updater
  (`update.googleapis.com/service/update2/json`); Safe Browsing (`safebrowsing.googleapis.com/v4`
  + client reports); optimization guide; content-autofill; translate; sync; feed; time
  (`clients2.google.com/time`). Some of these need Google API keys that a plain Chromium build
  lacks (sync, translate). The variations, component-updater, domain-reliability and Safe Browsing
  traffic can run without keys.
  → Recommendation: **replace NeoBrowser** with a de-Googled Chromium build (e.g. Cromite) or
  Firefox-based (Mull/IronFox), rather than patching `libchrome.so`. Onyx's per-app e-ink
  settings apply to the new package too.

- **kreader2 reading statistics** (`com.onyx.android.sdk.statistics.OnyxStatistics`): records
  document open/close/finish, page changes with duration, annotations add/update/delete,
  dictionary lookups, text selection, form fields, battery and network changes. Events come in via
  the cross-app broadcast extra `args_statistics_data` (`StatisticsPushReceiver`) and are always
  written to the **local** statistics DB. The **feature toggle defaults to on**
  (`key_library_enable_statistics_preference`, default `true`). Every 5 events (or on type
  0/9/10) a `PushStatisticsRequest` is built, but `checkPushable()` uploads only with **Wi-Fi +
  logged-in Onyx account + `isAutoSyncReaderStatistics` + a cluster with statistics enabled**. The
  upload goes to `<cluster international data URL>/api/1/` with the **MAC address** stamped on
  every record plus the account id. → Without an Onyx account nothing leaves the device.
  **Patched (P3 rule 3):** `PushStatisticsRequest.checkPushable()Z` → false in
  kreader2, ksync and knote2 (all three carry the shared-SDK class). The local reading-time
  statistics UI keeps working; nothing is uploaded even with an account.

## Still to inventory
- Static triage of all bundled apps done (above). Traced: kreader2 statistics, app-market, NeoBrowser, MTA
  (dormant without WeChat). Still open: ai-assistant backend calls, `services.jar` networking.
- Native: `libota_jni.so`, any daemon in `system/bin` / `vendor/bin` with sockets.
- What breaks offline: handwriting recognition (MyScript activation), EAC cloud configs, fonts,
  dictionaries, note sync — needs per-feature verification.
