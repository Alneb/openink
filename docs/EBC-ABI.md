# E-ink display path and the `/dev/ebc` ioctl ABI — BOOX Note Air 5C, firmware 4.2.1

Everything here comes from static analysis of
`vmlinux.elf` (Ghidra headless decompile, no debug info), the boot `dtb` + `dtbo.img`, and the
`surfaceflinger` binary. Not yet checked on a device. Related: [KERNEL.md](KERNEL.md) (symbols,
config), [ONYX-EPD-PEN.md](ONYX-EPD-PEN.md) (framework / SF / pen side).

## TL;DR — the hardware is not what the kernel config suggests
- SoC is **Qualcomm Lagoon (SM6350 / SM7225)**: boot dtb `qcom,msm-id = <434 …, 459 …>`, model
  "Lagoon MTP NoteAir5C v10/v20/v30" in `dtbo.img`. The `*_SDM662` kernel config names (which
  KERNEL.md used to infer SDM662) are leftovers from a kernel shared across BOOX models.
- The e-ink timing controller is a **Lattice CertusPro-NX FPGA, LFCPNX-100** (driver name `lfe5u`,
  after the ECP5 it was first written for; it also supports `lfe5um` = LFE5UM-85F).
  - The SoC sends **plain DSI video**, 2480×1860 at 38 Hz (`0x9b0 × 0x744`, framerate `0x26`), in
    an MDSS panel node whose name is a leftover from a Qualcomm reference board ("rm69299 amoled
    … visionox") and that carries `onyx,tcon-mode-support`.
  - The FPGA turns frames into panel drive: **the waveforms are applied inside the FPGA**.
  - It is controlled over **I2C at 0x34** (`onyx,lfe5u_i2c`, `cfa_mode = <1>` = Kaleido colour
    filter, plus `epdc-a2-enhance-enable`, `epdc-gu-regal-enable`, `epdc-regal_nm-enable`).
  - Its bitstream is loaded over **SPI** (`onyx,lfcpnx`, 10 MHz, `fw-product-id = <0x9e>`, with an
    overlay fragment setting `0xa7`), driven by the rails/GPIOs `fpga_12v_core_en`, `fpga_30v_en`,
    `reset`, `standby`, `program_enable` / `program_init` / `program_done`.
- The **software EPDC** (`onyx_epdc_*`, `onyx,sepdc` / `onyx,sepdc_mfd`, the waveform parsers
  `onyx_waveform_parse_pvi/imx`, `epdc_ioctl`) is compiled in but **not in this board's device
  tree**, so it never probes. On this device no `.wbf` waveform is parsed by the kernel: kxrefs
  shows `onyx_waveform_parse_*` is only called from `onyx_epdc_fb_fw_handler`.

## Built-in firmware (`CONFIG_EXTRA_FIRMWARE`, extracted)
`docs/scripts/extract_builtin_fw.py` → `firmware/onyx/builtin_fw/` (gitignored). The
`.builtin_fw` table's pointers are zero in the Image (filled by boot-time relocation), so the
script uses the table sizes, a 16-byte layout and the `_fw_*_bin` symbols. All 15 offsets are
checked against their symbols.

| blob | size | role |
|---|---|---|
| `lfcpnx/lfcpnx100_tcon_fw_{99,9e,9f,a2,a5,a7}.bin` | 3 145 744 B each | FPGA TCON bitstreams, one per `fw-product-id` (this board: 0x9e, overlay 0xa7) |
| `mxo/mxo1300_nvcm_8{1,2,3,4,6,7}.ied`, `mxo/mxo4300_nvcm_8{1,2}.ied` | 39–104 KB | Lattice MachXO NVCM images (`onyx,mxo`): another model's TCON |
| `waveform/eink_waveform.wbf` | 225 533 B | waveform for the software EPDC (unused here) |

- `/waveform/eink_waveform.wbf` in `system.img` (393 123 B) is a **different file**. Since the FPGA
  path parses no wbf, its consumer is still to be found (SF? an app?). The per-device
  `/onyxconfig` path (`/%s/waveform/`) belongs to the same software-EPDC loader.
- Bitstreams are **AES-encrypted**. See the probe sequence: the key lives in FPGA OTP.

## FPGA probe sequence (`lfe5u_probe`, SPI driver)
1. Parse DT: rails `vdd1v8`, `vdd1v2`, `vddpanel`, `vddvref`, `vsys`, … (each optional), the
   GPIOs above, flags `upgrade-fw-force`, `upgrade-fw-disable`, `verity-fw-enable`,
   `encry-fw-disable`, `vcom-spiflash-save`, `freeze-disable-core-voltage`, `fw-product-id`.
2. Power up, read `chip_id` (0x43301101 / 0x43301141 / 0x43400f01 → name table; unknown ids are
   forced to LFE5UM-85F), detect the config SPI flash, set QPI mode.
3. Read the **usercode** (version_id, product_id, vendor_id, encry_flag).
4. **AES key**: `CMD_LSC_READ_CIPHER_KEY`; if blank, `CMD_LSC_PROG_CIPHER_KEY` programs it, then
   `CMD_LSC_PROG_OTP` locks OTP. One-time, irreversible fuses.
5. If the flash usercode doesn't match the built-in image (or `upgrade-fw-force`):
   `lfe5u_upgrade_firmware_from_builtin`. It builds the name
   `lfcpnx/<chip>_tcon_fw_<product-id>.bin` (`lfe5u_generate_firmware_filename`), then
   `request_firmware_nowait`, and **rewrites the FPGA's SPI config flash**.
6. `lfe5u_load_fw`: PROGRAMN low, wait 50 ms, high, `lfe5u_config_done_wait` (DONE pin); 2 tries.
7. `onyx_tcon_vcom_set`. The panel **VCOM is kept in the FPGA's SPI flash**
   (`lfe5u_flash_read/write_vcom`, `vcom-spiflash-save`).
8. Create class `ebc` + sysfs attributes, register the `ebc_misc` device (`/dev/ebc`) and the
   PM notifier.

> **Rebuild-critical:**
> - A rebuilt kernel must carry **the same bitstreams** (or `upgrade-fw-disable`). Otherwise it
>   reflashes the TCON's SPI flash at boot.
> - Never touch the sysfs attributes `aes_key_store`, `otp_store`, `force_update_fw_store` or
>   `erase_show`. They write OTP fuses or the FPGA flash, which can permanently stop the display
>   from working.
> - The VCOM in that flash is per-panel calibration: back it up via `vcom_value_show` before any
>   experiment.

sysfs attributes (class `ebc`): `dump_reg`, `power`, `sleep`, `update`, `force_update_fw`,
`chipid`, `version`, `verity`, `verity_repeat`, `verity_fw`, `overlay`, `erase`, `reset`,
`extbuf_test`, `vcom_value`, `scheme`, `a2_mode_type`, `aes_key`, `otp`.

## `/dev/ebc` file operations
- `ebc_open`: binds `file->private_data` (+200) to `g_lfe5u_ctrl`. On first open it reads the panel
  size (`dsi_panel_res_get`) and allocates the handwriting buffer
  (`onyx_tcon_handwrite_buf_malloc`, ION).
- `ebc_mmap`: maps that **handwriting buffer** (`virt_buf_handwrite`, a dma-buf) into user space.
  SurfaceFlinger draws instant ink there.
- `ebc_ioctl(file, cmd, arg)`: first waits for `lfe5u_wait_resume_late_comelete`.
  - Command numbers are **plain integers**, not `_IOW`-encoded (a Rockchip-EBC heritage).
  - Unknown commands return **0** (silent no-op). SF also sends numbers this driver does not
    implement (0x700f, 0x7010, 0x7016, 0x701e, 0x7022), left over from other BOOX backends.
  - Scalar arguments are a `u32` at `arg`, read or written in place.

### Commands (0x70xx: display/update)
| cmd | name (from log strings) | arg | effect |
|---|---|---|---|
| 0x7002 | GET_EBC_DRIVER_SN | out char[0x1e] | copies `"ONYX_TCON_DRIVER_VERSION_1.00"` |
| 0x7006 | SET_EBC_UPDATE_SCHEME | in u32 scheme | `onyx_tcon_set_upd_scheme(ctrl, s, 1, 0)`; for s≠3 records `fb_update_time` |
| 0x7008 | (set waiting_for_all_lut_free) | in u32 | stub: "but not achieve!" |
| 0x7009 | (wait/all-free query) | out u32 | always writes 1 |
| 0x700a | ? | in u32 | stored at ctrl+0x94 |
| 0x700b | SET_EBC_FORCE_WAVEFORM | — | stub: "not achieve!" |
| **0x700c** | **SET_EBC_SEND_UPDATE** | in `struct ebc_update` (0x28 B) | see below |
| 0x700e | (set clear colour) | in u32 | global `clear_color_val` (used by the clear path) |
| 0x701d | (sync extbuf ← fb) | — | `onyx_tcon_ext_buf_sync_with_fb` + `onyx_tcon_sync_hw_buf_with_fb(ctrl, 1)` |
| 0x701f | SET_EBC_EXTBUF_SYNC_FB_ENABLE | in u32 (1/0) | flag ctrl+0x78; records `fb_update_time` |
| 0x7021 | (get gray/colour mode) | out u32 | ctrl+0x88: when 0, the update path converts RGB→gray (`rgb32_to_ggg32_32_neon`) |
| 0x7024 | (A2 cut frames / A2 mode type) | in u32 0..7 | 0..6 → reg 4 `CMD_TCON_A2_FRAME_NUM` (+ctrl+0xa4); in the other state 7→2, 1..6→1, 0→0 = `a2_mode_new` |
| 0x7025 | ? | in u32 <4 | clears ctrl+0x98 under the scheme mutex |
| 0x7026 | (CFA mode) | in u32 | ctrl+0x8c = (arg≠0); logs "enable/disable cfa mode" (the strings are inverted) |
| 0x7027 | (GC full / display mode) | in s32 | arg<0 → reg 1 = arg−1; else reg 6 = base(ctrl+0x9c) + arg·16 |
| 0x7028 | (get A2 cut frames) | out u32 | ctrl+0xa4 |
| 0x7029 | ? | out u32 | byte ctrl+0xb8 |

### Commands 0x72xx = write TCON register, 0x73xx = read it
I2C write is the 2-byte message `[reg, value]` to 0x34 (`lfe5u_ctrl_send_cmd` →
`lfe5u_i2c_write_reg`); reads use `i2c_smbus_read_byte_data(reg)`.

| reg | name | write cmd | notes |
|---|---|---|---|
| 0x01 | CMD_TCON_GC_FULL_MODE | (via 0x700c / 0x7027) | values 4/5/6/7 chosen by waveform_mode |
| 0x02 | CMD_TCON_AUTO_MODE_CTRL | 0x7202 | value = arg − (arg&0xf > 2); read back with **0x7302** |
| 0x04 | CMD_TCON_A2_FRAME_NUM | 0x7204 | |
| 0x05 | CMD_TCON_CONTRAST | 0x7205 | |
| 0x06 | CMD_TCON_DISPLAY_MODE | 0x7206 | |
| 0x0a / 0x0b | CMD_TCON_FILTER_P1 / P2 | 0x720e | arg low byte → P1, next byte → P2 |
| 0x0c / 0x0d | CMD_TCON_A2_THRESHOLD_WHITE / BLACK | 0x720c | low byte white, next byte black |
| 0x0e | CMD_TCON_DEBOUNCE_ENABLE | 0x720d | |
| 0x10 | CMD_TCON_DEBOUNCE_PARM | 0x7210 | read back with **0x7310** |
| 0x11 | CMD_TCON_DEBOUNCE_THRESHOLD | 0x7211 | |
| — | a2_enhance state | 0x720f | "not used!" |
| — | sync hw buf with fb | 0x7212 | `onyx_tcon_sync_hw_buf_with_fb` |
| — | update enable | 0x7213 | arg 0/1 → `onyx_tcon_set_update_enable` |
| — | (cached byte ctrl+0xb5) | read 0x7311 | |

SF uses 0x720f, 0x7210, 0x7211, 0x7212 and 0x7310.

Registers written only by the kernel itself (not reachable through a 0x72xx command):

| reg | name | writer |
|---|---|---|
| 0x07 | CMD_TCON_UI_MODE_SET | `onyx_tcon_sync_hw_buf_with_fb` (UI-mode updates: values `x|3`, `x|6`, `x|7`, `x|0x12`, 0x20) |
| 0x08 | CMD_TCON_VCOM | `lfe5u_ctrl_set_vcom` (range-checked; read back with `lfe5u_ctrl_get_vcom`; a "workaround" variant re-writes it) |
| 0x15 | CMD_TCON_FB_SYNC_AUTO | `onyx_tcon_sync_hw_buf_with_fb(ctrl, on)`: FPGA copies each video frame into its own buffer automatically |
| 0x1a | CMD_TCON_UPDATE_ENABLE | `onyx_tcon_set_update_enable` (0x7213) |
| ? | CMD_TCON_VERSION, _PM_STATE, _FLG_STATE, _WF_SEND_STATE, _ERROR | names in `lfe5u_ctrl_send_cmd_ext` / dump; numbers not yet read |

### `struct ebc_update` (SET_EBC_SEND_UPDATE, 0x28 bytes) — `mxcfb_update_data` layout
```c
struct ebc_update {          /* offsets from the copy_from_user target in ebc_ioctl */
    u32 top, left, width, height;   /* 0x00 rect — logged as rect[left top width height] */
    u32 waveform_mode;              /* 0x10 1, 2, 4, 0xff seen in the GC-full mapping */
    u32 update_mode;                /* 0x14 1 = full */
    u32 update_marker;              /* 0x18 */
    s32 temp;                       /* 0x1c */
    u32 flags;                      /* 0x20 bit16, bit17: special paths; bit18 = EPDC_FLAG_HANDWRITE */
    u32 reserved;                   /* 0x24 (not read on the paths decompiled so far) */
};
```
Flow:
1. `pm_wakeup_ws_event(1500 ms)`.
2. **HANDWRITE (bit 18)**: marker += 60000, then clear the extbuf-sync flags. If the scheme is not 3
   (handwrite) and the handwriting isn't paused, reject ("rejust EPDC_FLAG_HANDWRITE"). If it was
   paused, resume with `set_upd_scheme(3)`. Then `onyx_tcon_handwrite_submit(ctrl, &upd)`.
3. **Normal**: if neither bit16 nor bit17 is set, choose reg 1 GC_FULL_MODE:
   update_mode 1 + waveform 2 → 7; waveform 1 → 5, 2 → 6, 4 → 4, 0xff → skip.
4. One flag path (bit16/17) waits 20 × 1 ms and `memset`s the extbuf to `clear_color_val`
   (screen clear).
5. `onyx_tcon_display_extbuf(&upd, extbuf, w, h, convert_gray = ctrl+0x88 == 0)`:
   - throttles to one frame per 25 ms (`0x61a9` µs, up to 11 × 10 ms waits);
   - copies (or RGB→gray converts with NEON) into the DRM writeback fb `wf_fb`;
   - `dma_flush`, `sde_plane_set_upd_ext`, `onyx_tcon_drm_mode_setplane`.

   So an "update" is: set the FPGA mode registers over I2C, then **push the frame through the
   Qualcomm display plane**. The FPGA applies the waveform.

### Instant ink: the handwriting side channel (no video frame involved)
- `ebc_open` allocates `virt_buf_handwrite`: an ION dma-buf of panel_w × panel_h × 4 bytes
  (RGBA32), initialised to 0xff (white). `ebc_mmap` exposes it to SurfaceFlinger, which renders
  stroke pixels straight into it.
- SF then sends **SET_EBC_SEND_UPDATE with flags bit 18 (HANDWRITE)** and the dirty rect →
  `onyx_tcon_handwrite_submit`.
  - It splits the rect by rows so each packet stays ≤ 0xffdc (65 500) bytes.
  - Each chunk goes through `onyx_tcon_handwrite_create_cmd_packet` into a vmalloc'd cmd buffer,
    then `dsi_panel_update_handwrite(panel, len)`: a **DSI command-mode transfer to the FPGA**,
    sent in parallel with the running video stream.
- Packet layout (multi-byte fields little-endian; x is aligned to 16 px on both ends):
  ```
  +0x00 u32 magic 0x55aa55aa
  +0x04 u8  0x21            command "handwrite region"
  +0x05 u16 x0 & ~0xf       left, aligned down
  +0x07 u16 y0              top
  +0x09 u16 x1 & ~0xf       (left + width + 15), aligned
  +0x0b u16 y1              top + height
  +0x0d u8  waveform_mode   from ebc_update.waveform_mode
  +0x0e u16 0
  +0x10 RGBA32 pixels, rows y0..y1-1, columns x0..x1-1, copied from virt_buf_handwrite
  ```
  Return/length = payload | 0x10 (the header). Rect sanity check: non-negative and within the panel.
- So the latency path is **evdev → SF stroke render → shared buffer → DSI command → FPGA**. The
  Android composition and the 38 Hz video frame are skipped, which is why instant ink is fast
  while normal updates (scheme ≠ 3) wait behind `onyx_android_refreash_enable(0)`.

### Update schemes (`onyx_tcon_set_upd_scheme(ctrl, scheme, drm_change, force)`)
- scheme = arg & 0xf (1 maps to 2), valid 0..5. Bits 4..7 are passed to `set_display_mode`.
- **3 = HANDWRITE**: `onyx_android_refreash_enable(0)`, which **stops normal Android refreshes**
  while ink is drawn. The display mode becomes 4.
- Leaving 3 (or force, or a resume from pause): `onyx_android_refreash_enable(1)`.
  Display mode: scheme 5 → 2, 4 → 1, other → 0.
- This is the kernel half of the pen-state machine in ONYX-EPD-PEN.md: pen state 2 → SF sets scheme
  3; app content updates during handwriting are held back until the scheme changes.

## Who may open `/dev/ebc`
- `vendor/etc/init/hw/init.onyx.rc`: `chmod 0666 /dev/ebc`, i.e. world read/write at the DAC level.
- SELinux (`vendor_file_contexts`: `/dev/ebc u:object_r:ebc_device:s0`; `vendor_sepolicy.cil`):
  the only allow rules are
  `(allow surfaceflinger_30_0 ebc_device (chr_file (ioctl read write map open)))` and the same for
  `charger_30_0`. So SELinux is the real gate: apps (untrusted_app, platform_app,
  system_server) cannot open it. Everything app-side goes through SF's private transactions.
- Binaries that contain the path: `system/bin/surfaceflinger`, `system/bin/charger` (off-mode
  charging screen). No vendor binary or HAL uses it.
- For a rebuilt image: keep both rules. A custom e-ink daemon would need its own domain + allow
  rule. Never relax this to app domains: 0x72xx writes FPGA registers, and the sysfs side can
  write OTP.

## SurfaceFlinger side (call sites)
- SF wraps each ioctl in a tiny generic helper (log "ioctl with %d failed"). Call sites named by
  nearby strings (adrp+add resolution over `llvm-objdump` output; SF is stripped):
  - **open path** ("Going to open %s" `/dev/ebc`, "Error mapping the framebuffer"): 0x701d (sync
    extbuf ← fb), 0x700b (force waveform, a stub), 0x7008 (wait LUT free, a stub).
  - **refresh path** ("refresh screen (%d, %d - %d, %d) waveform_mode %d flags 0x%x marker %d",
    "refresh out of screen region"): 0x700c, 0x701d, 0x7010.
  - `onyx_epdc_screenRefresh` / `onyx_epdc_hwscreenRefresh` → 0x700c ("SET_EBC_SEND_UPDATE retval").
  - The rest are plain setters/getters: 0x7006, 0x7009, 0x700a, 0x700e, 0x700f, 0x7016, 0x701e,
    0x701f, 0x7021, 0x7022, 0x7024, 0x7025, 0x7026, 0x7029, 0x720f–0x7212, 0x7310.
- SF never sends 0x7202–0x720e, and only SF and charger may open the device, so on this firmware
  contrast, A2 thresholds, debounce and filters stay at the FPGA's defaults (or are set from the
  charger). Not exercised.

## The two `.wbf` files
- Kernel built-in `waveform/eink_waveform.wbf` (225 533 B): only for the software EPDC, unused here.
- `system.img:/waveform/eink_waveform.wbf` (393 123 B): no reader found. The path string appears
  once in system.img (the directory entry itself); no reference in framework/services sources or
  in any bundled app's dex strings. Only ProductionTest knows wbf paths, and they are other ones:
  `/onyxconfig/waveform/waveform.wbf` and `/system/etc/waveform/` (waveform tests for other models).
  Likely a leftover from another model's build. Don't remove it until checked on a device
  (`lsof`/`inotify` or a strace of SF at boot).

## Tooling added (all in `docs/scripts/`)
- `decompile_kernel.sh <outdir> <sym|0xaddr>...` + `ghidra/DecompileFunctions.java`: Ghidra 12
  headless decompile. The import uses `-noanalysis` (full analysis of the 66 MB ELF would take
  hours); functions are created on demand. The project lives in `firmware/ghidra/`; output is in
  `firmware/onyx/kernel_c/` (gitignored).
- `annotate_kstrings.py <vmlinux.elf> <file.c>...`: inlines string literals (`0x…/*"text"*/`),
  rendering the KERN_ level as `<n>`.
- `summarize_cases.py <file.c>`: per-`case` strings, callees and copy sizes of a decompiled switch.
- `kxrefs.py <vmlinux.elf> <kallsyms.txt> <sym>...`: BL/B caller scan (no disassembler needed).
- `fdt_dump.py <dtb|dtbo.img>`: DTB and Android-DTBO → dts-like text (no `dtc` on the host). It
  handles unaligned entries: alignment is relative to each tree. Outputs:
  `firmware/unpacked/boot/dtb.dts`, `firmware/unpacked/dtbo.dts` (v30 = first 5938 lines →
  `dtbo_v30.dts`).
- `extract_builtin_fw.py`: see above.

## Open
- Map each SF ioctl call site to its wrapper (SWEpdcManager / HWEpdcManager) and the SF transaction
  codes (`processOnyxRequests`).
- Confirm on a device that nothing opens `/waveform/eink_waveform.wbf`.
- Check whether charger sends any 0x72xx tunables.
- Flags bit16 vs bit17 (which one is "clear screen"), and reg 6 display-mode values.
