use super::*;

pub async fn pattern_import(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PatternImportRequest>,
) -> Result<Json<PatternImportResponse>, AppError> {
    let fmt = req.format.to_lowercase();
    let need_text = || -> Result<&str, AppError> {
        req.content.as_deref().ok_or_else(|| {
            AppError::BadRequest(format!("format '{}' requires text in `content`", fmt))
        })
    };
    let need_bytes = || -> Result<&[u8], AppError> {
        req.bytes.as_deref().ok_or_else(|| {
            AppError::BadRequest(format!("format '{}' requires raw bytes in `bytes`", fmt))
        })
    };
    let (pattern, centibpm) = match fmt.as_str() {
        "toml" => (
            formats::toml_fmt::import(need_text()?).map_err(AppError::Midi)?,
            None,
        ),
        "json" => (
            formats::json::import(need_text()?).map_err(AppError::Midi)?,
            None,
        ),
        "steps" => {
            let document = formats::steps_txt::import_document(need_text()?)
                .map_err(|error| AppError::BadRequest(error.to_string()))?;
            (document.pattern, document.centibpm)
        }
        "pat" => (
            formats::pat::import(need_text()?).map_err(AppError::Midi)?,
            None,
        ),
        "seq" => (
            formats::seq::import(need_bytes()?).map_err(AppError::Midi)?,
            None,
        ),
        "mid" => {
            let opts = state.midi.import_options.clone();
            let mut resolver = formats::mid_import::LowestPitchResolver;
            (
                formats::mid_import::import(need_bytes()?, &opts, &mut resolver)
                    .map_err(AppError::Midi)?,
                None,
            )
        }
        other => {
            return Err(AppError::BadRequest(format!(
                "unsupported format '{}' (supported: toml, json, steps, pat, seq, mid)",
                other
            )));
        }
    };
    let web = pattern_to_web(&pattern);
    Ok(Json(PatternImportResponse {
        pattern: web,
        centibpm,
    }))
}
