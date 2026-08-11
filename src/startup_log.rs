//! Startup failure log for launcher-started sessions.
//!
//! The launcher spawns the control process, prints that it did so, and
//! exits. It never learns whether the child came up. A child that fails
//! during startup writes its console message to a console the user may
//! not be watching, or may not have at all, so the failure reads as "the
//! GUI does nothing".
//!
//! Every non-zero exit therefore also appends to a file beside the
//! executable. The file records what was run and what went wrong, so a
//! failure can be read after the fact instead of caught in the act.
//!
//! Writing the log is best effort. A failure here is reported to stderr
//! and never changes the exit path.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub const LOG_FILE_NAME: &str = "td3-control-startup-error.log";

/// Path the log is written to: beside the executable, which is where
/// the launcher spawned it from and where the user can find it.
pub fn log_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join(LOG_FILE_NAME))
}

/// One log entry for a failed run.
pub fn entry(error: &str, exit_code: i32) -> String {
    let mut text = String::new();
    let _ = writeln!(text, "---- td3-control startup failure ----");
    let _ = writeln!(text, "when:      {}", timestamp());
    let _ = writeln!(text, "exit code: {}", exit_code);
    let _ = writeln!(text, "command:   {}", command_line());
    let _ = writeln!(
        text,
        "cwd:       {}",
        std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|e| format!("<unavailable: {}>", e))
    );
    let _ = writeln!(text, "error:     {}", error);
    let _ = writeln!(text);
    text
}

/// Append an entry, best effort. Returns false when nothing was written.
pub fn record(error: &str, exit_code: i32) -> bool {
    let Some(path) = log_path() else {
        return false;
    };
    let text = entry(error, exit_code);
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        Ok(mut file) => match file.write_all(text.as_bytes()) {
            Ok(()) => {
                eprintln!("       Details written to {}", path.display());
                true
            }
            Err(err) => {
                eprintln!("       (could not write {}: {})", path.display(), err);
                false
            }
        },
        Err(err) => {
            eprintln!("       (could not open {}: {})", path.display(), err);
            false
        }
    }
}

fn command_line() -> String {
    std::env::args().collect::<Vec<_>>().join(" ")
}

/// Seconds since the epoch. Avoids a date dependency for a diagnostic
/// file whose entries only need to be orderable and comparable against
/// a wall clock.
fn timestamp() -> String {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(since) => format!("{} (unix seconds)", since.as_secs()),
        Err(_) => "<clock before epoch>".to_string(),
    }
}
