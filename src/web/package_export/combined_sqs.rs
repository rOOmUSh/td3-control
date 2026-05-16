use super::*;

pub(super) fn build_combined_sqs(
    acid: &[Pattern; 4],
    bass: &[Pattern; 4],
    bass_full: Option<&[Pattern; 20]>,
) -> Result<Vec<u8>, Td3Error> {
    let silent_payload = payload_for_silent()?;
    let mut placements: std::collections::HashMap<(u8, u8), Vec<u8>> =
        std::collections::HashMap::new();

    for (i, pattern) in acid.iter().enumerate() {
        let group = 0u8;
        let slot_num = i as u8;
        let side = 0u8;
        placements.insert(
            (group, (side << 3) | slot_num),
            payload_for(pattern, group, slot_num, side)?,
        );
    }

    match bass_full {
        Some(full) => {
            for (idx, pattern) in full.iter().enumerate() {
                let group = (idx / 8) as u8;
                let slot_num = (idx % 8) as u8;
                let side = 1u8;
                placements.insert(
                    (group, (side << 3) | slot_num),
                    payload_for(pattern, group, slot_num, side)?,
                );
            }
        }
        None => {
            for (i, pattern) in bass.iter().enumerate() {
                let group = 0u8;
                let slot_num = i as u8;
                let side = 1u8;
                placements.insert(
                    (group, (side << 3) | slot_num),
                    payload_for(pattern, group, slot_num, side)?,
                );
            }
        }
    }

    let mut records: Vec<BankRecord> = Vec::with_capacity(RECORD_COUNT);
    for idx in 0..RECORD_COUNT {
        let group = (idx / 16) as u8;
        let slot_addr = (idx % 16) as u8;
        let payload = match placements.remove(&(group, slot_addr)) {
            Some(p) => p,
            None => silent_payload.clone(),
        };
        records.push(BankRecord {
            group,
            slot_addr,
            payload,
        });
    }

    let records: [BankRecord; RECORD_COUNT] =
        records.try_into().map_err(|_: Vec<BankRecord>| {
            Td3Error::Other(".sqs combined: built record count mismatch".to_string())
        })?;

    let bank = Bank {
        product_bytes: PRODUCT_UTF16BE.to_vec(),
        version_bytes: VERSION_UTF16BE.to_vec(),
        records,
    };
    serialize_bank(&bank)
}

fn payload_for(
    pattern: &Pattern,
    patgroup: u8,
    slot_num: u8,
    side: u8,
) -> Result<Vec<u8>, Td3Error> {
    let sysex = pattern_to_sysex(pattern, patgroup, slot_num, side)?;
    let want = 3 + PAYLOAD_LEN as usize;
    if sysex.len() < want {
        return Err(Td3Error::Other(format!(
            "pattern_to_sysex returned {} bytes, expected at least {}",
            sysex.len(),
            want
        )));
    }
    Ok(sysex[3..want].to_vec())
}

fn payload_for_silent() -> Result<Vec<u8>, Td3Error> {
    payload_for(&silent_pattern()?, 0, 0, 0)
}
