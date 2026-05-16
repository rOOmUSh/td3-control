use super::*;

pub const ROOT_FOLDER: &str = "TD-3 Patterns Progression";

#[derive(Debug)]
pub struct PackageExportInput<'a> {
    pub formats: &'a [String],
    pub combined_rbs: bool,
    pub combined_sqs: bool,
    pub scale_name: &'a str,
    pub acid_patterns: &'a [Pattern; 4],
    pub basslines: &'a [Pattern; 4],
    pub basslines_full: Option<&'a [Pattern; 20]>,
    pub midi_opts: &'a crate::formats::mid::MidiExportOptions,
}

#[derive(Debug)]
pub struct PackageExportResult {
    pub zip_name: String,
    pub saved_path: String,
    pub file_count: u32,
    pub created_at: String,
}
