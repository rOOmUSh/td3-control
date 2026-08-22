use serde::{Deserialize, Serialize};

use crate::formats::steps_txt::{StepsTxtExportMeta, StepsTxtMeta};

// ---------------------------------------------------------------------------
// StepDSL v1.1 metadata on the wire
// ---------------------------------------------------------------------------

/// Per-step lanes and page state carried by a `.steps.txt` document.
/// Sent by the browser on export (every field optional; an absent field
/// exports its default) and returned on import (a field is absent when
/// the document did not carry it or carried it unusably).
#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WebStepsMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_cutoffs: Option<Vec<u32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_gates: Option<Vec<u32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cutoff_lane_on: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_lane_on: Option<bool>,
    /// Present means `triplet_morph=on` with this percentage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub triplet_morph_percent: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_update: Option<bool>,
}

impl WebStepsMeta {
    pub fn from_document(meta: &StepsTxtMeta) -> Self {
        Self {
            step_cutoffs: meta
                .cutoff
                .map(|values| values.iter().map(|v| u32::from(*v)).collect()),
            step_gates: meta
                .gate
                .map(|values| values.iter().map(|v| u32::from(*v)).collect()),
            cutoff_lane_on: meta.cutoff_lane_on,
            gate_lane_on: meta.gate_lane_on,
            triplet_morph_percent: meta.triplet_morph_percent.map(u32::from),
            live_update: meta.live_update,
        }
    }

    /// True when no field is set, so the export uses the defaults.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Convert to export metadata. Lane lists must hold exactly 16 values
    /// within range; an absent lane keeps its default.
    pub fn to_export_meta(&self) -> Result<StepsTxtExportMeta, String> {
        let mut out = StepsTxtExportMeta::default();
        if let Some(cutoffs) = &self.step_cutoffs {
            out.cutoff = lane_values(cutoffs, "stepCutoffs", 0, 127)?;
        }
        if let Some(gates) = &self.step_gates {
            out.gate = lane_values(gates, "stepGates", 1, 100)?;
        }
        out.cutoff_lane_on = self.cutoff_lane_on.unwrap_or(false);
        out.gate_lane_on = self.gate_lane_on.unwrap_or(false);
        out.triplet_morph_percent = match self.triplet_morph_percent {
            Some(percent) if percent > 100 => {
                return Err(format!(
                    "tripletMorphPercent must be 0-100, got {}",
                    percent
                ))
            }
            Some(0) => None,
            Some(percent) => Some(percent as u8),
            None => None,
        };
        out.live_update = self.live_update.unwrap_or(false);
        Ok(out)
    }
}

/// Exactly 16 values, each within `min..=max`.
pub fn lane_values(values: &[u32], name: &str, min: u32, max: u32) -> Result<[u8; 16], String> {
    if values.len() != 16 {
        return Err(format!(
            "{} must have exactly 16 values, got {}",
            name,
            values.len()
        ));
    }
    let mut out = [0u8; 16];
    for (i, value) in values.iter().enumerate() {
        if *value < min || *value > max {
            return Err(format!(
                "{}[{}] must be {}-{}, got {}",
                name, i, min, max, value
            ));
        }
        out[i] = *value as u8;
    }
    Ok(out)
}
