//! Writer. Always emits the v1.1 tag, the full header, and `CO`/`GT` on
//! every active row.

use std::fmt::Write;

use crate::pattern::Pattern;
use crate::step;

use super::super::note_name;
use super::{StepsTxtExportMeta, STEPDSL_TAG_V1_1};

fn on_off(value: bool) -> &'static str {
    if value {
        "on"
    } else {
        "off"
    }
}

pub(super) fn render(pattern: &Pattern, bpm: Option<&str>, meta: &StepsTxtExportMeta) -> String {
    let mut out = String::new();
    writeln!(&mut out, "format={STEPDSL_TAG_V1_1}").ok();
    writeln!(&mut out, "active_steps={}", pattern.active_steps).ok();
    writeln!(&mut out, "triplet_time={}", on_off(pattern.triplet)).ok();
    writeln!(
        &mut out,
        "triplet_morph={}",
        on_off(meta.triplet_morph_percent.is_some())
    )
    .ok();
    writeln!(
        &mut out,
        "triplet_morph_percentage={}",
        meta.triplet_morph_percent.unwrap_or(0).min(100)
    )
    .ok();
    if let Some(value) = bpm {
        writeln!(&mut out, "bpm={value}").ok();
    }
    writeln!(&mut out, "live_update={}", on_off(meta.live_update)).ok();
    writeln!(&mut out, "pattern_co_lane={}", on_off(meta.cutoff_lane_on)).ok();
    writeln!(&mut out, "pattern_gt_lane={}", on_off(meta.gate_lane_on)).ok();
    writeln!(&mut out).ok();

    let row_count = usize::from(pattern.active_steps).min(step::Step::COUNT);
    for idx in 0..row_count {
        let current = &pattern.step[idx];
        writeln!(
            &mut out,
            "{:02} {:>2}:{}{}{}:{}|CO:{}|GT:{}",
            idx + 1,
            note_name(current.note),
            current.transpose.steps_symbol() as char,
            current.accent.steps_symbol() as char,
            current.slide.steps_symbol() as char,
            current.time.steps_token(),
            meta.cutoff[idx].min(127),
            meta.gate[idx].clamp(1, 100),
        )
        .ok();
    }

    writeln!(&mut out).ok();
    writeln!(&mut out, "# NOTE:TAS:TIME|CO:cutoff|GT:gate").ok();
    writeln!(&mut out, "# transpose: U|D|-").ok();
    writeln!(&mut out, "# accent: A|-").ok();
    writeln!(&mut out, "# slide: S|-").ok();
    writeln!(&mut out, "# time: N|T|R|TR").ok();
    writeln!(&mut out, "# Cutoff Control | CO:0-127").ok();
    writeln!(&mut out, "# Gate Control | GT:1-100").ok();
    writeln!(
        &mut out,
        "# Lanes | pattern_co_lane, pattern_gt_lane: on/off"
    )
    .ok();
    writeln!(&mut out, "# Live Update | live_update: on/off").ok();

    out
}
