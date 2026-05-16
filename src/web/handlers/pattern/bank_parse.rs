use super::*;

pub async fn pattern_parse_bank(
    Json(req): Json<PatternParseBankRequest>,
) -> Result<Json<PatternParseBankResponse>, AppError> {
    let fmt = req.format.to_lowercase();
    let slots = match fmt.as_str() {
        "sqs" => parse_bank_sqs(&req.bytes).map_err(AppError::Midi)?,
        "rbs" => parse_bank_rbs(&req.bytes).map_err(AppError::Midi)?,
        other => {
            return Err(AppError::BadRequest(format!(
                "unsupported bank format '{}' (supported: sqs, rbs)",
                other
            )));
        }
    };
    Ok(Json(PatternParseBankResponse { slots }))
}

fn dashed_slot_key(group: u8, slot_addr: u8) -> String {
    let slot_num = slot_addr & 0x7;
    let side = slot_addr >> 3;
    format!(
        "G{}-P{}{}",
        group + 1,
        slot_num + 1,
        if side == 0 { 'A' } else { 'B' }
    )
}

fn parse_bank_sqs(data: &[u8]) -> Result<Vec<PatternParseBankSlot>, Td3Error> {
    let bank = formats::sqs::parse_bank(data)?;
    let mut out = Vec::with_capacity(bank.records.len());
    for rec in bank.records.iter() {
        let slot_key = dashed_slot_key(rec.group, rec.slot_addr);
        let empty = formats::sqs::is_silent(&rec.payload);
        let pattern = if empty {
            None
        } else {
            let mut sysex = Vec::with_capacity(115);
            sysex.push(0x78);
            sysex.push(rec.group);
            sysex.push(rec.slot_addr);
            sysex.extend_from_slice(&rec.payload);
            let pat = crate::pattern::sysex_to_pattern(&sysex)?;
            Some(pattern_to_web(&pat))
        };
        out.push(PatternParseBankSlot {
            slot_key: slot_key.clone(),
            empty,
            display_name: slot_key,
            pattern,
        });
    }
    Ok(out)
}

fn parse_bank_rbs(data: &[u8]) -> Result<Vec<PatternParseBankSlot>, Td3Error> {
    let song = formats::rbs::RbsSong::parse(data)?;
    let patterns = song.patterns();
    let mut out = Vec::with_capacity(patterns.len());
    for (flat, pattern) in patterns.iter().enumerate() {
        let device = flat / formats::rbs::SLOTS_PER_DEVICE;
        let within_device = flat % formats::rbs::SLOTS_PER_DEVICE;
        let group = (within_device / formats::rbs::SLOTS_PER_GROUP) as u8;
        let slot = (within_device % formats::rbs::SLOTS_PER_GROUP) as u8;
        let slot_addr = slot | ((device as u8) << 3);
        let slot_key = dashed_slot_key(group, slot_addr);

        let is_padding = song.has_padding_signature(flat);
        let all_rest = pattern
            .step
            .iter()
            .all(|s| s.time == crate::step::Time::Rest);
        let empty = is_padding || all_rest;

        let web = if empty {
            None
        } else {
            Some(pattern_to_web(pattern))
        };
        out.push(PatternParseBankSlot {
            slot_key: slot_key.clone(),
            empty,
            display_name: slot_key,
            pattern: web,
        });
    }
    Ok(out)
}
