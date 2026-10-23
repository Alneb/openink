//! The refresh engine: requests in, panel updates out (docs/EPDC-ENGINE.md).
//!
//! Pure and clock-free: every call takes the caller's time, `poll` returns the updates due now and
//! `next_deadline` says when to poll again. The same engine drives `View.refreshScreen` on a stock
//! device (Java executes the updates) and `/dev/ebc` on a flashed one.

use std::num::NonZeroU32;

use openink_core::{ModeFlags, UpdateMode, Waveform};

use crate::geom::{Millis, Rect};
use crate::intent::{ghost_effect, quality, GhostEffect, Intent};
use crate::ledger::GhostLedger;

/// Tunables. Defaults are the measured proposal (docs/EPDC-ENGINE.md); every one is a named field, none is buried.
#[derive(Clone, Debug)]
pub struct Policy {
    /// Ghost-ledger cell size in pixels.
    pub tile: i32,
    /// Pending updates of the same mode closer than this (px) merge into one.
    pub merge_gap: i32,
    /// A burst of requests is collected for this long before it is sent (coalescing window).
    pub coalesce_ms: u64,
    /// After pen-up, wait this long for the next stroke before repainting (handwriting rhythm).
    pub pen_up_repaint_ms: u64,
    /// A request deferred by writing longer than this is sent at the next pen-up without waiting.
    pub max_defer_ms: u64,
    /// Ledger cost (A2-frame equivalents) at which a tile needs a cleanup flash.
    pub ghost_threshold: f32,
    /// Cleanup only after the screen has been quiet this long (never mid-interaction).
    pub idle_cleanup_ms: u64,
    /// A cleanup covering at least this fraction of the screen becomes a full-screen flash.
    pub full_screen_fraction: f32,
    /// Stock `SET_GC_REFRESH_INTERVAL` compatibility: every Nth partial update is sent as a full GC.
    pub gc_every: Option<NonZeroU32>,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            tile: 64,
            merge_gap: 16,
            coalesce_ms: 25,
            pen_up_repaint_ms: 500,
            max_defer_ms: 3000,
            ghost_threshold: 6.0,
            idle_cleanup_ms: 1500,
            full_screen_fraction: 0.5,
            gc_every: None,
        }
    }
}

/// Why an update was sent; carried with every update so logs explain the panel's behaviour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// A caller's request (possibly merged with others).
    Request,
    /// A request promoted to a full GC by `Policy::gc_every`.
    GcInterval,
    /// The ghost ledger crossed `ghost_threshold` and the screen was idle.
    Cleanup,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Update {
    pub rect: Rect,
    pub mode: UpdateMode,
    pub reason: Reason,
}

#[derive(Clone, Copy, Debug)]
struct Pending {
    rect: Rect,
    mode: UpdateMode,
    since: Millis,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pen {
    Idle,
    Contact,
    /// Lifted at this time; the next stroke may follow within `pen_up_repaint_ms`.
    Lifted(Millis),
}

pub struct Engine {
    screen: Rect,
    policy: Policy,
    pending: Vec<Pending>,
    ledger: GhostLedger,
    pen: Pen,
    /// Last request, pen event or update: cleanup waits for quiet.
    last_activity: Millis,
    partials_since_gc: u32,
    /// App-scope mode (stock APPLY_APP_SCOPE_UPDATE): replaces the mode of every non-raw request.
    scope_mode: Option<UpdateMode>,
}

impl Engine {
    pub fn new(screen: Rect, policy: Policy) -> Self {
        let ledger = GhostLedger::new(screen, policy.tile);
        Self { screen, policy, pending: Vec::new(), ledger, pen: Pen::Idle, last_activity: Millis(0), partials_since_gc: 0, scope_mode: None }
    }

    pub fn screen(&self) -> Rect {
        self.screen
    }

    pub fn policy(&self) -> &Policy {
        &self.policy
    }

    pub fn set_gc_every(&mut self, n: Option<NonZeroU32>) {
        self.policy.gc_every = n;
        self.partials_since_gc = 0;
    }

    pub fn set_scope_mode(&mut self, m: Option<UpdateMode>) {
        self.scope_mode = m;
    }

    /// Queue a refresh of `rect` (clipped to the screen; off-screen requests are dropped).
    pub fn request(&mut self, rect: Rect, intent: Intent, now: Millis) {
        let Some(rect) = rect.intersect(&self.screen) else { return };
        let mode = match (intent, self.scope_mode) {
            (Intent::Raw(_) | Intent::Clean, _) | (_, None) => intent.mode(),
            (_, Some(m)) => m,
        };
        self.last_activity = now;
        self.enqueue(Pending { rect, mode, since: now });
    }

    /// Merge into the queue without reordering pixels: a request may join an older entry of the
    /// same mode only if no later entry of another mode touches it in between.
    fn enqueue(&mut self, mut new: Pending) {
        let gap = self.policy.merge_gap;
        let mut i = self.pending.len();
        while i > 0 {
            i -= 1;
            let e = self.pending[i];
            if e.mode == new.mode && e.rect.near(&new.rect, gap) {
                new = Pending { rect: e.rect.union(&new.rect), mode: e.mode, since: e.since.min(new.since) };
                self.pending.remove(i);
            } else if e.rect.intersect(&new.rect).is_some() {
                if new.rect.contains(&e.rect) && quality(new.mode) >= quality(e.mode) {
                    new.since = new.since.min(e.since);
                    self.pending.remove(i); // fully redrawn later by an equal-or-better waveform
                } else {
                    break; // order matters from here on
                }
            }
        }
        self.pending.push(new);
    }

    pub fn pen_down(&mut self, now: Millis) {
        self.pen = Pen::Contact;
        self.last_activity = now;
    }

    /// `stroke` is the ink's bounding box (already inflated by half the width), repainted with
    /// `Intent::Ink` once writing pauses.
    pub fn pen_up(&mut self, stroke: Option<Rect>, now: Millis) {
        self.pen = Pen::Lifted(now);
        self.last_activity = now;
        if let Some(r) = stroke {
            self.request(r, Intent::Ink, now);
        }
    }

    /// When the queued work may go out, or `None` while writing holds it.
    fn ready_at(&self) -> Option<Millis> {
        let oldest = self.pending.iter().map(|p| p.since).min()?;
        let coalesced = oldest.plus(self.policy.coalesce_ms);
        match self.pen {
            Pen::Contact => None,
            Pen::Idle => Some(coalesced),
            Pen::Lifted(at) => {
                let waited_too_long = at.since(oldest) >= self.policy.max_defer_ms;
                Some(if waited_too_long { coalesced.max(at) } else { coalesced.max(at.plus(self.policy.pen_up_repaint_ms)) })
            }
        }
    }

    fn cleanup_at(&self) -> Option<Millis> {
        let writing = matches!(self.pen, Pen::Contact);
        if writing || !self.pending.is_empty() || self.ledger.max() < self.policy.ghost_threshold {
            return None;
        }
        let mut t = self.last_activity.plus(self.policy.idle_cleanup_ms);
        if let Pen::Lifted(at) = self.pen {
            t = t.max(at.plus(self.policy.pen_up_repaint_ms));
        }
        Some(t)
    }

    /// The earliest time `poll` will have something to do.
    pub fn next_deadline(&self) -> Option<Millis> {
        match (self.ready_at(), self.cleanup_at()) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// Everything queued, now, skipping the coalescing and pen-up waits (`waitForUpdateFinished`).
    /// Still nothing while the pen touches: the stroke is never overdrawn.
    pub fn flush(&mut self, now: Millis) -> Vec<Update> {
        if matches!(self.pen, Pen::Contact) {
            return Vec::new();
        }
        self.drain(now)
    }

    fn drain(&mut self, now: Millis) -> Vec<Update> {
        let mut out = Vec::new();
        for p in std::mem::take(&mut self.pending) {
            let u = self.promote(Update { rect: p.rect, mode: p.mode, reason: Reason::Request });
            self.ledger.apply(&u.rect, ghost_effect(u.mode));
            out.push(u);
        }
        if !out.is_empty() {
            self.last_activity = now;
            if let Pen::Lifted(_) = self.pen {
                self.pen = Pen::Idle;
            }
        }
        out
    }

    /// The updates due at `now`, in the order they must reach the panel.
    pub fn poll(&mut self, now: Millis) -> Vec<Update> {
        let mut out = Vec::new();
        if self.ready_at().is_some_and(|t| t <= now) {
            out = self.drain(now);
        } else if self.cleanup_at().is_some_and(|t| t <= now) {
            if let Some(dirty) = self.ledger.dirty_bounds(self.policy.ghost_threshold) {
                let full = dirty.area() as f32 >= self.policy.full_screen_fraction * self.screen.area() as f32;
                let rect = if full { self.screen } else { dirty };
                let u = Update { rect, mode: Intent::Clean.mode(), reason: Reason::Cleanup };
                self.ledger.apply(&rect, GhostEffect::Reset);
                self.last_activity = now;
                out.push(u);
            }
        }
        out
    }

    /// Stock GC-interval behaviour: count partial updates, send every Nth as a full GC16.
    fn promote(&mut self, u: Update) -> Update {
        let Some(n) = self.policy.gc_every else { return u };
        if ghost_effect(u.mode) == GhostEffect::Reset {
            self.partials_since_gc = 0;
            return u;
        }
        self.partials_since_gc += 1;
        if self.partials_since_gc < n.get() {
            return u;
        }
        self.partials_since_gc = 0;
        Update { rect: u.rect, mode: UpdateMode::new(Waveform::Gc16, ModeFlags::FULL), reason: Reason::GcInterval }
    }
}
