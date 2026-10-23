//! Pure, host-testable encoding of Onyx's SurfaceFlinger calls: what goes into the Parcel after
//! the interface token, in order, and what the reply holds. Mirrors `android.onyx.ViewUpdateHelper`
//! (spec: impl/spec/sf_protocol.json). The client adds screen offsets itself: there is no `View`
//! here, so every rectangle is already in screen pixels.

use openink_core::{Code, PenCommand, RegionMode, StrokeStyle, UpdateMode};

/// One Parcel write. The spec names these `Int`, `Float`, `IntArray`.
#[derive(Clone, Debug, PartialEq)]
pub enum Arg {
    I32(i32),
    F32(f32),
    /// Java `writeIntArray`: length, then the values.
    I32s(Vec<i32>),
}

/// What the reply Parcel carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reply {
    None,
    I32,
}

/// A rectangle in screen pixels, `left/top` inclusive, `right/bottom` exclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl ScreenRect {
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self { left, top, right, bottom }
    }
    pub const fn width(self) -> i32 {
        self.right - self.left
    }
    pub const fn height(self) -> i32 {
        self.bottom - self.top
    }
}

/// A complete call. `method` is the ViewUpdateHelper name it mirrors (used by the spec test).
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub method: &'static str,
    pub code: Code,
    pub args: Vec<Arg>,
    pub reply: Reply,
}

/// The process id the pen-state and post calls carry: SF keys its handwriting session by pid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pid(pub i32);

fn req(method: &'static str, code: Code, args: Vec<Arg>) -> Request {
    Request { method, code, args, reply: Reply::None }
}

fn flat(rects: &[ScreenRect]) -> Vec<i32> {
    rects.iter().flat_map(|r| [r.left, r.top, r.right, r.bottom]).collect()
}

/// `refreshScreen(l, t, w, h, mode)`.
pub fn refresh(area: ScreenRect, mode: UpdateMode) -> Request {
    use Arg::I32;
    req("refreshScreen", Code::RefreshScreen,
        vec![I32(area.left), I32(area.top), I32(area.width()), I32(area.height()), I32(mode.to_onyx())])
}

pub fn wait_for_update_finished() -> Request {
    req("waitForUpdateFinished", Code::WaitForUpdateFinished, vec![])
}

pub fn set_pen_state(cmd: PenCommand, pid: Pid) -> Request {
    req("setScreenHandWritingPenState", Code::SetScreenHandwritingPenState, vec![Arg::I32(cmd as i32), Arg::I32(pid.0)])
}

/// Reply: the controller state (`PenCtrlState::from_reply`).
pub fn pen_state() -> Request {
    Request { reply: Reply::I32, ..req("getPenState", Code::GetPenState, vec![]) }
}

/// Where SF may draw ink. The leading 1 is the helper's "a view was given" flag: our rectangles
/// are always screen-absolute, which is what the helper produces after adding the view offset.
pub fn set_region_limit(rects: &[ScreenRect]) -> Request {
    req("setScreenHandWritingRegionLimit", Code::SetScreenHandwritingRegionLimit, vec![Arg::I32(1), Arg::I32s(flat(rects))])
}

pub fn set_region_exclude(rects: &[ScreenRect]) -> Request {
    req("setScreenHandWritingRegionExclude", Code::SetScreenHandwritingRegionExclude, vec![Arg::I32(1), Arg::I32s(flat(rects))])
}

pub fn set_region_mode(mode: RegionMode) -> Request {
    req("setScreenHandWritingRegionMode", Code::SetScreenHandwritingRegionMode, vec![Arg::I32(mode as i32)])
}

pub fn set_stroke_width(px: f32) -> Request {
    req("setStrokeWidth", Code::SetStrokeWidth, vec![Arg::F32(px)])
}

/// ARGB, as `android.graphics.Color`.
pub fn set_stroke_color(argb: i32) -> Request {
    req("setStrokeColor", Code::SetStrokeColor, vec![Arg::I32(argb)])
}

pub fn set_stroke_style(style: StrokeStyle) -> Request {
    req("setStrokeStyle", Code::SetStrokeStyle, vec![Arg::I32(style as i32)])
}

pub fn set_brush_raw_drawing(enabled: bool) -> Request {
    req("setBrushRawDrawingEnabled", Code::SetBrushRawDrawingEnabled, vec![Arg::I32(enabled as i32)])
}

/// The eraser call's second argument in every SDK call seen (TouchHelper's default); meaning unknown.
pub const ERASER_DEFAULT_ARG: i32 = 5;

/// `arg` is [`ERASER_DEFAULT_ARG`] in practice.
pub fn set_eraser_raw_drawing(enabled: bool, arg: i32) -> Request {
    req("setEraserRawDrawingEnabled", Code::SetEraserRawDrawingEnabled, vec![Arg::I32(enabled as i32), Arg::I32(arg)])
}

/// Show the app's picture over the quick ink. The leading -1 is what the helper always sends.
pub fn enable_post(enable: bool, pid: Pid) -> Request {
    req("enablePost", Code::EnablePost, vec![Arg::I32(-1), Arg::I32(enable as i32), Arg::I32(pid.0)])
}

pub fn pen_up() -> Request {
    req("penUp", Code::PenUp, vec![])
}

/// Repaint `area` from the app's buffer in handwriting mode; `flag` is the helper's boolean.
pub fn handwriting_repaint(area: ScreenRect, flag: bool) -> Request {
    req("handwritingRepaint", Code::HandwritingRepaint, vec![Arg::I32(flag as i32), Arg::I32s(flat(&[area]))])
}

/// The open → render-on sequence (docs/ONYX-EPD-PEN.md, "Full SDK open sequence"), as data.
pub fn open_sequence(region: ScreenRect, width_px: f32, pid: Pid) -> Vec<Request> {
    vec![
        set_stroke_style(StrokeStyle::Pencil),
        set_pen_state(PenCommand::Start, pid),
        set_region_limit(&[region]),
        set_region_mode(RegionMode::Multi),
        set_stroke_width(width_px),
        set_pen_state(PenCommand::Resume, pid),
        set_brush_raw_drawing(true),
        set_eraser_raw_drawing(false, ERASER_DEFAULT_ARG),
    ]
}

/// The close sequence: style 0, post, pause, quit.
pub fn close_sequence(pid: Pid) -> Vec<Request> {
    vec![
        set_stroke_style(StrokeStyle::Pencil),
        enable_post(true, pid),
        set_pen_state(PenCommand::Pause, pid),
        set_pen_state(PenCommand::Quit, pid),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_sequence_resumes_after_start() {
        let seq = open_sequence(ScreenRect::new(0, 130, 1860, 2480), 3.0, Pid(42));
        let states: Vec<_> = seq.iter().filter(|r| r.code == Code::SetScreenHandwritingPenState).map(|r| r.args[0].clone()).collect();
        assert_eq!(states, [Arg::I32(1), Arg::I32(2)]);
        assert_eq!(seq[2].args, [Arg::I32(1), Arg::I32s(vec![0, 130, 1860, 2480])]);
    }

    #[test]
    fn refresh_sends_width_and_height() {
        let r = refresh(ScreenRect::new(10, 20, 110, 220), UpdateMode::DU);
        assert_eq!(r.args[..4], [Arg::I32(10), Arg::I32(20), Arg::I32(100), Arg::I32(200)]);
    }
}
