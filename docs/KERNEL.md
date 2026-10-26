# Kernel — BOOX Note Air 5C (firmware 4.2.1)

## Identity
- `Linux version 4.19.157-perf-g77c132eccfab-dirty (onyx@onyxUbuntu) (clang 10.0.7 NDK, GNU ld 2.27)
  #38 SMP PREEMPT Tue Jul 28 10:48:06 CST 2026` — Qualcomm msm-4.19 tree, local uncommitted
  changes ("-dirty"), built ~6 days before the 2026-08-03 system build.
- SoC: **Qualcomm Lagoon (SM6350 / SM7225)**, from the device tree (`qcom,msm-id` 434/459,
  model "Lagoon MTP NoteAir5C v10/v20/v30"). **Correction:** this doc first said
  SDM662, inferred from `CONFIG_*_SDM662` names. Those names come from a kernel shared across BOOX
  models. See [EBC-ABI.md](EBC-ABI.md).
- Boot image header v2, `os version 11.0.0`, patch 2025-10.

## Artifacts (gitignored, in `firmware/unpacked/boot/`)
- `kernel` (raw arm64 Image), `kernel.config` (IKCONFIG), `dtb` (407904 B).
- `vmlinux.elf` — rebuilt ELF with symbols via `vmlinux-to-elf` (installed in `.tools/venv`):
  `kallsyms_offsets` at file offset 0x02cc8700, base `0xffffff8008080000`, 121 895 symbols
  → `kallsyms.txt` (`nm vmlinux.elf`).

## Onyx kernel config (56 `CONFIG_*ONYX*` options)
**E-ink**: **correction:** on this board the e-ink controller is a **Lattice
CertusPro-NX FPGA (LFCPNX-100)** fed by DSI video and controlled over I2C/SPI (driver `lfe5u`,
details in [EBC-ABI.md](EBC-ABI.md)). The software EPDC options below are compiled in, but its DT
node (`onyx,sepdc`) is absent, so it never probes:
`FB_ONYX_SOFTWARE_EPDC`, `FB_ONYX_SOFTWARE_EPDC_SDM662_PERF`, `ONYX_EPDC_USE_MIPI_SDM662`,
`ONYX_EPDC_TCON_TYPE_LCDIF`, `ONYX_EPDC_SEND_MODE_VIDEO`, `ONYX_EPDC_SEGMENT`,
`ONYX_EPDC_DISPLAY_BUF_PIXEL_FORMAT_RGBA`, `ONYX_EPDC_PANEL_DATA_CTRL_END_SWAP`,
`ONYX_EPDC_HANDWRITE_BUF_MALLOC_FROM_ION`, `ONYX_EPDC_POWER_USE_REGULATOR`, `ONYX_EPDC_MFD_REGISTER`,
`ONYX_EPDC_INIT_WAVE_FIRMWARE`, `ONYX_EPDC_FIRMWARE_WAVEFORM_KERNEL`,
**`ONYX_EPDC_WAVEFORM_VENDOR_DIR="onyxconfig"`**, `ONYX_EPDC_DEBUG_PRINTK`,
panel **`ONYX_EINK_ES133TT3_2200x1650`** (config name; verify against the actual 5C panel via dtb).

> **Rebuild-critical:** keep `onyxconfig` intact (per-device data). On this board the waveform
> is applied **inside the FPGA**, whose bitstream is built into the kernel and whose SPI flash
> holds the panel VCOM. See EBC-ABI.md for what must never be rewritten. (The `/onyxconfig`
> waveform path belongs to the unused software EPDC.) A rebuilt image must never format
> or overwrite `onyxconfig`; back it up from the device before any flashing experiment.

**Touch / pen**: `TOUCHSCREEN_ONYX_WACOM` (+ `ONYX_WACOM_FW_UPDATE`), `TOUCHSCREEN_ONYX_PARADE`,
`TOUCHSCREEN_ONYX_CYTTSP5/6`, `TOUCHSCREEN_ONYX_FTS` (+ `ONYX_FTS_FREEZE_EVENT_COMPENSATION`),
`TOUCHSCREEN_ONYX_ELAN`, `TOUCHSCREEN_ONYX_ISTARIC`, `ONYX_TP_REPORT_STYLUS_PEN_TYPE`,
`ONYX_TP_PM_NOTIFIER`, `ONYX_TP_EXTRA`, `ONYX_TP_DEBUG_ENABLE`, `ONYX_CYTP_LO_FILTER_EN`
(→ `/sys/onyx_misc/cytp_lo_filter`, also referenced by SurfaceFlinger). One kernel serves many
BOOX models, hence the many alternative controller drivers.

**Platform / power**: `ONYX_MISC` (`/sys/onyx_misc/*`), `ONYX_PM`, `ONYX_PM_NOTIFY`, `ONYX_HALL`
(cover sensor), `ONYX_HOLD_WAKEUP_IN_CHARGING`, `ONYX_FAST_WAKE_FROM_FREEZE`,
`ONYX_FREEZE_TIME_CHECK`, `ONYX_SMP2P_WAKEUP_ENABLE`, `ONYX_MMC_DET_WAKEUP`, `ONYX_POWER_OFF`,
`ONYX_CHG_FULL_CHECK`, `ONYX_CHARGING_SOC_CTRL_EN`, `ONYX_BAD_USB_CHARGING_ENABLE`,
`ONYX_USB_DEVICES_NODE`, `ONYX_STATIC_MTP_BUF`, `ONYX_CONSOLE_EN`.
**Front light**: `ONYX_BACKLIGHT`, `ONYX_BACKLIGHT_LM3630A` (TI LM3630A, two channels → warm/white),
`ONYX_LED_BRIGHTNESS_CTRL`. **Other**: `ONYX_KEYBOARD` (keyboard cover), `ONYX_MIC_SWITCH_AUTO_DETECT`,
fingerprint drivers `ONYX_FINGERPRINT_{CHIPSAILING,FORTSENSE,MICROARRAY}`.

## Onyx symbols by subsystem (from `kallsyms.txt`)
Prefix counts (text symbols): `onyx_epdc` 114, `onyx_tp` 45, `onyx_tcon` 42, `onyx_keyboard` 37,
`onyx_kb` 24, `epdc_panel` 20, `onyx_fp` 16, `onyx_pm` 14, `onyx_hall` 13, `onyx_bl` 13,
`onyx_waveform` 11, `epdc_waveform` 8, `onyx_misc` 7, `ebc_*` ~25.

### `/dev/ebc` — the e-ink device interface
`ebc_misc` (misc device), `ebc_fops`, `ebc_open`, **`ebc_ioctl`**, `ebc_io_ctl_hook`, **`ebc_mmap`**,
buffer pool `ebc_buf_init/ebc_empty_buf_get/ebc_dsp_buf_get/ebc_add_to_dsp_buf_list/
ebc_remove_from_dsp_buf_list/ebc_find_buf_by_phy_addr/ebc_phy_buf_base_get/ebc_virt_buf_base_get`,
timing `ebc_set_timing_buf(_16bits, _16bits_v1/_v2/_lvds)`, `ebc_check_timing_buf_16bits(_all)`,
`ebc_get_lt / ebc_get_fdl_pos / ebc_get_ldl_pos` (line timing, first/last data line),
`ebc_set_eink_buf`, `ebc_lcdc_dev`. The naming and structure match **Rockchip's open-source EBC
driver** (rk_ebc) ported to Qualcomm — a good reference for reconstructing the ioctl ABI.
SurfaceFlinger's ioctl: `SET_EBC_SEND_UPDATE` (string in `surfaceflinger`).

### EPDC core
`onyx_epdc_set_mode / set_timing / set_screen_info / set_blank / set_night_mode / set_temp /
get_temp_from_pmic / get_status / get_extbuf / set|get_pwrdown_delay / update_wb_sg /
waveform_buf_release_work_func`; waveform queue `epdc_waveform_buf_{init,get,get_no_wait,queue,
dequeue,release,release_and_clean}`, `epdc_waveform_list_get_sum`.
→ Flow: update request → pick waveform by mode + **panel temperature (PMIC sensor)** → compute
per-frame pixel drive data → queue frames → TCON scans them out.

### TCON (scan-out over the Qualcomm MDSS/DRM path)
`onyx_tcon_init / panel_init / send_cmd / parse_cmd / cmd_queue / cmd_triger / set_display_mode /
set_a2_mode_type / set_update_enable / is_sleep / drm_mode_setplane`, external buffer sync
`onyx_tcon_display_extbuf(_for_sync, _backup_from_fb, _sync_work_commit, _test)`,
`onyx_tcon_ext_buf_sync_with_fb`, `onyx_tcon_dispfb_sync_to_extbuf`, `onyx_tcon_save_extbuf_to_disk`
(debug dump), **handwriting** `onyx_tcon_handwrite_buf_malloc / handwrite_create_cmd_packet /
handwrite_submit` (the kernel half of SurfaceFlinger's direct pen drawing).

## Kernel source / GPL
The kernel is GPL-2.0; Onyx should provide source for `4.19.157-perf-g77c132eccfab`. Without it,
the Onyx drivers are reconstructed from `vmlinux.elf` (symbols, no debug info). Next: decompile
`ebc_ioctl` (ioctl numbers + structs), `onyx_misc` attributes, and the dtb panel/touch nodes.
