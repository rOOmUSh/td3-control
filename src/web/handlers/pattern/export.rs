use super::*;

#[derive(Clone, Copy)]
enum RbsExportMode {
    Alternate,
    Serial,
}

pub async fn export_pool(
    payload: Result<Json<ExportPoolRequest>, JsonRejection>,
) -> Result<Json<ExportPoolResponse>, AppError> {
    let req = json_payload(payload, "pattern export pool")?;
    let mut files = Vec::new();
    for (i, web) in req.patterns.iter().enumerate() {
        let pattern = web_to_pattern(web)?;
        let toml_str = formats::toml_fmt::export(&pattern).map_err(AppError::Midi)?;
        let json_str = formats::json::export(&pattern).map_err(AppError::Midi)?;
        let steps_str = formats::steps_txt::export(&pattern);
        files.push(ExportedFile {
            name: format!("pattern_{:03}", i + 1),
            toml: toml_str,
            json: json_str,
            steps: steps_str,
        });
    }
    Ok(Json(ExportPoolResponse { files }))
}

pub async fn pattern_export(
    State(state): State<Arc<AppState>>,
    payload: Result<Json<PatternExportRequest>, JsonRejection>,
) -> Result<Response, AppError> {
    let req = json_payload(payload, "pattern export")?;
    let pattern = web_to_pattern(&req.pattern)?;
    let (bytes, mime): (Vec<u8>, &'static str) = match req.format.to_lowercase().as_str() {
        "toml" => (
            formats::toml_fmt::export(&pattern)
                .map_err(AppError::Midi)?
                .into_bytes(),
            "application/toml; charset=utf-8",
        ),
        "json" => (
            formats::json::export(&pattern)
                .map_err(AppError::Midi)?
                .into_bytes(),
            "application/json; charset=utf-8",
        ),
        "steps_txt" | "steps" => (
            formats::steps_txt::export(&pattern).into_bytes(),
            "text/plain; charset=utf-8",
        ),
        "pat" => (
            formats::pat::export(&pattern).into_bytes(),
            "text/plain; charset=utf-8",
        ),
        "seq" => (
            formats::seq::export(&pattern).map_err(AppError::Midi)?,
            "application/octet-stream",
        ),
        "mid" => {
            let opts = state.midi.export_options.clone();
            (
                formats::mid::export(&pattern, "G1P1A", &opts).map_err(AppError::Midi)?,
                "audio/midi",
            )
        }
        "rbs" => (export_rbs(&req)?, "application/octet-stream"),
        "sqs" => {
            return Err(AppError::BadRequest(
                "sqs is a bank-level format; single-pattern export is not supported".to_string(),
            ));
        }
        other => {
            return Err(AppError::BadRequest(format!(
                "unsupported format '{}' (supported: toml, json, steps_txt, pat, seq, mid, rbs)",
                other
            )));
        }
    };
    let resp = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .body(Body::from(bytes))
        .map_err(|e| Td3Error::Other(format!("failed to build export response: {}", e)))?;
    Ok(resp)
}

fn export_rbs(req: &PatternExportRequest) -> Result<Vec<u8>, AppError> {
    let mode = parse_rbs_export_mode(req.rbs_mode.as_deref())?;
    let count = if req.patterns.is_empty() {
        1
    } else {
        req.patterns.len()
    };
    if count > formats::rbs::TOTAL_SLOTS {
        return Err(AppError::BadRequest(format!(
            "rbs export supports at most {} patterns, got {}",
            formats::rbs::TOTAL_SLOTS,
            count
        )));
    }

    let mut song = formats::rbs::RbsSong::blank().map_err(AppError::Midi)?;
    if req.patterns.is_empty() {
        let pattern = web_to_pattern(&req.pattern)?;
        let (device, group, slot) = rbs_export_target(0, mode)?;
        song.set_pattern(device, group, slot, pattern);
    } else {
        for (idx, web) in req.patterns.iter().enumerate() {
            let pattern = web_to_pattern(web)?;
            let (device, group, slot) = rbs_export_target(idx, mode)?;
            song.set_pattern(device, group, slot, pattern);
        }
    }
    song.serialize().map_err(AppError::Midi)
}

fn parse_rbs_export_mode(value: Option<&str>) -> Result<RbsExportMode, AppError> {
    match value.unwrap_or("SERIAL") {
        "ALTERNATE" => Ok(RbsExportMode::Alternate),
        "SERIAL" => Ok(RbsExportMode::Serial),
        other => Err(AppError::BadRequest(format!(
            "invalid rbs_mode '{}' (expected ALTERNATE or SERIAL)",
            other
        ))),
    }
}

fn rbs_export_target(index: usize, mode: RbsExportMode) -> Result<(usize, usize, usize), AppError> {
    if index >= formats::rbs::TOTAL_SLOTS {
        return Err(AppError::BadRequest(format!(
            "rbs export index {} out of range",
            index
        )));
    }
    let (device, flat_in_device) = match mode {
        RbsExportMode::Serial => {
            if index < formats::rbs::SLOTS_PER_DEVICE {
                (0, index)
            } else {
                (1, index - formats::rbs::SLOTS_PER_DEVICE)
            }
        }
        RbsExportMode::Alternate => {
            let pair = index / 2;
            (index % 2, pair)
        }
    };
    let group = flat_in_device / formats::rbs::SLOTS_PER_GROUP;
    let slot = flat_in_device % formats::rbs::SLOTS_PER_GROUP;
    Ok((device, group, slot))
}
