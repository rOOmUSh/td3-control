use super::*;

pub(super) fn render_format(
    fmt_id: &str,
    pattern: &Pattern,
    address: &str,
    midi_opts: &MidiExportOptions,
) -> Result<(&'static str, Vec<u8>), Td3Error> {
    match fmt_id {
        "mid" => Ok(("mid", formats::mid::export(pattern, address, midi_opts)?)),
        "steps_txt" => Ok((
            "steps.txt",
            formats::steps_txt::export(pattern).into_bytes(),
        )),
        "seq" => Ok(("seq", formats::seq::export(pattern)?)),
        "pat" => Ok(("pat", formats::pat::export(pattern).into_bytes())),
        "rbs" => Ok(("rbs", rbs::export_single(clone_pattern(pattern)?)?)),
        "json" => Ok(("json", formats::json::export(pattern)?.into_bytes())),
        "toml" => Ok(("toml", formats::toml_fmt::export(pattern)?.into_bytes())),
        other => Err(Td3Error::FormatError(format!(
            "unsupported per-pattern format '{}'",
            other
        ))),
    }
}

pub(super) fn clone_pattern(p: &Pattern) -> Result<Pattern, Td3Error> {
    Pattern::new(p.triplet, p.active_steps, p.step)
}
