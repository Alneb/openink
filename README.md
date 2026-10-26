# openink

An open reimplementation of the Boox Note Air 5C's pen and e-ink stack, and the research it is built on.

Boox e-ink tablets draw ink through closed parts: Onyx's pen library (`libneo_pen`), a private e-ink layer inside
SurfaceFlinger, the Onyx SDK and MyScript's recognisers. This project documents how those parts behave, from the
outside, and rebuilds them as open code an app can use on a stock, locked device.

> **Status: research in progress.** Parts are verified on the device, parts are read from static analysis only (the
> docs say which). Interfaces still change.

## What is here

- **`impl/`**: the open implementation (MIT).
  - Rust crates: `openink-stroke` (pressure/tilt-shaped strokes for the knote2 pen set), `openink-pen` (the
    digitizer), `openink-epdc` (a refresh engine), `openink-shape` (a shape recogniser with its own trained weights),
    `openink-hwr` (a handwriting recogniser), `openink-core` (the typed protocol model), `openink-nn`, `openink-sf`,
    `openink-ebc`, `openink-jni`.
  - Java: `org.openink.*` (the app-facing API), `onyx-compat` (the Onyx SDK's pen API on top of openink), a demo app.
  - `impl/spec/`: the SurfaceFlinger protocol as data, from which the Rust and Java code tables are generated.
- **`docs/`**: the findings, from the e-ink hardware and the kernel driver to Boox Notes' file format and its network
  traffic. Start at [docs/INDEX.md](docs/INDEX.md).
- **How far it is**: [docs/PROGRESS.md](docs/PROGRESS.md) counts what is done and what is missing, layer by layer
  (2026-10-26: the app-process half about 83 %, the system half about 30 %).

Atlas ([github.com/Alneb/atlas](https://github.com/Alneb/atlas)) uses openink's stroke and shape crates for its pen
canvas on Boox tablets.

## Build and test

```sh
cd impl && ./check.sh --no-device-builds     # Rust tests, clippy, the Java host checks
./check.sh                                   # also the Android builds: needs the NDK and android.jar
```

The Android builds look for the NDK, `android.jar` and the build tools in `$OPENINK_TOOLCHAINS` (default
`impl/.toolchains`).

## Scope and disclaimer

This is independent interoperability research on a device the authors own. It is **not affiliated with or endorsed
by Onyx International, MyScript or Google**; their names are used only to say what is being described.

The repository contains **no firmware, no proprietary binaries, no decompiled source and no vendor keys**: only
descriptions of behaviour, the formats and protocols needed to work with the device, and code written from those
descriptions. Do not open issues or pull requests that add any of those.

The trained weights in `impl/crates/*/weights/` were trained here on synthetic data, open datasets (credited in
each weights README) and the authors' own writing.

## License

[MIT](LICENSE). The training data credits in the weights READMEs apply to the weights.
