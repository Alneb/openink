//! Linux tablet protocol → pen events. One [`PenEvent`] per `SYN_REPORT` that changes something.

/// Linux input constants used here (linux/input-event-codes.h).
pub mod codes {
    pub const EV_SYN: u16 = 0x00;
    pub const EV_KEY: u16 = 0x01;
    pub const EV_ABS: u16 = 0x03;
    pub const SYN_REPORT: u16 = 0;
    pub const SYN_DROPPED: u16 = 3;
    pub const ABS_X: u16 = 0x00;
    pub const ABS_Y: u16 = 0x01;
    pub const ABS_PRESSURE: u16 = 0x18;
    pub const ABS_DISTANCE: u16 = 0x19;
    pub const ABS_TILT_X: u16 = 0x1a;
    pub const ABS_TILT_Y: u16 = 0x1b;
    pub const BTN_TOOL_PEN: u16 = 0x140;
    pub const BTN_TOOL_RUBBER: u16 = 0x141;
    pub const BTN_TOOL_BRUSH: u16 = 0x142;
    pub const BTN_TOOL_PENCIL: u16 = 0x143;
    pub const BTN_TOOL_AIRBRUSH: u16 = 0x144;
    /// Tool keys that mean "the writing end is in range". The Note Air 5C's Wacom digitizer has
    /// no BTN_TOOL_PEN at all: it advertises BRUSH (and RUBBER), so matching only PEN misses it.
    pub const PEN_TOOLS: [u16; 4] = [BTN_TOOL_PEN, BTN_TOOL_BRUSH, BTN_TOOL_PENCIL, BTN_TOOL_AIRBRUSH];
    pub const BTN_TOUCH: u16 = 0x14a;
    pub const BTN_STYLUS: u16 = 0x14b;
    pub const BTN_STYLUS2: u16 = 0x14c;
}
use codes::*;

/// One `struct input_event` on a 64-bit kernel: timeval (2 × i64), u16 type, u16 code, i32 value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawEvent {
    pub time_us: i64,
    pub kind: u16,
    pub code: u16,
    pub value: i32,
}

impl RawEvent {
    pub const SIZE: usize = 24;

    pub fn parse(b: &[u8; Self::SIZE]) -> Self {
        let i64_at = |o: usize| i64::from_ne_bytes(b[o..o + 8].try_into().unwrap());
        Self {
            time_us: i64_at(0) * 1_000_000 + i64_at(8),
            kind: u16::from_ne_bytes([b[16], b[17]]),
            code: u16::from_ne_bytes([b[18], b[19]]),
            value: i32::from_ne_bytes(b[20..24].try_into().unwrap()),
        }
    }
}

/// An axis range from `EVIOCGABS`, used to normalise values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AbsRange {
    pub min: i32,
    pub max: i32,
}

impl AbsRange {
    fn normalise(self, v: i32) -> f32 {
        if self.max <= self.min {
            return 0.0;
        }
        ((v - self.min) as f32 / (self.max - self.min) as f32).clamp(0.0, 1.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Pen,
    /// The eraser end (`BTN_TOOL_RUBBER`).
    Eraser,
}

/// What happened at this report. `ForcedUp` mirrors the stock reader's action 6 ("pen force
/// release"): contact was lost without an up event, e.g. after `SYN_DROPPED`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PenAction {
    HoverEnter,
    Hover,
    Down,
    Move,
    Up,
    HoverLeave,
    ForcedUp,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PenEvent {
    pub action: PenAction,
    pub tool: Tool,
    /// Raw device coordinates (mapping to the screen is the caller's: it depends on rotation).
    pub x: i32,
    pub y: i32,
    /// 0..=1 from the device's pressure range.
    pub pressure: f32,
    pub tilt_x: i32,
    pub tilt_y: i32,
    /// `BTN_STYLUS` (the barrel button; the stock reader uses it for side-button erase).
    pub side_button: bool,
    pub time_us: i64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Axes {
    x: i32,
    y: i32,
    pressure: i32,
    tilt_x: i32,
    tilt_y: i32,
}

/// The device's current state, read with EVIOCGKEY/EVIOCGABS after `SYN_DROPPED`: the kernel only
/// sends changes, so a pen held down through an overflow never re-sends `BTN_TOUCH`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub x: i32,
    pub y: i32,
    pub pressure: i32,
    pub tilt_x: i32,
    pub tilt_y: i32,
    /// Any of [`PEN_TOOLS`] held.
    pub pen_tool: bool,
    pub eraser_tool: bool,
    pub touching: bool,
    pub side: bool,
}

/// Pen state machine. Contact comes from `BTN_TOUCH`, proximity from `BTN_TOOL_*`.
#[derive(Debug)]
pub struct PenDecoder {
    pressure: AbsRange,
    axes: Axes,
    tool: Option<Tool>,
    /// The tool of the last report that had one: the leave/up after it is still that tool's.
    last_tool: Tool,
    touching: bool,
    side: bool,
    /// What the last emitted event said, to derive transitions.
    was_in_range: bool,
    was_touching: bool,
    /// After `SYN_DROPPED` the kernel discards events up to the next `SYN_REPORT`.
    dropping: bool,
    /// The drop is over; the caller may [`PenDecoder::resync`] from the device's real state.
    resync_pending: bool,
    /// Time of the last report, stamped on the resync event.
    last_time_us: i64,
}

impl PenDecoder {
    pub fn new(pressure: AbsRange) -> Self {
        Self {
            pressure,
            axes: Axes::default(),
            tool: None,
            last_tool: Tool::Pen,
            touching: false,
            side: false,
            was_in_range: false,
            was_touching: false,
            dropping: false,
            resync_pending: false,
            last_time_us: 0,
        }
    }

    /// True after a `SYN_DROPPED` has been skipped: the decoder assumes "pen away" until
    /// [`Self::resync`] gives it the device's real state.
    pub fn needs_resync(&self) -> bool {
        self.resync_pending
    }

    /// Adopt the device's state after a drop. A pen still on the glass yields a fresh `Down`
    /// (the drop already sent `ForcedUp`), a hovering one `HoverEnter`.
    pub fn resync(&mut self, s: Snapshot) -> Option<PenEvent> {
        self.resync_pending = false;
        self.axes = Axes { x: s.x, y: s.y, pressure: s.pressure, tilt_x: s.tilt_x, tilt_y: s.tilt_y };
        self.tool = if s.eraser_tool { Some(Tool::Eraser) } else if s.pen_tool { Some(Tool::Pen) } else { None };
        self.touching = s.touching;
        self.side = s.side;
        self.report(self.last_time_us)
    }

    /// Feed one raw event; returns an event at `SYN_REPORT` when the report changed something.
    pub fn feed(&mut self, ev: RawEvent) -> Option<PenEvent> {
        if self.dropping {
            if ev.kind == EV_SYN && ev.code == SYN_REPORT {
                self.dropping = false;
                self.resync_pending = true;
                self.last_time_us = ev.time_us;
            }
            return None;
        }
        match (ev.kind, ev.code) {
            (EV_SYN, SYN_DROPPED) => return self.dropped(ev.time_us),
            (EV_SYN, SYN_REPORT) => return self.report(ev.time_us),
            (EV_ABS, ABS_X) => self.axes.x = ev.value,
            (EV_ABS, ABS_Y) => self.axes.y = ev.value,
            (EV_ABS, ABS_PRESSURE) => self.axes.pressure = ev.value,
            (EV_ABS, ABS_TILT_X) => self.axes.tilt_x = ev.value,
            (EV_ABS, ABS_TILT_Y) => self.axes.tilt_y = ev.value,
            (EV_KEY, BTN_TOOL_RUBBER) => self.set_tool(Tool::Eraser, ev.value != 0),
            (EV_KEY, c) if PEN_TOOLS.contains(&c) => self.set_tool(Tool::Pen, ev.value != 0),
            (EV_KEY, BTN_TOUCH) => self.touching = ev.value != 0,
            (EV_KEY, BTN_STYLUS) => self.side = ev.value != 0,
            _ => {}
        }
        None
    }

    fn set_tool(&mut self, tool: Tool, on: bool) {
        if on {
            self.tool = Some(tool);
        } else if self.tool == Some(tool) {
            self.tool = None;
        }
    }

    fn event(&self, action: PenAction, tool: Tool, time_us: i64) -> PenEvent {
        PenEvent {
            action,
            tool,
            x: self.axes.x,
            y: self.axes.y,
            pressure: self.pressure.normalise(self.axes.pressure),
            tilt_x: self.axes.tilt_x,
            tilt_y: self.axes.tilt_y,
            side_button: self.side,
            time_us,
        }
    }

    fn report(&mut self, time_us: i64) -> Option<PenEvent> {
        self.last_time_us = time_us;
        let in_range = self.tool.is_some() || self.touching;
        // Keep reporting the tool through the leave event, where `self.tool` is already cleared.
        let tool = self.tool.unwrap_or(self.last_tool);
        self.last_tool = tool;
        let action = match (self.was_in_range, self.was_touching, in_range, self.touching) {
            (_, false, _, true) => PenAction::Down,
            (_, true, _, true) => PenAction::Move,
            (_, true, _, false) => PenAction::Up, // also when lifted and out of range in one report
            (false, false, true, false) => PenAction::HoverEnter,
            (true, false, true, false) => PenAction::Hover,
            (true, false, false, false) => PenAction::HoverLeave,
            (false, false, false, false) => return None,
        };
        self.was_in_range = in_range;
        self.was_touching = self.touching;
        Some(self.event(action, tool, time_us))
    }

    fn dropped(&mut self, time_us: i64) -> Option<PenEvent> {
        self.dropping = true;
        let was_touching = self.was_touching;
        let tool = self.tool.unwrap_or(self.last_tool);
        self.touching = false;
        self.was_touching = false;
        self.was_in_range = false;
        self.tool = None;
        was_touching.then(|| self.event(PenAction::ForcedUp, tool, time_us))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(kind: u16, code: u16, value: i32) -> RawEvent {
        RawEvent { time_us: 0, kind, code, value }
    }
    fn syn() -> RawEvent {
        ev(EV_SYN, SYN_REPORT, 0)
    }
    fn run(d: &mut PenDecoder, evs: &[RawEvent]) -> Vec<PenAction> {
        evs.iter().filter_map(|e| d.feed(*e)).map(|e| e.action).collect()
    }

    #[test]
    fn hover_stroke_leave() {
        let mut d = PenDecoder::new(AbsRange { min: 0, max: 4095 });
        let actions = run(&mut d, &[
            ev(EV_KEY, BTN_TOOL_PEN, 1), ev(EV_ABS, ABS_X, 10), syn(),
            ev(EV_ABS, ABS_X, 11), syn(),
            ev(EV_KEY, BTN_TOUCH, 1), ev(EV_ABS, ABS_PRESSURE, 2048), syn(),
            ev(EV_ABS, ABS_X, 12), syn(),
            ev(EV_KEY, BTN_TOUCH, 0), ev(EV_ABS, ABS_PRESSURE, 0), syn(),
            ev(EV_KEY, BTN_TOOL_PEN, 0), syn(),
        ]);
        use PenAction::*;
        assert_eq!(actions, [HoverEnter, Hover, Down, Move, Up, HoverLeave]);
    }

    #[test]
    fn brush_tool_is_the_pen() {
        let mut d = PenDecoder::new(AbsRange { min: 0, max: 4095 });
        let e = run(&mut d, &[ev(EV_KEY, BTN_TOOL_BRUSH, 1), syn()]);
        assert_eq!(e, [PenAction::HoverEnter]);
        d.feed(ev(EV_KEY, BTN_TOUCH, 1));
        assert_eq!(d.feed(syn()).unwrap().tool, Tool::Pen);
    }

    #[test]
    fn pressure_is_normalised() {
        let mut d = PenDecoder::new(AbsRange { min: 0, max: 4096 });
        d.feed(ev(EV_KEY, BTN_TOUCH, 1));
        d.feed(ev(EV_ABS, ABS_PRESSURE, 1024));
        let e = d.feed(syn()).unwrap();
        assert_eq!(e.action, PenAction::Down);
        assert!((e.pressure - 0.25).abs() < 1e-6);
    }

    #[test]
    fn eraser_end_is_reported() {
        let mut d = PenDecoder::new(AbsRange { min: 0, max: 1 });
        d.feed(ev(EV_KEY, BTN_TOOL_RUBBER, 1));
        d.feed(ev(EV_KEY, BTN_TOUCH, 1));
        assert_eq!(d.feed(syn()).unwrap().tool, Tool::Eraser);
    }

    #[test]
    fn eraser_keeps_its_tool_through_up_and_leave() {
        let mut d = PenDecoder::new(AbsRange { min: 0, max: 1 });
        let evs = [
            ev(EV_KEY, BTN_TOOL_RUBBER, 1), syn(),
            ev(EV_KEY, BTN_TOUCH, 1), syn(),
            ev(EV_KEY, BTN_TOUCH, 0), ev(EV_KEY, BTN_TOOL_RUBBER, 0), syn(), // lifted and gone at once
            ev(EV_KEY, BTN_TOOL_RUBBER, 1), syn(),
            ev(EV_KEY, BTN_TOOL_RUBBER, 0), syn(),
        ];
        let got: Vec<_> = evs.iter().filter_map(|e| d.feed(*e)).map(|e| (e.action, e.tool)).collect();
        use PenAction::*;
        assert_eq!(got, [(HoverEnter, Tool::Eraser), (Down, Tool::Eraser), (Up, Tool::Eraser),
                         (HoverEnter, Tool::Eraser), (HoverLeave, Tool::Eraser)]);
    }

    #[test]
    fn syn_dropped_forces_release_and_resyncs() {
        let mut d = PenDecoder::new(AbsRange { min: 0, max: 1 });
        run(&mut d, &[ev(EV_KEY, BTN_TOOL_PEN, 1), ev(EV_KEY, BTN_TOUCH, 1), syn()]);
        let forced = d.feed(ev(EV_SYN, SYN_DROPPED, 0)).unwrap();
        assert_eq!(forced.action, PenAction::ForcedUp);
        // Events up to the next SYN_REPORT are discarded, then decoding resumes cleanly.
        assert_eq!(run(&mut d, &[ev(EV_ABS, ABS_X, 5), syn()]), []);
        assert_eq!(run(&mut d, &[ev(EV_KEY, BTN_TOOL_PEN, 1), syn()]), [PenAction::HoverEnter]);
    }

    #[test]
    fn resync_after_drop_restores_a_held_pen() {
        let mut d = PenDecoder::new(AbsRange { min: 0, max: 100 });
        run(&mut d, &[ev(EV_KEY, BTN_TOOL_BRUSH, 1), ev(EV_KEY, BTN_TOUCH, 1), syn()]);
        assert_eq!(d.feed(ev(EV_SYN, SYN_DROPPED, 0)).unwrap().action, PenAction::ForcedUp);
        assert!(!d.needs_resync(), "still inside the drop");
        let end = RawEvent { time_us: 77, ..syn() };
        assert_eq!(d.feed(end), None);
        assert!(d.needs_resync());
        // Without a resync the held pen would read as away; with it, a new stroke starts.
        let e = d.resync(Snapshot { x: 5, y: 6, pressure: 50, pen_tool: true, touching: true, ..Default::default() }).unwrap();
        assert_eq!((e.action, e.x, e.y, e.time_us), (PenAction::Down, 5, 6, 77));
        assert!((e.pressure - 0.5).abs() < 1e-6);
        assert!(!d.needs_resync());
        assert_eq!(run(&mut d, &[ev(EV_ABS, ABS_X, 9), syn()]), [PenAction::Move]);
    }

    #[test]
    fn resync_with_pen_away_is_silent() {
        let mut d = PenDecoder::new(AbsRange { min: 0, max: 1 });
        d.feed(ev(EV_SYN, SYN_DROPPED, 0));
        d.feed(syn());
        assert_eq!(d.resync(Snapshot::default()), None);
    }

    #[test]
    fn parses_input_event_bytes() {
        let mut b = [0u8; 24];
        b[0..8].copy_from_slice(&2i64.to_ne_bytes());
        b[8..16].copy_from_slice(&500i64.to_ne_bytes());
        b[16..18].copy_from_slice(&EV_ABS.to_ne_bytes());
        b[18..20].copy_from_slice(&ABS_PRESSURE.to_ne_bytes());
        b[20..24].copy_from_slice(&(-7i32).to_ne_bytes());
        assert_eq!(RawEvent::parse(&b), RawEvent { time_us: 2_000_500, kind: EV_ABS, code: ABS_PRESSURE, value: -7 });
    }
}
