//! Linux/Android evdev I/O: find the stylus by capabilities and stream [`RawEvent`]s.

use crate::decode::{codes::*, AbsRange, RawEvent, Snapshot};
use std::fs::File;
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

// _IOC(dir, 'E', nr, size) with the generic encoding used on arm64 and x86.
const fn ioc_read(nr: u32, size: u32) -> u32 {
    (2 << 30) | (size << 16) | ((b'E' as u32) << 8) | nr
}
const fn eviocgbit(ev: u16, len: u32) -> u32 {
    ioc_read(0x20 + ev as u32, len)
}
const fn eviocgabs(abs: u16) -> u32 {
    ioc_read(0x40 + abs as u32, 24) // struct input_absinfo: 6 × i32
}
const fn eviocgkey(len: u32) -> u32 {
    ioc_read(0x18, len)
}
const fn eviocgname(len: u32) -> u32 {
    ioc_read(0x06, len)
}

fn ioctl_buf(f: &File, req: u32, buf: &mut [u8]) -> io::Result<()> {
    // SAFETY: the kernel writes at most buf.len() bytes, which the request encodes.
    let r = unsafe { libc::ioctl(f.as_raw_fd(), req as _, buf.as_mut_ptr()) };
    if r < 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
}

fn has_bit(bits: &[u8], n: u16) -> bool {
    bits.get(n as usize / 8).is_some_and(|b| b & (1 << (n % 8)) != 0)
}

/// A stylus device: what the capability probe found.
#[derive(Debug)]
pub struct PenDevice {
    pub path: PathBuf,
    pub name: String,
    pub x: AbsRange,
    pub y: AbsRange,
    pub pressure: AbsRange,
    pub has_eraser: bool,
    pub has_tilt: bool,
    file: File,
}

impl PenDevice {
    /// Open `path` if it is a pen: ABS_X/Y/PRESSURE plus any pen-type tool key ([`PEN_TOOLS`]).
    /// Shape-based, so a renamed or different digitizer still works; touchscreens report no tool
    /// keys (only BTN_TOUCH / MT axes) and are skipped.
    pub fn probe(path: &Path) -> io::Result<Option<Self>> {
        let file = File::open(path)?;
        let mut abs = [0u8; 8];
        let mut key = [0u8; 96];
        ioctl_buf(&file, eviocgbit(EV_ABS, abs.len() as u32), &mut abs)?;
        ioctl_buf(&file, eviocgbit(EV_KEY, key.len() as u32), &mut key)?;
        let is_pen = [ABS_X, ABS_Y, ABS_PRESSURE].iter().all(|&c| has_bit(&abs, c)) 
            && PEN_TOOLS.iter().any(|&t| has_bit(&key, t));
        if !is_pen {
            return Ok(None);
        }
        let range = |code| -> io::Result<AbsRange> {
            let mut info = [0u8; 24];
            ioctl_buf(&file, eviocgabs(code), &mut info)?;
            let at = |i: usize| i32::from_ne_bytes(info[i * 4..i * 4 + 4].try_into().unwrap());
            Ok(AbsRange { min: at(1), max: at(2) })
        };
        let mut name = [0u8; 128];
        let _ = ioctl_buf(&file, eviocgname(name.len() as u32), &mut name);
        let name = String::from_utf8_lossy(name.split(|&b| b == 0).next().unwrap_or(&[])).into_owned();
        Ok(Some(Self {
            path: path.to_owned(),
            name,
            x: range(ABS_X)?,
            y: range(ABS_Y)?,
            pressure: range(ABS_PRESSURE)?,
            has_eraser: has_bit(&key, BTN_TOOL_RUBBER),
            has_tilt: has_bit(&abs, ABS_TILT_X),
            file,
        }))
    }

    /// First pen among `/dev/input/event*`. Errors on each candidate are returned in `skipped`
    /// rather than swallowed, so "no pen found" can say why (permissions, SELinux, ...).
    pub fn find() -> io::Result<Discovery> {
        let mut paths: Vec<_> = std::fs::read_dir("/dev/input")?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("event")))
            .collect();
        paths.sort();
        let mut skipped = Vec::new();
        for p in paths {
            match Self::probe(&p) {
                Ok(Some(d)) => return Ok(Discovery { pen: Some(d), skipped }),
                Ok(None) => {}
                Err(error) => skipped.push(Skipped { path: p, error }),
            }
        }
        Ok(Discovery { pen: None, skipped })
    }

    /// Current key and axis state, for [`crate::PenDecoder::resync`] after `SYN_DROPPED`.
    pub fn snapshot(&self) -> io::Result<Snapshot> {
        let mut key = [0u8; 96];
        ioctl_buf(&self.file, eviocgkey(key.len() as u32), &mut key)?;
        let value = |code| -> io::Result<i32> {
            let mut info = [0u8; 24];
            ioctl_buf(&self.file, eviocgabs(code), &mut info)?;
            Ok(i32::from_ne_bytes(info[0..4].try_into().unwrap()))
        };
        let tilt = |code| if self.has_tilt { value(code) } else { Ok(0) };
        Ok(Snapshot {
            x: value(ABS_X)?,
            y: value(ABS_Y)?,
            pressure: value(ABS_PRESSURE)?,
            tilt_x: tilt(ABS_TILT_X)?,
            tilt_y: tilt(ABS_TILT_Y)?,
            pen_tool: PEN_TOOLS.iter().any(|&t| has_bit(&key, t)),
            eraser_tool: has_bit(&key, BTN_TOOL_RUBBER),
            touching: has_bit(&key, BTN_TOUCH),
            side: has_bit(&key, BTN_STYLUS),
        })
    }

    /// The decoded event for `raw`, plus — when `raw` ended a `SYN_DROPPED` gap — the event from
    /// re-reading the device state. Read loops call this instead of `dec.feed` directly.
    pub fn decode(&self, dec: &mut crate::PenDecoder, raw: RawEvent) -> [Option<crate::PenEvent>; 2] {
        let first = dec.feed(raw);
        let second = if dec.needs_resync() {
            // A failed read keeps "pen away" (what the drop already assumed); the next contact recovers.
            dec.resync(self.snapshot().unwrap_or_default())
        } else {
            None
        };
        [first, second]
    }

    /// For poll(2) alongside a wakeup fd (the stock reader's `notifyFd` pattern).
    pub fn raw_fd(&self) -> std::os::fd::RawFd {
        self.file.as_raw_fd()
    }

    /// Block for the next raw event.
    pub fn next_event(&mut self) -> io::Result<RawEvent> {
        let mut b = [0u8; RawEvent::SIZE];
        self.file.read_exact(&mut b)?;
        Ok(RawEvent::parse(&b))
    }
}

/// A candidate node that could not be probed, and why.
#[derive(Debug)]
pub struct Skipped {
    pub path: PathBuf,
    pub error: io::Error,
}

/// Result of [`PenDevice::find`]: the pen (if any) plus every node that failed to probe.
#[derive(Debug)]
pub struct Discovery {
    pub pen: Option<PenDevice>,
    pub skipped: Vec<Skipped>,
}

impl Discovery {
    /// One-line "path: error, ..." summary of the skipped nodes, for error messages.
    pub fn skipped_summary(&self) -> String {
        let v: Vec<String> = self.skipped.iter().map(|s| format!("{}: {}", s.path.display(), s.error)).collect();
        v.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ioctl_numbers_match_linux_headers() {
        // Values from <linux/input.h> on arm64/x86_64.
        assert_eq!(eviocgabs(ABS_PRESSURE), 0x8018_4558);
        assert_eq!(eviocgbit(EV_KEY, 96), 0x8060_4521);
        assert_eq!(eviocgname(128), 0x8080_4506);
        assert_eq!(eviocgkey(96), 0x8060_4518);
    }
}
