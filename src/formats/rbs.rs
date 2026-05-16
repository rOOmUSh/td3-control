//! Propellerhead ReBirth RB-338 `.rbs` song file - reader & writer.
//!
//! An `.rbs` song embeds two TB-303 synth chunks, each holding 32 patterns
//! (4 banks × 8 slots). We treat the file as a flat 64-pattern bank:
//!
//!   Device 1 (first `303 ` chunk)  → "A-side"  G1P1A..G4P8A
//!   Device 2 (second `303 ` chunk) → "B-side"  G1P1B..G4P8B
//!
//! Within each device, records are ordered bank-major: record
//! `group * 8 + slot` (both 0-indexed) maps to TD-3 group `group+1`,
//! slot `slot+1` on that side.
//!
//! Non-303 chunks (HEAD, GLOB, MIXR, DELY, DIST, COMP, 808, 909, TRAK, …)
//! are opaque to us. On serialize we start from a template (either the
//! source file's own bytes for round-trip, or a bundled default for fresh
//! writes) and rewrite *only* the 32 pattern-record slots inside each
//! `303 ` chunk - everything else (tempo, mixer, arrangement, …) survives
//! verbatim.
//!

use std::convert::TryInto;

use crate::error::Td3Error;
use crate::formats::rbs_codec::{
    decode_step, encode_step_sequence, normalize_decoded_tie_runs, DecodeCarry,
};
use crate::pattern::Pattern;
use crate::step::{Step, Time};

// ---------------------------------------------------------------------------
// Layout constants
// ---------------------------------------------------------------------------

/// IFF chunk ID for a TB-303 device (4 ASCII bytes, trailing space).
const CHUNK_303: &[u8; 4] = b"303 ";

/// Expected active-step count in every observed fixture.
const DEFAULT_ACTIVE_STEPS: u8 = 16;

pub const DEVICES: usize = 2;
pub const GROUPS_PER_DEVICE: usize = 4;
pub const SLOTS_PER_GROUP: usize = 8;
pub const SLOTS_PER_DEVICE: usize = GROUPS_PER_DEVICE * SLOTS_PER_GROUP; // 32
pub const TOTAL_SLOTS: usize = SLOTS_PER_DEVICE * DEVICES; // 64
pub const STEPS_PER_PATTERN: usize = 16;

/// Bytes per 34-byte pattern record: 2-byte header + 16 × 2-byte steps.
pub const RECORD_LEN: usize = 2 + STEPS_PER_PATTERN * 2;

/// Bytes of synth-knob config preceding the 32 pattern records inside a `303 ` chunk.
pub const CONFIG_LEN: usize = 9;

/// Expected payload size of a `303 ` chunk.
pub const CHUNK_303_PAYLOAD_LEN: usize = CONFIG_LEN + SLOTS_PER_DEVICE * RECORD_LEN;

/// Bundled default template (a verbatim copy of `docs/JAM PATTERN.rbs`).
/// Used when a fresh `.rbs` is written without an explicit template.
pub const DEFAULT_TEMPLATE: &[u8] = include_bytes!("rbs_template.rbs");

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

/// A parsed `.rbs` song as a 64-pattern bank plus the raw template bytes.
///
/// `patterns[device * 32 + group * 8 + slot]` addresses a single slot.
/// Use [`index_for`] to compute the flat index from `(device, group, slot)`.
#[derive(Debug)]
pub struct RbsSong {
    template: Vec<u8>,
    patterns: Vec<Pattern>,
}

impl RbsSong {
    /// Build a blank song using the bundled template and 64 silent patterns.
    pub fn blank() -> Result<Self, Td3Error> {
        let mut patterns = Vec::with_capacity(TOTAL_SLOTS);
        for _ in 0..TOTAL_SLOTS {
            patterns.push(silent_pattern()?);
        }
        Ok(Self {
            template: DEFAULT_TEMPLATE.to_vec(),
            patterns,
        })
    }

    /// Parse `.rbs` bytes. Keeps the source bytes as the serialize template
    /// so round-tripping preserves every non-303 chunk.
    pub fn parse(data: &[u8]) -> Result<Self, Td3Error> {
        let chunks = find_303_chunks(data)?;
        let mut patterns = Vec::with_capacity(TOTAL_SLOTS);
        for (device_idx, &chunk_off) in chunks.iter().take(DEVICES).enumerate() {
            let payload = chunk_payload(data, chunk_off)?;
            for rec_idx in 0..SLOTS_PER_DEVICE {
                let off = CONFIG_LEN + rec_idx * RECORD_LEN;
                let rec = &payload[off..off + RECORD_LEN];
                patterns.push(decode_record(rec, device_idx, rec_idx)?);
            }
        }
        Ok(Self {
            template: data.to_vec(),
            patterns,
        })
    }

    /// Serialize the song back to `.rbs` bytes by cloning the template and
    /// overwriting the 32 pattern records inside each `303 ` chunk.
    pub fn serialize(&self) -> Result<Vec<u8>, Td3Error> {
        if self.patterns.len() != TOTAL_SLOTS {
            return Err(Td3Error::FormatError(format!(
                ".rbs song has {} patterns (expected {})",
                self.patterns.len(),
                TOTAL_SLOTS
            )));
        }
        let chunks = find_303_chunks(&self.template)?;
        let mut out = self.template.clone();

        for (device_idx, &chunk_off) in chunks.iter().take(DEVICES).enumerate() {
            // Validate payload size *after* we know the chunk offset so any
            // template corruption is reported here, not via silent overflow.
            let _ = chunk_payload(&out, chunk_off)?;
            for rec_idx in 0..SLOTS_PER_DEVICE {
                let flat = device_idx * SLOTS_PER_DEVICE + rec_idx;
                let encoded = encode_record(&self.patterns[flat])?;
                let payload_start = chunk_off + 8 + CONFIG_LEN;
                let dst_start = payload_start + rec_idx * RECORD_LEN;
                out[dst_start..dst_start + RECORD_LEN].copy_from_slice(&encoded);
            }
        }
        Ok(out)
    }

    /// Access the flat pattern array (length == `TOTAL_SLOTS`).
    pub fn patterns(&self) -> &[Pattern] {
        &self.patterns
    }

    /// Read a single slot by address.
    #[allow(dead_code)]
    pub fn pattern_at(&self, device: usize, group: usize, slot: usize) -> &Pattern {
        &self.patterns[index_for(device, group, slot)]
    }

    /// Replace a single slot by address.
    pub fn set_pattern(&mut self, device: usize, group: usize, slot: usize, pattern: Pattern) {
        self.patterns[index_for(device, group, slot)] = pattern;
    }

    /// ReBirth empty-slot signature: any step whose transpose byte has both
    /// UP (0x04) and DOWN (0x08) bits set. Canonical encoders never emit this
    /// combination - ReBirth uses pseudo-random padding for unused slots, so a
    /// single `0x0C` mask on any step proves the record is factory padding
    /// rather than authored content.
    pub fn has_padding_signature(&self, flat_index: usize) -> bool {
        if flat_index >= TOTAL_SLOTS {
            return false;
        }
        let chunks = match find_303_chunks(&self.template) {
            Ok(c) => c,
            Err(_) => return false,
        };
        let device_idx = flat_index / SLOTS_PER_DEVICE;
        let rec_idx = flat_index % SLOTS_PER_DEVICE;
        let payload_start = chunks[device_idx] + 8 + CONFIG_LEN;
        let rec_start = payload_start + rec_idx * RECORD_LEN;
        let rec_end = rec_start + RECORD_LEN;
        if rec_end > self.template.len() {
            return false;
        }
        let rec = &self.template[rec_start..rec_end];
        for step_idx in 0..STEPS_PER_PATTERN {
            let flag = rec[2 + step_idx * 2 + 1];
            if flag & 0x0C == 0x0C {
                return true;
            }
        }
        false
    }
}

/// Flat index for `(device=0|1, group=0..3, slot=0..7)`.
///
/// `device 0` = A-side; `device 1` = B-side. Within a device, records are
/// ordered bank-major: `group * 8 + slot`.
pub fn index_for(device: usize, group: usize, slot: usize) -> usize {
    device * SLOTS_PER_DEVICE + group * SLOTS_PER_GROUP + slot
}

// ---------------------------------------------------------------------------
// Chunk location
// ---------------------------------------------------------------------------

/// Scan for both `303 ` chunks. Accepts a chunk only when its BE size field
/// equals the canonical 1097-byte payload length - this filters out stray
/// `"303 "` substrings that may appear inside opaque chunks.
fn find_303_chunks(data: &[u8]) -> Result<[usize; DEVICES], Td3Error> {
    let mut hits: Vec<usize> = Vec::new();
    let mut i = 0usize;
    while i + 8 <= data.len() {
        if &data[i..i + 4] == CHUNK_303 {
            let size =
                u32::from_be_bytes(data[i + 4..i + 8].try_into().map_err(|_| {
                    Td3Error::FormatError(".rbs chunk-size slice failed".to_string())
                })?) as usize;
            if size == CHUNK_303_PAYLOAD_LEN && i + 8 + size <= data.len() {
                hits.push(i);
                i += 8 + size;
                continue;
            }
        }
        i += 1;
    }
    if hits.len() != DEVICES {
        return Err(Td3Error::FormatError(format!(
            ".rbs must contain exactly {} `303 ` chunks of {} bytes, found {}",
            DEVICES,
            CHUNK_303_PAYLOAD_LEN,
            hits.len()
        )));
    }
    Ok([hits[0], hits[1]])
}

/// Return the payload slice of a `303 ` chunk located at `chunk_off`.
fn chunk_payload(data: &[u8], chunk_off: usize) -> Result<&[u8], Td3Error> {
    let start = chunk_off + 8;
    let end = start + CHUNK_303_PAYLOAD_LEN;
    if end > data.len() {
        return Err(Td3Error::FormatError(format!(
            ".rbs `303 ` chunk at {:#x} overflows file (needs {} bytes)",
            chunk_off, CHUNK_303_PAYLOAD_LEN
        )));
    }
    Ok(&data[start..end])
}

// ---------------------------------------------------------------------------
// Record encode / decode
// ---------------------------------------------------------------------------

/// Decode a 34-byte pattern record into a TD-3 `Pattern`.
///
/// Header byte 0 is treated as metadata (observed as `0x01` on a "currently
/// selected" slot in the 2-device fixture; step bytes otherwise identical
/// to a normal `0x00` header). `active_steps` lives in the low byte.
fn decode_record(rec: &[u8], device_idx: usize, rec_idx: usize) -> Result<Pattern, Td3Error> {
    if rec.len() != RECORD_LEN {
        return Err(Td3Error::FormatError(format!(
            ".rbs record dev{} rec{} has length {} (expected {})",
            device_idx,
            rec_idx,
            rec.len(),
            RECORD_LEN
        )));
    }
    let active_steps = rec[1];
    if active_steps == 0 || active_steps > STEPS_PER_PATTERN as u8 {
        return Err(Td3Error::FormatError(format!(
            ".rbs record dev{} rec{} active_steps={} out of range (1..=16)",
            device_idx, rec_idx, active_steps
        )));
    }

    let mut steps: [Step; 16] = Default::default();
    let mut raw_steps = [(0u8, 0u8); 16];
    let mut carry = DecodeCarry::default();
    for (step_idx, step) in steps.iter_mut().take(STEPS_PER_PATTERN).enumerate() {
        let off = 2 + step_idx * 2;
        let pitch = rec[off];
        let flag = rec[off + 1];
        raw_steps[step_idx] = (pitch, flag);
        *step = decode_step(pitch, flag, &mut carry).map_err(|e| {
            Td3Error::FormatError(format!(
                ".rbs record dev{} rec{} step{}: {}",
                device_idx,
                rec_idx,
                step_idx + 1,
                e
            ))
        })?;
    }
    normalize_decoded_tie_runs(&raw_steps, &mut steps);

    Pattern::new(false, active_steps, steps)
}

/// Encode a TD-3 `Pattern` into a 34-byte pattern record.
///
/// `triplet` is a TD-3-only concept (not representable in the `.rbs`
/// layout); re-encoded slots always get the canonical `00 10` header, so
/// triplet state from a TD-3-authored pattern is lost through an `.rbs`
/// round-trip. Active-step count is clamped into the byte-0x01 field.
fn encode_record(pattern: &Pattern) -> Result<[u8; RECORD_LEN], Td3Error> {
    pattern.validate()?;
    let mut out = [0u8; RECORD_LEN];
    out[0] = 0x00;
    out[1] = pattern.active_steps;
    for (step_idx, (pitch, flag)) in encode_step_sequence(&pattern.step)
        .iter()
        .copied()
        .enumerate()
    {
        out[2 + step_idx * 2] = pitch;
        out[2 + step_idx * 2 + 1] = flag;
    }
    Ok(out)
}

/// A silent pattern has `active_steps = 16` and all 16 steps set to
/// `Time::Rest`, which encodes to 16 × `(0x00, 0x10)` in the RBS stream.
fn silent_pattern() -> Result<Pattern, Td3Error> {
    let mut steps: [Step; 16] = Default::default();
    for s in steps.iter_mut() {
        s.time = Time::Rest;
    }
    Pattern::new(false, DEFAULT_ACTIVE_STEPS, steps)
}

// ---------------------------------------------------------------------------
// File-level helpers (matching the other format modules)
// ---------------------------------------------------------------------------

/// Convenience: parse a blob and return the 64 patterns as a `Vec`.
pub fn import_bank(data: &[u8]) -> Result<Vec<Pattern>, Td3Error> {
    let song = RbsSong::parse(data)?;
    let RbsSong { patterns, .. } = song;
    Ok(patterns)
}

/// Convenience: build an `.rbs` blob from 64 patterns using the bundled
/// template. `patterns[0..32]` → Device 1 (A-side); `patterns[32..64]` →
/// Device 2 (B-side).
pub fn export_bank(patterns: Vec<Pattern>) -> Result<Vec<u8>, Td3Error> {
    if patterns.len() != TOTAL_SLOTS {
        return Err(Td3Error::FormatError(format!(
            ".rbs export expects {} patterns, got {}",
            TOTAL_SLOTS,
            patterns.len()
        )));
    }
    let song = RbsSong {
        template: DEFAULT_TEMPLATE.to_vec(),
        patterns,
    };
    song.serialize()
}

/// Convenience: parse an `.rbs` and return the single pattern at
/// `(device, group, slot)`. Used when the user converts `foo.rbs → bar.X`
/// without a slot selector - callers default to Device 1 / Bank A / Slot 1.
pub fn import_single(
    data: &[u8],
    device: usize,
    group: usize,
    slot: usize,
) -> Result<Pattern, Td3Error> {
    let mut song = RbsSong::parse(data)?;
    // Move the pattern out of the vec by swap-remove on the flat index.
    let idx = index_for(device, group, slot);
    if idx >= song.patterns.len() {
        return Err(Td3Error::FormatError(format!(
            ".rbs pattern index {} out of range ({}×{}×{})",
            idx, DEVICES, GROUPS_PER_DEVICE, SLOTS_PER_GROUP
        )));
    }
    Ok(song.patterns.swap_remove(idx))
}

/// Convenience: place a single pattern at `(device=0, group=0, slot=0)` in
/// the bundled template; all other slots remain silent.
pub fn export_single(pattern: Pattern) -> Result<Vec<u8>, Td3Error> {
    export_single_at(pattern, 0, 0, 0)
}

/// Place a single pattern at the specified `(device, group, slot)` in the
/// bundled template; all other slots remain silent. Address bounds are
/// validated so callers can feed parsed user addresses without extra checks.
///
/// `device` 0 → ReBirth Device 1 (A-side, `G*P*A`).
/// `device` 1 → ReBirth Device 2 (B-side, `G*P*B`).
pub fn export_single_at(
    pattern: Pattern,
    device: usize,
    group: usize,
    slot: usize,
) -> Result<Vec<u8>, Td3Error> {
    if device >= DEVICES || group >= GROUPS_PER_DEVICE || slot >= SLOTS_PER_GROUP {
        return Err(Td3Error::FormatError(format!(
            ".rbs export address out of range: device={} group={} slot={} (max {}/{}/{})",
            device,
            group,
            slot,
            DEVICES - 1,
            GROUPS_PER_DEVICE - 1,
            SLOTS_PER_GROUP - 1
        )));
    }
    let mut song = RbsSong::blank()?;
    song.set_pattern(device, group, slot, pattern);
    song.serialize()
}
