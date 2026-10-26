# Docs index

Findings from studying the Boox Note Air 5C, and the open implementation built from them (`impl/`). Research in
progress: what is marked "not yet checked on a device" is read from static analysis only.

## The open stack (`impl/`)
- [REFACTOR-IMPL-2026-10.md](REFACTOR-IMPL-2026-10.md): the October 2026 cleanup of `impl/`: recogniser split by shape family, shared JNI marshalling, one Catmull-Rom basis; how it was proven behaviour-preserving.
- [OPEN-STACK.md](OPEN-STACK.md): the open reimplementation (Rust + Java) of the e-ink, stylus and API layers: decisions, layers L1–L5, spec, crates, build.
- [STROKE.md](STROKE.md): openink-stroke, an open replacement for `libneo_pen`: the knote2 pen set, width models, wire format, preview.
- [EPDC-ENGINE.md](EPDC-ENGINE.md): the open refresh engine (openink-epdc + EpdEngine.java): intents, merging, pen holding, ghost ledger, cleanup, BYPASS.
- [SHAPE-ENGINE.md](SHAPE-ENGINE.md): openink-shape, the open shape recogniser: classifier + BiGRU segmenter + geometric fits, its own features and weights.
- [HWR-ENGINE.md](HWR-ENGINE.md): openink-hwr, the open handwriting recogniser: Hershey synthetic ink, BiGRU-CTC (torch → `.oisw`), Rust inference with a parity test, JNI/Java; the shared openink-nn layer.
- [ONYX-COMPAT.md](ONYX-COMPAT.md): `onyx-compat`, the Onyx SDK pen API (`TouchHelper`, `RawInputCallback`, `TouchPoint`) over openink, port-by-imports.
- [API-COVERAGE.md](API-COVERAGE.md): every Onyx SDK public method (EpdController, TouchHelper, RawInputReader…) against onyx-compat.

## How the device works
- [ONYX-EPD-PEN.md](ONYX-EPD-PEN.md): e-ink refresh and the pen stack: SurfaceFlinger's private transaction codes, `/dev/ebc`, the FIFO, the native pen reader.
- [SF-ONYX.md](SF-ONYX.md): the Onyx e-ink layer inside the stock surfaceflinger: 915 functions named from MiniDebugInfo, modules, shaders.
- [SF-INSTANT-INK.md](SF-INSTANT-INK.md): how instant ink reaches the screen.
- [EBC-ABI.md](EBC-ABI.md): the e-ink hardware (a Lattice FPGA timing controller over DSI, I2C and SPI), the `/dev/ebc` ioctl table, the update struct, schemes.
- [KERNEL.md](KERNEL.md): the 4.19 kernel (SoC SM6350/SM7225), Onyx's configs, `/dev/ebc`, the timing controller, symbols.
- [SHAPE-RECOGNITION.md](SHAPE-RECOGNITION.md): knote2's circle and rectangle snapping: hold detection, recognition, resolvers; what is open and what is closed.
- [SHAPE-RE.md](SHAPE-RE.md): MyScript Shape (Boox Notes' closed recogniser) from the outside: its container, class hierarchy, tolerances, the two networks.
- [HWR-OFFLINE.md](HWR-OFFLINE.md): handwriting recognition offline: the engines, bundled and downloaded language packs, open alternatives.
- [KNOTE2.md](KNOTE2.md): knote2's storage: Room databases, the binary point-file format, recovered protobuf schemas ([formats/knote2-proto/](formats/knote2-proto/)).
- [NETWORK-DEPENDENCIES.md](NETWORK-DEPENDENCIES.md): every vendor server the device talks to, and its telemetry.

## Scripts (`docs/scripts/`)
- Notes and formats: [ksync_decode.py](scripts/ksync_decode.py) + [test_ksync_decode.py](scripts/test_ksync_decode.py) (KSync notes → JSON), [rnote_to_notes.py](scripts/rnote_to_notes.py), [extract_protos.py](scripts/extract_protos.py) (`.proto` files from full-protobuf classes).
- The SurfaceFlinger protocol: [extract_sf_protocol.py](scripts/extract_sf_protocol.py) (→ `impl/spec/sf_protocol.json`), [gen_sf_codes.py](scripts/gen_sf_codes.py) (the Rust/Java code tables), [sf_inventory.py](scripts/sf_inventory.py), [sf_labels.py](scripts/sf_labels.py), [sf_dispatch.py](scripts/sf_dispatch.py).
- The SDK: [sdk_bindings.py](scripts/sdk_bindings.py), [sdk_coverage.py](scripts/sdk_coverage.py) (the API-COVERAGE table).
- Static analysis helpers: [kxrefs.py](scripts/kxrefs.py) (arm64 caller scan), [fdt_dump.py](scripts/fdt_dump.py) (DTB/DTBO → text), [apk_net_triage.py](scripts/apk_net_triage.py), [myscript_res.py](scripts/myscript_res.py).
- Training: [train_shape.py](scripts/train_shape.py), [train_hwr.py](scripts/train_hwr.py), [hwr_synth.py](scripts/hwr_synth.py), [hwr_words.py](scripts/hwr_words.py), [hwr_kinds.py](scripts/hwr_kinds.py), [physics_vocab.py](scripts/physics_vocab.py).

## Markdown outside `docs/`
Kept next to what they describe, where a tool or a reader looks for them:
- [../README.md](../README.md): the project's front page.
- [../AGENTS.md](../AGENTS.md): rules for AI coding agents (agents look for this file at the repository root).
- [../impl/java/neo-oracle/README.md](../impl/java/neo-oracle/README.md): the on-device oracle that runs the stock pen library beside openink-stroke.
- [../impl/crates/openink-shape/weights/README.md](../impl/crates/openink-shape/weights/README.md): where the shape weights come from and how to retrain them.
- [../impl/crates/openink-hwr/weights/README.md](../impl/crates/openink-hwr/weights/README.md): where the handwriting weights come from, and the parity fixture.
