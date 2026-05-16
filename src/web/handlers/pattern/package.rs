use super::*;

pub async fn export_progression_package(
    State(state): State<Arc<AppState>>,
    payload: Result<Json<ExportPackageRequest>, JsonRejection>,
) -> Result<Json<ExportPackageResponse>, AppError> {
    let req = json_payload(payload, "progression export package")?;
    validate_package_request(&req)?;

    let acid: [crate::pattern::Pattern; 4] = [
        web_to_pattern(&req.acid_patterns[0])?,
        web_to_pattern(&req.acid_patterns[1])?,
        web_to_pattern(&req.acid_patterns[2])?,
        web_to_pattern(&req.acid_patterns[3])?,
    ];
    let bass: [crate::pattern::Pattern; 4] = [
        web_to_pattern(&req.basslines[0])?,
        web_to_pattern(&req.basslines[1])?,
        web_to_pattern(&req.basslines[2])?,
        web_to_pattern(&req.basslines[3])?,
    ];
    let bass_full_arr = basslines_full_array(&req)?;

    let midi_opts = state.midi.export_options.clone();
    let input = crate::web::package_export::PackageExportInput {
        formats: &req.formats,
        combined_rbs: req.combined_formats.rbs,
        combined_sqs: req.combined_formats.sqs,
        scale_name: &req.scale_name,
        acid_patterns: &acid,
        basslines: &bass,
        basslines_full: bass_full_arr.as_ref(),
        midi_opts: &midi_opts,
    };

    let working_dir = match &req.working_dir {
        Some(path) if !path.is_empty() => std::path::PathBuf::from(path),
        _ => std::env::current_dir()
            .map_err(|e| AppError::Midi(Td3Error::Other(format!("current_dir: {}", e))))?,
    };

    let result = tokio::task::block_in_place(|| {
        crate::web::package_export::export_package(&input, &working_dir)
    })
    .map_err(AppError::Midi)?;

    Ok(Json(ExportPackageResponse {
        ok: true,
        package_id: req.package_id,
        zip_name: result.zip_name,
        saved_path: result.saved_path,
        created_at: result.created_at,
        file_count: result.file_count,
    }))
}

fn validate_package_request(req: &ExportPackageRequest) -> Result<(), AppError> {
    if req.package_id.is_empty() {
        return Err(AppError::BadRequest("packageId is required".to_string()));
    }
    if req.acid_patterns.len() != 4 {
        return Err(AppError::BadRequest(format!(
            "expected 4 acidPatterns, got {}",
            req.acid_patterns.len()
        )));
    }
    if req.basslines.len() != 4 {
        return Err(AppError::BadRequest(format!(
            "expected 4 basslines, got {}",
            req.basslines.len()
        )));
    }
    if let Some(full) = &req.basslines_full {
        if full.len() != 20 {
            return Err(AppError::BadRequest(format!(
                "expected 20 basslinesFull (5 archetypes × 4 positions), got {}",
                full.len()
            )));
        }
    }

    const KNOWN_FORMATS: &[&str] = &["mid", "steps_txt", "seq", "pat", "rbs", "json", "toml"];
    for fmt in &req.formats {
        if !KNOWN_FORMATS.contains(&fmt.as_str()) {
            return Err(AppError::BadRequest(format!(
                "unknown format '{}': expected one of {:?}",
                fmt, KNOWN_FORMATS
            )));
        }
    }
    if req.formats.is_empty() && !req.combined_formats.rbs && !req.combined_formats.sqs {
        return Err(AppError::BadRequest(
            "at least one format must be selected".to_string(),
        ));
    }
    Ok(())
}

fn basslines_full_array(
    req: &ExportPackageRequest,
) -> Result<Option<[crate::pattern::Pattern; 20]>, AppError> {
    let Some(full) = &req.basslines_full else {
        return Ok(None);
    };

    let mut patterns = Vec::with_capacity(20);
    for wp in full {
        patterns.push(web_to_pattern(wp)?);
    }

    patterns
        .try_into()
        .map(Some)
        .map_err(|_: Vec<crate::pattern::Pattern>| {
            AppError::BadRequest("basslinesFull: internal length conversion failed".to_string())
        })
}
