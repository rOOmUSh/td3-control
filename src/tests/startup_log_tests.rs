//! Startup failure log content and placement.

use crate::startup_log::{entry, log_path, LOG_FILE_NAME};

#[test]
fn entry_records_what_a_failed_run_needs_to_be_diagnosed() {
    let text = entry("could not open TD-3 MIDI port. driver said: allocated", 3);

    // The error itself, verbatim.
    assert!(
        text.contains("could not open TD-3 MIDI port. driver said: allocated"),
        "lost the error: {text}"
    );
    // The exit code distinguishes a busy device from a port collision.
    assert!(text.contains("exit code: 3"), "lost the exit code: {text}");
    // Which invocation failed. The launcher passes the scratch slot, the
    // web port and the MIDI names as arguments, so the command line says
    // what it asked for.
    assert!(text.contains("command:"), "lost the command line: {text}");
    // Where it ran. A launcher started by double-click hands the child a
    // working directory the user never chose.
    assert!(text.contains("cwd:"), "lost the working directory: {text}");
    assert!(text.contains("when:"), "lost the timestamp: {text}");
}

#[test]
fn entries_append_rather_than_replace() {
    // Two failures in a row must both survive: the interesting case is
    // usually the difference between attempts.
    let first = entry("first failure", 3);
    let second = entry("second failure", 1);
    let combined = format!("{first}{second}");

    assert_eq!(
        combined.matches("td3-control startup failure").count(),
        2,
        "each entry carries its own header"
    );
    assert!(combined.contains("first failure"));
    assert!(combined.contains("second failure"));
    assert!(
        first.ends_with("\n\n"),
        "entries are separated so a reader can tell them apart"
    );
}

#[test]
fn log_sits_beside_the_executable() {
    // The launcher spawns the child from the executable's own directory,
    // so that is where the user will look.
    let path = log_path().expect("current_exe resolves under test");
    assert_eq!(
        path.file_name().and_then(|name| name.to_str()),
        Some(LOG_FILE_NAME)
    );
    let exe = std::env::current_exe().expect("current exe");
    assert_eq!(path.parent(), exe.parent());
}
