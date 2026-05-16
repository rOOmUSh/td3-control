use super::*;

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
        "rbs" => (
            formats::rbs::export_single(pattern).map_err(AppError::Midi)?,
            "application/octet-stream",
        ),
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
