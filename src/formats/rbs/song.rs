use crate::error::Td3Error;
use crate::pattern::Pattern;

use super::chunks::{chunk_payload, find_303_chunks};
use super::record::{decode_record, encode_record, silent_pattern};
use super::{
    CONFIG_LEN, DEFAULT_TEMPLATE, DEVICES, GROUPS_PER_DEVICE, RECORD_LEN, SLOTS_PER_DEVICE,
    SLOTS_PER_GROUP, STEPS_PER_PATTERN, TOTAL_SLOTS,
};

/// A parsed `.rbs` song as a 64-pattern bank plus the raw template bytes.
#[derive(Debug)]
pub struct RbsSong {
    pub(super) template: Vec<u8>,
    pub(super) patterns: Vec<Pattern>,
}

impl RbsSong {
    /// Build a blank song using the bundled template and silent patterns.
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

    /// Parse `.rbs` bytes and keep the source bytes as the serialize template.
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

    /// Serialize by cloning the template and overwriting pattern records.
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

    /// Access the flat pattern array.
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

    /// Detect the ReBirth empty-slot padding signature.
    pub fn has_padding_signature(&self, flat_index: usize) -> bool {
        if flat_index >= TOTAL_SLOTS {
            return false;
        }
        let chunks = match find_303_chunks(&self.template) {
            Ok(chunks) => chunks,
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
pub fn index_for(device: usize, group: usize, slot: usize) -> usize {
    device * SLOTS_PER_DEVICE + group * SLOTS_PER_GROUP + slot
}

pub(super) fn validate_address(device: usize, group: usize, slot: usize) -> Result<(), Td3Error> {
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
    Ok(())
}
