# The Onyx layer inside SurfaceFlinger — inventory and reverse-engineering plan

**Request (user, 2026-10-17):** *"can you please reimplemente the surfaceflinger"*, then
*"the main goal is a 100% remplementaion of the whole drawing + eink stack … we want something
functional for a locked device"*, refresh policy = **own policy**, and *"remember you have rea, we need
to understand 100% of all the function in this stack and reimplement them"*.

Binary: `firmware/onyx/sf/surfaceflinger` (system image, firmware 4.2.1, 12.5 MB, arm64, stripped,
sha256 `b99500764a28d6b64d74b55f21f8aead6a28d52fe8e4f910506d4d84b7e245c3`).

## Key finding: the function names are in the binary
`surfaceflinger` is stripped, but it keeps Android **MiniDebugInfo**: section `.gnu_debugdata` is an
xz-compressed ELF whose `.symtab` names **24 366 local functions** (with C++ signatures). So every
Onyx function has its original class, method name and parameter types. Earlier passes (`strings` only; REA `search_strings` timed out) did not use this.

Reproduce (writes to `firmware/onyx/sf/inv/`, gitignored):
```sh
python3 -I docs/scripts/sf_inventory.py firmware/onyx/sf/surfaceflinger firmware/onyx/sf/inv
# [inventory] 3/3 block 0x5cfac8..0x615d10: 915 Onyx functions, 240448 bytes, 119 classes
```
Outputs: `debugdata.elf` (the symbol ELF), `onyx_funcs.tsv` (addr, size, signature, class),
`onyx_classes.tsv`.

## Where the Onyx code is
One contiguous block of `.text`, **0x5cfac8 – 0x615d10** (from `MemoryPainter.cpp` to
`ThemeManager`), between AOSP's `TunnelModeEnabledReporter` and the HIDL `displayservice`. Static
initialisers name the source files: `MemoryPainter.cpp`, `log_utils.cpp`, `string_utils.cpp`,
`charcoal_texture_palette.cpp`, `charcoal_textures.cpp`, `neo_charcoal_pen.cpp`, `smooth_filter.cpp`,
`spline_interpolator.cpp`, `EpdcManager.cpp`, `SurfaceFlingerHelper.cpp`, `HWEpdcManager.cpp`.
Outside the block: the RenderEngine shader factories (`CfaShaderFactory`, `CfaGrayDitherShaderFactory`,
`BWGrayDitherShaderFactory`, ~0x6e1bec) and the call sites Onyx patched into AOSP code (to be listed
from cross-references).

## Modules (from the symbol table; sizes are code bytes)
| module | classes | fns | bytes | role (from names + strings; to be confirmed by decompilation) |
|---|---|---|---|---|
| **Binder entry** | `SurfaceFlingerHelper` | 81 | 21 628 | `processOnyxRequests(code, Parcel, Parcel*)` @0x60d008 (6 KB switch) + one `handleXxx` per request, most posted as lambdas to the main thread |
| **EPDC core** | `EpdcManager`, `HWEpdcManager`, `SWEpdcManager`, `FBDev`, `Debouncer` | 84+31+13+27+2 | ~20 000 | update queue, merge, commit to `/dev/ebc` (`onyx_epdc_screenRefresh`, `SET_EBC_SEND_UPDATE`), fb mapping |
| **Schemas** (the display modes) | `EpdcSchemaManager`, `BaseEpdcSchema`, `NormalSchema`, `HWNormalSchema`, `BootSchema`, `HandwritingSchema`, `DreamSchema`, `HWDreamSchema`, `ExtBufferSchema`, `PowerOffSchema`, `DebouncerSchema` | ~25 | ~8 000 | "### Activate schema: currentSchema %d, newSchema %d, lastSchema %d"; handwriting blocks schema switches |
| **Update policy** | `UpdateEntry`, `FixedUpdateEntryManager`, `UpdateEntryDetector`, `EnhanceStrategy`, `LayerTracker`, `LayerFilter`/`LayerOnFilter`/`LayerOffFilter`/`RotateLayerOffFilter` | ~30 | ~11 000 | per-app/per-layer modes (app scope, transient updates, bypass-until-layer-on/off), GC interval counting ("repeatMode, count, repeatLimit") |
| **Image path** | `EpdImageHandler`, `BwEpdImageHandler`, `ColorEpdImageHandler`, the 3 shader factories, `ThemeManager` | ~30 | ~9 000 | gamma LUT, mono level, saturation/brightness, CPU vs GPU dither, CFA (Kaleido) |
| **SF's own pen input** | `TouchReader`, `TouchConsumer`, `EventQueue`, `PenManager`, `BaseReader`, `BtnReader`, `DrawReader`, `EraseReader`, `BaseDrawReader`, `BaseEraseReader` | ~70 | ~12 000 | `/dev/input/event%d` reader for instant ink (same design as `libonyx_pen_touch_reader.so`); "HandlePenTrigger, sid %d value %d pid %d" |
| **Instant-ink painters** | `MemoryPainter`, `PainterBase`, `PainterNeoBrushPen`, `PainterCharcoal(V2)`, `PainterDash`, `PainterMarker`, `PainterSquare`, `PainterFountainV2`, `PainterSoftEraser`, `PenSkRender` | ~110 | ~30 000 | draw the live stroke into the `/dev/ebc` handwrite buffer |
| **neo::pen** (stroke geometry) | `NeoFountainPen(V2)`, `NeoBrushPen`, `NeoMarkerPen`, `NeoPencilPen`, `NeoCharcoalPen(V2)`, `NeoBallpointPen`, `NeoSquarePen`, splines, filters, brushes, `neopen_*` C API | ~300 | ~90 000 | a static copy of `libneo_pen.so`, already studied ([STROKE.md](STROKE.md)) and ported (`impl/crates/openink-stroke`) |

## Strings (log messages) that pin behaviour
Read through `docs/scripts/safecat.sh`; full list in `firmware/onyx/sf/onyx_strings.txt`. Notable:
`processOnyxRequests %x`; `### sf epdc region received: %d %d %d %d mode: %d`; `commitEpdc`;
`refresh screen (%d, %d - %d, %d) waveform_mode %d flags 0x%x marker %d`;
`### add full screen epdc, mVisibleRegionsDirty %d mGeometryDirty %d`; `### create epdc by update entry.`;
`### getTransformedEpdcWrapper, create full epdc %s`; `### warning: commit epdc in handwriting mode.`;
`### do not switch epd Schema in handwriting mode`; `### Restore to lastSchema: %d`;
`### startHandwriting` / `### stopHandwriting`; `### startDebouncer` / `### debouncer refresh. update mode %d`;
`### EUS, scheme %d`; `### setFBSchema mode 0x%x, changeDither %d, clearType %d`;
`%s mode 0x%x, repeatMode %d, count %d, repeatLimit %d, request %d, isCPUDither %d, isGPUDither %d`;
`### setSaturation / setBrightness / setGamma / setDither / setDitherFilterTolerance`;
`pen control state: %s, penTriggered: %d, current epdc schema: %d, androidDrawing: %d`;
`Set limit region: (%d %d) - (%d %d)`, `Set exclude region: …`; devices `/dev/ebc`,
`/dev/input/event%d`, `/sys/onyx_misc/cytp_lo_filter`; property `vendor.onyx.htcon`; input name `onyx_emp`.

## The e-ink shaders (recovered verbatim)
RenderEngine carries the SkSL source as text; 5 Onyx shaders were extracted to
`firmware/onyx/sf/onyx_shaders.sksl` (gitignored; Onyx's code, reimplemented rather than copied):
1. **BW / gray** (1 077 B): `convertGray` (Rec.601 luma 0.299/0.587/0.114), `applyNewGammaCorrection`
   (luma × 255 → 1-D `gammaLUT` lookup), `applyMonoLevel` (r ≥ monoLevel → white);
   `enableColorAdjust ≤ 0` → plain gray.
2. **Colour (Kaleido CFA) adjust** (5 298 B): uniforms gamma, saturation, brightness,
   saturationFactor, noiseStrength, ditherFilterTolerance; RGB↔HSL (public gists credited in the
   source), `applyHSB` (saturation above `saturationFactor` doubles), `enhanceRGBSaturation`,
   `applyGamma` = pow(c, 1/gamma), per-channel hash noise (`randUniformHash`, `channelRand`),
   `randomDither` skipped when `isNearBlackOrWhite(rgb, ditherFilterTolerance)`.
3. **Blue-noise dither** variants (2 363 / 2 318 / 750 B): `ditherThreshold(coord)`,
   `ditherLevels(g, coord, levels)`, `blueNoiseDither(color, coord, levels)`, same near-black/white skip.

## Entry path: who may call, and where each code goes

Reproduce: `python3 -I docs/scripts/sf_dispatch.py firmware/onyx/sf/surfaceflinger firmware/onyx/sf/inv impl/spec/sf_protocol.json`
(writes `inv/dispatch.tsv` and `inv/dispatch.md`; the jump tables are found by instruction shape, not by address).

1. **`SurfaceFlinger::onTransact` (0x590f14)**: for every code > 0x48 it first calls
   **`SurfaceFlingerHelper::CheckTransactCodeCredentials(code)` (0x60cee8)**. If that returns true, the AOSP
   check (calling uid 1000/1003, or `PermissionCache::checkPermission(ACCESS_SURFACE_FLINGER)`) is
   **skipped**. Otherwise the AOSP check runs and a denied caller gets `-1` plus an error log.
2. The function is a set of bitmasks over code ranges (transcribed in `sf_dispatch.py:exempt`):
   **102 of the 112 named codes are exempt, so any app may call them** (the Binder interface token
   `android.ui.ISurfaceComposer` is still checked by `checkInterface`). The 10 codes that are not exempt are all
   no-ops in the dispatcher (below): MERGE_DISPLAY_UPDATE, MERGE_DISPLAY_UPDATE_BY_COUNT, COLLECT_PEN_MOVE,
   COLLECT_PEN_UP, TRANSFER_EPDC, SHOW_SCREEN, SEND_UPDATE_INFO, ENABLE_BIG_PEN, STOP_SYSTEM, CHANGE_ORIENTATION.
   Two unnamed codes are exempt as well: `0xff0000` (a no-op) and `0xff0027` (it has a handler; see the table).
   - This records stock behaviour (the reimplemented dispatcher must reproduce the whitelist for compatibility).
     It does **not** change the open stack's scope: on the stock device the display integration goes only
     through the whitelisted `ViewUpdateHelper` / public `View.refreshScreen` path (API-COVERAGE.md scope
     rule); no direct Binder client to SurfaceFlinger's private interface is built.
3. After `BnSurfaceComposer::onTransact` returns UNKNOWN_TRANSACTION and the interface check passes, codes above
   AOSP's range reach **`SurfaceFlingerHelper::processOnyxRequests(code, data, reply)` (0x60d008, 6020 B)**.
   That is the only call site (0x591ad8). Its result becomes the transact status.
4. `processOnyxRequests` logs the code through `SurfaceFlingerHelper::debug`, then switches through **two halfword
   jump tables**: codes `0x10002a..0x100201` (472 slots @ 0x34c824) and `0xff0000..0xff0070` (113 slots @ 0x34cbd4).
   It compares `0x100300` and `0x100400` explicitly. Everything else, including 0x100029 (below the first table),
   reaches the shared tail at 0x60e6b8, which **returns 0**.
5. **No-ops in 4.2.1** (they jump straight to the tail): MERGE_DISPLAY_UPDATE, RUN_SH, MERGE_DISPLAY_UPDATE_BY_COUNT,
   COLLECT_PEN_MOVE, COLLECT_PEN_UP, TRANSFER_EPDC, SCREENSHOT_GRANT_PERMISSION, SHOW_SCREEN, SEND_UPDATE_INFO,
   SET_APPLICATION_DISPLAY_SCHEME, SCREENSHOT, ENABLE_BIG_PEN, ENABLE_ONYX_TPD, STOP_SYSTEM, CHANGE_ORIENTATION.
   `RUN_SH` and `SCREENSHOT` are exempt but do nothing, so there is no shell or capture behind them. A
   reimplementation returns 0 for all 15.
6. Many cases are inline: a Parcel read stored straight into a global or an `EpdcManager` field (e.g.
   SET_BRUSH_RAW_DRAWING_ENABLED: `readInt32() != 0` → byte at 0xb1ed5c). The Ghidra decompile of
   `processOnyxRequests` gives the field each one writes; see the per-class sections.

### Code → handler table (fw 4.2.1)

"calls" lists the named calls a case makes before it leaves, de-duplicated. An empty cell means a fully inline
case, such as a field read or write.

| code | name | any app? | case | calls (in order, until the case leaves) |
|---|---|---|---|---|
| `0x100029` | MERGE_DISPLAY_UPDATE | no | `0x60e6b8` | (no-op: return 0) |
| `0x10002a` | GET_MAX_TOUCH_PRESSURE | yes | `0x60d08c` |  |
| `0x10002b` | SET_TRIGGER_BY_PEN_RECT | yes | `0x60d4a0` | SurfaceFlingerHelper::handleSetTriggerByPen |
| `0x10002c` | SET_SCREEN_HANDWRITING_REGION_MODE | yes | `0x60d560` | Parcel::readInt32 → SurfaceFlingerHelper::handleSetHandWritingRegionMode |
| `0x10002d` | byp@ss | yes | `0x60d574` | Parcel::readInt32 |
| `0x10002e` | COLOR_FILTER | yes | `0x60d4e4` | Parcel::readInt32 → SurfaceFlingerHelper::handleColorFilter |
| `0x10002f` | RUN_SH | yes | `0x60e6b8` | (no-op: return 0) |
| `0x100030` | MERGE_DISPLAY_UPDATE_BY_COUNT | no | `0x60e6b8` | (no-op: return 0) |
| `0x100040` | CLEAR_APP_SCOPE_UPDATE | yes | `0x60d428` | Parcel::readInt32 → SurfaceFlingerHelper::handleClearAppScopeUpdate |
| `0x100041` | IS_PEN_STATE_VALID | yes | `0x60d43c` | TouchReader::isPenStateValid |
| `0x100042` | APPLY_SF_DEBUG | yes | `0x60d358` | Parcel::readInt32 → SurfaceFlingerHelper::applySFDebug |
| `0x100043` | GET_PEN_STATE | yes | `0x60d740` |  |
| `0x100044` | CALIBRATION_RESET | yes | `0x60d4b0` | TouchReader::detect |
| `0x100045` | COLLECT_PEN_MOVE | no | `0x60e6b8` | (no-op: return 0) |
| `0x100046` | COLLECT_PEN_UP | no | `0x60e6b8` | (no-op: return 0) |
| `0x100047` | HANDWRITING_REPAINT | yes | `0x60d398` | Parcel::readInt32 → Parcel::dataAvail → SurfaceFlingerHelper::handleHandWritingRepaint |
| `0x100048` | MARK_OEC_SERVICE_READY | yes | `0x60d454` | Parcel::readInt32 |
| `0x100049` | IS_OEC_SERVICE_READY | yes | `0x60d46c` |  |
| `0x100050` | IS_IN_FAST_MODE | yes | `0x60d4c4` | FixedUpdateEntryManager::inApplicationFastMode |
| `0x100051` | USE_GC_FOR_NEW_SURFACE | yes | `0x60d47c` | Parcel::readInt32 |
| `0x100052` | SET_GC_REFRESH_INTERVAL | yes | `0x60d80c` | Parcel::readInt32 |
| `0x100053` | GET_COLOR_TYPE | yes | `0x60d82c` | EpdcManager::getCfaMode |
| `0x100054` | FILL_WHITE_ON_WAKEUP | yes | `0x60d4f8` | Parcel::readInt32 → EpdcManager::enableFillWhiteOnWakeup |
| `0x100055` | SET_EPD_TURBO | yes | `0x60d278` | Parcel::readInt32 |
| `0x100056` | byp@ss_UNTIL_LAYER_OFF | yes | `0x60d754` | Parcel::readInt32 → Parcel::readString16 → String8::String8 → String16::~String16 → LayerTracker::setLayerOffFilter |
| `0x100057` | WHITE_CLEAR | yes | `0x60d890` |  |
| `0x100058` | byp@ss_UNTIL_LAYER_ON | yes | `0x60d918` | Parcel::readString16 → String8::String8 → String16::~String16 → LayerTracker::setLayerOnFilter |
| `0x100059` | TRANSFER_EPDC | no | `0x60e6b8` | (no-op: return 0) |
| `0x100071` | GET_EPD_TURBO | yes | `0x60d844` |  |
| `0x100072` | SCREENSHOT_GRANT_PERMISSION | yes | `0x60e6b8` | (no-op: return 0) |
| `0x100073` | SET_ANTI_FLICKER | yes | `0x60d204` | Parcel::readInt32 → EpdcManager::setAntiFlicker |
| `0x100074` | ENABLE_DITHER | yes | `0x60d1d8` | Parcel::readInt32 → EpdImageHandler::setEnableDither |
| `0x100075` | ENABLE_COLOR_CU | yes | `0x60d310` | Parcel::readInt32 → EpdcManager::setEnableColorCU |
| `0x100076` | APPLY_SATURATION_MIN_VALUE | yes | `0x60d1c4` | Parcel::readInt32 → SurfaceFlingerHelper::handleApplySaturationMinValue |
| `0x100077` | TOGGLE_DREAM | yes | `0x60d2c0` | Parcel::readString16 → String8::String8 → String16::~String16 → Parcel::readInt32 → SurfaceFlingerHelper::handleToggleDream |
| `0x100078` | ENABLE_BW_MODE | yes | `0x60d25c` | Parcel::readInt32 → SurfaceFlingerHelper::handleEnableBWMode |
| `0x100079` | SET_ENHANCE_STRATEGY | yes | `0x60d178` | Parcel::readInt32 |
| `0x100080` | SET_DITHER_THRESHOLD | yes | `0x60d29c` | Parcel::readInt32 → EpdcManager::setDitherThreshold |
| `0x100081` | ENABLE_ENHANCE | yes | `0x60d130` | Parcel::readInt32 |
| `0x100082` | SET_CAPTURE_EXCLUDE_LAYERS | yes | `0x60d7b0` | pthread_mutex_lock → Parcel::readString16 → String8::String8 → std::__1::vector<String8,_std::__1::allocator<String8>_>::insert → String8::~String8 → String16::~String16 → pthread_mutex_unl0ck |
| `0x100083` | POKE_USER_ACTIVITY | yes | `0x60d900` | SurfaceFlingerHelper::handlePokeUserActivity |
| `0x100084` | APPLY_COLOR_PARAMETER | yes | `0x60d8a8` | Parcel::readInt32 → SurfaceFlingerHelper::handleApplyColorParameter |
| `0x100085` | SHUTDOWN | yes | `0x60d90c` | SurfaceFlingerHelper::handleShutdown |
| `0x100086` | GET_NIGHT_MODE_SUPPORT | yes | `0x60d338` | EpdcManager::supportNightMode → Parcel::writeBool |
| `0x100087` | SET_ENABLE_NIGHT_MODE | yes | `0x60d228` | Parcel::readBool → EpdcManager::enableNightMode |
| `0x100088` | APPLY_SATURATION_FACTOR | yes | `0x60d958` | Parcel::readFloat → SurfaceFlingerHelper::handleApplySaturationFactor |
| `0x100089` | APPLY_NOISE_STRENGTH | yes | `0x60d96c` | Parcel::readFloat → SurfaceFlingerHelper::handleApplyNoiseStrength |
| `0x100090` | APPLY_DITHER_FILTER_TOLERANCE | yes | `0x60d36c` | Parcel::readFloat → SurfaceFlingerHelper::handleApplyDitherFilterTolerance |
| `0x100091` | ENABLE_COLOR_ADJUST | yes | `0x60d380` | Parcel::readBool → SurfaceFlingerHelper::handleEnableColorAdjust |
| `0x100092` | SET_AUTO_SYNC_BUF_ENABLE | yes | `0x60d864` | Parcel::readBool |
| `0x100100` | SET_ENABLE_PEN_SIDE_BUTTON | yes | `0x60d8e0` | Parcel::readInt32 |
| `0x100101` | SET_ERASER_RAW_DRAWING_ENABLED | yes | `0x60d530` | Parcel::readInt32 |
| `0x100102` | SET_BRUSH_RAW_DRAWING_ENABLED | yes | `0x60d158` | Parcel::readInt32 |
| `0x100200` | GET_STROKE_PARAMETERS | yes | `0x60d980` | Parcel::readInt32 → MemoryPainter::getStrokeParameters → Parcel::writeInt32 → Parcel::writeFloat |
| `0x100201` | SET_STROKE_PARAMETERS | yes | `0x60d598` | Parcel::readInt32 |
| `0x100300` | GET_EPD_TO_VIEW_MATRIX | yes | `0x60e540` | ui::Transform::inverse → Parcel::writeInt32 → ui::Transform::operator__ → Parcel::writeFloat → ui::Transform::~Transform |
| `0x100400` | SAVE_PEN_ATTACHED_FB | yes | `0x60d120` | MemoryPainter::saveAttachedFB |
| `0xff0000` | ? | yes | `0x60dce8` | Parcel::readInt32 |
| `0xff0001` | REFRESH_SCREEN | yes | `0x60df48` | Parcel::readInt32 → SurfaceFlingerHelper::handleRefreshScreen |
| `0xff0002` | SHOW_SCREEN | no | `0x60e6b8` | (no-op: return 0) |
| `0xff0004` | APPLY_APP_SCOPE_UPDATE | yes | `0x60e02c` | Parcel::readInt32 → SurfaceFlingerHelper::handleApplyAppScopeUpdate |
| `0xff0005` | SEND_UPDATE_INFO | no | `0x60e6b8` | (no-op: return 0) |
| `0xff0006` | SET_STROKE_COLOR | yes | `0x60e0d0` | Parcel::readInt32 → SurfaceFlingerHelper::handleSetStrokeColor |
| `0xff0007` | SET_STROKE_WIDTH | yes | `0x60dc78` | Parcel::readFloat → SurfaceFlingerHelper::handleSetStrokeWidth |
| `0xff0008` | SET_STROKE_STYLE | yes | `0x60e0bc` | Parcel::readInt32 → SurfaceFlingerHelper::handleSetStrokeStyle |
| `0xff0009` | SET_PAINTER_STYLE | yes | `0x60dca0` | Parcel::readInt32 → SurfaceFlingerHelper::handleSetPainterStyle |
| `0xff000a` | MOVE_TO | yes | `0x60dc1c` | Parcel::readInt32 → Parcel::readFloat → SurfaceFlingerHelper::handleMoveTo |
| `0xff000b` | LINE_TO | yes | `0x60dbc0` | Parcel::readInt32 → Parcel::readFloat → SurfaceFlingerHelper::handleLineTo |
| `0xff000c` | ENABLE_POST | yes | `0x60e120` | Parcel::readInt32 → SurfaceFlingerHelper::handleEnablePost |
| `0xff000d` | SET_SCREEN_HANDWRITING_PEN_STATE | yes | `0x60dff0` | Parcel::readInt32 → SurfaceFlingerHelper::handleSetHandWritingPenState |
| `0xff000e` | SET_SCREEN_HANDWRITING_REGION_LIMIT | yes | `0x60e1c0` | Parcel::readInt32 → Parcel::dataAvail |
| `0xff000f` | APPLY_GAMMA_CORRECTION | yes | `0x60db98` | Parcel::readInt32 → SurfaceFlingerHelper::handleGammaCorrection |
| `0xff0010` | QUAD_TO | yes | `0x60db20` | Parcel::readInt32 → Parcel::readFloat → SurfaceFlingerHelper::handleQuadTo |
| `0xff0011` | START_STROKE | yes | `0x60dd0c` | Parcel::readFloat → SurfaceFlingerHelper::handleStartStroke |
| `0xff0012` | ADD_STROKE_POINT | yes | `0x60d0c0` | Parcel::readFloat |
| `0xff0013` | FINISH_STROKE | yes | `0x60d0c0` | Parcel::readFloat |
| `0xff0014` | REPAINT_EVERY_THING | yes | `0x60e398` |  |
| `0xff0015` | SET_APPLICATION_DISPLAY_SCHEME | yes | `0x60e6b8` | (no-op: return 0) |
| `0xff0016` | SCREENSHOT | yes | `0x60e6b8` | (no-op: return 0) |
| `0xff0017` | WAIT_FOR_UPDATE_FINISHED | yes | `0x60db7c` | SurfaceFlingerHelper::handleWaitForUpdateFinished |
| `0xff0018` | ENABLE_BIG_PEN | no | `0x60e6b8` | (no-op: return 0) |
| `0xff0019` | ENABLE_ONYX_TPD | yes | `0x60e6b8` | (no-op: return 0) |
| `0xff001a` | GET_TOUCH_WIDTH | yes | `0x60dfdc` |  |
| `0xff001b` | GET_TOUCH_HEIGHT | yes | `0x60da60` |  |
| `0xff001c` | SUPPORT_REGAL | yes | `0x60db88` | SurfaceFlingerHelper::handleSupportRegal |
| `0xff001d` | STOP_SYSTEM | no | `0x60e6b8` | (no-op: return 0) |
| `0xff001e` | SHOW_IMAGE | yes | `0x60da88` | Parcel::readInt32 → SurfaceFlingerHelper::showImage |
| `0xff001f` | CHANGE_ORIENTATION | no | `0x60e6b8` | (no-op: return 0) |
| `0xff0020` | GET_EPD_HEIGHT | yes | `0x60e1ac` |  |
| `0xff0021` | SET_TRIGGER | yes | `0x60dd48` | Parcel::readInt32 |
| `0xff0022` | SET_SCREEN_HANDWRITING_REGION_EXCLUDE | yes | `0x60dd8c` | Parcel::readInt32 → Parcel::dataAvail |
| `0xff0023` | REPAINT_EVERY_THING_WITH_MODE | yes | `0x60e084` | Parcel::readInt32 |
| `0xff0024` | SET_UPD_LIST_SIZE | yes | `0x60e0f8` | Parcel::readInt32 → SurfaceFlingerHelper::handleSetUpdListSize |
| `0xff0025` | APP_DIE | yes | `0x60e0e4` | Parcel::readInt32 → SurfaceFlingerHelper::handleAppDie |
| `0xff0026` | APPLY_GC_ONCE | yes | `0x60e37c` |  |
| `0xff0027` | ? | yes | `0x60d9f8` |  |
| `0xff0028` | SET_VCOM | yes | `0x60dfa0` | Parcel::readString16 → SurfaceFlingerHelper::handleSetVCom → String16::~String16 |
| `0xff002a` | ENABLE_REGAL | yes | `0x60e018` | Parcel::readInt32 → SurfaceFlingerHelper::handleEnableRegal |
| `0xff002b` | MAP_EPD_TO_VIEW | yes | `0x60da28` | Parcel::readFloat → SurfaceFlingerHelper::transformFromEpd |
| `0xff002c` | MAP_VIEW_TO_EPD | yes | `0x60e4e8` | Parcel::readFloat → ui::Transform::transform → Parcel::writeFloat |
| `0xff002d` | MAP_FROM_RAW_TOUCH_POINT | yes | `0x60e14c` | Parcel::readFloat → SurfaceFlingerHelper::transformFromEpd → Parcel::writeFloat |
| `0xff002e` | MAP_TO_RAW_TOUCH_POINT | yes | `0x60e430` | Parcel::readFloat → ui::Transform::transform → Parcel::writeFloat |
| `0xff002f` | GET_EPD_WIDTH | yes | `0x60dc8c` |  |
| `0xff0060` | APPLY_SATURATION | yes | `0x60dd78` | Parcel::readInt32 → SurfaceFlingerHelper::handleApplySaturation |
| `0xff0061` | APPLY_BRIGHTNESS | yes | `0x60da08` | Parcel::readInt32 → SurfaceFlingerHelper::handleApplyBrightness |
| `0xff0062` | SET_GRAYSCALE_MODE | yes | `0x60e4a8` | EpdcManager::getFBDev → Parcel::readInt32 → FBDev::setGrayscaleMode |
| `0xff0063` | APPLY_MONO_LEVEL | yes | `0x60e41c` | Parcel::readInt32 → SurfaceFlingerHelper::handleApplyMonoLevel |
| `0xff0064` | DEBOUNCER | yes | `0x60e3c0` | Parcel::readInt32 → SurfaceFlingerHelper::handleDebouncer |
| `0xff0065` | DEBOUNCE_INC_REFRESH | yes | `0x60d9ec` | SurfaceFlingerHelper::handleDebouncerIncRefresh |
| `0xff0066` | APPLY_TRANSIENT_UPDATE | yes | `0x60e4d4` | Parcel::readInt32 → SurfaceFlingerHelper::handleApplyTransientScopeUpdate |
| `0xff0067` | CLEAR_TRANSIENT_UPDATE | yes | `0x60e10c` | Parcel::readInt32 → SurfaceFlingerHelper::handleClearTransientScopeUpdate |
| `0xff0068` | PEN_UP | yes | `0x60da1c` | SurfaceFlingerHelper::handlePenUp |
| `0xff0069` | SET_DEBOUNCER_TRANSIENT_UPDATE_MODE | yes | `0x60da74` | Parcel::readInt32 → SurfaceFlingerHelper::handleSetDebouncerTransientUpdateMode |
| `0xff0070` | byp@ss_UNTIL_ROTATE_LAYER_OFF | yes | `0x60dac8` | Parcel::readInt32 → Parcel::readString16 → String8::String8 → String16::~String16 → LayerTracker::setRotateLayerOffFilter → String8::~String8 |

## Findings from the decompile (fw 4.2.1)
Each item is confirmed against the disassembly where it matters. Ghidra loads this PIE at image base 0x100000, so
add 0x100000 to an ELF address to find it in the Ghidra project. `DecompileSf.java` does this for the labels.
- **BYPASS (0x10002d)**: `n >= 0` sets `EpdcManager`+0x2c = n, `n < 0` calls `decreaseBypass(-n)` (clamped at
  0). Every frame, `NormalSchema::output` / `HWNormalSchema::output` call `EpdcManager::bypassUpdate()`, which
  decrements the counter and returns `UpdateEntry::disableUpdateEntry()` (skip this frame's panel update) while
  it was > 0, else `nullUpdateEntry()`. Use in EPDC-ENGINE.md, "who owns the refresh".
- **Handwriting region mode (0x10002c)**: the posted lambda stores the int at `PenManager`+0x38.
  `PenManager::inValidRegion()` (0x5d99d4) checks the pen point, inflated by half the stroke width (+0x34), against
  the limit rects (count +0x103c, rects from +0x3c) and the exclude rects (count +0x2040, from +0x1040).
  - **Mode 0** = any limit rect (multi-region).
  - **Mode != 0** latches the first matching rect index in +0x2044. With mode 1 (single region), later points
    outside that rect are invalid.
  Our docs had 0/1 reversed at first; `RegionMode` in openink-core is now `Multi = 0, Single = 1`.
  The stock SDK open sequence sends 0.
- **SET_BRUSH_RAW_DRAWING_ENABLED (0x100102)**: `readInt32() != 0` → byte at ELF 0xb1ed5c (inline in the dispatcher).

## Plan (the order of the RE pass)
1. Ghidra (via REA, provider ghidra 12.1.4) full analysis of the binary, names applied from
   `onyx_funcs.tsv`.
2. Decompile, class by class, in dependency order: `SurfaceFlingerHelper::processOnyxRequests`
   (code → handler table for all 112 codes) → handlers → `EpdcManager` → `HW/SWEpdcManager` +
   `FBDev` → schemas → update policy → image handlers → pen input → painters.
3. Cross-references into AOSP code: every place stock SF code calls Onyx code (composition hook,
   layer creation, display power) — the patch points.
4. One section per class here: fields (struct layout from the constructor), each method's behaviour,
   constants, and the reimplementation status.
