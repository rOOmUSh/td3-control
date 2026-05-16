use super::*;

pub(crate) fn validate_group(patgroup: u8) -> Result<u8, AppError> {
    if !(1..=4).contains(&patgroup) {
        return Err(AppError::BadRequest(format!(
            "patgroup must be 1-4, got {}",
            patgroup
        )));
    }
    Ok(patgroup - 1)
}

pub(crate) fn validate_pattern(pattern: u8, side: &str) -> Result<(u8, u8), AppError> {
    if !(1..=8).contains(&pattern) {
        return Err(AppError::BadRequest(format!(
            "pattern must be 1-8, got {}",
            pattern
        )));
    }
    let side = match side.to_uppercase().as_str() {
        "A" => 0u8,
        "B" => 1u8,
        _ => {
            return Err(AppError::BadRequest(format!(
                "side must be A or B, got '{}'",
                side
            )));
        }
    };
    Ok((pattern - 1, side))
}

pub(crate) fn pattern_to_web(pattern: &crate::pattern::Pattern) -> WebPattern {
    WebPattern::from_pattern(pattern)
}

pub(crate) fn web_to_pattern(web: &WebPattern) -> Result<crate::pattern::Pattern, AppError> {
    web.to_pattern()
        .map_err(|err| AppError::BadRequest(err.to_string()))
}
