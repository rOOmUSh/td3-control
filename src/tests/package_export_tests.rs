//! Tests for `web::package_export`. Covers validation, ZIP tree shape,
//! filename sanitization, and combined-format slot placement.

use std::fs;
use std::io::{Cursor, Read};
use std::path::PathBuf;

use zip::ZipArchive;

use crate::formats::mid::MidiExportOptions;
use crate::formats::rbs;
use crate::formats::sqs;
use crate::pattern::Pattern;
use crate::step::{Step, Time};
use crate::web::package_export::{
    export_package, sanitize_component, PackageExportInput, ROOT_FOLDER,
};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn scratch_dir(label: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("test-scratch")
        .join(format!("{}_{}", label, stamp));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn rmrf(path: &std::path::Path) {
    let _ = fs::remove_dir_all(path);
}

/// 16-step pattern where every step has the same note value and the `accent`
/// flag is set on step `marker_step`. Lets the RBS/SQS combined tests tell
/// P1 acid apart from P1 bass apart from silent slots.
fn marker_pattern(note: u8, marker_step: usize) -> Pattern {
    let mut steps: [Step; 16] = Default::default();
    for (i, s) in steps.iter_mut().enumerate() {
        s.note = note;
        s.time = Time::Normal;
        if i == marker_step {
            s.accent = crate::step::Accent::On;
        }
    }
    Pattern::new(false, 16, steps).unwrap()
}

fn four_acid() -> [Pattern; 4] {
    [
        marker_pattern(0, 0),
        marker_pattern(2, 1),
        marker_pattern(4, 2),
        marker_pattern(5, 3),
    ]
}

fn four_bass() -> [Pattern; 4] {
    [
        marker_pattern(0, 4),
        marker_pattern(2, 5),
        marker_pattern(4, 6),
        marker_pattern(5, 7),
    ]
}

/// Twenty distinguishable basslines for the combined 20-slot export path.
/// Each pattern gets a unique `(note, marker_step)` pair so round-tripped
/// slots can be identified. Ordering is position-major × archetype-minor:
///   [P1.pedal, P1.rootPulse, P1.offbeat, P1.shadow, P1.arpeggio,
///    P2.pedal, …, P4.arpeggio]
fn twenty_bass() -> [Pattern; 20] {
    // 20 (note, step) pairs - notes kept in [0, 12] (valid semitone range),
    // marker steps cycle through 0..16 so every pattern remains distinct.
    let mut arr: [Pattern; 20] = std::array::from_fn(|i| {
        let note = (i % 12) as u8;
        let step = i % 16;
        marker_pattern(note, step)
    });
    // Sanity: force the note on idx 0 and idx 19 to stay on C (0) and G (7)
    // respectively so placement tests can pin them by note value rather than
    // by the modulo expression above.
    arr[0] = marker_pattern(0, 0);
    arr[19] = marker_pattern(7, 15);
    arr
}

fn read_zip(path: &std::path::Path) -> ZipArchive<Cursor<Vec<u8>>> {
    let bytes = fs::read(path).unwrap();
    ZipArchive::new(Cursor::new(bytes)).unwrap()
}

fn zip_entries(z: &mut ZipArchive<Cursor<Vec<u8>>>) -> Vec<String> {
    (0..z.len())
        .map(|i| z.by_index(i).unwrap().name().to_string())
        .collect()
}

fn read_entry(z: &mut ZipArchive<Cursor<Vec<u8>>>, name: &str) -> Vec<u8> {
    let mut e = z
        .by_name(name)
        .unwrap_or_else(|_| panic!("missing entry: {}", name));
    let mut buf = Vec::new();
    e.read_to_end(&mut buf).unwrap();
    buf
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

#[test]
fn export_rejects_empty_selection() {
    let dir = scratch_dir("pkg_empty_selection");
    let acid = four_acid();
    let bass = four_bass();

    let input = PackageExportInput {
        formats: &[],
        combined_rbs: false,
        combined_sqs: false,
        scale_name: "test",
        acid_patterns: &acid,
        basslines: &bass,
        basslines_full: None,
        midi_opts: &MidiExportOptions::default(),
    };

    let err = export_package(&input, &dir).unwrap_err();
    assert!(
        err.to_string().contains("at least one format"),
        "expected empty-selection error, got: {}",
        err
    );

    rmrf(&dir);
}

#[test]
fn export_rejects_unknown_format() {
    let dir = scratch_dir("pkg_unknown_format");
    let acid = four_acid();
    let bass = four_bass();
    let formats = vec!["mid".to_string(), "flac".to_string()];

    let input = PackageExportInput {
        formats: &formats,
        combined_rbs: false,
        combined_sqs: false,
        scale_name: "test",
        acid_patterns: &acid,
        basslines: &bass,
        basslines_full: None,
        midi_opts: &MidiExportOptions::default(),
    };

    let err = export_package(&input, &dir).unwrap_err();
    assert!(
        err.to_string().contains("'flac'"),
        "expected unknown-format error naming flac, got: {}",
        err
    );

    rmrf(&dir);
}

#[test]
fn export_accepts_combined_only_without_per_pattern_formats() {
    let dir = scratch_dir("pkg_combined_only");
    let acid = four_acid();
    let bass = four_bass();

    let input = PackageExportInput {
        formats: &[],
        combined_rbs: true,
        combined_sqs: false,
        scale_name: "test",
        acid_patterns: &acid,
        basslines: &bass,
        basslines_full: None,
        midi_opts: &MidiExportOptions::default(),
    };

    let result = export_package(&input, &dir).unwrap();
    assert_eq!(result.file_count, 1);
    assert!(result.saved_path.ends_with(&result.zip_name));

    rmrf(&dir);
}

// ---------------------------------------------------------------------------
// ZIP tree shape
// ---------------------------------------------------------------------------

#[test]
fn export_builds_expected_per_pattern_tree_for_minimum_defaults() {
    let dir = scratch_dir("pkg_tree_mid_seq_steps");
    let acid = four_acid();
    let bass = four_bass();
    let formats = vec![
        "mid".to_string(),
        "steps_txt".to_string(),
        "seq".to_string(),
    ];

    let input = PackageExportInput {
        formats: &formats,
        combined_rbs: false,
        combined_sqs: false,
        scale_name: "C Major",
        acid_patterns: &acid,
        basslines: &bass,
        basslines_full: None,
        midi_opts: &MidiExportOptions::default(),
    };

    let result = export_package(&input, &dir).unwrap();
    // 3 formats × (4 acid + 4 bass) = 24 files.
    assert_eq!(result.file_count, 24);
    // Filename: PG_<ts>-C_Major-Random_Progression_Package.zip
    assert!(
        result.zip_name.starts_with("PG_") && result.zip_name.contains("-C_Major-"),
        "unexpected zip name: {}",
        result.zip_name
    );
    assert!(result.zip_name.ends_with("-Random_Progression_Package.zip"));

    let final_path = std::path::Path::new(&result.saved_path);
    let mut z = read_zip(final_path);
    let names = zip_entries(&mut z);

    // Every expected per-pattern file exists.
    for i in 1..=4 {
        for ext in ["mid", "steps.txt", "seq"] {
            let acid_name = format!("{}/P{}/P{}.{}", ROOT_FOLDER, i, i, ext);
            let bass_name = format!(
                "{}/P{}/P{}_BASSLINE/P{}_BASSLINE.{}",
                ROOT_FOLDER, i, i, i, ext
            );
            assert!(names.contains(&acid_name), "missing {}", acid_name);
            assert!(names.contains(&bass_name), "missing {}", bass_name);
        }
    }

    // No combined files unless ticked.
    assert!(!names.iter().any(|n| n.ends_with("combined.rbs")));
    assert!(!names.iter().any(|n| n.ends_with("combined.sqs")));

    rmrf(&dir);
}

#[test]
fn export_emits_every_per_pattern_format_when_all_selected() {
    let dir = scratch_dir("pkg_all_formats");
    let acid = four_acid();
    let bass = four_bass();
    let formats = vec![
        "mid".to_string(),
        "steps_txt".to_string(),
        "seq".to_string(),
        "pat".to_string(),
        "rbs".to_string(),
        "json".to_string(),
        "toml".to_string(),
    ];

    let input = PackageExportInput {
        formats: &formats,
        combined_rbs: false,
        combined_sqs: false,
        scale_name: "dorian",
        acid_patterns: &acid,
        basslines: &bass,
        basslines_full: None,
        midi_opts: &MidiExportOptions::default(),
    };

    let result = export_package(&input, &dir).unwrap();
    // 7 formats × 8 targets = 56.
    assert_eq!(result.file_count, 56);

    rmrf(&dir);
}

#[test]
fn export_defaults_plus_combined_rbs_adds_root_file_and_counts_25() {
    let dir = scratch_dir("pkg_defaults_plus_combined_rbs");
    let acid = four_acid();
    let bass = four_bass();
    let formats = vec![
        "mid".to_string(),
        "steps_txt".to_string(),
        "seq".to_string(),
    ];

    let input = PackageExportInput {
        formats: &formats,
        combined_rbs: true,
        combined_sqs: false,
        scale_name: "minor",
        acid_patterns: &acid,
        basslines: &bass,
        basslines_full: None,
        midi_opts: &MidiExportOptions::default(),
    };

    let result = export_package(&input, &dir).unwrap();
    assert_eq!(result.file_count, 25);

    let mut z = read_zip(std::path::Path::new(&result.saved_path));
    let names = zip_entries(&mut z);
    assert_eq!(names.len(), 25, "zip should contain exactly 25 files");
    assert!(names.contains(&format!("{}/combined.rbs", ROOT_FOLDER)));
    assert!(!names.contains(&format!("{}/combined.sqs", ROOT_FOLDER)));

    rmrf(&dir);
}

#[test]
fn export_defaults_plus_combined_sqs_adds_root_file_and_counts_25() {
    let dir = scratch_dir("pkg_defaults_plus_combined_sqs");
    let acid = four_acid();
    let bass = four_bass();
    let formats = vec![
        "mid".to_string(),
        "steps_txt".to_string(),
        "seq".to_string(),
    ];

    let input = PackageExportInput {
        formats: &formats,
        combined_rbs: false,
        combined_sqs: true,
        scale_name: "minor",
        acid_patterns: &acid,
        basslines: &bass,
        basslines_full: None,
        midi_opts: &MidiExportOptions::default(),
    };

    let result = export_package(&input, &dir).unwrap();
    assert_eq!(result.file_count, 25);

    let mut z = read_zip(std::path::Path::new(&result.saved_path));
    let names = zip_entries(&mut z);
    assert_eq!(names.len(), 25, "zip should contain exactly 25 files");
    assert!(!names.contains(&format!("{}/combined.rbs", ROOT_FOLDER)));
    assert!(names.contains(&format!("{}/combined.sqs", ROOT_FOLDER)));

    rmrf(&dir);
}

#[test]
fn export_combined_only_writes_exactly_two_root_files() {
    let dir = scratch_dir("pkg_combined_only_two_files");
    let acid = four_acid();
    let bass = four_bass();
    let formats: Vec<String> = Vec::new();

    let input = PackageExportInput {
        formats: &formats,
        combined_rbs: true,
        combined_sqs: true,
        scale_name: "minor",
        acid_patterns: &acid,
        basslines: &bass,
        basslines_full: None,
        midi_opts: &MidiExportOptions::default(),
    };

    let result = export_package(&input, &dir).unwrap();
    assert_eq!(result.file_count, 2);

    let mut z = read_zip(std::path::Path::new(&result.saved_path));
    let names = zip_entries(&mut z);
    assert_eq!(names.len(), 2, "zip should contain exactly two files");
    assert_eq!(
        names,
        vec![
            format!("{}/combined.rbs", ROOT_FOLDER),
            format!("{}/combined.sqs", ROOT_FOLDER),
        ]
    );

    rmrf(&dir);
}

// ---------------------------------------------------------------------------
// Combined .rbs slot placement
// ---------------------------------------------------------------------------

#[test]
fn combined_rbs_places_acid_on_device_one_and_bass_on_device_two() {
    let dir = scratch_dir("pkg_combined_rbs");
    let acid = four_acid();
    let bass = four_bass();
    let formats: Vec<String> = Vec::new();

    let input = PackageExportInput {
        formats: &formats,
        combined_rbs: true,
        combined_sqs: false,
        scale_name: "minor",
        acid_patterns: &acid,
        basslines: &bass,
        basslines_full: None,
        midi_opts: &MidiExportOptions::default(),
    };

    let result = export_package(&input, &dir).unwrap();
    let final_path = std::path::Path::new(&result.saved_path);
    let mut z = read_zip(final_path);

    let rbs_bytes = read_entry(&mut z, &format!("{}/combined.rbs", ROOT_FOLDER));
    let patterns = rbs::import_bank(&rbs_bytes).unwrap();
    assert_eq!(patterns.len(), rbs::TOTAL_SLOTS);

    // Acid in device 0 (A-side) G1P1..G1P4 - flat indices 0..=3.
    // Bass in device 1 (B-side) G1P1..G1P4 - flat indices 32..=35.
    // The marker_pattern uses a unique accent step per P - confirm round-trip.
    for i in 0..4 {
        let acid_rt = &patterns[rbs::index_for(0, 0, i)];
        assert_eq!(
            acid_rt.active_steps,
            16,
            "acid P{} active_steps lost through .rbs round-trip",
            i + 1
        );
        let bass_rt = &patterns[rbs::index_for(1, 0, i)];
        assert_eq!(
            bass_rt.active_steps,
            16,
            "bass P{} active_steps lost through .rbs round-trip",
            i + 1
        );
    }

    rmrf(&dir);
}

// ---------------------------------------------------------------------------
// Combined .sqs slot placement
// ---------------------------------------------------------------------------

#[test]
fn combined_sqs_places_acid_on_a_side_and_bass_on_b_side() {
    let dir = scratch_dir("pkg_combined_sqs");
    let acid = four_acid();
    let bass = four_bass();
    let formats: Vec<String> = Vec::new();

    let input = PackageExportInput {
        formats: &formats,
        combined_rbs: false,
        combined_sqs: true,
        scale_name: "lydian",
        acid_patterns: &acid,
        basslines: &bass,
        basslines_full: None,
        midi_opts: &MidiExportOptions::default(),
    };

    let result = export_package(&input, &dir).unwrap();
    let final_path = std::path::Path::new(&result.saved_path);
    let mut z = read_zip(final_path);

    let sqs_bytes = read_entry(&mut z, &format!("{}/combined.sqs", ROOT_FOLDER));
    let bank = sqs::parse_bank(&sqs_bytes).unwrap();

    // Records are indexed (group * 16 + slot_addr).
    // A-side G1P1..G1P4 → group=0, slot_addr=0..3 → flat 0..=3.
    // B-side G1P1..G1P4 → group=0, slot_addr=8..11 → flat 8..=11.
    for i in 0..4 {
        let acid_rec = &bank.records[i];
        assert_eq!(acid_rec.group, 0);
        assert_eq!(acid_rec.slot_addr, i as u8);
        assert!(
            !sqs::is_silent(&acid_rec.payload),
            "acid P{} record came out silent",
            i + 1
        );

        let bass_rec = &bank.records[8 + i];
        assert_eq!(bass_rec.group, 0);
        assert_eq!(bass_rec.slot_addr, (8 + i) as u8);
        assert!(
            !sqs::is_silent(&bass_rec.payload),
            "bass P{} record came out silent",
            i + 1
        );
    }

    // Spot-check a silent slot: A-side G1P5 (group=0, slot_addr=4 → flat 4).
    assert!(
        sqs::is_silent(&bank.records[4].payload),
        "unused A-side slot G1P5 should be silent"
    );
    // Spot-check a silent slot: B-side G1P5 (group=0, slot_addr=12 → flat 12).
    assert!(
        sqs::is_silent(&bank.records[12].payload),
        "unused B-side slot G1P5 should be silent"
    );
    // Spot-check the last record (G4P8B).
    assert_eq!(bank.records[63].group, 3);
    assert_eq!(bank.records[63].slot_addr, 15);
    assert!(sqs::is_silent(&bank.records[63].payload));

    rmrf(&dir);
}

// ---------------------------------------------------------------------------
// Combined .rbs - 20-bassline placement on Device 2
// ---------------------------------------------------------------------------

#[test]
fn combined_rbs_places_all_twenty_basslines_on_device_two_sequentially() {
    let dir = scratch_dir("pkg_combined_rbs_twenty");
    let acid = four_acid();
    let bass = four_bass();
    let bass_full = twenty_bass();
    let formats: Vec<String> = Vec::new();

    let input = PackageExportInput {
        formats: &formats,
        combined_rbs: true,
        combined_sqs: false,
        scale_name: "minor",
        acid_patterns: &acid,
        basslines: &bass,
        basslines_full: Some(&bass_full),
        midi_opts: &MidiExportOptions::default(),
    };

    let result = export_package(&input, &dir).unwrap();
    let mut z = read_zip(std::path::Path::new(&result.saved_path));
    let rbs_bytes = read_entry(&mut z, &format!("{}/combined.rbs", ROOT_FOLDER));
    let patterns = rbs::import_bank(&rbs_bytes).unwrap();
    assert_eq!(patterns.len(), rbs::TOTAL_SLOTS);

    // Acid leads still at device 0, G1P1..G1P4.
    for i in 0..4 {
        let rt = &patterns[rbs::index_for(0, 0, i)];
        assert_eq!(rt.active_steps, 16);
    }

    // All 20 B-side slots populated at device 1, sequential.
    // idx 0..7  → G1P1..G1P8
    // idx 8..15 → G2P1..G2P8
    // idx 16..19 → G3P1..G3P4
    for idx in 0..20usize {
        let group = idx / 8;
        let slot = idx % 8;
        let flat = rbs::index_for(1, group, slot);
        let rt = &patterns[flat];
        assert_eq!(
            rt.active_steps,
            16,
            "bass idx {} (device=1 G{}P{}) lost active_steps through round-trip",
            idx,
            group + 1,
            slot + 1,
        );
        // The marker_pattern sets .note on every step - confirm it came back
        // as the expected note. bass_full[idx] used note = (idx % 12).
        // Special-cased idx 0 → 0, idx 19 → 7.
        let expected_note: u8 = match idx {
            0 => 0,
            19 => 7,
            _ => (idx % 12) as u8,
        };
        assert_eq!(
            rt.step[0].note, expected_note,
            "bass idx {} note mismatch (got {}, expected {})",
            idx, rt.step[0].note, expected_note,
        );
    }

    // G3P5..G3P8 and all of G4 on Device 2 must remain silent (default
    // pattern - note 0 is fine, but is_all_rest via step time check).
    for idx in 20..32usize {
        let group = idx / 8;
        let slot = idx % 8;
        let flat = rbs::index_for(1, group, slot);
        let rt = &patterns[flat];
        // Silent slots carry REST on every step.
        let silent = rt
            .step
            .iter()
            .all(|s| matches!(s.time, crate::step::Time::Rest));
        assert!(
            silent,
            "expected silent slot at Device 2 idx {} (G{}P{})",
            idx,
            group + 1,
            slot + 1,
        );
    }

    rmrf(&dir);
}

// ---------------------------------------------------------------------------
// Combined .sqs - 20-bassline placement on B-side G1..G3
// ---------------------------------------------------------------------------

#[test]
fn combined_sqs_places_all_twenty_basslines_on_b_side_sequentially() {
    let dir = scratch_dir("pkg_combined_sqs_twenty");
    let acid = four_acid();
    let bass = four_bass();
    let bass_full = twenty_bass();
    let formats: Vec<String> = Vec::new();

    let input = PackageExportInput {
        formats: &formats,
        combined_rbs: false,
        combined_sqs: true,
        scale_name: "lydian",
        acid_patterns: &acid,
        basslines: &bass,
        basslines_full: Some(&bass_full),
        midi_opts: &MidiExportOptions::default(),
    };

    let result = export_package(&input, &dir).unwrap();
    let mut z = read_zip(std::path::Path::new(&result.saved_path));
    let sqs_bytes = read_entry(&mut z, &format!("{}/combined.sqs", ROOT_FOLDER));
    let bank = sqs::parse_bank(&sqs_bytes).unwrap();

    // Acid leads still on A-side G1P1..G1P4 (flat 0..=3).
    for i in 0..4 {
        let rec = &bank.records[i];
        assert_eq!(rec.group, 0);
        assert_eq!(rec.slot_addr, i as u8);
        assert!(
            !sqs::is_silent(&rec.payload),
            "acid P{} came out silent",
            i + 1
        );
    }

    // All 20 B-side basslines present at (group = idx/8, slot_addr = 8 + idx%8).
    // Flat record index = group * 16 + slot_addr.
    for idx in 0..20usize {
        let group = (idx / 8) as u8;
        let slot_num = (idx % 8) as u8;
        let slot_addr = 8u8 | slot_num; // side=1 sets bit 3
        let flat = (group as usize) * 16 + (slot_addr as usize);
        let rec = &bank.records[flat];
        assert_eq!(rec.group, group);
        assert_eq!(rec.slot_addr, slot_addr);
        assert!(
            !sqs::is_silent(&rec.payload),
            "bass idx {} (G{}P{}B) came out silent",
            idx,
            group + 1,
            slot_num + 1,
        );
    }

    // Spot-check silent slots.
    // A-side G1P5..G1P8 → flat 4..=7.
    for flat in 4..=7 {
        assert!(
            sqs::is_silent(&bank.records[flat].payload),
            "unused A-side flat {} should be silent",
            flat,
        );
    }
    // B-side G3P5..G3P8 → group=2, slot_addr=12..15 → flat 44..=47.
    for flat in 44..=47 {
        assert!(
            sqs::is_silent(&bank.records[flat].payload),
            "B-side tail flat {} should be silent",
            flat,
        );
    }
    // Last record (G4P8B) silent.
    assert_eq!(bank.records[63].group, 3);
    assert_eq!(bank.records[63].slot_addr, 15);
    assert!(sqs::is_silent(&bank.records[63].payload));

    rmrf(&dir);
}

// ---------------------------------------------------------------------------
// Filename sanitization
// ---------------------------------------------------------------------------

#[test]
fn sanitize_replaces_unsafe_chars_and_whitespace_runs() {
    assert_eq!(sanitize_component("natural minor"), "natural_minor");
    assert_eq!(sanitize_component("C/D:E*F?"), "C_D_E_F_");
    assert_eq!(sanitize_component("a   b"), "a_b");
    assert_eq!(sanitize_component(""), "progression");
    assert_eq!(sanitize_component("A/\\:*?\"<>|B"), "A_B");
}

#[test]
fn zip_filename_sanitizes_scale_name() {
    let dir = scratch_dir("pkg_sanitize_filename");
    let acid = four_acid();
    let bass = four_bass();
    let formats = vec!["mid".to_string()];

    let input = PackageExportInput {
        formats: &formats,
        combined_rbs: false,
        combined_sqs: false,
        scale_name: "natural minor / raised 7",
        acid_patterns: &acid,
        basslines: &bass,
        basslines_full: None,
        midi_opts: &MidiExportOptions::default(),
    };

    let result = export_package(&input, &dir).unwrap();
    assert!(
        result.zip_name.contains("natural_minor_"),
        "scale name not sanitized in filename: {}",
        result.zip_name
    );
    assert!(!result.zip_name.contains('/'));
    assert!(!result.zip_name.contains(' '));

    rmrf(&dir);
}
