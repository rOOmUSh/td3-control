//! Turn per-row `CO`/`GT` fields into document lanes.
//!
//! A lane is present only when every active row carried a numeric value;
//! a missing or invalid value on any active row makes the lane absent,
//! so the document behaves as a v1 file for that lane. Rows beyond
//! `active_steps` never decide presence, but their values are kept when
//! present so a later `active_steps` increase finds them. The lane switch
//! comes from the header key when given; otherwise all active values
//! equal means off and any difference means on.

use crate::step;

use super::row::Field;

pub(super) struct LaneResolution {
    pub values: Option<[u8; step::Step::COUNT]>,
    pub lane_on: Option<bool>,
}

pub(super) fn resolve_lane(
    fields: &[Field; step::Step::COUNT],
    active_steps: usize,
    default: u8,
    header_switch: Option<bool>,
) -> LaneResolution {
    let active = active_steps.clamp(1, step::Step::COUNT);
    let complete = fields[..active]
        .iter()
        .all(|field| matches!(field, Field::Value(_)));
    if !complete {
        return LaneResolution {
            values: None,
            lane_on: None,
        };
    }
    let mut values = [default; step::Step::COUNT];
    for (slot, field) in values.iter_mut().zip(fields.iter()) {
        if let Field::Value(v) = field {
            *slot = *v;
        }
    }
    let lane_on = header_switch.or_else(|| {
        let first = values[0];
        Some(values[..active].iter().any(|v| *v != first))
    });
    LaneResolution {
        values: Some(values),
        lane_on,
    }
}
