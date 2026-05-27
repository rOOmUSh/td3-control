//! Unit tests for `src/library/*`.
//!
//! Covers:
//! - Store: load/reload round-trip, item CRUD + filters, snapshot
//!   create/rename/pin, tag add/remove, search + tag filters.
//! - Compare: identical and differing patterns, snapshot slot-state grid.
//! - Merge plan: structural shape of the produced plan.
//! - Scanner: extension classification and recursive walk.

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::bank::backup::BackupKind;
use crate::bank::inventory::BackupInventoryEntry;
use crate::formats::mid_import::MidiImportOptions;
use crate::library::compare::{compare_items, compare_snapshots, SlotCompareState};
use crate::library::merge_plan::{build_merge_plan, MergeAction};
use crate::library::model::{
    AnalysisStatus, DuplicateStatus, FileIndexEntry, FileIngestStatus, ImportBatch, LibraryItem,
    PatternRelation, RelationKind, SnapshotOrigin, SnapshotSlot, SourceKind, TagKind,
};
use crate::library::persistence;
use crate::library::scanner;
use crate::library::store::{self, LibraryStore};
use crate::library::ItemFilter;

use crate::pattern::Pattern;
use crate::step;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_library_path(tag: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    // Each test gets its own parent directory so that sidecar storage
    // (derived from `catalog.path.parent()`) is fully isolated - otherwise
    // concurrent tests clobber each other's `bank-library-patterns/` and
    // the subsequent cleanup wipes sidecars mid-run.
    let dir =
        std::env::temp_dir().join(format!("td3-library-test-{}-{}-{}-{}", tag, pid, n, nanos));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp library dir");
    let path = dir.join("catalog.json");
    let _ = std::fs::remove_file(&path);
    path
}

fn sample_item(id: &str, name: &str) -> LibraryItem {
    LibraryItem {
        item_id: id.to_string(),
        display_name: name.to_string(),
        source_kind: SourceKind::File,
        source_label: "sample".to_string(),
        source_path: Some(format!("/tmp/{}.seq", name)),
        created_at: "20260101T000000Z".to_string(),
        updated_at: "20260101T000000Z".to_string(),
        tags: vec![],
        favorite: false,
        archived: false,
        slot_key: None,
        snapshot_id: None,
        snapshot_name: None,
        format: Some("seq".to_string()),
        scale_name: None,
        root_note: None,
        duplicate_status: DuplicateStatus::Unknown,
        related_group_count: 0,
        analysis_status: AnalysisStatus::Unknown,
        notes: None,
        content_hash: None,
    }
}

fn make_pattern(notes: &[u8]) -> Pattern {
    let mut steps: [step::Step; 16] = Default::default();
    for (i, &n) in notes.iter().enumerate().take(16) {
        steps[i] = step::Step {
            note: n,
            transpose: step::Transpose::Normal,
            accent: step::Accent::Off,
            slide: step::Slide::Off,
            time: step::Time::Normal,
        };
    }
    Pattern::new(false, 16, steps).expect("valid pattern")
}

// ---------------------------------------------------------------------------
// Store: load + reload
// ---------------------------------------------------------------------------

#[test]
fn store_roundtrip_empty() {
    let path = temp_library_path("roundtrip");
    let store = LibraryStore::load_or_create(&path).unwrap();
    assert!(path.exists(), "load_or_create must materialize the file");

    let items = store.list_items(&ItemFilter::default()).unwrap();
    assert!(items.is_empty());
    drop(store);

    // Reopen the same file and verify default empty shape. The seeded
    // `safe-live` system tag is expected to persist across reloads; it is
    // not a user-populated row so it does not count as "populated" content.
    let store2 = LibraryStore::load_or_create(&path).unwrap();
    assert!(store2
        .list_items(&ItemFilter::default())
        .unwrap()
        .is_empty());
    assert!(store2.list_snapshots().unwrap().is_empty());
    let tags = store2.list_tags().unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].label, "safe-live");
}

#[test]
fn store_imports_legacy_json_into_sqlite_on_first_open() {
    let dir = temp_library_path("legacy-import")
        .parent()
        .expect("temp library dir")
        .to_path_buf();
    let sqlite_path = dir.join("catalog.sqlite3");
    let legacy_path = dir.join("catalog.json");

    let mut legacy = store::LibraryData::default();
    legacy
        .items
        .push(sample_item("item_legacy", "legacy-alpha"));
    std::fs::write(
        &legacy_path,
        serde_json::to_vec_pretty(&legacy).expect("serialize legacy catalog"),
    )
    .expect("write legacy catalog");

    let store = LibraryStore::load_or_create(&sqlite_path).unwrap();
    assert!(sqlite_path.exists(), "sqlite catalog should be created");

    let items = store.list_items(&ItemFilter::default()).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].item_id, "item_legacy");
    assert_eq!(items[0].display_name, "legacy-alpha");

    drop(store);
    let reopened = LibraryStore::load_or_create(&sqlite_path).unwrap();
    let items = reopened.list_items(&ItemFilter::default()).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].item_id, "item_legacy");
}

#[test]
fn store_crud_items() {
    let path = temp_library_path("crud");
    let store = LibraryStore::load_or_create(&path).unwrap();

    let a = store.upsert_item(sample_item("item_a", "alpha")).unwrap();
    let b = store.upsert_item(sample_item("item_b", "beta")).unwrap();
    assert_eq!(a.item_id, "item_a");
    assert_eq!(b.item_id, "item_b");

    // Reload from disk and confirm persistence.
    drop(store);
    let store = LibraryStore::load_or_create(&path).unwrap();
    let all = store.list_items(&ItemFilter::default()).unwrap();
    assert_eq!(all.len(), 2);

    assert!(store.get_item("item_a").unwrap().is_some());
    assert!(store.get_item("missing").unwrap().is_none());

    assert!(store.delete_item("item_a").unwrap());
    assert!(!store.delete_item("item_a").unwrap());
    assert_eq!(store.list_items(&ItemFilter::default()).unwrap().len(), 1);
}

#[test]
fn store_snapshot_create_rename_pin() {
    let path = temp_library_path("snap");
    let store = LibraryStore::load_or_create(&path).unwrap();

    let snap = store
        .create_snapshot(
            "Initial".to_string(),
            Some("desc".to_string()),
            SnapshotOrigin::Manual,
        )
        .unwrap();
    assert_eq!(snap.name, "Initial");
    assert!(!snap.pinned);

    let renamed = store
        .rename_snapshot(&snap.snapshot_id, "Renamed".to_string())
        .unwrap()
        .expect("snapshot exists");
    assert_eq!(renamed.name, "Renamed");

    let pinned = store
        .pin_snapshot(&snap.snapshot_id, true)
        .unwrap()
        .expect("snapshot exists");
    assert!(pinned.pinned);

    // 64-slot padded grid
    let slots = store.list_snapshot_slots(&snap.snapshot_id).unwrap();
    assert_eq!(slots.len(), 64);
    assert!(slots.iter().all(|s| s.empty));
    assert!(slots.iter().any(|s| s.slot_key == "G1-P1A"));
    assert!(slots.iter().any(|s| s.slot_key == "G4-P8B"));
}

#[test]
fn store_tags_add_remove() {
    let path = temp_library_path("tags");
    let store = LibraryStore::load_or_create(&path).unwrap();

    let _ = store.upsert_item(sample_item("item_a", "alpha")).unwrap();

    store.add_tag_to_item("item_a", "acid").unwrap();
    store.add_tag_to_item("item_a", "bright").unwrap();
    store.add_tag_to_item("item_a", "acid").unwrap(); // idempotent

    // Two user tags plus the seeded `safe-live` system tag.
    let tags = store.list_tags().unwrap();
    let user_tags: Vec<_> = tags.iter().filter(|t| t.kind != TagKind::System).collect();
    assert_eq!(user_tags.len(), 2);

    let item = store.get_item("item_a").unwrap().unwrap();
    assert!(item.tags.contains(&"acid".to_string()));
    assert!(item.tags.contains(&"bright".to_string()));

    store.remove_tag_from_item("item_a", "acid").unwrap();
    let item = store.get_item("item_a").unwrap().unwrap();
    assert!(!item.tags.contains(&"acid".to_string()));
    assert!(item.tags.contains(&"bright".to_string()));
}

#[test]
fn store_item_filter_by_search_and_tag() {
    let path = temp_library_path("filter");
    let store = LibraryStore::load_or_create(&path).unwrap();

    store.upsert_item(sample_item("item_a", "alpha")).unwrap();
    store.upsert_item(sample_item("item_b", "bravo")).unwrap();
    store.add_tag_to_item("item_a", "acid").unwrap();

    // Search by display_name substring (case-insensitive)
    let f = ItemFilter {
        search: Some("ALPH".into()),
        ..Default::default()
    };
    let hits = store.list_items(&f).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].item_id, "item_a");

    // Filter by tag
    let f = ItemFilter {
        tag: Some("acid".into()),
        ..Default::default()
    };
    let hits = store.list_items(&f).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].item_id, "item_a");

    // Source-kind filter
    let f = ItemFilter {
        source_kind: Some(SourceKind::File),
        ..Default::default()
    };
    assert_eq!(store.list_items(&f).unwrap().len(), 2);

    let f = ItemFilter {
        source_kind: Some(SourceKind::Generated),
        ..Default::default()
    };
    assert!(store.list_items(&f).unwrap().is_empty());
}

#[test]
fn store_item_filter_related_only_is_computed_dynamically() {
    let path = temp_library_path("filter-related");
    let store = LibraryStore::load_or_create(&path).unwrap();

    let mut a = sample_item("item_a", "alpha");
    a.scale_name = Some("Phrygian".into());
    let mut b = sample_item("item_b", "bravo");
    b.scale_name = Some("Phrygian".into());
    let c = sample_item("item_c", "charlie");

    store.upsert_item(a).unwrap();
    store.upsert_item(b).unwrap();
    store.upsert_item(c).unwrap();

    let hits = store
        .list_items(&ItemFilter {
            related_only: true,
            ..Default::default()
        })
        .unwrap();
    let ids: Vec<&str> = hits.iter().map(|item| item.item_id.as_str()).collect();
    assert_eq!(hits.len(), 2);
    assert!(ids.contains(&"item_a"));
    assert!(ids.contains(&"item_b"));
}

#[test]
fn store_item_filter_failed_imports_uses_file_index_failures() {
    let path = temp_library_path("filter-failed");
    let store = LibraryStore::load_or_create(&path).unwrap();

    let mut a = sample_item("item_a", "alpha");
    a.source_path = Some("/tmp/failing.seq".into());
    let mut b = sample_item("item_b", "bravo");
    b.source_path = Some("/tmp/ok.seq".into());
    store.upsert_item(a).unwrap();
    store.upsert_item(b).unwrap();
    store
        .append_file_index_entry(FileIndexEntry {
            path: "/tmp/failing.seq".into(),
            size: 128,
            hash_sha256: Some("deadbeef".into()),
            discovered_at: "20260101T000000Z".into(),
            format: Some("seq".into()),
            status: FileIngestStatus::Failed,
            error: Some("parse failed".into()),
            batch_id: Some("batch_failed".into()),
            duplicate_of: None,
            item_id: None,
        })
        .unwrap();

    let hits = store
        .list_items(&ItemFilter {
            failed_imports_only: true,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].item_id, "item_a");
}

// ---------------------------------------------------------------------------
// Compare
// ---------------------------------------------------------------------------

#[test]
fn compare_items_same_pattern() {
    let a = make_pattern(&[0, 2, 4, 5, 7, 9, 11, 12, 0, 2, 4, 5, 7, 9, 11, 12]);
    let b = make_pattern(&[0, 2, 4, 5, 7, 9, 11, 12, 0, 2, 4, 5, 7, 9, 11, 12]);
    let report = compare_items(&a, &b);
    assert!(report.identical);
    assert_eq!(report.note_diff, 0);
    assert!(report.summary.to_lowercase().contains("identical"));
}

#[test]
fn compare_items_diffs() {
    let a = make_pattern(&[0; 16]);
    let b = make_pattern(&[0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    let report = compare_items(&a, &b);
    assert!(!report.identical);
    assert_eq!(report.note_diff, 1);
    assert_eq!(report.accent_diff, 0);
    assert!(report.summary.contains("1 note change"));
}

#[test]
fn compare_snapshots_slot_states() {
    let src = vec![
        SnapshotSlot {
            snapshot_id: "s1".into(),
            slot_key: "G1-P1A".into(),
            item_id: Some("item_x".into()),
            empty: false,
            display_name: None,
        },
        SnapshotSlot {
            snapshot_id: "s1".into(),
            slot_key: "G1-P1B".into(),
            item_id: None,
            empty: true,
            display_name: None,
        },
    ];
    let dst = vec![
        SnapshotSlot {
            snapshot_id: "s2".into(),
            slot_key: "G1-P1A".into(),
            item_id: None,
            empty: true,
            display_name: None,
        },
        SnapshotSlot {
            snapshot_id: "s2".into(),
            slot_key: "G1-P1B".into(),
            item_id: Some("item_y".into()),
            empty: false,
            display_name: None,
        },
    ];

    let report = compare_snapshots(&src, &dst, |_| None);
    assert_eq!(report.slots.len(), 64);

    let g1p1a = report
        .slots
        .iter()
        .find(|r| r.slot_key == "G1-P1A")
        .unwrap();
    assert_eq!(g1p1a.state, SlotCompareState::SourceOnly);

    let g1p1b = report
        .slots
        .iter()
        .find(|r| r.slot_key == "G1-P1B")
        .unwrap();
    assert_eq!(g1p1b.state, SlotCompareState::TargetOnly);

    assert_eq!(report.source_only_count, 1);
    assert_eq!(report.target_only_count, 1);
    assert_eq!(report.empty_both_count, 62);
}

// ---------------------------------------------------------------------------
// Merge plan
// ---------------------------------------------------------------------------

#[test]
fn merge_plan_structure() {
    let src = vec![SnapshotSlot {
        snapshot_id: "s1".into(),
        slot_key: "G1-P1A".into(),
        item_id: Some("item_x".into()),
        empty: false,
        display_name: None,
    }];
    let dst: Vec<SnapshotSlot> = vec![];
    let compare = compare_snapshots(&src, &dst, |_| None);
    let plan = build_merge_plan("s1", "s2", &compare, &["G1-P1A".to_string()]);

    assert_eq!(plan.source_snapshot_id, "s1");
    assert_eq!(plan.target_snapshot_id, "s2");
    assert_eq!(plan.steps.len(), 64);
    let g1p1a = plan.steps.iter().find(|s| s.slot_key == "G1-P1A").unwrap();
    assert_eq!(g1p1a.action, MergeAction::Copy);
    assert!(plan.copy_count >= 1);
}

// ---------------------------------------------------------------------------
// Scanner
// ---------------------------------------------------------------------------

#[test]
fn scanner_classifies_extensions() {
    let root = std::env::temp_dir().join(format!(
        "td3-scanner-test-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    let files = [
        ("a.seq", "seq"),
        ("b.syx", "syx"),
        ("c.steps.txt", "steps"),
        ("d.mid", "mid"),
        ("e.unknown", ""),
    ];
    for (name, _) in files.iter() {
        std::fs::write(root.join(name), b"").unwrap();
    }

    let entries = scanner::scan_folder(&root, true).unwrap();
    assert!(entries.len() >= files.len());

    let get_format = |name: &str| -> Option<String> {
        entries
            .iter()
            .find(|e| e.path.to_lowercase().ends_with(name))
            .and_then(|e| e.format.clone())
    };
    let get_status = |name: &str| -> FileIngestStatus {
        entries
            .iter()
            .find(|e| e.path.to_lowercase().ends_with(name))
            .map(|e| e.status)
            .expect("entry present")
    };

    assert_eq!(get_format("a.seq"), Some("seq".into()));
    assert_eq!(get_format("b.syx"), Some("syx".into()));
    assert_eq!(get_format("c.steps.txt"), Some("steps".into()));
    assert_eq!(get_format("d.mid"), Some("mid".into()));
    assert_eq!(get_format("e.unknown"), None);
    assert_eq!(get_status("e.unknown"), FileIngestStatus::Unsupported);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn candidate_filename_filter() {
    use crate::library::ingest::is_candidate_filename;

    // Binary/parsed formats: accept by extension regardless of name.
    assert!(is_candidate_filename("foo.seq"));
    assert!(is_candidate_filename("FOO.SQS"));
    assert!(is_candidate_filename("bar.syx"));
    assert!(is_candidate_filename("bar.mid"));
    assert!(is_candidate_filename("bar.pat"));
    assert!(is_candidate_filename("bar.rbs"));

    // Only the `.steps.txt` suffix - never plain `.txt`.
    assert!(is_candidate_filename("G1P1A.steps.txt"));
    assert!(is_candidate_filename("anything.steps.txt"));
    assert!(!is_candidate_filename("notes.txt"));
    assert!(!is_candidate_filename("readme.txt"));

    // JSON / TOML: only when the name embeds a G\dP\d[AB] or G\d-P\d[AB]
    // slot marker (matches both shapes the exporter emits).
    assert!(is_candidate_filename("G1P1A.json"));
    assert!(is_candidate_filename("export-g4p8b.toml"));
    assert!(is_candidate_filename("my_G2P4A_session.json"));
    assert!(is_candidate_filename("G1-P1A.json"));
    assert!(is_candidate_filename("G4-P8B.toml"));
    assert!(is_candidate_filename("backup_g2-p4a_v3.json"));
    assert!(!is_candidate_filename("Cargo.toml"));
    assert!(!is_candidate_filename("package.json"));
    assert!(!is_candidate_filename("G9P9C.json")); // C is not a valid side
    assert!(!is_candidate_filename("GaPbA.json")); // non-digits
    assert!(!is_candidate_filename("G1--P1A.json")); // double-dash not allowed
    assert!(!is_candidate_filename("G1-PxA.json")); // non-digit after dash

    // Everything else is rejected.
    assert!(!is_candidate_filename("main.rs"));
    assert!(!is_candidate_filename(".gitignore"));
    assert!(!is_candidate_filename("image.png"));
}

#[test]
fn candidate_scan_skips_oversized_json_and_toml() {
    let dir = fresh_tmp_dir("candidate-size-limits");

    std::fs::write(dir.join("G1P1A.json"), vec![b' '; 2550]).unwrap();
    std::fs::write(dir.join("G1P1B.json"), vec![b' '; 2551]).unwrap();
    std::fs::write(dir.join("G1P2A.toml"), vec![b' '; 1900]).unwrap();
    std::fs::write(dir.join("G1P2B.toml"), vec![b' '; 1901]).unwrap();
    std::fs::write(dir.join("bank.sqs"), vec![0u8; 3000]).unwrap();

    let paths = crate::library::ingest::list_candidate_files(&dir, false).unwrap();
    let names: std::collections::HashSet<String> = paths
        .iter()
        .filter_map(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.to_string())
        })
        .collect();

    assert!(names.contains("G1P1A.json"));
    assert!(!names.contains("G1P1B.json"));
    assert!(names.contains("G1P2A.toml"));
    assert!(!names.contains("G1P2B.toml"));
    assert!(names.contains("bank.sqs"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn candidate_scan_orders_formats_by_import_priority() {
    let dir = fresh_tmp_dir("candidate-priority");
    let files = [
        "G4P2A.mid",
        "G4P2A.pat",
        "G4P2A.toml",
        "G4P2A.json",
        "G4P2A.steps.txt",
        "G4P2A.syx",
        "G4P2A.seq",
        "bank.sqs",
    ];
    for name in files {
        std::fs::write(dir.join(name), b"").unwrap();
    }

    let paths = crate::library::ingest::list_candidate_files(&dir, false).unwrap();
    let names: Vec<String> = paths
        .iter()
        .filter_map(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.to_string())
        })
        .collect();

    assert_eq!(
        names,
        vec![
            "G4P2A.seq",
            "G4P2A.syx",
            "G4P2A.steps.txt",
            "G4P2A.json",
            "G4P2A.toml",
            "G4P2A.pat",
            "G4P2A.mid",
            "bank.sqs",
        ]
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn derived_duplicate_truth_matches_same_folder_native_file() {
    let dir = fresh_tmp_dir("derived-truth");
    let other_dir = fresh_tmp_dir("derived-truth-other");
    let pat_path = dir.join("G1P1A.pat");
    let seq_path = dir.join("G1P1A.seq");
    let other_seq_path = other_dir.join("G1P1A.seq");

    std::fs::write(&pat_path, b"pat").unwrap();
    std::fs::write(&other_seq_path, b"seq").unwrap();
    assert!(crate::library::ingest::native_truth_for_derived_path(
        &pat_path,
        std::slice::from_ref(&other_seq_path)
    )
    .is_none());

    std::fs::write(&seq_path, b"seq").unwrap();
    let truth = crate::library::ingest::native_truth_for_derived_path(
        &pat_path,
        &[other_seq_path.clone(), seq_path.clone()],
    );
    assert_eq!(truth.as_deref(), Some(seq_path.as_path()));

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&other_dir);
}

// ---------------------------------------------------------------------------
// Backup inventory → snapshot sync
// ---------------------------------------------------------------------------

fn write_backup_zip(path: &std::path::Path, slot_folders: &[&str]) {
    let file = std::fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let opts: zip::write::SimpleFileOptions =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    // A minimal root entry so the zip is not literally empty.
    zip.start_file("bank_manifest.json", opts).unwrap();
    zip.write_all(b"{}").unwrap();
    for folder in slot_folders {
        zip.start_file(format!("{}/{}.syx", folder, folder), opts)
            .unwrap();
        zip.write_all(b"placeholder").unwrap();
    }
    zip.finish().unwrap();
}

fn write_backup_zip_with_bank_sqs(path: &std::path::Path, bank_sqs: &[u8]) {
    let file = std::fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let opts: zip::write::SimpleFileOptions =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("bank_manifest.json", opts).unwrap();
    zip.write_all(b"{}").unwrap();
    zip.start_file("bank.sqs", opts).unwrap();
    zip.write_all(bank_sqs).unwrap();
    zip.finish().unwrap();
}

#[test]
fn sync_backup_inventory_idempotent() {
    let dir = std::env::temp_dir().join(format!(
        "td3-sync-test-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let lib_path = temp_library_path("sync");
    let store = LibraryStore::load_or_create(&lib_path).unwrap();

    // Build a fake backup zip on disk with two populated slots.
    let zip_path = dir.join("bank_preimport_backup_20260414T111111-deadbeefcafebabe.zip");
    write_backup_zip(&zip_path, &["G1P1A", "G4P8B"]);

    let entry = BackupInventoryEntry {
        path: zip_path.clone(),
        filename: "bank_preimport_backup_20260414T111111-deadbeefcafebabe.zip".to_string(),
        kind: BackupKind::PreImport,
        timestamp: "20260414T111111".to_string(),
        short_hash: "deadbeefcafebabe".to_string(),
        size_bytes: std::fs::metadata(&zip_path).unwrap().len(),
    };

    // First sync: one snapshot added.
    let added = store
        .sync_backup_inventory(std::slice::from_ref(&entry))
        .unwrap();
    assert_eq!(added, 1);
    let snaps = store.list_snapshots().unwrap();
    assert_eq!(snaps.len(), 1);
    let snap = &snaps[0];
    assert_eq!(snap.origin, SnapshotOrigin::Backup);
    assert_eq!(
        snap.backup_path.as_deref(),
        Some(zip_path.to_string_lossy().as_ref())
    );
    assert_eq!(snap.slot_count, 2);

    // 64-slot grid: exactly two non-empty slots.
    let slots = store.list_snapshot_slots(&snap.snapshot_id).unwrap();
    assert_eq!(slots.len(), 64);
    let non_empty: Vec<&SnapshotSlot> = slots.iter().filter(|s| !s.empty).collect();
    assert_eq!(non_empty.len(), 2);
    assert!(non_empty.iter().any(|s| s.slot_key == "G1-P1A"));
    assert!(non_empty.iter().any(|s| s.slot_key == "G4-P8B"));

    // Second sync with the same entry: no new snapshot added.
    let added2 = store
        .sync_backup_inventory(std::slice::from_ref(&entry))
        .unwrap();
    assert_eq!(added2, 0);
    assert_eq!(store.list_snapshots().unwrap().len(), 1);
    drop(store);

    // Reloaded store must still treat SQLite as authoritative for idempotency.
    let reloaded = LibraryStore::load_or_create(&lib_path).unwrap();
    let added3 = reloaded.sync_backup_inventory(&[entry]).unwrap();
    assert_eq!(added3, 0);
    assert_eq!(reloaded.list_snapshots().unwrap().len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&lib_path);
}

#[test]
fn sync_backup_inventory_treats_content_hash_lookup_error_as_new_item() {
    let dir = std::env::temp_dir().join(format!(
        "td3-sync-lookup-error-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let lib_path = temp_library_path("sync-lookup-error");
    let store = LibraryStore::load_or_create(&lib_path).unwrap();

    let pattern = make_pattern(&[0, 2, 4, 5, 7, 9, 11, 12]);
    let content_hash = crate::library::duplicates::pattern_hash(&pattern);
    let bank_sqs = build_minimal_sqs_bank(&[0, 2, 4, 5, 7, 9, 11, 12]);
    let zip_path = dir.join("bank_ui_backup_20260414T111333-abcdef0123456789.zip");
    write_backup_zip_with_bank_sqs(&zip_path, &bank_sqs);

    {
        let conn = rusqlite::Connection::open(&lib_path).unwrap();
        conn.execute(
            "INSERT INTO items (position, item_id, json, content_hash) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                0i64,
                "bad_json_item",
                "{not valid json",
                content_hash.as_str()
            ],
        )
        .unwrap();
    }

    let entry = BackupInventoryEntry {
        path: zip_path.clone(),
        filename: "bank_ui_backup_20260414T111333-abcdef0123456789.zip".to_string(),
        kind: BackupKind::PreUi,
        timestamp: "20260414T111333".to_string(),
        short_hash: "abcdef0123456789".to_string(),
        size_bytes: std::fs::metadata(&zip_path).unwrap().len(),
    };

    let added = store.sync_backup_inventory(&[entry]).unwrap();
    assert_eq!(added, 1);

    let conn = rusqlite::Connection::open(&lib_path).unwrap();
    let matching_rows: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM items WHERE content_hash = ?1",
            rusqlite::params![content_hash.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(matching_rows, 2);
    assert_eq!(store.list_snapshots().unwrap().len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&lib_path);
}

#[test]
fn sync_backup_inventory_reports_bad_zip() {
    let dir = std::env::temp_dir().join(format!(
        "td3-sync-bad-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let lib_path = temp_library_path("sync-bad");
    let store = LibraryStore::load_or_create(&lib_path).unwrap();

    let zip_path = dir.join("bank_ui_backup_20260414T111222-abcdef0123456789.zip");
    std::fs::write(&zip_path, b"not a real zip").unwrap();

    let entry = BackupInventoryEntry {
        path: zip_path.clone(),
        filename: "bank_ui_backup_20260414T111222-abcdef0123456789.zip".to_string(),
        kind: BackupKind::PreUi,
        timestamp: "20260414T111222".to_string(),
        short_hash: "abcdef0123456789".to_string(),
        size_bytes: 14,
    };

    let err = store.sync_backup_inventory(&[entry]).unwrap_err();
    assert!(err.to_string().to_lowercase().contains("zip"));
    assert!(store.list_snapshots().unwrap().is_empty());

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&lib_path);
}

#[test]
fn bulk_tag_persists_across_reload() {
    let path = temp_library_path("bulk-tag");
    let store = LibraryStore::load_or_create(&path).unwrap();

    store.upsert_item(sample_item("item_a", "alpha")).unwrap();
    store.upsert_item(sample_item("item_b", "bravo")).unwrap();
    store
        .bulk_tag(
            &["item_a".to_string(), "item_b".to_string()],
            &["acid".to_string(), "bright".to_string()],
            &[],
        )
        .unwrap();
    drop(store);

    let reloaded = LibraryStore::load_or_create(&path).unwrap();
    let acid_hits = reloaded
        .list_items(&ItemFilter {
            tag: Some("acid".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(acid_hits.len(), 2);
    assert!(acid_hits
        .iter()
        .all(|item| item.tags.contains(&"bright".to_string())));

    let tags = reloaded.list_tags().unwrap();
    let user_labels: Vec<&str> = tags
        .iter()
        .filter(|tag| tag.kind != TagKind::System)
        .map(|tag| tag.label.as_str())
        .collect();
    assert!(user_labels.contains(&"acid"));
    assert!(user_labels.contains(&"bright"));
}

#[test]
fn list_pattern_relations_reads_from_sqlite() {
    let path = temp_library_path("relations");
    let mut data = store::LibraryData::default();
    data.pattern_relations.push(PatternRelation {
        from_item_id: "item_a".into(),
        to_item_id: "item_b".into(),
        kind: RelationKind::NearDuplicate,
        score: 0.91,
    });
    persistence::save(&path, &data).unwrap();

    let store = LibraryStore::load_or_create(&path).unwrap();
    let relations = store.list_pattern_relations().unwrap();
    assert_eq!(relations.len(), 1);
    assert_eq!(relations[0].from_item_id, "item_a");
    assert_eq!(relations[0].to_item_id, "item_b");
    assert_eq!(relations[0].kind, RelationKind::NearDuplicate);

    let conn = rusqlite::Connection::open(&path).unwrap();
    let stored_kind: String = conn
        .query_row(
            "SELECT kind FROM pattern_relations WHERE from_item_id = ?1",
            rusqlite::params!["item_a"],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored_kind, "near_duplicate");
}

#[test]
fn store_save_preserves_sql_only_tables() {
    let path = temp_library_path("save-preserves-sql");
    let store = LibraryStore::load_or_create(&path).unwrap();
    store.upsert_item(sample_item("item_a", "alpha")).unwrap();

    let batch = store.create_import_batch(Some("/tmp".into())).unwrap();
    store
        .append_file_index_entry(FileIndexEntry {
            path: "/tmp/alpha.seq".into(),
            size: 128,
            hash_sha256: Some("deadbeef".into()),
            discovered_at: "20260101T000000Z".into(),
            format: Some("seq".into()),
            status: FileIngestStatus::Imported,
            error: None,
            batch_id: Some(batch.batch_id.clone()),
            duplicate_of: None,
            item_id: Some("item_a".into()),
        })
        .unwrap();

    store.save().unwrap();
    drop(store);

    let reloaded = LibraryStore::load_or_create(&path).unwrap();
    assert_eq!(reloaded.list_import_batches().unwrap().len(), 1);
    assert_eq!(reloaded.list_file_index().unwrap().len(), 1);
    assert_eq!(
        reloaded.list_items(&ItemFilter::default()).unwrap().len(),
        1
    );
}

#[test]
fn now_iso_has_expected_shape() {
    let ts = store::now_iso();
    // YYYY-MM-DD_HH-MM-SSZ -> 20 characters
    assert_eq!(ts.len(), 20, "unexpected timestamp: {}", ts);
    assert!(ts.ends_with('Z'));
    assert_eq!(ts.chars().nth(4), Some('-'));
    assert_eq!(ts.chars().nth(7), Some('-'));
    assert_eq!(ts.chars().nth(10), Some('_'));
    assert_eq!(ts.chars().nth(13), Some('-'));
    assert_eq!(ts.chars().nth(16), Some('-'));
}

// ---------------------------------------------------------------------------
// Ingest pipeline
// ---------------------------------------------------------------------------

use crate::formats::rbs as rbs_format;
use crate::formats::seq as seq_format;
use crate::formats::sqs as sqs_format;
use crate::formats::syx as syx_format;
use crate::library::ingest;
use crate::pattern::pattern_to_sysex;

/// Lay down a fresh scratch dir for a single ingest test and return its path.
/// Caller is responsible for `remove_dir_all` at the end.
fn fresh_tmp_dir(tag: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("td3-ingest-test-{}-{}-{}", tag, pid, n));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir scratch");
    dir
}

fn write_seq_file(dir: &std::path::Path, name: &str, notes: &[u8]) -> PathBuf {
    let pattern = make_pattern(notes);
    let bytes = seq_format::export(&pattern).expect("export seq");
    let path = dir.join(name);
    std::fs::write(&path, &bytes).expect("write seq");
    path
}

#[test]
fn ingest_seq_file_creates_item() {
    let dir = fresh_tmp_dir("seq-item");
    let lib_path = temp_library_path("seq-ingest");
    let store = LibraryStore::load_or_create(&lib_path).unwrap();

    let seq_path = write_seq_file(&dir, "sample.seq", &[0, 2, 4, 5, 7, 9, 11, 12]);
    let batch = store
        .create_import_batch(Some(dir.to_string_lossy().to_string()))
        .unwrap();

    let outcome = ingest::ingest_path(
        &store,
        &seq_path,
        &batch.batch_id,
        &MidiImportOptions::default(),
    )
    .unwrap();
    assert_eq!(outcome.entry.status, FileIngestStatus::Imported);
    assert_eq!(outcome.entry.format.as_deref(), Some("seq"));
    assert!(outcome.entry.item_id.is_some(), "item_id must be wired");
    assert!(
        outcome.entry.hash_sha256.is_some(),
        "file hash must be captured"
    );

    let items = store.list_items(&ItemFilter::default()).unwrap();
    assert_eq!(items.len(), 1);
    assert!(
        items[0].content_hash.is_some(),
        "ingested items must carry a content_hash"
    );
    assert!(items[0].tags.iter().any(|t| t == "format:seq"));

    // Auto-kind tag bookkeeping
    let tags = store.list_tags().unwrap();
    let fmt_tag = tags
        .iter()
        .find(|t| t.label == "format:seq")
        .expect("format:seq tag was ensured");
    assert_eq!(fmt_tag.kind, TagKind::Auto);

    // Batch entry is persisted
    let entries = store.list_batch_entries(&batch.batch_id).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].status, FileIngestStatus::Imported);

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&lib_path);
}

#[test]
fn ingest_steps_file_accepts_rows_only_through_active_steps() {
    let dir = fresh_tmp_dir("steps-active-only");
    let lib_path = temp_library_path("steps-active-only-ingest");
    let store = LibraryStore::load_or_create(&lib_path).unwrap();

    let steps_path = dir.join("active-only.steps.txt");
    std::fs::write(
        &steps_path,
        include_str!("../../tests/fixtures/eight_step_active_only.steps.txt"),
    )
    .expect("write steps txt");
    let batch = store
        .create_import_batch(Some(dir.to_string_lossy().to_string()))
        .unwrap();

    let outcome = ingest::ingest_path(
        &store,
        &steps_path,
        &batch.batch_id,
        &MidiImportOptions::default(),
    )
    .unwrap();
    assert_eq!(outcome.entry.status, FileIngestStatus::Imported);
    assert_eq!(outcome.entry.format.as_deref(), Some("steps"));
    assert!(outcome.entry.item_id.is_some(), "item_id must be wired");

    let items = store.list_items(&ItemFilter::default()).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].format.as_deref(), Some("steps"));
    assert!(items[0].tags.iter().any(|t| t == "format:steps"));

    let sidecar_dir = store.pattern_sidecar_dir();
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&lib_path);
    let _ = std::fs::remove_dir_all(&sidecar_dir);
}

#[test]
fn ingest_duplicate_file_marked_skipped() {
    let dir = fresh_tmp_dir("dup");
    let lib_path = temp_library_path("dup-ingest");
    let store = LibraryStore::load_or_create(&lib_path).unwrap();

    let a = write_seq_file(&dir, "first.seq", &[1, 2, 3, 4]);
    let b = write_seq_file(&dir, "second.seq", &[1, 2, 3, 4]); // identical bytes
    let batch = store.create_import_batch(None).unwrap();

    let first =
        ingest::ingest_path(&store, &a, &batch.batch_id, &MidiImportOptions::default()).unwrap();
    assert_eq!(first.entry.status, FileIngestStatus::Imported);
    let first_item_id = first.entry.item_id.clone().expect("first has item");

    let second =
        ingest::ingest_path(&store, &b, &batch.batch_id, &MidiImportOptions::default()).unwrap();
    assert_eq!(second.entry.status, FileIngestStatus::DuplicateSkipped);
    assert_eq!(
        second.entry.duplicate_of.as_deref(),
        Some(first_item_id.as_str())
    );

    // Only one LibraryItem persists.
    let items = store.list_items(&ItemFilter::default()).unwrap();
    assert_eq!(items.len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&lib_path);
}

#[test]
fn ingest_duplicate_priority_prefers_seq_over_syx() {
    let dir = fresh_tmp_dir("priority-dup");
    let lib_path = temp_library_path("priority-dup-ingest");
    let store = LibraryStore::load_or_create(&lib_path).unwrap();

    let pattern = make_pattern(&[0, 2, 4, 5, 7, 9, 11, 12]);
    let seq_path = dir.join("G1P1A.seq");
    let syx_path = dir.join("G1P1A.syx");
    std::fs::write(&seq_path, seq_format::export(&pattern).unwrap()).unwrap();
    std::fs::write(&syx_path, syx_format::export(&pattern, 0, 0, 0).unwrap()).unwrap();

    let mut paths = vec![syx_path.clone(), seq_path.clone()];
    ingest::sort_import_paths(&mut paths);
    assert_eq!(
        paths[0].file_name().and_then(|name| name.to_str()),
        Some("G1P1A.seq")
    );
    assert_eq!(
        paths[1].file_name().and_then(|name| name.to_str()),
        Some("G1P1A.syx")
    );

    let batch = store.create_import_batch(None).unwrap();
    let mut entries = Vec::new();
    for path in paths {
        let outcome = ingest::ingest_path(
            &store,
            &path,
            &batch.batch_id,
            &MidiImportOptions::default(),
        )
        .unwrap();
        entries.push(outcome.entry);
    }

    assert_eq!(entries[0].format.as_deref(), Some("seq"));
    assert_eq!(entries[0].status, FileIngestStatus::Imported);
    assert_eq!(entries[1].format.as_deref(), Some("syx"));
    assert_eq!(entries[1].status, FileIngestStatus::DuplicateSkipped);

    let items = store.list_items(&ItemFilter::default()).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].format.as_deref(), Some("seq"));

    let sidecar_dir = store.pattern_sidecar_dir();
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&lib_path);
    let _ = std::fs::remove_dir_all(&sidecar_dir);
}

#[test]
fn delete_import_batch_removes_entries_and_owned_items() {
    let dir = fresh_tmp_dir("del-batch");
    let lib_path = temp_library_path("del-batch");
    let store = LibraryStore::load_or_create(&lib_path).unwrap();

    // Batch A imports a single .seq → creates item A. Batch B is separate
    // and imports a different .seq → creates item B.
    let a_seq = write_seq_file(&dir, "a.seq", &[1, 2, 3, 4]);
    let b_seq = write_seq_file(&dir, "b.seq", &[9, 8, 7, 6]);

    let batch_a = store
        .create_import_batch(Some(dir.to_string_lossy().to_string()))
        .unwrap();
    let oa = ingest::ingest_path(
        &store,
        &a_seq,
        &batch_a.batch_id,
        &MidiImportOptions::default(),
    )
    .unwrap();
    assert_eq!(oa.entry.status, FileIngestStatus::Imported);
    let item_a = oa.entry.item_id.clone().expect("item A created");

    let batch_b = store.create_import_batch(None).unwrap();
    let ob = ingest::ingest_path(
        &store,
        &b_seq,
        &batch_b.batch_id,
        &MidiImportOptions::default(),
    )
    .unwrap();
    assert_eq!(ob.entry.status, FileIngestStatus::Imported);
    let item_b = ob.entry.item_id.clone().expect("item B created");

    // Sanity: two items, two batches, two file-index rows.
    assert_eq!(store.list_items(&ItemFilter::default()).unwrap().len(), 2);
    assert_eq!(store.list_import_batches().unwrap().len(), 2);

    // Delete batch A → its entry + item A are gone; batch B + item B remain.
    let report = store.delete_import_batch(&batch_a.batch_id).unwrap();
    assert_eq!(report.removed_entries, 1);
    assert_eq!(report.removed_items, 1);
    assert_eq!(report.removed_snapshots, 0);

    let items_after = store.list_items(&ItemFilter::default()).unwrap();
    assert_eq!(items_after.len(), 1);
    assert_eq!(items_after[0].item_id, item_b);

    let batches_after = store.list_import_batches().unwrap();
    assert_eq!(batches_after.len(), 1);
    assert_eq!(batches_after[0].batch_id, batch_b.batch_id);

    // Sidecar for item A gone, sidecar for item B survives.
    assert!(store.pattern_bytes_for(&item_a).is_none());
    assert!(store.pattern_bytes_for(&item_b).is_some());

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&lib_path);
}

#[test]
fn delete_import_batch_preserves_items_referenced_by_other_batches() {
    let dir = fresh_tmp_dir("del-batch-dup");
    let lib_path = temp_library_path("del-batch-dup");
    let store = LibraryStore::load_or_create(&lib_path).unwrap();

    // Batch A creates item I. Batch B sees the same content at a different
    // path and records a DuplicateSkipped entry pointing to item I. Deleting
    // batch A must NOT delete item I because batch B still depends on it.
    let a = write_seq_file(&dir, "first.seq", &[1, 2, 3, 4]);
    let b = write_seq_file(&dir, "second.seq", &[1, 2, 3, 4]);

    let batch_a = store.create_import_batch(None).unwrap();
    let oa =
        ingest::ingest_path(&store, &a, &batch_a.batch_id, &MidiImportOptions::default()).unwrap();
    let item_i = oa.entry.item_id.clone().unwrap();

    let batch_b = store.create_import_batch(None).unwrap();
    let ob =
        ingest::ingest_path(&store, &b, &batch_b.batch_id, &MidiImportOptions::default()).unwrap();
    assert_eq!(ob.entry.status, FileIngestStatus::DuplicateSkipped);
    assert_eq!(ob.entry.duplicate_of.as_deref(), Some(item_i.as_str()));

    let report = store.delete_import_batch(&batch_a.batch_id).unwrap();
    assert_eq!(report.removed_entries, 1);
    assert_eq!(
        report.removed_items, 0,
        "item is still referenced by batch B - must survive"
    );

    // Item I is still there.
    let items = store.list_items(&ItemFilter::default()).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].item_id, item_i);

    // Batch B survives too, its DuplicateSkipped entry untouched.
    let entries = store.list_batch_entries(&batch_b.batch_id).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].status, FileIngestStatus::DuplicateSkipped);
    assert_eq!(entries[0].duplicate_of.as_deref(), Some(item_i.as_str()));

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&lib_path);
}

#[test]
fn delete_import_batch_leaves_source_files_on_disk() {
    let dir = fresh_tmp_dir("del-batch-disk");
    let lib_path = temp_library_path("del-batch-disk");
    let store = LibraryStore::load_or_create(&lib_path).unwrap();

    let seq = write_seq_file(&dir, "keep-me.seq", &[5, 6, 7, 8]);
    let batch = store.create_import_batch(None).unwrap();
    let _ =
        ingest::ingest_path(&store, &seq, &batch.batch_id, &MidiImportOptions::default()).unwrap();

    assert!(seq.exists(), "precondition: file is on disk");
    store.delete_import_batch(&batch.batch_id).unwrap();
    assert!(seq.exists(), "source file must survive a batch delete");

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&lib_path);
}

#[test]
fn delete_import_batch_reloads_current_sqlite_state() {
    let path = temp_library_path("del-batch-reload");
    let store = LibraryStore::load_or_create(&path).unwrap();

    let mut data = store::LibraryData::default();
    data.items.push(sample_item("item_a", "alpha"));
    data.file_index.push(FileIndexEntry {
        path: "/tmp/alpha.seq".into(),
        size: 128,
        hash_sha256: Some("deadbeef".into()),
        discovered_at: "20260101T000000Z".into(),
        format: Some("seq".into()),
        status: FileIngestStatus::Imported,
        error: None,
        batch_id: Some("batch_sql".into()),
        duplicate_of: None,
        item_id: Some("item_a".into()),
    });
    data.import_batches.push(ImportBatch {
        batch_id: "batch_sql".into(),
        started_at: "20260101T000000Z".into(),
        finished_at: Some("20260101T000100Z".into()),
        scan_root: Some("/tmp".into()),
        files_found: 1,
        files_imported: 1,
        duplicates_skipped: 0,
        unsupported: 0,
        failed: 0,
    });
    persistence::save(&path, &data).unwrap();

    let report = store.delete_import_batch("batch_sql").unwrap();
    assert_eq!(report.removed_entries, 1);
    assert_eq!(report.removed_items, 1);
    assert_eq!(report.removed_snapshots, 0);
    assert!(store.list_items(&ItemFilter::default()).unwrap().is_empty());
    assert!(store.list_file_index().unwrap().is_empty());
    assert!(store.list_import_batches().unwrap().is_empty());
}

#[test]
fn delete_import_batch_scrubs_deleted_item_from_surviving_snapshot_slot() {
    let path = temp_library_path("del-batch-slot-scrub");
    let store = LibraryStore::load_or_create(&path).unwrap();

    let mut data = store::LibraryData::default();
    let mut item_a = sample_item("item_a", "alpha");
    item_a.source_path = Some("/tmp/a.seq".into());
    let mut item_b = sample_item("item_b", "bravo");
    item_b.source_path = Some("/tmp/other.seq".into());
    data.items.push(item_a);
    data.items.push(item_b);
    data.file_index.push(FileIndexEntry {
        path: "/tmp/a.seq".into(),
        size: 128,
        hash_sha256: Some("deadbeef".into()),
        discovered_at: "20260101T000000Z".into(),
        format: Some("seq".into()),
        status: FileIngestStatus::Imported,
        error: None,
        batch_id: Some("batch_sql".into()),
        duplicate_of: None,
        item_id: Some("item_a".into()),
    });
    data.import_batches.push(ImportBatch {
        batch_id: "batch_sql".into(),
        started_at: "20260101T000000Z".into(),
        finished_at: Some("20260101T000100Z".into()),
        scan_root: Some("/tmp".into()),
        files_found: 1,
        files_imported: 1,
        duplicates_skipped: 0,
        unsupported: 0,
        failed: 0,
    });
    data.snapshots.push(crate::library::model::Snapshot {
        snapshot_id: "snap_mixed".into(),
        name: "custom-mixed".into(),
        created_at: "20260101T000000Z".into(),
        origin: SnapshotOrigin::Imported,
        slot_count: 2,
        description: None,
        pinned: false,
        tags: Vec::new(),
        backup_path: None,
    });
    data.snapshot_slots.push(SnapshotSlot {
        snapshot_id: "snap_mixed".into(),
        slot_key: "G1-P1A".into(),
        item_id: Some("item_a".into()),
        empty: false,
        display_name: Some("G1-P1A".into()),
    });
    data.snapshot_slots.push(SnapshotSlot {
        snapshot_id: "snap_mixed".into(),
        slot_key: "G1-P1B".into(),
        item_id: Some("item_b".into()),
        empty: false,
        display_name: Some("G1-P1B".into()),
    });
    persistence::save(&path, &data).unwrap();

    let report = store.delete_import_batch("batch_sql").unwrap();
    assert_eq!(report.removed_entries, 1);
    assert_eq!(report.removed_items, 1);
    assert_eq!(report.removed_snapshots, 0);

    let slots = store.list_snapshot_slots("snap_mixed").unwrap();
    let deleted_slot = slots.iter().find(|slot| slot.slot_key == "G1-P1A").unwrap();
    assert!(deleted_slot.empty);
    assert!(deleted_slot.item_id.is_none());

    let surviving_slot = slots.iter().find(|slot| slot.slot_key == "G1-P1B").unwrap();
    assert_eq!(surviving_slot.item_id.as_deref(), Some("item_b"));
    assert!(!surviving_slot.empty);
}

#[test]
fn ingest_unsupported_file_marked_unsupported() {
    let dir = fresh_tmp_dir("unsupported");
    let lib_path = temp_library_path("unsupp-ingest");
    let store = LibraryStore::load_or_create(&lib_path).unwrap();

    let path = dir.join("notes.txt");
    std::fs::write(&path, b"hello world").unwrap();
    let batch = store.create_import_batch(None).unwrap();

    let outcome = ingest::ingest_path(
        &store,
        &path,
        &batch.batch_id,
        &MidiImportOptions::default(),
    )
    .unwrap();
    assert_eq!(outcome.entry.status, FileIngestStatus::Unsupported);
    assert!(outcome.entry.item_id.is_none());
    assert!(store.list_items(&ItemFilter::default()).unwrap().is_empty());

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&lib_path);
}

#[test]
fn ingest_failed_parse_recorded() {
    let dir = fresh_tmp_dir("failed");
    let lib_path = temp_library_path("failed-ingest");
    let store = LibraryStore::load_or_create(&lib_path).unwrap();

    // A .seq file whose bytes are not valid .seq (wrong magic).
    let path = dir.join("broken.seq");
    std::fs::write(&path, b"not a real seq file at all").unwrap();
    let batch = store.create_import_batch(None).unwrap();

    let outcome = ingest::ingest_path(
        &store,
        &path,
        &batch.batch_id,
        &MidiImportOptions::default(),
    )
    .unwrap();
    assert_eq!(outcome.entry.status, FileIngestStatus::Failed);
    assert!(outcome.entry.error.is_some());

    // No LibraryItem created.
    assert!(store.list_items(&ItemFilter::default()).unwrap().is_empty());

    // But the file index row is persisted so the UI can show it.
    let entries = store.list_batch_entries(&batch.batch_id).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].status, FileIngestStatus::Failed);

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&lib_path);
}

/// Build a minimal `.sqs` bank where record 0 is a real pattern and the
/// other 63 slots are silent (all-REST). Returns the bank byte vector.
fn build_minimal_sqs_bank(pattern_notes: &[u8]) -> Vec<u8> {
    let pattern = make_pattern(pattern_notes);
    let sysex = pattern_to_sysex(&pattern, 0, 0, 0).expect("sysex");
    let real_payload: Vec<u8> = sysex[3..].to_vec();
    assert_eq!(real_payload.len(), 112);

    // A silent payload: all-REST sentinel. The 112-byte buffer is zeroed and
    // the 4-byte REST mask at offset 0x6C is set to 0F 0F 0F 0F.
    let mut silent = vec![0u8; 112];
    silent[0x6C] = 0x0F;
    silent[0x6D] = 0x0F;
    silent[0x6E] = 0x0F;
    silent[0x6F] = 0x0F;

    let mut records: Vec<sqs_format::BankRecord> = Vec::with_capacity(64);
    for slot_index in 0..64u8 {
        let group = slot_index / 16;
        let slot_addr = slot_index % 16;
        let payload = if slot_index == 0 {
            real_payload.clone()
        } else {
            silent.clone()
        };
        records.push(sqs_format::BankRecord {
            group,
            slot_addr,
            payload,
        });
    }
    let arr: [sqs_format::BankRecord; 64] = records.try_into().unwrap();
    let bank = sqs_format::Bank {
        product_bytes: sqs_format::PRODUCT_UTF16BE.to_vec(),
        version_bytes: sqs_format::VERSION_UTF16BE.to_vec(),
        records: arr,
    };
    sqs_format::serialize_bank(&bank).expect("serialize bank")
}

#[test]
fn ingest_sqs_creates_snapshot_and_items() {
    let dir = fresh_tmp_dir("sqs");
    let lib_path = temp_library_path("sqs-ingest");
    let store = LibraryStore::load_or_create(&lib_path).unwrap();

    let bytes = build_minimal_sqs_bank(&[0, 2, 4, 5, 7, 9, 11, 12]);
    let path = dir.join("bank.sqs");
    std::fs::write(&path, &bytes).unwrap();

    let batch = store.create_import_batch(None).unwrap();
    let outcome = ingest::ingest_path(
        &store,
        &path,
        &batch.batch_id,
        &MidiImportOptions::default(),
    )
    .unwrap();
    assert_eq!(outcome.entry.status, FileIngestStatus::Imported);

    // Exactly one snapshot created, with 64 slot rows (from upsert_snapshot_slot
    // per non-silent + padded for silent).
    let snaps = store.list_snapshots().unwrap();
    let sqs_snap = snaps
        .iter()
        .find(|s| s.origin == SnapshotOrigin::Imported)
        .expect("imported snapshot exists");
    let slots = store.list_snapshot_slots(&sqs_snap.snapshot_id).unwrap();
    assert_eq!(slots.len(), 64);
    let non_empty: Vec<_> = slots.iter().filter(|s| !s.empty).collect();
    assert_eq!(non_empty.len(), 1, "only the seeded slot is populated");
    assert_eq!(non_empty[0].slot_key, "G1-P1A");
    assert!(non_empty[0].item_id.is_some());

    // And exactly one LibraryItem was created (snapshot-slot-backed).
    let items = store.list_items(&ItemFilter::default()).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].source_kind, SourceKind::SnapshotSlot);
    assert!(items[0].tags.iter().any(|t| t == "snapshot-origin"));

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&lib_path);
}

#[test]
fn persist_snapshot_slot_failure_marks_entry_failed() {
    let path = temp_library_path("slot-fail");
    let store = LibraryStore::load_or_create(&path).unwrap();
    let snapshot = store
        .create_snapshot("slot failure".to_string(), None, SnapshotOrigin::Imported)
        .unwrap();
    let mut entry = FileIndexEntry {
        path: "bank.sqs".to_string(),
        size: 0,
        hash_sha256: None,
        discovered_at: store::now_iso(),
        format: Some("sqs".to_string()),
        status: FileIngestStatus::Parsed,
        error: None,
        batch_id: None,
        duplicate_of: None,
        item_id: None,
    };
    let slot = SnapshotSlot {
        snapshot_id: snapshot.snapshot_id,
        slot_key: "G1-P1A".to_string(),
        item_id: None,
        empty: true,
        display_name: Some("G1-P1A".to_string()),
    };

    let parked = replace_db_with_directory(&path);
    let ok = ingest::persist_snapshot_slot(&store, &mut entry, slot);
    restore_db_from_park(&path, &parked);

    assert!(!ok);
    assert_eq!(entry.status, FileIngestStatus::Failed);
    let err = entry.error.unwrap_or_default();
    assert!(
        err.contains("G1-P1A: slot:"),
        "slot write error must be surfaced: {}",
        err
    );
}

/// Build an `.rbs` blob with the given authored patterns placed at their
/// `(device, group, slot)` addresses; all other slots stay silent (all-rest).
fn build_rbs_with_slots(authored: Vec<(usize, usize, usize, Pattern)>) -> Vec<u8> {
    let mut song = rbs_format::RbsSong::blank().expect("blank rbs");
    for (device, group, slot, pat) in authored.into_iter() {
        song.set_pattern(device, group, slot, pat);
    }
    song.serialize().expect("serialize rbs")
}

#[test]
fn ingest_rbs_creates_snapshot_and_single_item() {
    let dir = fresh_tmp_dir("rbs");
    let lib_path = temp_library_path("rbs-ingest");
    let store = LibraryStore::load_or_create(&lib_path).unwrap();

    let pat = make_pattern(&[0, 2, 4, 5, 7, 9, 11, 12]);
    let bytes = build_rbs_with_slots(vec![(0, 0, 0, pat)]);
    let path = dir.join("song.rbs");
    std::fs::write(&path, &bytes).unwrap();

    let batch = store.create_import_batch(None).unwrap();
    let outcome = ingest::ingest_path(
        &store,
        &path,
        &batch.batch_id,
        &MidiImportOptions::default(),
    )
    .unwrap();
    assert_eq!(outcome.entry.status, FileIngestStatus::Imported);

    let snaps = store.list_snapshots().unwrap();
    let rbs_snap = snaps
        .iter()
        .find(|s| s.origin == SnapshotOrigin::Imported)
        .expect("imported snapshot exists");
    let slots = store.list_snapshot_slots(&rbs_snap.snapshot_id).unwrap();
    assert_eq!(slots.len(), 64);
    let non_empty: Vec<_> = slots.iter().filter(|s| !s.empty).collect();
    assert_eq!(non_empty.len(), 1, "only the authored slot is populated");
    assert_eq!(non_empty[0].slot_key, "G1-P1A");
    assert!(non_empty[0].item_id.is_some());

    let items = store.list_items(&ItemFilter::default()).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].source_kind, SourceKind::SnapshotSlot);
    assert_eq!(items[0].format.as_deref(), Some("rbs"));
    assert!(items[0].tags.iter().any(|t| t == "snapshot-origin"));
    assert!(items[0].tags.iter().any(|t| t == "format:rbs"));

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&lib_path);
}

#[test]
fn ingest_rbs_registers_slots_on_both_devices() {
    let dir = fresh_tmp_dir("rbs-ab");
    let lib_path = temp_library_path("rbs-ab-ingest");
    let store = LibraryStore::load_or_create(&lib_path).unwrap();

    let a_side = make_pattern(&[0, 2, 4, 5, 7, 9, 11, 12]);
    let b_side = make_pattern(&[12, 11, 9, 7, 5, 4, 2, 0]);
    let bytes = build_rbs_with_slots(vec![(0, 0, 0, a_side), (1, 0, 0, b_side)]);
    let path = dir.join("song.rbs");
    std::fs::write(&path, &bytes).unwrap();

    let batch = store.create_import_batch(None).unwrap();
    let outcome = ingest::ingest_path(
        &store,
        &path,
        &batch.batch_id,
        &MidiImportOptions::default(),
    )
    .unwrap();
    assert_eq!(outcome.entry.status, FileIngestStatus::Imported);

    let snaps = store.list_snapshots().unwrap();
    let rbs_snap = snaps
        .iter()
        .find(|s| s.origin == SnapshotOrigin::Imported)
        .expect("imported snapshot exists");
    let slots = store.list_snapshot_slots(&rbs_snap.snapshot_id).unwrap();
    let non_empty: Vec<_> = slots.iter().filter(|s| !s.empty).collect();
    assert_eq!(non_empty.len(), 2, "A-side + B-side authored slots");
    let keys: Vec<&str> = non_empty.iter().map(|s| s.slot_key.as_str()).collect();
    assert!(
        keys.contains(&"G1-P1A"),
        "A-side slot key present: {:?}",
        keys
    );
    assert!(
        keys.contains(&"G1-P1B"),
        "B-side slot key present: {:?}",
        keys
    );

    let items = store.list_items(&ItemFilter::default()).unwrap();
    assert_eq!(items.len(), 2);

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&lib_path);
}

#[test]
fn ingest_rbs_jam_fixture_registers_authored_slot() {
    let dir = fresh_tmp_dir("rbs-jam");
    let lib_path = temp_library_path("rbs-jam-ingest");
    let store = LibraryStore::load_or_create(&lib_path).unwrap();

    let fixture = PathBuf::from("docs/JAM PATTERN.rbs");
    if !fixture.exists() {
        eprintln!("skipping: {} not found", fixture.display());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&lib_path);
        return;
    }
    let bytes = std::fs::read(&fixture).expect("read JAM fixture");
    let path = dir.join("jam.rbs");
    std::fs::write(&path, &bytes).unwrap();

    let batch = store.create_import_batch(None).unwrap();
    let outcome = ingest::ingest_path(
        &store,
        &path,
        &batch.batch_id,
        &MidiImportOptions::default(),
    )
    .unwrap();
    assert_eq!(outcome.entry.status, FileIngestStatus::Imported);

    let snaps = store.list_snapshots().unwrap();
    let rbs_snap = snaps
        .iter()
        .find(|s| s.origin == SnapshotOrigin::Imported)
        .expect("imported snapshot exists");
    let slots = store.list_snapshot_slots(&rbs_snap.snapshot_id).unwrap();
    assert_eq!(slots.len(), 64);
    let a1 = slots
        .iter()
        .find(|s| s.slot_key == "G1-P1A")
        .expect("G1-P1A slot present");
    assert!(!a1.empty, "JAM fixture has authored content at G1-P1A");
    let non_empty_count = slots.iter().filter(|s| !s.empty).count();
    let empty_count = slots.iter().filter(|s| s.empty).count();
    assert!(empty_count > 0, "at least some JAM slots filtered as empty");

    let items = store.list_items(&ItemFilter::default()).unwrap();
    assert!(!items.is_empty(), "JAM fixture should yield items");
    assert!(items.len() <= 64, "items never exceed 64 slots");
    assert!(
        items.len() <= non_empty_count,
        "items <= non-empty slot count"
    );

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&lib_path);
}

#[test]
fn retry_failed_reprocesses_entries() {
    let dir = fresh_tmp_dir("retry");
    let lib_path = temp_library_path("retry-ingest");
    let store = LibraryStore::load_or_create(&lib_path).unwrap();

    // Start with a broken .seq - ingest marks it Failed.
    let path = dir.join("late.seq");
    std::fs::write(&path, b"bad seq bytes").unwrap();
    let batch = store.create_import_batch(None).unwrap();
    let first = ingest::ingest_path(
        &store,
        &path,
        &batch.batch_id,
        &MidiImportOptions::default(),
    )
    .unwrap();
    assert_eq!(first.entry.status, FileIngestStatus::Failed);

    // Fix the file on disk, then retry.
    let pattern = make_pattern(&[7, 9, 11, 12]);
    let fixed = seq_format::export(&pattern).expect("export");
    std::fs::write(&path, &fixed).unwrap();

    let retried = ingest::retry_failed(&store, first.entry, &MidiImportOptions::default()).unwrap();
    assert_eq!(retried.status, FileIngestStatus::Imported);
    assert!(retried.item_id.is_some());

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&lib_path);
}

// ---------------------------------------------------------------------------
// Compare (extended fields) + duplicate clustering + sidecars
// ---------------------------------------------------------------------------

use crate::library::duplicates::{compute_clusters, DuplicateClusterKind};

fn make_pattern_with_accent(notes: &[u8], accents: &[bool]) -> Pattern {
    let mut steps: [step::Step; 16] = Default::default();
    for (i, &n) in notes.iter().enumerate().take(16) {
        let accent = if *accents.get(i).unwrap_or(&false) {
            step::Accent::On
        } else {
            step::Accent::Off
        };
        steps[i] = step::Step {
            note: n,
            transpose: step::Transpose::Normal,
            accent,
            slide: step::Slide::Off,
            time: step::Time::Normal,
        };
    }
    Pattern::new(false, 16, steps).expect("valid pattern")
}

#[test]
fn compare_items_exact_match_returns_zero_diffs() {
    let a = make_pattern(&[0, 2, 4, 5, 7, 9, 11, 12, 0, 2, 4, 5, 7, 9, 11, 12]);
    let b = make_pattern(&[0, 2, 4, 5, 7, 9, 11, 12, 0, 2, 4, 5, 7, 9, 11, 12]);
    let r = compare_items(&a, &b);
    assert!(r.identical);
    assert_eq!(r.note_diff, 0);
    assert_eq!(r.differ_steps.len(), 0);
    assert!(r.same_rhythm);
    assert!((r.duplicate_score - 1.0).abs() < 1e-6);
    assert!((r.relatedness_score - 1.0).abs() < 1e-6);
}

#[test]
fn compare_items_note_changes_counted() {
    let a = make_pattern(&[0; 16]);
    let mut notes_b = [0u8; 16];
    notes_b[3] = 5;
    notes_b[7] = 9;
    let b = make_pattern(&notes_b);
    let r = compare_items(&a, &b);
    assert_eq!(r.note_diff, 2);
    assert_eq!(r.differ_steps, vec![3, 7]);
    assert!(r.same_rhythm, "all Normal gates → identical rhythm");
    // Two edits, same rhythm, no transpose → near-duplicate zone.
    assert!(r.duplicate_score >= 0.7 && r.duplicate_score <= 0.95);
    assert!(r.relatedness_score < 1.0);
}

#[test]
fn compare_items_rhythm_same_returns_same_rhythm_flag() {
    // Same time-gate + accent + slide arrangement, different pitches →
    // same rhythm_fingerprint by construction.
    let a = make_pattern(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    let b = make_pattern(&[7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7]);
    let r = compare_items(&a, &b);
    assert_eq!(r.note_diff, 16);
    assert_eq!(r.accent_diff, 0);
    assert!(
        r.same_rhythm,
        "identical gate/accent/slide layout → same rhythm"
    );
    assert!(!r.identical);

    // Accent change re-shapes the rhythm fingerprint → same_rhythm = false.
    let c = make_pattern_with_accent(&[0; 16], &[false; 16]);
    let mut accents = [false; 16];
    accents[5] = true;
    let d = make_pattern_with_accent(&[0; 16], &accents);
    let r2 = compare_items(&c, &d);
    assert_eq!(r2.accent_diff, 1);
    assert!(!r2.same_rhythm, "accent change alters rhythm fingerprint");
}

#[test]
fn duplicates_exact_cluster_detected() {
    let lib_path = temp_library_path("dup-exact");
    let store = LibraryStore::load_or_create(&lib_path).unwrap();

    let pat = make_pattern(&[0, 2, 4, 5, 7, 9, 11, 12]);
    let sysex = pattern_to_sysex(&pat, 0, 0, 0).unwrap();
    let payload = sysex[3..].to_vec();
    assert_eq!(payload.len(), 112);

    // Two items with the same 112-byte sidecar.
    let now = store::now_iso();
    let mut a = sample_item("item_a", "a");
    a.created_at = now.clone();
    a.updated_at = now.clone();
    let mut b = sample_item("item_b", "b");
    b.created_at = now.clone();
    b.updated_at = now.clone();
    store.upsert_item(a).unwrap();
    store.upsert_item(b).unwrap();
    store.write_pattern_bytes("item_a", &payload).unwrap();
    store.write_pattern_bytes("item_b", &payload).unwrap();

    let clusters = compute_clusters(&store).unwrap();
    assert_eq!(clusters.len(), 1);
    assert_eq!(clusters[0].kind, DuplicateClusterKind::Exact);
    let mut ids = clusters[0].item_ids.clone();
    ids.sort();
    assert_eq!(ids, vec!["item_a".to_string(), "item_b".to_string()]);

    let _ = std::fs::remove_file(&lib_path);
    let sidecar_dir = store.pattern_sidecar_dir();
    let _ = std::fs::remove_dir_all(&sidecar_dir);
}

#[test]
fn duplicates_near_cluster_detected() {
    let lib_path = temp_library_path("dup-near");
    let store = LibraryStore::load_or_create(&lib_path).unwrap();

    let pat_a = make_pattern(&[0, 2, 4, 5, 7, 9, 11, 12, 0, 2, 4, 5, 7, 9, 11, 12]);
    // Change exactly 2 notes - within the near-duplicate budget.
    let pat_b = make_pattern(&[0, 2, 4, 5, 7, 9, 11, 12, 0, 2, 4, 6, 7, 8, 11, 12]);
    let sx_a = pattern_to_sysex(&pat_a, 0, 0, 0).unwrap();
    let sx_b = pattern_to_sysex(&pat_b, 0, 0, 0).unwrap();

    let now = store::now_iso();
    let mut a = sample_item("near_a", "a");
    a.created_at = now.clone();
    a.updated_at = now.clone();
    let mut b = sample_item("near_b", "b");
    b.created_at = now.clone();
    b.updated_at = now.clone();
    store.upsert_item(a).unwrap();
    store.upsert_item(b).unwrap();
    store.write_pattern_bytes("near_a", &sx_a[3..]).unwrap();
    store.write_pattern_bytes("near_b", &sx_b[3..]).unwrap();

    let clusters = compute_clusters(&store).unwrap();
    assert_eq!(clusters.len(), 1);
    assert_eq!(clusters[0].kind, DuplicateClusterKind::Near);
    let mut ids = clusters[0].item_ids.clone();
    ids.sort();
    assert_eq!(ids, vec!["near_a".to_string(), "near_b".to_string()]);

    let _ = std::fs::remove_file(&lib_path);
    let sidecar_dir = store.pattern_sidecar_dir();
    let _ = std::fs::remove_dir_all(&sidecar_dir);
}

#[test]
fn duplicates_unique_item_not_in_any_cluster() {
    let lib_path = temp_library_path("dup-unique");
    let store = LibraryStore::load_or_create(&lib_path).unwrap();

    let pat_a = make_pattern(&[0; 16]);
    let pat_b = make_pattern(&[1, 5, 7, 9, 11, 12, 6, 3, 8, 2, 4, 10, 0, 1, 2, 3]);
    let sx_a = pattern_to_sysex(&pat_a, 0, 0, 0).unwrap();
    let sx_b = pattern_to_sysex(&pat_b, 0, 0, 0).unwrap();

    let now = store::now_iso();
    let mut a = sample_item("solo_a", "a");
    a.created_at = now.clone();
    a.updated_at = now.clone();
    let mut b = sample_item("solo_b", "b");
    b.created_at = now.clone();
    b.updated_at = now.clone();
    store.upsert_item(a).unwrap();
    store.upsert_item(b).unwrap();
    store.write_pattern_bytes("solo_a", &sx_a[3..]).unwrap();
    store.write_pattern_bytes("solo_b", &sx_b[3..]).unwrap();

    let clusters = compute_clusters(&store).unwrap();
    // Very different notes, same all-Normal rhythm but >3 edits → no cluster.
    assert!(
        clusters.is_empty(),
        "solo patterns with >3 note edits must not cluster: {:?}",
        clusters
    );

    let _ = std::fs::remove_file(&lib_path);
    let sidecar_dir = store.pattern_sidecar_dir();
    let _ = std::fs::remove_dir_all(&sidecar_dir);
}

#[test]
fn sidecar_write_and_read_roundtrip() {
    let lib_path = temp_library_path("sidecar-rt");
    let store = LibraryStore::load_or_create(&lib_path).unwrap();

    let pat = make_pattern(&[0, 2, 4, 5, 7, 9, 11, 12]);
    let sysex = pattern_to_sysex(&pat, 0, 0, 0).unwrap();
    let payload = &sysex[3..];
    assert_eq!(payload.len(), 112);

    store.write_pattern_bytes("sc_item", payload).unwrap();
    let reloaded = store.pattern_bytes_for("sc_item").expect("sidecar present");
    assert_eq!(reloaded, payload);

    // Wrong payload length is rejected.
    let err = store.write_pattern_bytes("bad", &[0u8; 50]);
    assert!(err.is_err());

    let _ = std::fs::remove_file(&lib_path);
    let sidecar_dir = store.pattern_sidecar_dir();
    let _ = std::fs::remove_dir_all(&sidecar_dir);
}

#[test]
fn write_pattern_bytes_leaves_no_temp_file_on_success() {
    let lib_path = temp_library_path("sidecar-no-tmp");
    let store = LibraryStore::load_or_create(&lib_path).unwrap();

    let pat = make_pattern(&[0, 2, 4, 5, 7, 9, 11, 12]);
    let sysex = pattern_to_sysex(&pat, 0, 0, 0).unwrap();
    let payload = &sysex[3..];
    store.write_pattern_bytes("atomic_a", payload).unwrap();

    let sidecar_dir = store.pattern_sidecar_dir();
    let entries: Vec<_> = std::fs::read_dir(&sidecar_dir)
        .expect("sidecar dir exists")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        entries.iter().any(|n| n == "atomic_a.syx"),
        "final sidecar present: {:?}",
        entries
    );
    assert!(
        entries.iter().all(|n| !n.ends_with(".tmp")),
        "no temp file leaked: {:?}",
        entries
    );

    let _ = std::fs::remove_file(&lib_path);
    let _ = std::fs::remove_dir_all(&sidecar_dir);
}

#[test]
fn write_pattern_bytes_preserves_existing_on_invalid_length() {
    // The atomic write contract: a failed replacement must leave the
    // previously-written payload intact. Length validation rejects the
    // request before any I/O, which is one path that exercises the
    // "no in-place mutation, no half-written file" guarantee.
    let lib_path = temp_library_path("sidecar-preserve");
    let store = LibraryStore::load_or_create(&lib_path).unwrap();

    let pat = make_pattern(&[0, 2, 4, 5, 7, 9, 11, 12]);
    let sysex = pattern_to_sysex(&pat, 0, 0, 0).unwrap();
    let payload = sysex[3..].to_vec();
    store.write_pattern_bytes("preserve_a", &payload).unwrap();

    let err = store.write_pattern_bytes("preserve_a", &[0u8; 50]);
    assert!(err.is_err(), "short payload rejected");

    let still_there = store
        .pattern_bytes_for("preserve_a")
        .expect("original sidecar survives a rejected replacement");
    assert_eq!(still_there, payload);

    let _ = std::fs::remove_file(&lib_path);
    let sidecar_dir = store.pattern_sidecar_dir();
    let _ = std::fs::remove_dir_all(&sidecar_dir);
}

#[test]
fn write_pattern_bytes_returns_err_when_sidecar_dir_is_a_regular_file() {
    // Forcing the mkdir step to fail proves the function returns an error
    // rather than silently creating no file.
    let dir = fresh_tmp_dir("sidecar-blocked");
    let blocker = dir.join("blocker_path");
    std::fs::write(&blocker, b"not a directory").expect("create blocker file");

    let lib_path = dir.join("catalog.json");
    let store = LibraryStore::load_or_create_with_sidecar(&lib_path, blocker.clone()).unwrap();

    let pat = make_pattern(&[0, 2, 4, 5, 7, 9, 11, 12]);
    let sysex = pattern_to_sysex(&pat, 0, 0, 0).unwrap();
    let payload = &sysex[3..];

    let result = store.write_pattern_bytes("any_id", payload);
    assert!(
        result.is_err(),
        "write must fail when sidecar dir is a regular file"
    );

    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ingest_failure_when_sidecar_unwritable_does_not_commit_item() {
    // End-to-end check of the P1-A contract: if the sidecar payload cannot
    // be persisted, the ingest pipeline must roll back the catalog row
    // rather than leaving an item without its on-disk pattern bytes.
    let dir = fresh_tmp_dir("ingest-no-sidecar");
    let blocker = dir.join("blocked_sidecar");
    std::fs::write(&blocker, b"not a directory").expect("create blocker");

    let lib_path = dir.join("catalog.json");
    let store = LibraryStore::load_or_create_with_sidecar(&lib_path, blocker.clone()).unwrap();

    let seq_path = write_seq_file(&dir, "would-import.seq", &[0, 2, 4, 5, 7, 9, 11, 12]);
    let batch = store.create_import_batch(None).unwrap();

    let outcome = ingest::ingest_path(
        &store,
        &seq_path,
        &batch.batch_id,
        &MidiImportOptions::default(),
    )
    .unwrap();
    assert_eq!(outcome.entry.status, FileIngestStatus::Failed);
    assert!(outcome.entry.item_id.is_none(), "no item id leaked");
    let err = outcome.entry.error.unwrap_or_default();
    assert!(
        err.starts_with("sidecar:"),
        "error must mention sidecar failure: {}",
        err
    );

    let items = store.list_items(&ItemFilter::default()).unwrap();
    assert!(
        items.is_empty(),
        "no catalog item must remain when its sidecar could not be written"
    );

    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// P1-B: SQLite commit vs in-memory mirror ordering
// ---------------------------------------------------------------------------
//
// These tests prove that when SQLite persistence fails, the in-memory mirror
// is not mutated. They inject a write failure by replacing the catalog file
// with a directory of the same name; rusqlite's `Connection::open` fails on
// that path, every persistence call rejects the request, and the store's
// mirror must remain identical to its pre-attempt snapshot.

fn replace_db_with_directory(path: &std::path::Path) -> PathBuf {
    let parked = path.with_extension("parked");
    let _ = std::fs::remove_file(&parked);
    let _ = std::fs::remove_dir_all(&parked);
    std::fs::rename(path, &parked).expect("park db file");
    // Also clean any sqlite WAL/SHM siblings that could keep the path valid.
    for ext in ["-wal", "-shm", "-journal"] {
        let mut sibling = path.to_path_buf();
        let mut name = sibling.file_name().unwrap().to_os_string();
        name.push(ext);
        sibling.set_file_name(&name);
        let _ = std::fs::remove_file(&sibling);
    }
    std::fs::create_dir(path).expect("create blocking dir at db path");
    parked
}

fn restore_db_from_park(path: &std::path::Path, parked: &std::path::Path) {
    let _ = std::fs::remove_dir_all(path);
    std::fs::rename(parked, path).expect("restore db file");
}

#[test]
fn upsert_item_failure_does_not_mutate_mirror() {
    let path = temp_library_path("p1b-upsert");
    let store = LibraryStore::load_or_create(&path).unwrap();
    store.upsert_item(sample_item("item_a", "alpha")).unwrap();

    let before = store.mirror_snapshot_for_tests();
    let parked = replace_db_with_directory(&path);

    let result = store.upsert_item(sample_item("item_b", "beta"));
    assert!(result.is_err(), "upsert must fail when DB path is blocked");

    let after = store.mirror_snapshot_for_tests();
    assert_eq!(
        after.item_ids, before.item_ids,
        "mirror items must not gain item_b after a persistence failure"
    );

    restore_db_from_park(&path, &parked);
    drop(store);
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn delete_item_failure_does_not_remove_from_mirror() {
    let path = temp_library_path("p1b-delete");
    let store = LibraryStore::load_or_create(&path).unwrap();
    store.upsert_item(sample_item("item_a", "alpha")).unwrap();
    store.upsert_item(sample_item("item_b", "beta")).unwrap();

    let before = store.mirror_snapshot_for_tests();
    let parked = replace_db_with_directory(&path);

    let result = store.delete_item("item_a");
    assert!(result.is_err(), "delete must fail when DB path is blocked");

    let after = store.mirror_snapshot_for_tests();
    assert_eq!(
        after.item_ids, before.item_ids,
        "mirror items must still contain item_a after a failed delete"
    );

    restore_db_from_park(&path, &parked);
    drop(store);
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn set_favorite_failure_does_not_change_mirror_flag() {
    let path = temp_library_path("p1b-fav");
    let store = LibraryStore::load_or_create(&path).unwrap();
    store.upsert_item(sample_item("item_a", "alpha")).unwrap();

    let before = store.mirror_snapshot_for_tests();
    let parked = replace_db_with_directory(&path);

    let result = store.set_favorite("item_a", true);
    assert!(result.is_err(), "set_favorite must fail with blocked DB");

    let after = store.mirror_snapshot_for_tests();
    assert_eq!(
        after.item_favorites, before.item_favorites,
        "favorite flag in mirror must be unchanged after a failed write"
    );

    restore_db_from_park(&path, &parked);
    drop(store);
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn set_archived_failure_does_not_change_mirror_flag() {
    let path = temp_library_path("p1b-arch");
    let store = LibraryStore::load_or_create(&path).unwrap();
    store.upsert_item(sample_item("item_a", "alpha")).unwrap();

    let before = store.mirror_snapshot_for_tests();
    let parked = replace_db_with_directory(&path);

    let result = store.set_archived("item_a", true);
    assert!(result.is_err(), "set_archived must fail with blocked DB");

    let after = store.mirror_snapshot_for_tests();
    assert_eq!(after.item_archived, before.item_archived);

    restore_db_from_park(&path, &parked);
    drop(store);
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn create_snapshot_failure_does_not_mutate_mirror() {
    let path = temp_library_path("p1b-create-snap");
    let store = LibraryStore::load_or_create(&path).unwrap();

    let before = store.mirror_snapshot_for_tests();
    let parked = replace_db_with_directory(&path);

    let result = store.create_snapshot("Bad".to_string(), None, SnapshotOrigin::Manual);
    assert!(result.is_err(), "create_snapshot must fail with blocked DB");

    let after = store.mirror_snapshot_for_tests();
    assert_eq!(after.snapshot_ids, before.snapshot_ids);

    restore_db_from_park(&path, &parked);
    drop(store);
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn rename_snapshot_failure_does_not_change_mirror_name() {
    let path = temp_library_path("p1b-rename-snap");
    let store = LibraryStore::load_or_create(&path).unwrap();
    let snap = store
        .create_snapshot("Initial".to_string(), None, SnapshotOrigin::Manual)
        .unwrap();

    let before = store.mirror_snapshot_for_tests();
    let parked = replace_db_with_directory(&path);

    let result = store.rename_snapshot(&snap.snapshot_id, "Renamed".to_string());
    assert!(result.is_err(), "rename_snapshot must fail with blocked DB");

    let after = store.mirror_snapshot_for_tests();
    assert_eq!(after.snapshot_names, before.snapshot_names);

    restore_db_from_park(&path, &parked);
    drop(store);
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn pin_snapshot_failure_does_not_change_mirror_pinned() {
    let path = temp_library_path("p1b-pin-snap");
    let store = LibraryStore::load_or_create(&path).unwrap();
    let snap = store
        .create_snapshot("Initial".to_string(), None, SnapshotOrigin::Manual)
        .unwrap();

    let before = store.mirror_snapshot_for_tests();
    let parked = replace_db_with_directory(&path);

    let result = store.pin_snapshot(&snap.snapshot_id, true);
    assert!(result.is_err(), "pin_snapshot must fail with blocked DB");

    let after = store.mirror_snapshot_for_tests();
    assert_eq!(after.snapshot_pinned, before.snapshot_pinned);

    restore_db_from_park(&path, &parked);
    drop(store);
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn add_tag_to_item_failure_does_not_mutate_mirror() {
    let path = temp_library_path("p1b-add-tag");
    let store = LibraryStore::load_or_create(&path).unwrap();
    store.upsert_item(sample_item("item_a", "alpha")).unwrap();

    let before = store.mirror_snapshot_for_tests();
    let parked = replace_db_with_directory(&path);

    let result = store.add_tag_to_item("item_a", "newtag");
    assert!(result.is_err(), "add_tag_to_item must fail with blocked DB");

    let after = store.mirror_snapshot_for_tests();
    assert_eq!(after.tag_labels, before.tag_labels);
    assert_eq!(after.item_tags_per_item, before.item_tags_per_item);
    assert_eq!(after.item_tag_edges, before.item_tag_edges);

    restore_db_from_park(&path, &parked);
    drop(store);
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn store_reload_after_success_matches_db() {
    // Companion check: with the persist-then-mirror ordering, a successful
    // round of writes followed by reopening the store yields the same items.
    let path = temp_library_path("p1b-reload");
    {
        let store = LibraryStore::load_or_create(&path).unwrap();
        store.upsert_item(sample_item("item_a", "alpha")).unwrap();
        store.upsert_item(sample_item("item_b", "beta")).unwrap();
        store.set_favorite("item_a", true).unwrap();
        store.add_tag_to_item("item_b", "acid").unwrap();
    }
    let store2 = LibraryStore::load_or_create(&path).unwrap();
    let mut ids: Vec<String> = store2
        .list_items(&ItemFilter::default())
        .unwrap()
        .into_iter()
        .map(|i| i.item_id)
        .collect();
    ids.sort();
    assert_eq!(ids, vec!["item_a".to_string(), "item_b".to_string()]);
    let item_a = store2.get_item("item_a").unwrap().unwrap();
    assert!(item_a.favorite);
    let item_b = store2.get_item("item_b").unwrap().unwrap();
    assert!(item_b.tags.contains(&"acid".to_string()));

    let mirror = store2.mirror_snapshot_for_tests();
    let mut mirror_ids = mirror.item_ids.clone();
    mirror_ids.sort();
    assert_eq!(mirror_ids, ids, "mirror must equal DB after a clean reload");
}

#[test]
fn ingest_seq_file_writes_pattern_sidecar() {
    let dir = fresh_tmp_dir("sidecar-ingest");
    let lib_path = temp_library_path("sidecar-ingest");
    let store = LibraryStore::load_or_create(&lib_path).unwrap();

    let seq_path = write_seq_file(&dir, "with-sidecar.seq", &[0, 2, 4, 5, 7, 9, 11, 12]);
    let batch = store.create_import_batch(None).unwrap();
    let outcome = ingest::ingest_path(
        &store,
        &seq_path,
        &batch.batch_id,
        &MidiImportOptions::default(),
    )
    .unwrap();
    assert_eq!(outcome.entry.status, FileIngestStatus::Imported);
    let id = outcome.entry.item_id.clone().expect("item_id wired");

    let sidecar = store.pattern_bytes_for(&id).expect("sidecar written");
    assert_eq!(sidecar.len(), 112);

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&lib_path);
    let sidecar_dir = store.pattern_sidecar_dir();
    let _ = std::fs::remove_dir_all(&sidecar_dir);
}

#[test]
fn system_tag_safe_live_seeded() {
    let path = temp_library_path("sys-tag");
    let store = LibraryStore::load_or_create(&path).unwrap();
    let tags = store.list_tags().unwrap();
    let t = tags
        .iter()
        .find(|t| t.label == "safe-live")
        .expect("safe-live system tag seeded");
    assert_eq!(t.kind, TagKind::System);

    // Reopen: idempotent - no duplicate row.
    drop(store);
    let store = LibraryStore::load_or_create(&path).unwrap();
    let count = store
        .list_tags()
        .unwrap()
        .iter()
        .filter(|t| t.label == "safe-live")
        .count();
    assert_eq!(count, 1);

    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------
// Related groups + merge plan operations view
// ---------------------------------------------------------------------------

use crate::library::merge_plan::MergeOperationAction;
use crate::library::related::{compute_related_groups, GroupKind};

fn seed_item_with_scale_root(
    store: &LibraryStore,
    id: &str,
    scale: Option<&str>,
    root: Option<&str>,
) {
    let now = store::now_iso();
    let mut item = sample_item(id, id);
    item.created_at = now.clone();
    item.updated_at = now;
    item.scale_name = scale.map(|s| s.to_string());
    item.root_note = root.map(|s| s.to_string());
    store.upsert_item(item).unwrap();
}

#[test]
fn related_groups_same_scale_found() {
    let lib_path = temp_library_path("related-scale");
    let store = LibraryStore::load_or_create(&lib_path).unwrap();

    seed_item_with_scale_root(&store, "scale_a", Some("phrygian"), None);
    seed_item_with_scale_root(&store, "scale_b", Some("phrygian"), None);
    seed_item_with_scale_root(&store, "scale_c", Some("dorian"), None);
    seed_item_with_scale_root(&store, "scale_d", None, None);

    let groups = compute_related_groups(&store).unwrap();
    let scale_groups: Vec<_> = groups
        .iter()
        .filter(|g| g.kind == GroupKind::SameScale)
        .collect();
    assert_eq!(scale_groups.len(), 1, "only phrygian has ≥2 members");
    let g = scale_groups[0];
    let mut ids = g.item_ids.clone();
    ids.sort();
    assert_eq!(ids, vec!["scale_a".to_string(), "scale_b".to_string()]);
    assert_eq!(g.primary_scale.as_deref(), Some("phrygian"));
    assert!(g.reason.contains("phrygian"));
    assert!(g.label.contains("phrygian"));
    assert_eq!(g.item_count, 2);
    assert!(g.representative_ids.len() <= 4);

    let _ = std::fs::remove_file(&lib_path);
}

#[test]
fn related_groups_same_root_found() {
    let lib_path = temp_library_path("related-root");
    let store = LibraryStore::load_or_create(&lib_path).unwrap();

    seed_item_with_scale_root(&store, "root_a", None, Some("D"));
    seed_item_with_scale_root(&store, "root_b", None, Some("D"));
    seed_item_with_scale_root(&store, "root_c", None, Some("D"));
    seed_item_with_scale_root(&store, "root_d", None, Some("F#"));
    seed_item_with_scale_root(&store, "root_e", None, None);

    let groups = compute_related_groups(&store).unwrap();
    let root_groups: Vec<_> = groups
        .iter()
        .filter(|g| g.kind == GroupKind::SameRoot)
        .collect();
    assert_eq!(root_groups.len(), 1, "only D has ≥2 members");
    let g = root_groups[0];
    let mut ids = g.item_ids.clone();
    ids.sort();
    assert_eq!(
        ids,
        vec![
            "root_a".to_string(),
            "root_b".to_string(),
            "root_c".to_string()
        ]
    );
    assert_eq!(g.primary_root.as_deref(), Some("D"));
    assert_eq!(g.item_count, 3);

    let _ = std::fs::remove_file(&lib_path);
}

#[test]
fn related_groups_same_rhythm_uses_pattern_bytes() {
    let lib_path = temp_library_path("related-rhythm");
    let store = LibraryStore::load_or_create(&lib_path).unwrap();

    // Two patterns with identical rhythm fingerprints (all-Normal time/accent
    // with the default helper) but different note pitches.
    let pat_a = make_pattern(&[0, 2, 4, 5, 7, 9, 11, 12, 0, 2, 4, 5, 7, 9, 11, 12]);
    let pat_b = make_pattern(&[1, 3, 5, 6, 8, 10, 0, 1, 1, 3, 5, 6, 8, 10, 0, 1]);
    let sx_a = pattern_to_sysex(&pat_a, 0, 0, 0).unwrap();
    let sx_b = pattern_to_sysex(&pat_b, 0, 0, 0).unwrap();

    seed_item_with_scale_root(&store, "rhy_a", None, None);
    seed_item_with_scale_root(&store, "rhy_b", None, None);
    store.write_pattern_bytes("rhy_a", &sx_a[3..]).unwrap();
    store.write_pattern_bytes("rhy_b", &sx_b[3..]).unwrap();

    // Third item without sidecar - must be skipped.
    seed_item_with_scale_root(&store, "rhy_no_sidecar", None, None);

    let groups = compute_related_groups(&store).unwrap();
    let rhythm_groups: Vec<_> = groups
        .iter()
        .filter(|g| g.kind == GroupKind::SameRhythm)
        .collect();
    assert_eq!(rhythm_groups.len(), 1);
    let g = rhythm_groups[0];
    let mut ids = g.item_ids.clone();
    ids.sort();
    assert_eq!(ids, vec!["rhy_a".to_string(), "rhy_b".to_string()]);
    assert!(g.reason.to_lowercase().contains("rhythm"));

    let _ = std::fs::remove_file(&lib_path);
    let sidecar_dir = store.pattern_sidecar_dir();
    let _ = std::fs::remove_dir_all(&sidecar_dir);
}

#[test]
fn related_groups_skip_singletons() {
    let lib_path = temp_library_path("related-single");
    let store = LibraryStore::load_or_create(&lib_path).unwrap();

    // One unique scale, one unique root, no rhythm sidecars: nothing should
    // surface even though every classifier has a candidate row.
    seed_item_with_scale_root(&store, "solo_a", Some("dorian"), Some("E"));
    seed_item_with_scale_root(&store, "solo_b", Some("aeolian"), Some("G"));

    let groups = compute_related_groups(&store).unwrap();
    assert!(
        groups.is_empty(),
        "no group should have ≥2 members: {:?}",
        groups
    );

    let _ = std::fs::remove_file(&lib_path);
}

#[test]
fn related_groups_progression_family_tag() {
    let lib_path = temp_library_path("related-prog");
    let store = LibraryStore::load_or_create(&lib_path).unwrap();

    seed_item_with_scale_root(&store, "fam_a", None, None);
    seed_item_with_scale_root(&store, "fam_b", None, None);
    seed_item_with_scale_root(&store, "fam_c", None, None);
    store
        .add_tag_to_item("fam_a", "progression:i-iv-v")
        .unwrap();
    store
        .add_tag_to_item("fam_b", "progression:i-iv-v")
        .unwrap();
    store.add_tag_to_item("fam_c", "family:moods").unwrap();

    let groups = compute_related_groups(&store).unwrap();
    let prog: Vec<_> = groups
        .iter()
        .filter(|g| g.kind == GroupKind::ProgressionFamily)
        .collect();
    // Only the "progression:i-iv-v" tag has ≥2 members.
    assert_eq!(prog.len(), 1);
    assert!(prog[0].label.contains("i-iv-v"));

    let _ = std::fs::remove_file(&lib_path);
}

#[test]
fn merge_plan_copy_source_to_target_on_difference() {
    // Source and target both populated with non-matching item ids →
    // operations row reads `copy_source_to_target / different`.
    let src = vec![SnapshotSlot {
        snapshot_id: "s1".into(),
        slot_key: "G1-P1A".into(),
        item_id: Some("src_item".into()),
        empty: false,
        display_name: None,
    }];
    let dst = vec![SnapshotSlot {
        snapshot_id: "s2".into(),
        slot_key: "G1-P1A".into(),
        item_id: Some("dst_item".into()),
        empty: false,
        display_name: None,
    }];
    let compare = compare_snapshots(&src, &dst, |_| None);
    let plan = build_merge_plan("s1", "s2", &compare, &["G1-P1A".to_string()]);
    let g1p1a = plan
        .operations
        .iter()
        .find(|o| o.slot_key == "G1-P1A")
        .unwrap();
    assert_eq!(g1p1a.action, MergeOperationAction::CopySourceToTarget);
    assert_eq!(g1p1a.reason, "different");
    assert_eq!(plan.operations.len(), 64, "operations is the full grid");
}

#[test]
fn merge_plan_keep_target_on_identical() {
    // Resolve both sides to the same Pattern → SlotCompareState::Identical →
    // operations row should be `keep_target / identical` even when selected.
    let src = vec![SnapshotSlot {
        snapshot_id: "s1".into(),
        slot_key: "G1-P1A".into(),
        item_id: Some("same_a".into()),
        empty: false,
        display_name: None,
    }];
    let dst = vec![SnapshotSlot {
        snapshot_id: "s2".into(),
        slot_key: "G1-P1A".into(),
        item_id: Some("same_b".into()),
        empty: false,
        display_name: None,
    }];
    let pat = make_pattern(&[0, 2, 4, 5, 7, 9, 11, 12, 0, 2, 4, 5, 7, 9, 11, 12]);
    let compare = compare_snapshots(&src, &dst, |_id| {
        // Both items resolve to the same pattern → identical.
        Some(make_pattern(&[
            0, 2, 4, 5, 7, 9, 11, 12, 0, 2, 4, 5, 7, 9, 11, 12,
        ]))
    });
    let _ = pat;
    let plan = build_merge_plan("s1", "s2", &compare, &["G1-P1A".to_string()]);
    let g1p1a = plan
        .operations
        .iter()
        .find(|o| o.slot_key == "G1-P1A")
        .unwrap();
    assert_eq!(g1p1a.action, MergeOperationAction::KeepTarget);
    assert_eq!(g1p1a.reason, "identical");
}

#[test]
fn merge_plan_skip_empty_source() {
    // Both sides empty → operations row should be skip_empty_source.
    let src: Vec<SnapshotSlot> = vec![];
    let dst: Vec<SnapshotSlot> = vec![];
    let compare = compare_snapshots(&src, &dst, |_| None);
    let plan = build_merge_plan("s1", "s2", &compare, &[]);
    let g1p1a = plan
        .operations
        .iter()
        .find(|o| o.slot_key == "G1-P1A")
        .unwrap();
    assert_eq!(g1p1a.action, MergeOperationAction::SkipEmptySource);
    assert_eq!(g1p1a.reason, "source_empty_skipped");
}

#[test]
fn merge_plan_clear_target_when_source_empty_selected() {
    // Source slot empty, target slot populated, and the user explicitly
    // selected this slot → clear_target / target_empty.
    let src: Vec<SnapshotSlot> = vec![];
    let dst = vec![SnapshotSlot {
        snapshot_id: "s2".into(),
        slot_key: "G1-P1A".into(),
        item_id: Some("dst_item".into()),
        empty: false,
        display_name: None,
    }];
    let compare = compare_snapshots(&src, &dst, |_| None);
    let plan = build_merge_plan("s1", "s2", &compare, &["G1-P1A".to_string()]);
    let g1p1a = plan
        .operations
        .iter()
        .find(|o| o.slot_key == "G1-P1A")
        .unwrap();
    assert_eq!(g1p1a.action, MergeOperationAction::ClearTarget);
    assert_eq!(g1p1a.reason, "target_empty");
}
