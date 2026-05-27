use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const MAX_START_DELAY_MICROS: u64 = 60_000_000;

pub fn current_epoch_micros() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_micros() as u64)
        .unwrap_or(0)
}

pub fn current_epoch_millis() -> u64 {
    current_epoch_micros() / 1_000
}

pub fn resolve_start_target(target_epoch_micros: Option<u64>) -> Result<(u64, Duration), String> {
    let now = current_epoch_micros();
    let Some(target) = target_epoch_micros else {
        return Ok((now, Duration::ZERO));
    };

    if target > now.saturating_add(MAX_START_DELAY_MICROS) {
        return Err("targetEpochMicros must be within 60 seconds".to_string());
    }

    if target <= now {
        return Ok((now, Duration::ZERO));
    }

    Ok((target, Duration::from_micros(target - now)))
}
