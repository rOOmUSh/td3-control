use super::*;

pub fn export_package(
    input: &PackageExportInput,
    working_dir: &Path,
) -> Result<PackageExportResult, Td3Error> {
    validate_formats(input.formats, input.combined_rbs, input.combined_sqs)?;

    let now = SystemTime::now();
    let compact_ts = timestamp_compact(now);
    let iso_ts = timestamp_iso(now);
    let scale_safe = sanitize_component(input.scale_name);
    let zip_name = format!(
        "PG_{}-{}-Random_Progression_Package.zip",
        compact_ts, scale_safe
    );

    if !working_dir.is_dir() {
        fs::create_dir_all(working_dir).map_err(|e| {
            Td3Error::Other(format!(
                "create working dir {}: {}",
                working_dir.display(),
                e
            ))
        })?;
    }

    let (bytes, file_count) = build_zip_bytes(input)?;

    let final_path = working_dir.join(&zip_name);
    let tmp_path = working_dir.join(format!("{}.tmp", zip_name));

    write_then_rename(&tmp_path, &final_path, &bytes)?;

    Ok(PackageExportResult {
        zip_name,
        saved_path: final_path.display().to_string(),
        file_count,
        created_at: iso_ts,
    })
}

fn write_then_rename(tmp: &Path, final_path: &Path, bytes: &[u8]) -> Result<(), Td3Error> {
    {
        let mut f = File::create(tmp)
            .map_err(|e| Td3Error::Other(format!("create {}: {}", tmp.display(), e)))?;
        f.write_all(bytes).map_err(|e| {
            let _ = fs::remove_file(tmp);
            Td3Error::Other(format!("write {}: {}", tmp.display(), e))
        })?;
        f.sync_all().map_err(|e| {
            let _ = fs::remove_file(tmp);
            Td3Error::Other(format!("fsync {}: {}", tmp.display(), e))
        })?;
    }
    fs::rename(tmp, final_path).map_err(|e| {
        let _ = fs::remove_file(tmp);
        Td3Error::Other(format!(
            "rename {} -> {}: {}",
            tmp.display(),
            final_path.display(),
            e
        ))
    })
}

fn validate_formats(
    formats: &[String],
    combined_rbs: bool,
    combined_sqs: bool,
) -> Result<(), Td3Error> {
    if formats.is_empty() && !combined_rbs && !combined_sqs {
        return Err(Td3Error::FormatError(
            "at least one format must be selected".to_string(),
        ));
    }
    for fmt in formats {
        match fmt.as_str() {
            "mid" | "steps_txt" | "seq" | "pat" | "rbs" | "json" | "toml" => {}
            other => {
                return Err(Td3Error::FormatError(format!(
                    "unsupported per-pattern format '{}' (valid: mid, steps_txt, seq, pat, rbs, json, toml)",
                    other
                )));
            }
        }
    }
    Ok(())
}
