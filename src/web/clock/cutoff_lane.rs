//! Per-step Filter Cutoff lane for device-sequenced (LIVE) playback.
//!
//! While the clock thread drives the device, the device sequencer steps
//! its pattern on every sixth MIDI Clock pulse (eighth in triplet mode).
//! The lane mirrors the timing of the pattern the device is playing and
//! emits Control Change 74 on the pulse that starts each step, just
//! before that pulse's clock byte, so the filter is in position when
//! the device sounds the step.
//!
//! The browser owns which pattern plays and hands the thread a
//! [`LaneRequest`] through the shared [`LaneInbox`]: applied at once for
//! the pattern already playing, or at the next cycle boundary for a
//! pattern the timeline has pre-loaded into the scratch slot.

use std::sync::{Arc, Mutex};

use crate::step::Step;
use crate::web::midi_channel::channel_status;

/// Controller number for Filter Cutoff.
pub const FILTER_CUTOFF_CC: u8 = 74;

/// Timing of the playing pattern plus, optionally, the cutoff value per
/// step. `values == None` keeps the cycle arithmetic alive without
/// sending anything, so a later boundary-applied lane lands on time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CutoffLane {
    pub values: Option<[u8; Step::COUNT]>,
    pub active_steps: u8,
    pub triplet: bool,
    pub channel: u8,
}

impl CutoffLane {
    /// MIDI Clock pulses per sequencer step: 6 straight, 8 triplet.
    pub fn pulses_per_step(&self) -> u64 {
        if self.triplet {
            8
        } else {
            6
        }
    }

    /// Pulses in one pass over the active steps.
    pub fn pulses_per_cycle(&self) -> u64 {
        self.pulses_per_step() * u64::from(self.active_steps.clamp(1, Step::COUNT as u8))
    }
}

/// A lane handed to the clock thread. `at_cycle_boundary` defers the
/// switch until the current lane's cycle completes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaneRequest {
    pub lane: Option<CutoffLane>,
    pub at_cycle_boundary: bool,
}

/// Single-slot mailbox from handlers to the clock thread. A newer request
/// replaces an unread older one.
pub type LaneInbox = Arc<Mutex<Option<LaneRequest>>>;

pub fn new_lane_inbox() -> LaneInbox {
    Arc::new(Mutex::new(None))
}

/// Cycle bookkeeping on the clock thread. Fed every pulse index in
/// order, starting from 0 at MIDI Start.
#[derive(Debug, Default)]
pub struct LaneTracker {
    current: Option<CutoffLane>,
    queued: Option<Option<CutoffLane>>,
    cycle_start_pulse: u64,
}

impl LaneTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Accept a request. Immediate requests replace the current lane and
    /// keep the cycle phase; boundary requests wait for the next cycle
    /// start. With no current lane a boundary request has no cycle to
    /// wait for and applies at once.
    pub fn accept(&mut self, request: LaneRequest) {
        if request.at_cycle_boundary && self.current.is_some() {
            self.queued = Some(request.lane);
        } else {
            self.current = request.lane;
            self.queued = None;
        }
    }

    /// Drain the inbox without blocking: a handler holding the lock for a
    /// moment must never stall the clock.
    pub fn poll_inbox(&mut self, inbox: &LaneInbox) {
        if let Ok(mut slot) = inbox.try_lock() {
            if let Some(request) = slot.take() {
                self.accept(request);
            }
        }
    }

    /// Advance to `pulse_index` and return the Control Change to send
    /// before this pulse's clock byte, if this pulse starts a step with a
    /// lane value.
    pub fn on_pulse(&mut self, pulse_index: u64) -> Option<[u8; 3]> {
        let lane = self.current?;
        let mut relative = pulse_index.saturating_sub(self.cycle_start_pulse);
        if relative >= lane.pulses_per_cycle() {
            self.cycle_start_pulse = pulse_index;
            relative = 0;
            if let Some(queued) = self.queued.take() {
                self.current = queued;
            }
        }
        let lane = self.current?;
        let pulses_per_step = lane.pulses_per_step();
        if !relative.is_multiple_of(pulses_per_step) {
            return None;
        }
        let step = (relative / pulses_per_step) as usize;
        let values = lane.values?;
        let value = *values.get(step)?;
        Some([
            channel_status(0xB0, lane.channel),
            FILTER_CUTOFF_CC,
            value & 0x7F,
        ])
    }

    #[cfg(test)]
    pub(crate) fn current(&self) -> Option<CutoffLane> {
        self.current
    }
}
