//! Instant-ink pen control: the two vocabularies Onyx mixes in plain ints.
//!
//! `setScreenHandWritingPenState(int)` takes a *command*; `getPenState()` returns the SF
//! *controller state* (`OnyxPenStateUtil.CTRL_*`). The numbers overlap (0, 2) but mean different
//! things, so they are separate types here.

/// Sent with `SET_SCREEN_HANDWRITING_PEN_STATE` (together with the caller's pid).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum PenCommand {
    /// Leave handwriting mode.
    Quit = 0,
    /// Enter handwriting mode. On its own this does **not** render ink; follow with `Resume`.
    Start = 1,
    /// Render on (what `TouchHelper.setRawDrawingEnabled(true)` sends).
    Resume = 2,
    /// Render off, e.g. while the app redraws its own content.
    Pause = 3,
}

/// Returned by `GET_PEN_STATE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PenCtrlState {
    Stop,
    Running,
    Paused,
    /// A value the framework names nothing; kept so a reply is never misread as a known state.
    Other(i32),
}

impl PenCtrlState {
    pub const fn from_reply(v: i32) -> Self {
        match v {
            0 => Self::Stop,
            2 => Self::Running,
            4 => Self::Paused,
            other => Self::Other(other),
        }
    }

    /// `OnyxPenStateUtil.isPenDrawState()`: anything except stopped or paused.
    pub const fn is_drawing(self) -> bool {
        !matches!(self, Self::Stop | Self::Paused)
    }
}

/// Stroke style for `SET_STROKE_STYLE` (Onyx `EpdController` stroke styles).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum StrokeStyle {
    Pencil = 0,
    Brush = 1,
    Marker = 2,
    NeoBrush = 3,
    Charcoal = 4,
}

/// `SET_SCREEN_HANDWRITING_REGION_MODE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum RegionMode {
    /// A stroke may run through any limit rect (`PenManager::inValidRegion`, field +0x38 == 0).
    /// What the stock SDK open sequence sends.
    Multi = 0,
    /// A stroke stays in the limit rect it started in (the first hit is latched in +0x2044).
    Single = 1,
}

/// The SDK's open → enable → close order (docs/ONYX-EPD-PEN.md, "Full SDK open sequence").
/// Stopping after `Start` is the classic "ink not engaging" bug, so the sequence is data, not a
/// convention each caller has to remember.
pub const OPEN_SEQUENCE: [PenCommand; 2] = [PenCommand::Start, PenCommand::Resume];
pub const CLOSE_SEQUENCE: [PenCommand; 2] = [PenCommand::Pause, PenCommand::Quit];
