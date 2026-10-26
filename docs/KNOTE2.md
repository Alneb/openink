# knote2 (Onyx Notes) — storage formats beyond the pen layer

Static analysis of `firmware/onyx/pen/knote2_src/` (jadx output of
`knote2-release.apk`, firmware 4.2.1). Not yet checked against a real note on the device. The pen
and ink side of knote2 is in [ONYX-EPD-PEN.md](ONYX-EPD-PEN.md); its network behaviour is in
[NETWORK-DEPENDENCIES.md](NETWORK-DEPENDENCIES.md) and patch P3 (kept private).

## Where a note lives
App data root: `/data/data/com.onyx.android.note/` (`AppPaths.appFilesDir`).

| what | path | format |
|---|---|---|
| library: notes, templates, tags, links, resources | `databases/ShapeDatabase.db` | Room `NoteRoomDatabase` **v51**, plain SQLite |
| one document's pages + shapes | `databases/<documentId>.db` | Room `NewShapeRoomDatabase` **v27**, plain SQLite, one file per document |
| stroke points | `files/point/<documentId>/<pageId>/<pageId>#<revisionId>#points` | binary "point document" (below) |
| other DBs | `NoteRecordDatabase.db` (PBRecordEntity), `NoteConfigDatabase.db`, `NoteStatisticsDatabase.db`, `GroupDatabase.db`, `KSyncRecordDatabase.db` | Room |

Nothing is encrypted: `GenericRoomDatabaseManager` uses `Room.databaseBuilder(context, cls, name)`
with only a `NoDeleteCorruptionOpenHelperFactory` (no SQLCipher). The per-document databases are
opened through a `DocumentDatabaseRegistry` (a `ConcurrentHashMap` keyed by documentId, opened
lazily, closed explicitly).

## Database schemas
- `NoteRoomDatabase` v51 entities: NoteModel, ResourceModel, TemplateModel, TemplateCategoryModel,
  TemplateResourceModel, MaterialAddRecordModel, DataRepositoryDownloadRecordModel, LinkModel,
  TagModel, NoteAccessHistory, ShapeModel.
- `NewShapeRoomDatabase` v27 entities: VirtualTocModel, VirtualPageModel, VirtualDocModel,
  NotePageModel, **NewShapeModel**, HWRShapeModel, HWRDataModel, NoteOperationRecordModel.
  Migrations: `NewShapeRoomMigrations`, plus `DbflowToRoomMigration` (older notes were DBFlow).

`NewShapeModel` (table `NewShapeModel`, unique index on `shapeUniqueId`), including the columns
inherited from `ShapeModel`:

| column | type | notes |
|---|---|---|
| id | int PK autoincrement | |
| shapeUniqueId | text | UUID, 36 chars (the key used in point files) |
| documentUniqueId, pageUniqueId, groupId, appId | text | |
| shapeType | int | |
| color, fillColor, thickness, zorder | | |
| boundingRect, matrixValues | text | serialised RectF / Matrix |
| text, richText, textStyle, shapeLineStyle, shapeCreateArgs, extraAttributes | text | |
| orientation, layoutType, pageOriginWidth/Height, rotationPointX/YCoordinate, subPageName | | |
| revisionId, infoRevisionId, syncStatus | | sync bookkeeping (KSync / Couchbase) |
| linkIdList, tagId, resourceId, optionsRepo, shapeConnection | text | |
| coordType | int | |
| **pointSaveType** | int, default 0 | whether points are inline (`points` BLOB) or in a point file |
| points | BLOB | inline points |
| createdAt, updatedAt | | |

## Point document format (`PointDocumentLoaderV1`)
Java `DataOutput`/`ByteBuffer` defaults, so **big-endian**.
```
offset 0   header, 76 bytes
           i16   reserved (0)
           i16   version (1 = PointDocumentVersion.V1)
           u8[36] pageUniqueId  (UTF-8, right-padded with spaces)
           u8[36] revisionId    (UTF-8, right-padded with spaces)
offset 76  shape blocks, back to back; for each shape:
           i16 attrA, i16 attrB
           N × point, 16 bytes each:
               f32 x, f32 y, i16, i16, i32 time
xref       one 44-byte entry per shape: u8[36] shapeUniqueId, i32 offset, i32 length
           (length = 4 + 16·N, i.e. it includes the attr shorts)
last 4 B   i32 offset of the xref table
```
- The reader seeks to `size − 4`, reads the xref offset, then `parse(start, count)` reads entries
  `start .. start+count` (partial loads for big pages).
- Shapes over 20 000 points are decimated on read: keep every `max(1, round(n / 20000))`-th point
  ("large touch point count" warning). Reads are chunked at 80 MiB.
- File name `<pageId>#<revisionId>#points`, split on `#`, so a page directory can hold several
  revisions; `PagePointLoader` picks the requested one (or all).
- **The two i16 fields per point:** the writer emits `getSize()` then `getPressure()`, but the
  reader passes the shorts to `TinyPoint(x, y, s, s2, t)`, whose constructor stores the *first*
  as pressure and the second as size. Size and pressure therefore swap on every save/load cycle
  unless the jadx field mapping is wrong. Settle it with a real file: pressure varies within a
  stroke, size usually doesn't.

## Protobuf schemas (`docs/formats/knote2-proto/`)
knote2 uses full protobuf (not lite), so every generated class embeds its serialized
`FileDescriptorProto`. `docs/scripts/extract_protos.py <jadx sources> <out>` unescapes those Java
literals and prints the `.proto` files. **14 recovered**: CloudAnnotationProto, HWRInputProto,
ReaderCacheProto, ExtraInfoProto, LinkDataProto, NoteInfoProto, NotePageModelProto,
ResourceDocProto, SyncNoteDataProto, TagProto, TocProto, VirtualDocPageProto, and
the stroke schemas **NoteShapeDocProto** and **ReaderShapeDocProto**. (The script also emits the
Google well-known types bundled in the app — any, descriptor, timestamp, … — which are not kept.)

**The stroke schemas (recovered in).** Both descriptors fail to parse as shipped: the
first field (the file name) has a length prefix exactly **3 short** — 0x14 for the 23-char
`NoteShapeDocProto.proto`, 0x16 for the 25-char `ReaderShapeDocProto.proto` — so parsing derails at
byte 22. An earlier reading blamed jadx; reading the literal straight from the **smali** (baksmali of the
class's `<clinit>`, `firmware/patched/work/knote2/smali_classes8/.../NoteShapeDocProto.smali`) shows the
same bytes, so **the dex itself ships it**. The numbers explain it: 20 and 22 are exactly the lengths of
`NoteShapeProto.proto` and `ReaderShapeProto.proto`. The classes were renamed `…ShapeProto` →
`…ShapeDocProto` by a tool that rewrote the string literal (+"Doc", 3 chars) without fixing its
length prefix. (At runtime `internalBuildGeneratedFileFrom` should then fail in the class
initializer, so either these classes are never loaded or that path throws; the KSync shape zips are
read through other code.) `extract_protos.py` now retries a failed parse once with the name prefix
recomputed up to the first `.proto` (shape-based; a no-op for well-formed descriptors) and writes
the repair as a `// Note:` line in the output.

- `NoteShapeDocProto.proto`: `ShapeInfoProtoList { repeated ShapeInfoProto proto = 1; }` and
  `ShapeInfoProto` with the 26 fields listed below, now **with types**: `int64 createdAt/updatedAt`,
  `int32 color/zorder/shapeType/shapeStatus/fillColor`, `float thickness`, `bytes pointList` (24),
  everything else `string` (JSON payloads: boundingRect, matrixValues, textStyle, createArgs,
  lineStyle, connectionBean, optionsRepo, ...). Exactly the numbering `ksync_decode.py` uses.
- `ReaderShapeDocProto.proto`: the same two message names, but a **different, 36-field**
  `ShapeInfoProto` for annotations on book pages: `objectId` 2, `subPageUniqueId` 5, `appId` 6,
  `pageOriginWidth/Height` 7/8, `paintStyle` 10, `bookPosition` 17, `anchor` 18, `scribbleType` 19,
  `position` 23, `pagePositionV2/V3` + `pageUniqueIdV2/V3` 31–34, `positionVersion` 35,
  `bytes pointList` 36. Note both files declare top-level `ShapeInfoProto` with **no package**: the
  two schemas cannot be loaded into one descriptor pool together.

Field numbers of `NoteShapeDocProto.ShapeInfoProto` (also visible as `*_FIELD_NUMBER`
constants): uniqueId 1, createdAt 2, updatedAt 3, color 4, thickness 5, zorder 6, boundingRect 7,
matrixValues 8, textStyle 9, text 10, createArgs 11, shapeType 12, groupId 13, resource 14,
shapeStatus 15, revisionId 16, lineStyle 17, infoRevisionId 18, tagIdList 19, extra 20,
linkIdList 21, richText 22, fillColor 23, pointList 24, connectionBean 25, optionsRepo 26.

## KSync tree on /sdcard (readable without root)
`/sdcard/.ksync/` mirrors every note: `document/<docId>/` (shapes, pages, templates) and
`point/<docId>/<pageId>/` (points). Reference decoder: `docs/scripts/ksync_decode.py <copy of
.ksync> [docId…] > notes.json` (stdlib, read-only). Verified on all 25 documents of the author's
device (22 346 fountain strokes, every freehand shape found its points); page 1 of one note
rendered from the JSON matches the handwriting ("Quantum MonteCarlo", "Aver" per pen).
Hardening: a corrupt shape zip or point file is reported and skipped instead of
aborting the run; freehand strokes left without a point file are counted per file on stderr;
regression test `docs/scripts/test_ksync_decode.py` (synthetic tree, stdlib; the pre-fix decoder
fails it).

- `document/<docId>/shape/<pageId>#<infoRevisionId>#<ms>.zip`: one zip entry, a protobuf message
  whose **repeated field 1 is `ShapeInfoProto`** (field numbers above). Zips are incremental: the
  same `uniqueId` can appear in several; keep the highest `updatedAt` (3).
  - `color` (4): ARGB int32 as a sign-extended varint (−16777216 = opaque black).
  - `thickness` (5): float32 px (5.9055 = 0.5 mm at 300 dpi).
  - `boundingRect` (7), `createArgs` (11, pen args: dpi, maxPressure 4095, pressureSensitivity,
    smoothLevel, tilt), `lineStyle` (17), `connectionBean` (25), `optionsRepo` (26) are JSON strings.
  - `revisionId` (16) names the point file; `shapeType` (12) as in STROKE.md (5 fountain, 2 pencil
    path, 15 marker, 21 brush, 22 charcoal/pencil, 40 universal geometry, 16 edit text, 29 rich
    text, **2000 reference**: a copy shown on a neighbouring page, `connectionBean.shapeReferenceBean`
    gives the source page/shape and a matrix (e.g. translate −1860); skip it when converting).
  - No points inside (field 24 `pointList` unused in these files).
  - `shapeStatus` (15): `com.onyx.android.sdk.data.note.ShapeStatus` — **0 ENABLED, 1 REMOVED**
    (erased or deleted). The newest version of a shape decides. On the author's note
    `0181736c…` **21 of 43** current shapes are REMOVED: a converter that ignores this draws erased
    strokes back. `ksync_decode.py` skips them (count on stderr); `--keep-removed` keeps them with
    `"removed": true`. __
  - Pressure normalisation: each shape's `createArgs.maxPressure` (4095 on every real shape; the
    decoder falls back to 4095 only when it is absent).
- `point/<docId>/<pageId>/<pageId>#<revisionId>#points`: exactly the point document format above.
  Real data settles the i16 pair: **the second is pressure** (raw 1–4095, varies per point); the
  first takes few values per stroke, its bytes look like tilt (0x210B → 33, 11). `time` = ms since
  the stroke began. Coordinates are page px and may run past the page edge.
- `document/<docId>/virtual/page/pb/<id>`: repeated field 1 = page: 1 pageId, 2 created, 3 updated,
  4 float **1-based page index**, 6 JSON rect = the page's place on the document canvas (page 2 at
  left 1860, page 3 at top 2480 …), 7/8 page size 1860×2480, 9 `"geo_layout"`, 10 template path.
  The newest file (max field 3) is current.
- `virtual/doc/pb/<docId>`, `pageModel/pb/*` (layers JSON), `template/json/*.template_json`
  (background), `stash/archivedShape/<ms>/…zip` (older shape revisions, same format; ignored).

## Export / sync containers (seen, not yet decoded)
- `.note` export and the KSync tree: `KSyncConstant.docDirFilePath(...)` with `point/`, `page/`,
  `resource/`, `toc/`, `tag/`, `link/`, `extra/` subfolders; `shape.pb` and `.pb` files in the
  `ShapePBData` path. Export copies the point directory as-is (`NoteModelData`).
- `.cblite2.zip`: Couchbase Lite database bundles (the sync backend, see NETWORK-DEPENDENCIES).

## Open
- Decode one real note (needs the device: `adb backup` is blocked for this app, so this means
  root, or an export to `/sdcard`).
- ~~The size/pressure order in point files~~: second i16 = pressure (KSync data).
- ~~The two skipped stroke protos~~: recovered (the dex ships a stale length prefix).
- The `.note` export container layout.
