use std::path::{Path, PathBuf};

pub fn sort_import_paths(paths: &mut [PathBuf]) {
    paths.sort_by_cached_key(|path| import_path_sort_key(path));
}

pub fn import_priority_for_filename(lower_name: &str) -> u8 {
    if lower_name.ends_with(".seq") {
        1
    } else if lower_name.ends_with(".syx") {
        2
    } else if lower_name.ends_with(".steps.txt") {
        3
    } else if lower_name.ends_with(".json") {
        4
    } else if lower_name.ends_with(".toml") {
        5
    } else if lower_name.ends_with(".pat") {
        6
    } else if lower_name.ends_with(".mid") {
        7
    } else if lower_name.ends_with(".sqs") || lower_name.ends_with(".rbs") {
        8
    } else {
        9
    }
}

pub fn logical_pattern_name(lower_name: &str) -> String {
    const SUFFIXES: [&str; 9] = [
        ".steps.txt",
        ".seq",
        ".syx",
        ".json",
        ".toml",
        ".pat",
        ".mid",
        ".sqs",
        ".rbs",
    ];
    for suffix in SUFFIXES {
        if let Some(stem) = lower_name.strip_suffix(suffix) {
            return stem.to_string();
        }
    }
    lower_name.to_string()
}

pub fn lower_filename(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

pub fn parent_key(path: &Path) -> String {
    path.parent()
        .map(|parent| parent.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
struct ImportPathSortKey {
    priority: u8,
    directory: String,
    logical_name: String,
    filename: String,
}

fn import_path_sort_key(path: &Path) -> ImportPathSortKey {
    let filename = lower_filename(path);

    ImportPathSortKey {
        priority: import_priority_for_filename(&filename),
        directory: parent_key(path),
        logical_name: logical_pattern_name(&filename),
        filename,
    }
}
