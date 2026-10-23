//! `/dev/ebc` contract between the compositor (L3) and the kernel e-ink driver (L4).
//! Source: docs/EBC-ABI.md (decompiled `ebc_ioctl`, `onyx_tcon_handwrite_*`). Pure data + encoders;
//! actually issuing the ioctls needs a flashed device (stock SELinux allows only surfaceflinger).

/// ioctl command numbers. Raw values: the driver switches on them directly (no `_IOW` encoding).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Ioctl {
    GetDriverSn = 0x7002,
    SetUpdateScheme = 0x7006,
    SendUpdate = 0x700c,
    SetClearColor = 0x700e,
    SyncExtbufFromFb = 0x701d,
    SetExtbufSyncFbEnable = 0x701f,
    GetGrayMode = 0x7021,
    SetA2Mode = 0x7024,
    SetCfaMode = 0x7026,
    SetDisplayMode = 0x7027,
    GetA2CutFrames = 0x7028,
}

/// TCON register writes go through `0x7200 | reg`, reads through `0x7300 | reg`.
pub const fn tcon_write_cmd(reg: u8) -> u32 {
    0x7200 | reg as u32
}
pub const fn tcon_read_cmd(reg: u8) -> u32 {
    0x7300 | reg as u32
}

/// Update schemes (`onyx_tcon_set_upd_scheme`); 3 is the handwriting scheme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Scheme {
    Normal = 1,
    Keyboard = 2,
    Handwrite = 3,
    ApplicationAnimation = 4,
    SystemAnimation = 5,
}

/// `SET_EBC_SEND_UPDATE` argument (`mxcfb_update_data` layout, 0x28 bytes).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct EbcUpdate {
    pub top: u32,
    pub left: u32,
    pub width: u32,
    pub height: u32,
    pub waveform_mode: u32,
    /// 1 = full.
    pub update_mode: u32,
    pub update_marker: u32,
    pub temp: i32,
    pub flags: u32,
    pub reserved: u32,
}

const _: () = assert!(core::mem::size_of::<EbcUpdate>() == 0x28);

impl EbcUpdate {
    /// bit 18: route through the handwriting side channel instead of a video frame.
    pub const FLAG_HANDWRITE: u32 = 1 << 18;
}

pub const HANDWRITE_MAGIC: u32 = 0x55aa_55aa;
pub const HANDWRITE_CMD_REGION: u8 = 0x21;
pub const HANDWRITE_HEADER_LEN: usize = 0x10;
/// The driver splits a rect by rows so each DSI packet stays within this many bytes.
pub const HANDWRITE_MAX_PACKET: usize = 0xffdc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PacketError {
    Empty,
    OutsidePanel,
    /// A single row is wider than one packet can carry.
    RowTooWide,
    /// `buf` is not `panel_w × panel_h × 4` bytes, or the panel exceeds the header's u16 fields.
    BadBuffer,
}

/// One DSI command packet for the FPGA: header + RGBA32 pixels of rows `y0..y1`, columns
/// `x0..x1`, with x aligned to 16 px on both ends (as `onyx_tcon_handwrite_create_cmd_packet`).
#[derive(Debug, PartialEq, Eq)]
pub struct HandwritePacket {
    pub x0: u16,
    pub y0: u16,
    pub x1: u16,
    pub y1: u16,
    pub bytes: Vec<u8>,
}

/// Split `rect` of the RGBA32 handwrite buffer (`panel_w` × `panel_h`) into FPGA packets.
pub fn handwrite_packets(
    buf: &[u8],
    panel_w: u32,
    panel_h: u32,
    rect: Rect,
    waveform_mode: u8,
) -> Result<Vec<HandwritePacket>, PacketError> {
    if rect.width == 0 || rect.height == 0 {
        return Err(PacketError::Empty);
    }
    // Packet headers carry u16 coordinates; sizes are checked in usize so nothing can wrap.
    if panel_w > u16::MAX as u32 || panel_h > u16::MAX as u32 || buf.len() != panel_w as usize * panel_h as usize * 4 {
        return Err(PacketError::BadBuffer);
    }
    let fits = |start: u32, len: u32, max: u32| start.checked_add(len).is_some_and(|e| e <= max);
    if !fits(rect.left, rect.width, panel_w) || !fits(rect.top, rect.height, panel_h) {
        return Err(PacketError::OutsidePanel);
    }
    let x0 = rect.left & !0xf;
    let x1 = ((rect.left + rect.width + 15) & !0xf).min(panel_w);
    let row_bytes = ((x1 - x0) * 4) as usize;
    let rows_per_packet = (HANDWRITE_MAX_PACKET - HANDWRITE_HEADER_LEN) / row_bytes;
    if rows_per_packet == 0 {
        return Err(PacketError::RowTooWide);
    }
    let mut out = Vec::new();
    let mut y = rect.top;
    let end = rect.top + rect.height;
    while y < end {
        let y1 = (y + rows_per_packet as u32).min(end);
        let mut bytes = Vec::with_capacity(HANDWRITE_HEADER_LEN + row_bytes * (y1 - y) as usize);
        bytes.extend_from_slice(&HANDWRITE_MAGIC.to_le_bytes());
        bytes.push(HANDWRITE_CMD_REGION);
        for v in [x0, y, x1, y1] {
            bytes.extend_from_slice(&(v as u16).to_le_bytes());
        }
        bytes.push(waveform_mode);
        bytes.extend_from_slice(&0u16.to_le_bytes());
        for row in y..y1 {
            let start = (row as usize * panel_w as usize + x0 as usize) * 4;
            bytes.extend_from_slice(&buf[start..start + row_bytes]);
        }
        out.push(HandwritePacket { x0: x0 as u16, y0: y as u16, x1: x1 as u16, y1: y1 as u16, bytes });
        y = y1;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: u32 = 2480; // panel 2480x1860 (EBC-ABI.md)
    const H: u32 = 1860;

    #[test]
    fn header_layout() {
        let buf = vec![0xffu8; (W * H * 4) as usize];
        let p = handwrite_packets(&buf, W, H, Rect { left: 37, top: 10, width: 20, height: 3 }, 7).unwrap();
        assert_eq!(p.len(), 1);
        let b = &p[0].bytes;
        assert_eq!(&b[0..4], &[0xaa, 0x55, 0xaa, 0x55]);
        assert_eq!(b[4], 0x21);
        assert_eq!(u16::from_le_bytes([b[5], b[6]]), 32, "x0 aligned down");
        assert_eq!(u16::from_le_bytes([b[9], b[10]]), 64, "x1 = (37+20+15)&!15");
        assert_eq!(u16::from_le_bytes([b[11], b[12]]), 13);
        assert_eq!(b[13], 7);
        assert_eq!(b.len(), 0x10 + 3 * 32 * 4);
    }

    #[test]
    fn full_width_rect_splits_under_limit() {
        let buf = vec![0u8; (W * H * 4) as usize];
        let p = handwrite_packets(&buf, W, H, Rect { left: 0, top: 0, width: W, height: H }, 1).unwrap();
        assert!(p.iter().all(|x| x.bytes.len() <= HANDWRITE_MAX_PACKET));
        assert_eq!(p.first().unwrap().y0, 0);
        assert_eq!(p.last().unwrap().y1 as u32, H);
        assert!(p.windows(2).all(|w| w[0].y1 == w[1].y0), "rows are contiguous");
    }

    #[test]
    fn rejects_outside_panel() {
        let buf = vec![0u8; (W * H * 4) as usize];
        let r = Rect { left: W - 1, top: 0, width: 2, height: 1 };
        assert_eq!(handwrite_packets(&buf, W, H, r, 1), Err(PacketError::OutsidePanel));
        // left + width wraps u32: must not pass as "inside".
        let r = Rect { left: 16, top: 0, width: u32::MAX - 8, height: 1 };
        assert_eq!(handwrite_packets(&buf, W, H, r, 1), Err(PacketError::OutsidePanel));
    }

    #[test]
    fn rejects_a_buffer_of_the_wrong_size() {
        let buf = vec![0u8; 16];
        let r = Rect { left: 0, top: 0, width: 4, height: 1 };
        assert_eq!(handwrite_packets(&buf, W, H, r, 1), Err(PacketError::BadBuffer));
    }
}
