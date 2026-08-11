use crate::pattern::Pattern;
use crate::step::{Accent, Slide, Time};

use super::error::MorphPlanError;

/// Cells in the stored pattern array, independent of how many are live.
pub const PATTERN_CELL_COUNT: usize = 16;

/// Source steps per beat on the straight grid.
pub const STEPS_PER_BEAT: usize = 4;

/// Active-step counts the morph supports: whole four-step beats.
pub const SUPPORTED_ACTIVE_STEPS: [u8; 4] = [4, 8, 12, 16];

/// One source cell interpreted with TD-3 step semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceBoundary {
    pub step: usize,
    pub time: Time,
    /// True for a Normal cell that starts a note attack.
    pub starts_attack: bool,
    /// Attack step continued by this cell when it is a sounding tie.
    pub continues_owner: Option<usize>,
    /// True for a Rest or TieRest boundary that silences a live chain.
    pub cuts_sounding: bool,
    /// True for a tie with no live chain to continue. Acoustically inert.
    pub orphan_tie: bool,
}

/// One note attack (a Normal cell) with its audible context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceAttack {
    pub step: usize,
    /// Effective pitch after transpose. Used only for comparison.
    pub pitch: i16,
    pub accent: bool,
    pub slide: bool,
    /// Terminal cell of the attack's tie chain.
    pub group_end: usize,
    pub prev_attack: Option<usize>,
    pub next_attack: Option<usize>,
}

/// Semantic normalization of a straight source pattern of 4, 8, 12, or
/// 16 active steps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcePhrase {
    /// Live source steps: four per beat.
    pub active_steps: usize,
    /// Whole beats spanned by the source, 1 through 4.
    pub beat_count: usize,
    pub boundaries: Vec<SourceBoundary>,
    pub attacks: Vec<SourceAttack>,
    /// Connected slide edges (from attack step, to attack step). An edge
    /// exists only when the MIDI timeline would perform a connected
    /// slide: the from-attack has slide on and the to-attack directly
    /// follows the from-attack's tie chain. A rest breaks the edge.
    pub slide_edges: Vec<(usize, usize)>,
}

impl SourcePhrase {
    pub fn attack(&self, step: usize) -> Option<&SourceAttack> {
        self.attacks.iter().find(|attack| attack.step == step)
    }
}

/// Interpret the live source cells as boundaries, attacks, and slide
/// edges.
///
/// Rejects native-triplet sources and active-step counts that are not
/// whole four-step beats. Cells past `active_steps` are ignored. Sounding
/// state starts silent at the bar: a leading tie is a source orphan and
/// does not cyclically continue the final attack of the previous loop.
pub fn normalize_source(pattern: &Pattern) -> Result<SourcePhrase, MorphPlanError> {
    pattern
        .validate()
        .map_err(|err| MorphPlanError::InvalidSource(err.to_string()))?;
    if pattern.triplet {
        return Err(MorphPlanError::NativeTripletEnabled);
    }
    if !SUPPORTED_ACTIVE_STEPS.contains(&pattern.active_steps) {
        return Err(MorphPlanError::UnsupportedActiveSteps(pattern.active_steps));
    }
    let active_steps = pattern.active_steps as usize;
    let beat_count = active_steps / STEPS_PER_BEAT;

    let steps = &pattern.step;

    let mut attacks: Vec<SourceAttack> = Vec::new();
    for step in 0..active_steps {
        if steps[step].time != Time::Normal {
            continue;
        }
        let mut group_end = step;
        while group_end + 1 < active_steps && steps[group_end + 1].time == Time::Tie {
            group_end += 1;
        }
        let cell = &steps[step];
        attacks.push(SourceAttack {
            step,
            pitch: cell.note as i16 + cell.transpose.pitch_base_offset() as i16,
            accent: cell.accent == Accent::On,
            slide: cell.slide == Slide::On,
            group_end,
            prev_attack: None,
            next_attack: None,
        });
    }
    for index in 0..attacks.len() {
        attacks[index].prev_attack = index.checked_sub(1).map(|prev| attacks[prev].step);
        attacks[index].next_attack = attacks.get(index + 1).map(|next| next.step);
    }

    let mut slide_edges: Vec<(usize, usize)> = Vec::new();
    for attack in &attacks {
        if attack.slide
            && attack.group_end + 1 < active_steps
            && steps[attack.group_end + 1].time == Time::Normal
        {
            slide_edges.push((attack.step, attack.group_end + 1));
        }
    }

    let mut boundaries: Vec<SourceBoundary> = Vec::with_capacity(active_steps);
    let mut live_owner: Option<usize> = None;
    for (step, cell) in steps.iter().enumerate().take(active_steps) {
        let time = cell.time;
        let mut boundary = SourceBoundary {
            step,
            time,
            starts_attack: false,
            continues_owner: None,
            cuts_sounding: false,
            orphan_tie: false,
        };
        match time {
            Time::Normal => {
                boundary.starts_attack = true;
                live_owner = Some(step);
            }
            Time::Tie => match live_owner {
                Some(owner) => boundary.continues_owner = Some(owner),
                None => boundary.orphan_tie = true,
            },
            Time::Rest | Time::TieRest => {
                boundary.cuts_sounding = live_owner.is_some();
                live_owner = None;
            }
        }
        boundaries.push(boundary);
    }

    Ok(SourcePhrase {
        active_steps,
        beat_count,
        boundaries,
        attacks,
        slide_edges,
    })
}
