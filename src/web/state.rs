//! Shared application state for the web server.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::FromRef;
use tokio::sync::Mutex;

use super::clock::{AuditionRunner, ClockRunner};
use super::control_queue::ControlQueue;
use super::remote_sync::RemoteSyncQueue;
use super::scan_jobs::ScanJobRegistry;
use crate::formats::mid::MidiExportOptions;
use crate::formats::mid_import::MidiImportOptions;
use crate::library::LibraryStore;
use crate::td3_protocol::SyncSource;

/// Live progress for the currently-running (or last-completed) folder scan.
/// A single scan runs at a time from the Bank UI, so one shared instance is
/// enough. Counters are atomic so the progress polling endpoint never needs
/// to take a write lock while the scan loop is running.
pub struct ScanProgress {
    /// True while a scan is actively in progress.
    pub running: AtomicBool,
    /// Supported files discovered by the pre-scan listing step.
    pub found: AtomicUsize,
    /// Files processed so far (imported + duplicate + unsupported + failed).
    pub parsed: AtomicUsize,
    /// The folder path currently being scanned (last requested).
    pub path: std::sync::Mutex<String>,
    /// Populated when a scan finished with an error. Cleared on new start.
    pub last_error: std::sync::Mutex<Option<String>>,
    /// Bumps at the start of every scan; lets the UI detect a fresh run.
    pub generation: AtomicUsize,
}

impl ScanProgress {
    pub fn new() -> Self {
        Self {
            running: AtomicBool::new(false),
            found: AtomicUsize::new(0),
            parsed: AtomicUsize::new(0),
            path: std::sync::Mutex::new(String::new()),
            last_error: std::sync::Mutex::new(None),
            generation: AtomicUsize::new(0),
        }
    }
}

impl Default for ScanProgress {
    fn default() -> Self {
        Self::new()
    }
}

/// MIDI session holding open connections to the TD-3.
///
/// `out_conn` is `None` while the dedicated clock thread holds it
/// (during transport playback). Handlers that need to talk SysEx
/// should use `MidiSession::out_conn_mut`, which returns a clear
/// "transport is running" error in that window instead of silently
/// doing nothing or deadlocking.
pub struct MidiSession {
    /// Monotonic identity used to reject late connection restores from an
    /// older session after disconnect and reconnect.
    pub generation: u64,
    pub out_conn: Option<midir::MidiOutputConnection>,
    pub rx: std::sync::mpsc::Receiver<Vec<u8>>,
    /// Kept alive to hold the input connection open.
    pub _in_conn: midir::MidiInputConnection<()>,
    pub product_name: String,
    pub firmware_version: String,
    /// Last known TD-3 sequencer clock source.
    pub sync_source: SyncSource,
}

/// Transport clock state. The actual tick-emitting work runs in a
/// dedicated OS thread owned by `runner`; `centibpm` and `playing` are
/// cached here purely so the `/api/status` handler can report them
/// without reaching into the thread.
pub struct ClockState {
    /// MIDI session that donated the runner's output connection.
    pub session_generation: u64,
    /// Current tempo in centi-BPM (BPM x 100). Mirrors what the runner
    /// thread is using; 0.01 BPM resolution.
    pub centibpm: u32,
    /// Unix epoch in milliseconds captured when the transport start
    /// handler spawned the clock runner.
    pub started_at_epoch_ms: u64,
    /// Identifier returned to the browser so stale wrap polls can be
    /// rejected after a stop/start cycle.
    pub transport_id: u64,
    /// Whether the clock is running.
    pub playing: bool,
    /// Handle to the clock thread. Dropping this stops the thread.
    pub runner: Option<ClockRunner>,
}

/// Host-audition owner paired with the MIDI session that donated its output
/// connection so a late stop cannot restore into a reconnected session.
pub struct AuditionState {
    pub session_generation: u64,
    pub audition_id: u64,
    pub looping: bool,
    pub runner: AuditionRunner,
}

/// Scratch pattern slot configured at startup.
#[derive(Clone, Copy)]
pub struct ScratchSlot {
    pub patgroup: u8,
    pub slot: u8,
    pub side: u8,
}

/// Library catalog state shared by Bank Management handlers.
#[derive(Clone)]
pub struct LibraryState {
    /// Bank Management catalog (items, snapshots, tags, relations). Shared
    /// read-mostly between all handlers; mutations are serialized via the
    /// store's internal `RwLock`.
    pub store: Arc<LibraryStore>,
    /// Default backup directory, resolved at startup from `BACKUP_DIR_PATH`
    /// in `TD3_CONFIG.env` (overridable by `--backup-dir`). Used by
    /// `POST /api/bank/snapshots/sync-backups` as the fallback when the
    /// request omits `backup_dir`.
    pub backup_dir_path: String,
}

/// MIDI session and runtime format options.
#[derive(Clone)]
pub struct MidiState {
    pub session: Arc<Mutex<Option<MidiSession>>>,
    pub session_generation: Arc<AtomicU64>,
    pub scratch: ScratchSlot,
    /// Resolved MIDI runtime config - port substring, strict flag, and
    /// timeout. Replaces the per-handler hardcoded `"TD-3"` /
    /// `Duration::from_secs(5)` literals so the env file is the single
    /// source of truth at runtime.
    pub runtime: MidiRuntimeConfig,
    /// Resolved MIDI export options. Used by every runtime export path
    /// (snapshot export, package export, audition save, bank backup) so
    /// `MidiExportOptions::default()` is no longer reached at runtime.
    pub export_options: MidiExportOptions,
    /// Resolved MIDI import options. Reuses MIDI_EXPORT_* keys so the
    /// `.mid` round-trip stays lossless when operators tweak the env file.
    pub import_options: MidiImportOptions,
}

impl MidiState {
    pub fn next_session_generation(&self) -> u64 {
        self.session_generation
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
    }
}

/// User-facing configuration files and startup UI snapshot.
#[derive(Clone)]
pub struct ConfigState {
    /// Cached `UI_*` config subset served by `GET /api/config/env` so the
    /// frontend can stamp inputs at boot. Cloned from the startup `AppEnv`.
    pub ui_config: UiConfigSnapshot,
    /// Path to the on-disk `TD3_CONFIG.env`. The Settings → CONFIG UI reads
    /// this on every open (so rolled-back values reflect immediately) and
    /// writes through `env_writer::apply_updates` on save. Only the
    /// on-disk file is mutated; the running process keeps its startup
    /// snapshot until restart.
    pub env_file_path: PathBuf,
    /// Directory containing keyboard, scales, and progression JSON config.
    pub user_config_dir: PathBuf,
}

/// Transport, audition, and control-page handoff state.
#[derive(Clone)]
pub struct PlaybackState {
    /// Serializes transfer of the single MIDI output connection between the
    /// session, clock runner, audition runner, and disconnect path.
    pub midi_owner_lifecycle: Arc<Mutex<()>>,
    /// Number of detached MIDI cleanup jobs that still own an input or output
    /// connection. Reconnect stays blocked until every job has completed.
    pub midi_cleanup_pending: Arc<AtomicUsize>,
    pub clock: Arc<Mutex<Option<ClockState>>>,
    /// Per-step Filter Cutoff lane handed to the clock thread for LIVE
    /// playback. Survives transport stop so a lane set while stopped
    /// applies from the first pulse of the next start.
    pub cutoff_lane: crate::web::clock::cutoff_lane::LaneInbox,
    /// Host-sequenced audition runner. `Some` while a non-saving
    /// pattern audition is playing; mutually exclusive with `clock`.
    /// The thread silences sounding notes when stopped or dropped.
    pub audition: Arc<Mutex<Option<AuditionState>>>,
    pub transport_generation: Arc<AtomicU64>,
    /// Item ID of the LibraryItem currently being auditioned on the device.
    /// Set when the Bank UI asks to play a pattern; cleared on transport stop
    /// or when another item is queued. Read by `GET /api/bank/playing` so the
    /// UI can paint the correct play/stop state after a reload.
    pub playing_item_id: Arc<Mutex<Option<String>>>,
    /// Cross-page handoff queue for "Add to Control" from Bank surfaces.
    /// The Control page consumes this queue on boot and on
    /// `BroadcastChannel('td3-control-queue')` notifications.
    pub control_queue: Arc<ControlQueue>,
    /// Local command queue used by another td3-control server to trigger
    /// this page's browser-owned transport path.
    pub remote_sync: Arc<RemoteSyncQueue>,
}

/// Bank scan progress and asynchronous job registry.
#[derive(Clone)]
pub struct ScanState {
    /// Live progress for the currently-running folder scan. Polled by the
    /// Bank UI scan modal to show "Parsing N/M" with a status bar.
    pub progress: Arc<ScanProgress>,
    pub jobs: Arc<ScanJobRegistry>,
}

/// Shared state passed to all handlers via axum's State extractor.
pub struct AppState {
    pub library: LibraryState,
    pub midi: MidiState,
    pub config: ConfigState,
    pub playback: PlaybackState,
    pub scan: ScanState,
}

impl FromRef<Arc<AppState>> for LibraryState {
    fn from_ref(input: &Arc<AppState>) -> Self {
        input.library.clone()
    }
}

impl FromRef<Arc<AppState>> for MidiState {
    fn from_ref(input: &Arc<AppState>) -> Self {
        input.midi.clone()
    }
}

impl FromRef<Arc<AppState>> for ConfigState {
    fn from_ref(input: &Arc<AppState>) -> Self {
        input.config.clone()
    }
}

impl FromRef<Arc<AppState>> for PlaybackState {
    fn from_ref(input: &Arc<AppState>) -> Self {
        input.playback.clone()
    }
}

impl FromRef<Arc<AppState>> for ScanState {
    fn from_ref(input: &Arc<AppState>) -> Self {
        input.scan.clone()
    }
}

/// MIDI runtime config captured at server start from resolved runtime config.
#[derive(Clone)]
pub struct MidiRuntimeConfig {
    pub input_port_name: String,
    pub output_port_name: String,
    pub strict_name_match: bool,
    pub timeout: Duration,
    /// MIDI channel, 1 through 16, the connected device listens on.
    /// Channel-voice messages the app originates (host audition and note
    /// preview) carry it; SysEx and MIDI realtime do not.
    pub device_channel: u8,
}

/// Subset of `AppEnv` the browser actually needs. Populated once at startup
/// and served by `GET /api/config/env`.
#[derive(Clone)]
pub struct UiConfigSnapshot {
    /// Startup default for the transport bar's CH selector. The device
    /// channel can be changed live from the UI without a restart; this is
    /// only the value the control initialises to.
    pub midi_device_channel: u8,
    pub ui_auto_connect_to_midi: bool,
    pub ui_auto_set_live_update: bool,
    pub ui_default_bpm: u32,
    pub ui_default_triplet: bool,
    pub ui_max_bank_history_size: u32,
    pub ui_pattern_export_name_prompt: bool,
    pub ui_pattern_export_batch_delay_ms: u32,
    pub ui_rand_default_root: u8,
    pub ui_rand_default_scale: String,
    pub ui_rand_note_percent: u8,
    pub ui_rand_slide_percent: u8,
    pub ui_rand_acc_percent: u8,
    pub ui_rand_ud_percent: u8,
    pub progression_next_pattern_save_step: u32,
}

impl UiConfigSnapshot {
    /// Deterministic defaults for tests that construct an `AppState` directly.
    /// Values match the bundled `config/default_env.template` so tests behave
    /// the same as a fresh install.
    #[cfg(test)]
    pub fn for_tests() -> Self {
        Self {
            midi_device_channel: 1,
            ui_auto_connect_to_midi: true,
            ui_auto_set_live_update: true,
            ui_default_bpm: 120,
            ui_default_triplet: false,
            ui_max_bank_history_size: 200,
            ui_pattern_export_name_prompt: true,
            ui_pattern_export_batch_delay_ms: 2_000,
            ui_rand_default_root: 0,
            ui_rand_default_scale: "minor".into(),
            ui_rand_note_percent: 50,
            ui_rand_slide_percent: 20,
            ui_rand_acc_percent: 30,
            ui_rand_ud_percent: 30,
            progression_next_pattern_save_step: 2,
        }
    }
}

/// Grouped MIDI configuration passed to `AppState::new`.
pub struct MidiConfigBundle {
    pub runtime: MidiRuntimeConfig,
    pub export_options: MidiExportOptions,
    pub import_options: MidiImportOptions,
}

/// Grouped UI/config-path inputs passed to `AppState::new`.
pub struct AppConfigBundle {
    pub ui_config: UiConfigSnapshot,
    pub env_file_path: PathBuf,
    pub user_config_dir: PathBuf,
}

impl AppState {
    /// Test-only constructor - fills in deterministic MIDI runtime defaults
    /// and `MidiExportOptions::default()` / `MidiImportOptions::default()`
    /// so the existing test suite doesn't need to know about the new
    /// runtime-config fields. Production code goes through `AppState::new`.
    #[cfg(test)]
    pub fn for_tests(
        scratch: ScratchSlot,
        library: Arc<LibraryStore>,
        backup_dir_path: String,
        ui_config: UiConfigSnapshot,
        env_file_path: PathBuf,
    ) -> Arc<Self> {
        Self::new(
            scratch,
            library,
            backup_dir_path,
            AppConfigBundle {
                ui_config,
                env_file_path,
                user_config_dir: PathBuf::from("config"),
            },
            MidiConfigBundle {
                runtime: MidiRuntimeConfig {
                    input_port_name: "TD-3".into(),
                    output_port_name: "TD-3".into(),
                    strict_name_match: false,
                    timeout: Duration::from_secs(5),
                    device_channel: 1,
                },
                export_options: MidiExportOptions::default(),
                import_options: MidiImportOptions::default(),
            },
        )
    }

    pub fn new(
        scratch: ScratchSlot,
        library: Arc<LibraryStore>,
        backup_dir_path: String,
        app_config: AppConfigBundle,
        midi: MidiConfigBundle,
    ) -> Arc<Self> {
        let AppConfigBundle {
            ui_config,
            env_file_path,
            user_config_dir,
        } = app_config;
        let MidiConfigBundle {
            runtime: midi_runtime,
            export_options: midi_export_options,
            import_options: midi_import_options,
        } = midi;
        Arc::new(AppState {
            library: LibraryState {
                store: library,
                backup_dir_path,
            },
            midi: MidiState {
                session: Arc::new(Mutex::new(None)),
                session_generation: Arc::new(AtomicU64::new(1)),
                scratch,
                runtime: midi_runtime,
                export_options: midi_export_options,
                import_options: midi_import_options,
            },
            config: ConfigState {
                ui_config,
                env_file_path,
                user_config_dir,
            },
            playback: PlaybackState {
                midi_owner_lifecycle: Arc::new(Mutex::new(())),
                midi_cleanup_pending: Arc::new(AtomicUsize::new(0)),
                clock: Arc::new(Mutex::new(None)),
                cutoff_lane: crate::web::clock::cutoff_lane::new_lane_inbox(),
                audition: Arc::new(Mutex::new(None)),
                transport_generation: Arc::new(AtomicU64::new(1)),
                playing_item_id: Arc::new(Mutex::new(None)),
                control_queue: Arc::new(ControlQueue::new()),
                remote_sync: Arc::new(RemoteSyncQueue::new()),
            },
            scan: ScanState {
                progress: Arc::new(ScanProgress::new()),
                jobs: Arc::new(ScanJobRegistry::new()),
            },
        })
    }
}
