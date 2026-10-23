//! E-ink update modes: the `EINK_*` int of `android.onyx.ViewUpdateHelper`, as a typed value.
//!
//! Layout (docs/ONYX-EPD-PEN.md): the low nibble is the waveform, the remaining bits are flags.
//! Decoding is strict: an undefined waveform nibble or an unknown flag bit is an error, never a
//! silent default, so a misread mode cannot reach the panel.

use core::fmt;

/// Waveform nibble (`EINK_WAVEFORM_MODE_*`). Values 7, 10, 13–15 are undefined on this firmware.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Waveform {
    Init = 0,
    Du = 1,
    Gc16 = 2,
    Gc4 = 3,
    /// `EINK_WAVEFORM_MODE_ANIM`: A2, the fast 1-bit waveform.
    A2 = 4,
    Auto = 5,
    Regal = 6,
    Du4 = 8,
    RegalPlus = 9,
    Gcc16 = 11,
    DeepGc16 = 12,
}

impl Waveform {
    pub const MASK: u32 = 0xF;

    pub const fn from_nibble(n: u8) -> Option<Self> {
        Some(match n {
            0 => Self::Init,
            1 => Self::Du,
            2 => Self::Gc16,
            3 => Self::Gc4,
            4 => Self::A2,
            5 => Self::Auto,
            6 => Self::Regal,
            8 => Self::Du4,
            9 => Self::RegalPlus,
            11 => Self::Gcc16,
            12 => Self::DeepGc16,
            _ => return None,
        })
    }
}

/// The flag bits above the waveform nibble. A set of `EINK_*` bits, kept as raw bits so that
/// encoding is lossless; only bits in [`ModeFlags::KNOWN`] can be constructed.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ModeFlags(u32);

impl ModeFlags {
    pub const NONE: Self = Self(0);
    /// `EINK_AUTO_MODE_AUTOMATIC` (else regional).
    pub const AUTOMATIC: Self = Self(0x10);
    /// `EINK_UPDATE_MODE_FULL` (else partial).
    pub const FULL: Self = Self(0x20);
    /// `EINK_WAIT_MODE_WAIT`: block until the update completes.
    pub const WAIT: Self = Self(0x40);
    pub const COMBINE: Self = Self(0x80);
    pub const DITHER: Self = Self(0x100);
    pub const INVERT: Self = Self(0x200);
    pub const CONVERT: Self = Self(0x400);
    /// `EINK_DITHER_COLOR_Y1` (else Y4).
    pub const DITHER_Y1: Self = Self(0x800);
    pub const REGAL_D: Self = Self(0x1000);
    /// `EPDC_FLAG_HANDWRITE_GU`: the instant-ink repaint path.
    pub const HANDWRITE_GU: Self = Self(0x80000);
    /// `EINK_FLAG_SHUTDOWN` spans two bits (0x400000 | 0x100000).
    pub const SHUTDOWN: Self = Self(0x50_0000);
    /// `EINK_DITHER_X` = `EINK_ONYX_AUTO_MASK` (the "X" modes).
    pub const DITHER_X: Self = Self(0x100_0000);
    /// `EINK_APPLY_MONO` = `EINK_ONYX_GC_MASK`.
    pub const APPLY_MONO: Self = Self(0x200_0000);

    /// Every flag above, derived from the constants so the two cannot drift.
    pub const KNOWN: u32 = Self::AUTOMATIC.0 | Self::FULL.0 | Self::WAIT.0 | Self::COMBINE.0 | Self::DITHER.0
        | Self::INVERT.0 | Self::CONVERT.0 | Self::DITHER_Y1.0 | Self::REGAL_D.0 | Self::HANDWRITE_GU.0
        | Self::SHUTDOWN.0 | Self::DITHER_X.0 | Self::APPLY_MONO.0;


    pub const fn bits(self) -> u32 {
        self.0
    }
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

// Module-level so it is always evaluated (an associated const is not, unless used).
const _: () = assert!(ModeFlags::KNOWN == 0x0358_1FF0, "flag set changed: update docs/ONYX-EPD-PEN.md too");

impl core::ops::BitOr for ModeFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        self.union(rhs)
    }
}

impl fmt::Debug for ModeFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ModeFlags({:#x})", self.0)
    }
}

/// A complete update mode: what `refreshScreen(..., mode)` and friends take.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UpdateMode {
    pub waveform: Waveform,
    pub flags: ModeFlags,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModeError {
    /// `INVALID_MODE` (-1) or any other negative value.
    Negative(i32),
    UndefinedWaveform(u8),
    UnknownFlags(u32),
}

impl fmt::Display for ModeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Negative(v) => write!(f, "negative update mode {v} (INVALID_MODE?)"),
            Self::UndefinedWaveform(n) => write!(f, "waveform nibble {n} is undefined"),
            Self::UnknownFlags(b) => write!(f, "unknown flag bits {b:#x}"),
        }
    }
}

impl std::error::Error for ModeError {}

impl UpdateMode {
    pub const fn new(waveform: Waveform, flags: ModeFlags) -> Self {
        Self { waveform, flags }
    }

    /// Decode the Onyx `int`.
    pub fn from_onyx(value: i32) -> Result<Self, ModeError> {
        if value < 0 {
            return Err(ModeError::Negative(value));
        }
        let v = value as u32;
        let nibble = (v & Waveform::MASK) as u8;
        let waveform = Waveform::from_nibble(nibble).ok_or(ModeError::UndefinedWaveform(nibble))?;
        let rest = v & !Waveform::MASK;
        if rest & !ModeFlags::KNOWN != 0 {
            return Err(ModeError::UnknownFlags(rest & !ModeFlags::KNOWN));
        }
        Ok(Self { waveform, flags: ModeFlags(rest) })
    }

    /// Encode back to the Onyx `int`; `from_onyx(m.to_onyx()) == Ok(m)` for every mode.
    pub const fn to_onyx(self) -> i32 {
        (self.waveform as u32 | self.flags.0) as i32
    }

    // Named modes (`UI_*_MODE`), composed rather than copied; tests check them against the spec.
    pub const DEFAULT: Self = Self::new(Waveform::Auto, ModeFlags::NONE);
    pub const DU: Self = Self::new(Waveform::Du, ModeFlags::NONE);
    pub const GU: Self = Self::new(Waveform::Gc16, ModeFlags::NONE);
    pub const GC4: Self = Self::new(Waveform::Gc4, ModeFlags::NONE);
    pub const A2_PERFORMANCE: Self = Self::new(Waveform::A2, ModeFlags::NONE);
    pub const REGAL: Self = Self::new(Waveform::Regal, ModeFlags::NONE);
    pub const REGAL_PLUS: Self = Self::new(Waveform::RegalPlus, ModeFlags::NONE);
    /// Full GC16 with wait: the classic "flash" refresh.
    pub const GC: Self = Self::new(Waveform::Gc16, ModeFlags::FULL.union(ModeFlags::WAIT));
    pub const GCC: Self = Self::new(Waveform::Gcc16, ModeFlags::FULL.union(ModeFlags::WAIT));
    pub const DEEP_GC: Self = Self::new(Waveform::DeepGc16, ModeFlags::FULL.union(ModeFlags::WAIT));
    pub const A2_QUALITY: Self = Self::new(Waveform::A2, ModeFlags::DITHER.union(ModeFlags::DITHER_Y1));
    pub const DU_QUALITY: Self = Self::new(Waveform::Du, ModeFlags::DITHER.union(ModeFlags::DITHER_Y1));
    pub const DU4: Self = Self::new(Waveform::Du4, ModeFlags::DITHER.union(ModeFlags::DITHER_Y1));
    pub const X_A2: Self = Self::new(Waveform::A2, ModeFlags::DITHER_X);
    pub const X_DU: Self = Self::new(Waveform::Du, ModeFlags::DITHER_X);
    pub const MONO_A2: Self = Self::new(Waveform::A2, ModeFlags::APPLY_MONO);
    pub const GC_SHUTDOWN: Self = Self::new(Waveform::Gc16, ModeFlags::FULL.union(ModeFlags::WAIT).union(ModeFlags::SHUTDOWN));
    pub const REGAL_SHUTDOWN: Self = Self::new(Waveform::Regal, ModeFlags::SHUTDOWN);
    pub const HANDWRITING_REPAINT: Self = Self::new(Waveform::Gc16, ModeFlags::HANDWRITE_GU);

    /// `(spec name, mode)` for every named mode, for spec checks and the Java shim generator.
    pub const NAMED: &'static [(&'static str, Self)] = &[
        ("UI_DEFAULT_MODE", Self::DEFAULT),
        ("UI_DU_MODE", Self::DU),
        ("UI_GU_MODE", Self::GU),
        ("UI_GC4_MODE", Self::GC4),
        ("UI_A2_PERFORMANCE_MODE", Self::A2_PERFORMANCE),
        ("UI_REGAL_MODE", Self::REGAL),
        ("UI_REGAL_PLUS_MODE", Self::REGAL_PLUS),
        ("UI_GC_MODE", Self::GC),
        ("UI_GCC_MODE", Self::GCC),
        ("UI_DEEP_GC_MODE", Self::DEEP_GC),
        ("UI_A2_QUALITY_MODE", Self::A2_QUALITY),
        ("UI_DU_QUALITY_MODE", Self::DU_QUALITY),
        ("UI_DU4_MODE", Self::DU4),
        ("UI_X_A2_MODE", Self::X_A2),
        ("UI_X_DU_MODE", Self::X_DU),
        ("UI_MONO_A2_MODE", Self::MONO_A2),
        ("UI_GC_SHUTDOWN_MODE", Self::GC_SHUTDOWN),
        ("UI_REGAL_SHUTDOWN_MODE", Self::REGAL_SHUTDOWN),
        ("HAND_WRITING_REPAINT_MODE", Self::HANDWRITING_REPAINT),
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_valid_int_round_trips() {
        let mut ok = 0;
        for nibble in 0..16u32 {
            for bits in [0, 0x10, 0x60, 0x900, 0x80000, 0x50_0000, 0x100_0000, 0x200_0000, ModeFlags::KNOWN] {
                let v = (nibble | bits) as i32;
                if let Ok(m) = UpdateMode::from_onyx(v) {
                    assert_eq!(m.to_onyx(), v);
                    ok += 1;
                }
            }
        }
        assert_eq!(ok, 11 * 9, "11 defined waveforms x 9 flag sets");
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(UpdateMode::from_onyx(-1), Err(ModeError::Negative(-1)));
        assert_eq!(UpdateMode::from_onyx(7), Err(ModeError::UndefinedWaveform(7)));
        assert_eq!(UpdateMode::from_onyx(0x2002), Err(ModeError::UnknownFlags(0x2000)));
    }
}
