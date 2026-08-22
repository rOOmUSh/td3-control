//! Document parser: header lines, step rows, completeness checks, and
//! lane resolution.

use crate::error::Td3Error;
use crate::pattern::Pattern;
use crate::step;

use super::header::Header;
use super::lanes::resolve_lane;
use super::row::{parse_row, Field};
use super::{StepsTxtDocument, StepsTxtMeta, DEFAULT_CUTOFF, DEFAULT_GATE};

pub(super) fn import_document(data: &str) -> Result<StepsTxtDocument, Td3Error> {
    let mut header = Header::default();
    let mut steps: [step::Step; step::Step::COUNT] = Default::default();
    let mut seen = [false; step::Step::COUNT];
    let mut cutoff = [Field::Absent; step::Step::COUNT];
    let mut gate = [Field::Absent; step::Step::COUNT];

    for (line_idx, raw_line) in data.lines().enumerate() {
        let line_num = line_idx + 1;
        // Only a line starting with '#' (after trimming) is a comment.
        // Inline '#' is data: note names like C# contain it.
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if header.apply(raw_line, line, line_num)? {
            continue;
        }

        let row = parse_row(line, line_num)?;
        let slot = row.index - 1;
        if seen[slot] {
            return Err(Td3Error::FormatError(format!(
                "line {}: duplicate step index: {}",
                line_num, row.index
            )));
        }
        steps[slot] = row.step;
        cutoff[slot] = row.cutoff;
        gate[slot] = row.gate;
        seen[slot] = true;
    }

    let declared_active_steps = header.active_steps.unwrap_or(16);
    let triplet = header.triplet.unwrap_or(false);
    if !(1..=step::Step::COUNT as u8).contains(&declared_active_steps) {
        return Pattern::new(triplet, declared_active_steps, steps).map(|pattern| {
            StepsTxtDocument {
                pattern,
                centibpm: header.centibpm,
                meta: StepsTxtMeta::default(),
            }
        });
    }

    let active_range = usize::from(declared_active_steps);
    if seen[..active_range].iter().any(|present| !present) {
        let missing: Vec<u8> = seen
            .iter()
            .take(active_range)
            .enumerate()
            .filter_map(|(idx, present)| {
                if *present {
                    None
                } else {
                    Some((idx + 1) as u8)
                }
            })
            .collect();
        return Err(Td3Error::FormatError(format!(
            "missing steps: {:?}",
            missing
        )));
    }

    let pattern = Pattern::new(triplet, declared_active_steps, steps)?;

    let cutoff_lane = resolve_lane(&cutoff, active_range, DEFAULT_CUTOFF, header.cutoff_lane_on);
    let gate_lane = resolve_lane(&gate, active_range, DEFAULT_GATE, header.gate_lane_on);
    let triplet_morph_percent = match (header.triplet_morph, header.triplet_morph_percent) {
        (Some(true), Some(percent)) => Some(percent),
        _ => None,
    };

    let meta = StepsTxtMeta {
        centibpm: header.centibpm,
        cutoff: cutoff_lane.values,
        gate: gate_lane.values,
        cutoff_lane_on: cutoff_lane.lane_on,
        gate_lane_on: gate_lane.lane_on,
        triplet_morph_percent,
        live_update: header.live_update,
    };

    Ok(StepsTxtDocument {
        pattern,
        centibpm: header.centibpm,
        meta,
    })
}
