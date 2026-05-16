//! Pure duplicate-detection helpers.
//!
//! - `pattern_hash` → stable SHA-256 of the normalized pattern bytes.
//! - `rhythm_fingerprint` → hex of step-gate states, used to detect
//!   near-duplicates sharing the same groove.
//! - `is_near_duplicate` → same rhythm + ≤3 note changes.
//! - `compute_clusters` → scan the library store, grouping items into exact
//!   + near duplicate clusters based on cached pattern sidecars.
//!
//! The `/api/bank/duplicates` handler loads items from the store, decodes
//! their sidecar payloads, and returns one `DuplicateCluster` row per
//! grouping.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::Td3Error;
use crate::pattern::{sysex_to_pattern, Pattern};
use crate::step;

use super::model::DuplicateStatus;
use super::store::LibraryStore;

/// Compute a SHA-256 hash over the canonical byte serialization of a pattern
/// (note index + accent + slide + transpose + time per step, plus triplet +
/// active-steps). This is stable across runs.
pub fn pattern_hash(pat: &Pattern) -> String {
    let mut hasher = Sha256::new();
    hasher.update([pat.triplet as u8, pat.active_steps]);
    for s in pat.step.iter() {
        hasher.update([
            s.note,
            s.transpose as u8,
            s.accent as u8,
            s.slide as u8,
            s.time as u8,
        ]);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for b in digest.iter() {
        hex.push_str(&format!("{:02x}", b));
    }
    hex
}

/// 16-char hex fingerprint of the step-gate / accent rhythm - one nibble per
/// step packing `(time << 2) | (accent << 1) | slide`.
pub fn rhythm_fingerprint(pat: &Pattern) -> String {
    let mut s = String::with_capacity(16);
    for step in pat.step.iter() {
        let nib = ((step.time as u8 & 0b11) << 2)
            | ((step.accent as u8 & 1) << 1)
            | (step.slide as u8 & 1);
        s.push_str(&format!("{:x}", nib & 0x0F));
    }
    s
}

/// Near-duplicate: same rhythm fingerprint + at most `max_note_changes` step
/// notes differ (the default is 3).
pub fn is_near_duplicate(a: &Pattern, b: &Pattern, max_note_changes: u32) -> bool {
    if rhythm_fingerprint(a) != rhythm_fingerprint(b) {
        return false;
    }
    let mut diffs = 0u32;
    for i in 0..16 {
        if note_sig(a.step[i]) != note_sig(b.step[i]) {
            diffs += 1;
            if diffs > max_note_changes {
                return false;
            }
        }
    }
    diffs <= max_note_changes
}

fn note_sig(s: step::Step) -> (u8, u8) {
    (s.note, s.transpose as u8)
}

// ---------------------------------------------------------------------------
// Cluster detection
// ---------------------------------------------------------------------------

/// Classification of a duplicate cluster.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DuplicateClusterKind {
    /// Every item in the cluster shares the same `pattern_hash` (byte-exact).
    Exact,
    /// Items share a rhythm fingerprint and are pairwise "near duplicates"
    /// (≤3 note edits), but are not byte-exact duplicates.
    Near,
}

/// A cluster returned by `compute_clusters`. The cluster is always sorted by
/// `item_id` for determinism so the UI can render a stable list order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateCluster {
    pub cluster_id: String,
    pub kind: DuplicateClusterKind,
    pub item_ids: Vec<String>,
    /// The "representative" for the cluster - conventionally the lowest
    /// `item_id` (which roughly corresponds to the earliest-imported). Useful
    /// for "keep this one" UI actions.
    pub representative_id: String,
    /// One-line user-readable reasons for the grouping (e.g. "byte-exact
    /// duplicate", "same rhythm, 2 note edits").
    pub reasons: Vec<String>,
}

/// Compute all duplicate clusters present in the catalog. Returns clusters in
/// deterministic order: all exact clusters first (sorted by representative
/// item_id), then near clusters.
///
/// Items whose sidecar is missing or corrupt are simply omitted from clustering
/// - we never fabricate a cluster reason.
pub fn compute_clusters(store: &LibraryStore) -> Result<Vec<DuplicateCluster>, Td3Error> {
    // Load every item + attempt to load its pattern. The default filter has
    // `archived: None` which matches both archived and non-archived items,
    // so archival state does not skew duplicate detection.
    let items = store.list_items(&super::filter::ItemFilter::default())?;

    // (item_id, Pattern) for every item with a usable sidecar.
    let mut decoded: Vec<(String, Pattern)> = Vec::with_capacity(items.len());
    for item in &items {
        let Some(payload) = store.pattern_bytes_for(&item.item_id) else {
            continue;
        };
        if payload.len() != 112 {
            continue;
        }
        // `sysex_to_pattern` expects a 115-byte SysEx body: kind + group + slot
        // + 112-byte payload. Reconstitute using neutral group/slot values.
        let mut sysex = Vec::with_capacity(115);
        sysex.push(0x78);
        sysex.push(0x00);
        sysex.push(0x00);
        sysex.extend_from_slice(&payload);
        let Ok(pat) = sysex_to_pattern(&sysex) else {
            continue;
        };
        decoded.push((item.item_id.clone(), pat));
    }

    let mut clusters: Vec<DuplicateCluster> = Vec::new();

    // --- Exact clusters: group by pattern_hash. ---
    let mut by_hash: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (id, pat) in &decoded {
        by_hash
            .entry(pattern_hash(pat))
            .or_default()
            .push(id.clone());
    }
    // Remember which item_ids fell into an exact cluster so they don't
    // also show up in near clusters.
    let mut in_exact: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (_hash, mut members) in by_hash {
        if members.len() < 2 {
            continue;
        }
        members.sort();
        for id in &members {
            in_exact.insert(id.clone());
        }
        let representative_id = members[0].clone();
        clusters.push(DuplicateCluster {
            cluster_id: format!("dup-exact-{}", representative_id),
            kind: DuplicateClusterKind::Exact,
            item_ids: members,
            representative_id,
            reasons: vec!["byte-exact duplicate pattern".into()],
        });
    }

    // --- Near clusters: group by rhythm_fingerprint, prune exact-only rows,
    // then keep groups where every pair passes `is_near_duplicate`. ---
    // We store indices into `decoded` (instead of cloning Patterns, which
    // isn't `Clone`) and look them up by index when needed.
    let mut by_rhythm: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (idx, (id, pat)) in decoded.iter().enumerate() {
        if in_exact.contains(id) {
            continue;
        }
        by_rhythm
            .entry(rhythm_fingerprint(pat))
            .or_default()
            .push(idx);
    }
    for (_rhythm, member_indices) in by_rhythm {
        if member_indices.len() < 2 {
            continue;
        }
        // Simple connectivity: use the first as anchor, collect any member
        // within edit budget of the anchor.
        let anchor_idx = member_indices[0];
        let anchor_pat = &decoded[anchor_idx].1;
        let mut group_indices: Vec<usize> = vec![anchor_idx];
        for &idx in &member_indices[1..] {
            if is_near_duplicate(anchor_pat, &decoded[idx].1, 3) {
                group_indices.push(idx);
            }
        }
        if group_indices.len() < 2 {
            continue;
        }
        let mut ids: Vec<String> = group_indices
            .iter()
            .map(|&i| decoded[i].0.clone())
            .collect();
        ids.sort();
        let representative_id = ids[0].clone();
        // Summarise the worst-case edit distance to the anchor.
        let max_edits: u32 = group_indices
            .iter()
            .skip(1)
            .map(|&i| count_note_edits(anchor_pat, &decoded[i].1))
            .max()
            .unwrap_or(0);
        clusters.push(DuplicateCluster {
            cluster_id: format!("dup-near-{}", representative_id),
            kind: DuplicateClusterKind::Near,
            item_ids: ids,
            representative_id,
            reasons: vec![format!(
                "same rhythm, up to {} note edit(s) between members",
                max_edits
            )],
        });
    }

    Ok(clusters)
}

/// Best-effort write-through of duplicate statuses derived from `clusters`.
/// Items appearing in an exact cluster get `ExactDuplicate`, items in a near
/// cluster get `NearDuplicate`; items mentioned in neither stay `Unique` (if
/// they have a sidecar and were decoded - otherwise the handler can't know,
/// so we leave the field as-is).
pub fn statuses_from_clusters(
    clusters: &[DuplicateCluster],
    all_item_ids: &[String],
    decoded_ids: &[String],
) -> Vec<(String, DuplicateStatus)> {
    use std::collections::HashMap;
    let mut map: HashMap<String, DuplicateStatus> = HashMap::new();
    let decoded_set: std::collections::HashSet<&str> =
        decoded_ids.iter().map(|s| s.as_str()).collect();

    for id in all_item_ids {
        // Only downgrade to Unique for items we actually decoded; others stay
        // Unknown so the UI doesn't misreport never-decoded items.
        if decoded_set.contains(id.as_str()) {
            map.insert(id.clone(), DuplicateStatus::Unique);
        }
    }
    for c in clusters {
        let s = match c.kind {
            DuplicateClusterKind::Exact => DuplicateStatus::ExactDuplicate,
            DuplicateClusterKind::Near => DuplicateStatus::NearDuplicate,
        };
        for id in &c.item_ids {
            map.insert(id.clone(), s);
        }
    }
    map.into_iter().collect()
}

fn count_note_edits(a: &Pattern, b: &Pattern) -> u32 {
    let mut diffs = 0u32;
    for i in 0..16 {
        if note_sig(a.step[i]) != note_sig(b.step[i]) {
            diffs += 1;
        }
    }
    diffs
}
