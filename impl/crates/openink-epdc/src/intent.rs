//! What a refresh shows, and the waveform the engine picks for it (docs/EPDC-ENGINE.md, "Policy").
//! Callers say *what* changed; only Onyx-compatible callers name a raw mode.

use openink_core::{ModeFlags, UpdateMode, Waveform};

/// The content of a refresh request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Intent {
    /// Ordinary UI or text: 16-level partial update (GU). Readable, no flash.
    Ui,
    /// Photos / gradients: GC16 partial, with dithering off so the 16 levels are used as is.
    Image,
    /// Scrolling, dragging, animation: A2 (1-bit, fastest, ghosts most).
    Motion,
    /// The app's own repaint of finished ink (after pen-up): DU, 2-level, fast and flash-free.
    Ink,
    /// An explicit full cleanup: GC16 full (the "flash"), resets the ghost ledger under it.
    Clean,
    /// An Onyx `EINK_*` mode passed through unchanged (EpdController.refreshScreen compatibility).
    Raw(UpdateMode),
}

impl Intent {
    pub const fn mode(self) -> UpdateMode {
        match self {
            Self::Ui => UpdateMode::GU,
            Self::Image => UpdateMode::new(Waveform::Gc16, ModeFlags::NONE),
            Self::Motion => UpdateMode::A2_PERFORMANCE,
            Self::Ink => UpdateMode::DU,
            Self::Clean => UpdateMode::new(Waveform::Gc16, ModeFlags::FULL),
            Self::Raw(m) => m,
        }
    }

    /// Wire code for JNI (`EpdEngine.java` mirrors it). Raw modes travel as the Onyx int instead.
    pub const fn code(self) -> Option<i32> {
        match self {
            Self::Ui => Some(0),
            Self::Image => Some(1),
            Self::Motion => Some(2),
            Self::Ink => Some(3),
            Self::Clean => Some(4),
            Self::Raw(_) => None,
        }
    }

    pub const fn from_code(c: i32) -> Option<Self> {
        Some(match c {
            0 => Self::Ui,
            1 => Self::Image,
            2 => Self::Motion,
            3 => Self::Ink,
            4 => Self::Clean,
            _ => return None,
        })
    }
}

/// How much ghosting one update leaves on the pixels it drives, in "A2 frames" (A2 = 1.0).
/// Fast waveforms skip the intermediate levels, so remnants of the previous image survive;
/// a full GC-class update drives every pixel through black and white and resets the account.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GhostEffect {
    Add(f32),
    Reset,
}

pub fn ghost_effect(m: UpdateMode) -> GhostEffect {
    let full = m.flags.contains(ModeFlags::FULL);
    match m.waveform {
        Waveform::Init => GhostEffect::Reset,
        Waveform::Gc16 | Waveform::Gcc16 | Waveform::DeepGc16 if full => GhostEffect::Reset,
        Waveform::Gc16 | Waveform::Gcc16 | Waveform::DeepGc16 => GhostEffect::Add(0.1),
        Waveform::Regal | Waveform::RegalPlus => GhostEffect::Add(0.15),
        Waveform::Gc4 => GhostEffect::Add(0.3),
        // Auto: the controller picks; account it like a GU partial (its usual choice for UI).
        Waveform::Auto => GhostEffect::Add(0.1),
        Waveform::Du4 => GhostEffect::Add(0.4),
        Waveform::Du => GhostEffect::Add(0.5),
        Waveform::A2 => GhostEffect::Add(1.0),
    }
}

/// Merge rank: when two pending updates cover the same pixels, the higher-quality one may
/// absorb the other (driving a pixel with GC16 also shows what DU would have shown).
pub fn quality(m: UpdateMode) -> u8 {
    match ghost_effect(m) {
        GhostEffect::Reset => 4,
        GhostEffect::Add(c) if c <= 0.15 => 3,
        GhostEffect::Add(c) if c <= 0.4 => 2,
        GhostEffect::Add(c) if c < 1.0 => 1,
        GhostEffect::Add(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip() {
        for i in [Intent::Ui, Intent::Image, Intent::Motion, Intent::Ink, Intent::Clean] {
            assert_eq!(Intent::from_code(i.code().unwrap()), Some(i));
        }
        assert_eq!(Intent::from_code(5), None);
    }

    #[test]
    fn full_gc_resets_partial_adds() {
        assert_eq!(ghost_effect(UpdateMode::GC), GhostEffect::Reset);
        assert_eq!(ghost_effect(UpdateMode::GU), GhostEffect::Add(0.1));
        assert!(quality(UpdateMode::GC) > quality(UpdateMode::GU));
        assert!(quality(UpdateMode::GU) > quality(UpdateMode::DU));
        assert!(quality(UpdateMode::DU) > quality(UpdateMode::A2_PERFORMANCE));
    }
}
