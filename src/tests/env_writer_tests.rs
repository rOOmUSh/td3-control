//! Tests for `env_writer::apply_updates` - the comment-preserving
//! atomic rewriter used by the Settings UI save path.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::env_writer::{apply_updates, backup_path};

fn temp_dir(tag: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!(
        "td3-envwriter-{}-{}-{}",
        tag,
        std::process::id(),
        rand_suffix()
    ));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    base
}

fn rand_suffix() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

fn updates_of(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

// ── empty updates are a no-op ────────────────────────────────────────

#[test]
fn empty_updates_leaves_file_untouched() {
    let dir = temp_dir("empty");
    let path = dir.join("TD3_CONFIG.env");
    let original = "# Header comment\nWEB_PORT=3030\n";
    std::fs::write(&path, original).unwrap();

    apply_updates(&path, &HashMap::new()).unwrap();

    let after = std::fs::read_to_string(&path).unwrap();
    assert_eq!(after, original);
    // No .bak created for a no-op write.
    assert!(!backup_path(&path).exists());
}

// ── existing key gets rewritten in place ─────────────────────────────

#[test]
fn existing_key_rewritten_in_place() {
    let dir = temp_dir("inplace");
    let path = dir.join("TD3_CONFIG.env");
    std::fs::write(
        &path,
        "# Header\nWEB_PORT=3030\nWEB_BIND=127.0.0.1\n# trailing\n",
    )
    .unwrap();

    apply_updates(&path, &updates_of(&[("WEB_PORT", "4040")])).unwrap();

    let after = std::fs::read_to_string(&path).unwrap();
    assert!(after.contains("WEB_PORT=4040"));
    assert!(!after.contains("WEB_PORT=3030"));
    // Unrelated lines preserved.
    assert!(after.contains("# Header"));
    assert!(after.contains("# trailing"));
    assert!(after.contains("WEB_BIND=127.0.0.1"));
}

// ── comments and blank lines preserved byte-for-byte ─────────────────

#[test]
fn comments_and_blank_lines_preserved() {
    let dir = temp_dir("preserve");
    let path = dir.join("TD3_CONFIG.env");
    let original = "# Section A\n\nKEY_A=1\n\n# Section B -- important!\nKEY_B=foo\n\n";
    std::fs::write(&path, original).unwrap();

    // Touch nothing that exists; force an appended key instead.
    apply_updates(&path, &updates_of(&[("KEY_A", "2")])).unwrap();

    let after = std::fs::read_to_string(&path).unwrap();
    assert!(after.contains("# Section A"));
    assert!(after.contains("# Section B -- important!"));
    assert!(after.contains("KEY_A=2"));
    assert!(after.contains("KEY_B=foo"));
}

// ── quoted values stay quoted; bare values stay bare ─────────────────

#[test]
fn quoted_values_stay_quoted() {
    let dir = temp_dir("quoted");
    let path = dir.join("TD3_CONFIG.env");
    std::fs::write(&path, "WEB_BIND=\"127.0.0.1\"\nWEB_PORT=3030\n").unwrap();

    apply_updates(
        &path,
        &updates_of(&[("WEB_BIND", "0.0.0.0"), ("WEB_PORT", "4040")]),
    )
    .unwrap();

    let after = std::fs::read_to_string(&path).unwrap();
    assert!(after.contains("WEB_BIND=\"0.0.0.0\""), "got: {}", after);
    assert!(after.contains("WEB_PORT=4040"), "got: {}", after);
    assert!(!after.contains("WEB_PORT=\"4040\""));
}

// ── unmatched keys get appended ──────────────────────────────────────

#[test]
fn unmatched_key_is_appended_under_footer() {
    let dir = temp_dir("append");
    let path = dir.join("TD3_CONFIG.env");
    std::fs::write(&path, "WEB_PORT=3030\n").unwrap();

    apply_updates(&path, &updates_of(&[("BRAND_NEW_KEY", "42")])).unwrap();

    let after = std::fs::read_to_string(&path).unwrap();
    assert!(after.contains("# --- runtime additions ---"));
    assert!(after.contains("BRAND_NEW_KEY=42"));
    // Original content preserved.
    assert!(after.contains("WEB_PORT=3030"));
}

// ── .bak contains the pre-write file ─────────────────────────────────

#[test]
fn backup_contains_original_contents() {
    let dir = temp_dir("bak");
    let path = dir.join("TD3_CONFIG.env");
    let original = "# pre-write snapshot\nWEB_PORT=3030\n";
    std::fs::write(&path, original).unwrap();

    apply_updates(&path, &updates_of(&[("WEB_PORT", "4040")])).unwrap();

    let bak_path = backup_path(&path);
    assert!(bak_path.exists(), "backup file must be created");
    let bak_contents = std::fs::read_to_string(&bak_path).unwrap();
    assert_eq!(bak_contents, original);

    // Live file reflects new value.
    let live = std::fs::read_to_string(&path).unwrap();
    assert!(live.contains("WEB_PORT=4040"));
}

// ── repeated writes overwrite the single .bak generation ─────────────

#[test]
fn repeated_writes_refresh_backup() {
    let dir = temp_dir("bak-refresh");
    let path = dir.join("TD3_CONFIG.env");
    std::fs::write(&path, "WEB_PORT=3030\n").unwrap();

    apply_updates(&path, &updates_of(&[("WEB_PORT", "4040")])).unwrap();
    let bak_1 = std::fs::read_to_string(backup_path(&path)).unwrap();
    assert!(bak_1.contains("WEB_PORT=3030"));

    apply_updates(&path, &updates_of(&[("WEB_PORT", "5050")])).unwrap();
    let bak_2 = std::fs::read_to_string(backup_path(&path)).unwrap();
    // After a second write the .bak reflects the prior state (4040),
    // not the original (3030). Single-generation backup semantics.
    assert!(bak_2.contains("WEB_PORT=4040"));
    assert!(!bak_2.contains("WEB_PORT=3030"));

    let live = std::fs::read_to_string(&path).unwrap();
    assert!(live.contains("WEB_PORT=5050"));
}

// ── CRLF line endings preserved ──────────────────────────────────────

#[test]
fn crlf_endings_preserved() {
    let dir = temp_dir("crlf");
    let path = dir.join("TD3_CONFIG.env");
    let original = "# header\r\nWEB_PORT=3030\r\nWEB_BIND=127.0.0.1\r\n";
    std::fs::write(&path, original).unwrap();

    apply_updates(&path, &updates_of(&[("WEB_PORT", "4040")])).unwrap();

    let after = std::fs::read_to_string(&path).unwrap();
    // Every record separator must still be CRLF.
    assert!(
        after.contains("WEB_PORT=4040\r\n"),
        "CRLF lost: {:?}",
        after
    );
    assert!(!after.contains("\n\n"), "LF introduced: {:?}", after);
}

// ── multiple keys in one call ────────────────────────────────────────

#[test]
fn multiple_keys_rewritten_together() {
    let dir = temp_dir("multi");
    let path = dir.join("TD3_CONFIG.env");
    std::fs::write(
        &path,
        "WEB_PORT=3030\nWEB_BIND=127.0.0.1\nUI_DEFAULT_BPM=120\n",
    )
    .unwrap();

    apply_updates(
        &path,
        &updates_of(&[
            ("WEB_PORT", "4040"),
            ("WEB_BIND", "0.0.0.0"),
            ("UI_DEFAULT_BPM", "140"),
        ]),
    )
    .unwrap();

    let after = std::fs::read_to_string(&path).unwrap();
    assert!(after.contains("WEB_PORT=4040"));
    assert!(after.contains("WEB_BIND=0.0.0.0"));
    assert!(after.contains("UI_DEFAULT_BPM=140"));
}

// ── key-match is whole-key, not prefix ───────────────────────────────

#[test]
fn rewrite_matches_whole_key_not_prefix() {
    let dir = temp_dir("prefix");
    let path = dir.join("TD3_CONFIG.env");
    std::fs::write(&path, "WEB_PORT=3030\nWEB_PORT_EXTRA=keep-me\n").unwrap();

    apply_updates(&path, &updates_of(&[("WEB_PORT", "4040")])).unwrap();

    let after = std::fs::read_to_string(&path).unwrap();
    assert!(after.contains("WEB_PORT=4040"));
    assert!(
        after.contains("WEB_PORT_EXTRA=keep-me"),
        "prefix key got rewritten: {}",
        after
    );
}
