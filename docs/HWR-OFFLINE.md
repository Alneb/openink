# Handwriting recognition offline — engines, language packs, alternatives

User requirement (2026-10-16): handwriting recognition must keep working **with no vendor
server**, and open alternatives should be looked at. This doc maps every HWR path in firmware
4.2.1 and what each needs from the network. Network facts also in
[NETWORK-DEPENDENCIES.md](NETWORK-DEPENDENCIES.md); the activation patch is P1 in
[PATCHES.md](PATCHES.md).

## Two engines
`com.onyx.android.sdk.note.recognizer.RecognizerManager` keeps one `IHWRResourceDownloader` per
`HWREngineType`:

| engine | code | implementation | resources |
|---|---|---|---|
| **MyScript** (iink) | `…recognizer.onyxms` (`OnyxMSEngine`), service in ksync (`KHwrService`) | native `system/lib64/libMyScript*.so` | `/system/recognition_assets` (bundled) + language packs from **Onyx** |
| **Google ML Kit digital ink** | `…recognizer.onyxml` (`MLCommonRecognizer`, `MLMixedRecognizer`, `MLModelManager`) | `com.google.mlkit.vision.digitalink` (bundled AAR) | per-language models from **Google** (`RemoteModelManager`) |

MyScript is the main engine. ML Kit is the second engine, mixed in by `MLMixedRecognizer`.

## MyScript resources
- **Bundled in the image** (`system/recognition_assets`, 26 entries): `conf/{en_US, zh_CN, math,
  diagram, raw-content}.conf`, and `resources/en_US` (cursive + superimposed + text LK, ~13.7 MB),
  `zh_CN` (~36 MB), `math`, `shape`, `analyzer`, `mul` (English secondary). Path constant
  `SYSTEM_HWR_RESOURCE_CONF_PATH = /system/recognition_assets/conf`. These work offline once P1
  removes the activation gate.
- **Other languages = download from Onyx**: `MSResourceDownloader` (ksync) →
  `MyScriptResourceListRequest` → `GET <cluster international data URL>/api/myscripts?version=…`
  (`HWRResourceService.getMSResource`; `CURRENT_HWR_VERSION = 2.3`). It returns a list of
  `HWRResource {lang, url, md5, size, version, engineType, contentType}`. Each pack is saved as
  `/sdcard/.noteResource/<version>/<lang>.zip` (`HWResourceUtils.getHWRResSaveFilePath`) and
  unzipped there. The list is cached in ksync's local Couchbase DB (`saveToLocal` /
  `loadFromLocal`, `setForceLoadFromLocal`), so once downloaded it no longer needs the server.
  Same service: `POST /api/myScriptModels/check` (= the activation check P1 bypasses) and
  `POST /api/myScriptActives` (activation report, also bypassed by P1).
- `ONSCREEN_UNSUPPORTED_LANG_LIST = ar, fa_IR, he_IL, hi_IN, th_TH, ur_PK`: no on-screen
  (in-place) recognition for RTL / complex scripts.

### Offline plan for extra MyScript languages (no code change)
1. **One-time fetch, then air-gap.** Download the wanted packs once, either on the device or with
   `curl` from a PC against `/api/myscripts?version=2.3`. Keep the zips **and** the unpacked
   tree from `/sdcard/.noteResource/`. Shared storage survives app updates and can be restored
   by copying it back. Caveat: the cached list lives in ksync's Couchbase DB. After a factory
   reset the app may want the list again; to verify on a device.
2. **Bake into the image** (verified from code): add `conf/<lang>.conf` +
   `resources/<lang>/` under `/system/recognition_assets/`, the way en_US/zh_CN ship.
   - `.conf` is the standard MyScript iink bundle format:
     ```
     Bundle-Version: 1.0
     Bundle-Name: en_US
     Configuration-Script:
      AddResDir ../resources/

     Name: text
     Type: Text
     Configuration-Script:
      AddResource en_US/en_US-ak-cur.res
      AddResource en_US/en_US-lk-text.res
      EnableAlienCharacters
      SetTextListSize 1
      SetWordListSize 5
      SetCharListSize 1
     ```
     Bundled resources: `<lang>-ak-cur.res` (cursive), `<lang>-ak-superimposed.res`,
     `<lang>-lk-text.res` (lexicon); plus `mul/`, `math/`, `shape/`, `analyzer/`.
   - Engine search path (`OnyxMSEngine`, `configuration-manager.search-path`): APK
     `assets/conf` → `/system/recognition_assets/conf` → `/sdcard/.noteResource/2.3/…`
     (`RecognizerUtils.getConfigSearchPath(2.3f)`).
   - `HWResourceUtils.checkResExistsInSystem(lang)` = "`/system/recognition_assets/conf/<lang>.conf`
     exists" → the language counts as **installed, no download attempted**.
     `getSystemResList()` lists every `xx_YY.conf` there for the language picker.
   - So: unzip a downloaded pack once and copy its `<lang>.conf` + `resources/<lang>/` into the
     image. Check that the pack's conf also uses `AddResDir ../resources/`; if it uses another
     relative layout, mirror that layout.
- Licensing note: the packs are MyScript's proprietary resources, licensed to Onyx devices. Using
  them on the user's own device is the intended use. Do not redistribute them.

## ML Kit digital-ink models
- `MLModelManager`: `DigitalInkRecognitionModelIdentifier.fromLanguageTag(tag)` →
  `RemoteModelManager.download(model, DownloadConditions)` / `isModelDownloaded` /
  `getDownloadedModels` / `deleteDownloadedModel`. Models come from Google's servers (dl.google
  CDN) on demand and are stored in the app's private files. They are not in the image.
- Offline: download once, then the model stays. Pre-seeding into a fresh install means copying
  ML Kit's private model directory plus its shared-prefs bookkeeping. That's possible but fragile;
  prefer MyScript.
- P3 already removes ML Kit's **usage logging** (datatransport). Model downloads are separate,
  user-triggered, and still go to Google.

## Open-source alternatives (survey; nothing here is drop-in)
- There's no open on-device **online** handwriting recognizer for Latin cursive text at MyScript's
  level. Realistic options:
  - **Zinnia** (+ **Tegaki** models): open online character recognizer (SVM) for Japanese/Chinese,
    one character at a time. Good for CJK character input, not for sentences.
  - **Train a small model**: an LSTM/Transformer + CTC on open online-handwriting data (e.g. the
    IAM On-Line database, which has research-only terms), exported to ONNX/TFLite and run
    on-device. The stock image already ships ONNX Runtime pieces (`libMyScriptMLOrt.so` is
    MyScript's), but a self-contained runtime is cleaner. This is a project, not a patch.
  - Offline **OCR** of a rendered page (Tesseract): works for print-like handwriting only. It
    is not real HWR.
- **Recommendation**: keep MyScript (bundled, local, P1 removes its server gate) and pre-fetch the
  extra language packs once. Revisit an open model as a separate project.


## Update 2026-10-17: open models re-surveyed, and the decision
- **InkSight** (Google Research) has open code and weights ("Small-p", trained on public data). It is a large
  vision-language model that converts images to ink, not a real-time online recogniser for the device.
- **InkFM:** weights not confirmed public. **ML Kit Digital Ink:** free but closed. Offline image HTR (TrOCR,
  IAM-trained models, CENSUS-HWR) reads rendered images, not strokes.
- **Decision**: **our own online-ink recogniser**,
  built on the openink-shape pipeline (resampling, per-point features, a numpy-trained recurrent network, a
  `.oisw` file, a pure-Rust runtime). The shape engine comes first; see docs/SHAPE-ENGINE.md.
