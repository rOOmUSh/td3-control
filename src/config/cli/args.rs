use clap::Args;

use crate::formats::mid::MidiSlideMode;

#[derive(Args, Debug, Clone)]
pub struct PatternSlotArgs {
    /// Pattern slot address: G1P1A or G1-P1A (G1-4, P1-8, A/B; case-insensitive)
    pub slot: String,
}

#[derive(Args, Debug, Clone)]
pub struct MidiDeviceArgs {
    /// MIDI input port name (defaults to MIDI_PORT_SUBSTRING from TD3_CONFIG.env)
    #[arg(long = "midi-in")]
    pub midi_in: Option<String>,

    /// MIDI output port name (defaults to MIDI_PORT_SUBSTRING from TD3_CONFIG.env)
    #[arg(long = "midi-out")]
    pub midi_out: Option<String>,

    /// MIDI timeout in milliseconds (defaults to MIDI_TIMEOUT_MS from TD3_CONFIG.env)
    #[arg(long = "timeout-ms")]
    pub timeout_ms: Option<u64>,

    /// Require exact MIDI port name match (default: substring match)
    #[arg(long = "strict-device-name")]
    pub strict_device_name: bool,

    /// Number of retries on timeout (defaults to MIDI_RETRIES from TD3_CONFIG.env)
    #[arg(long = "retries")]
    pub retries: Option<u32>,
}

#[derive(Args, Debug, Clone)]
pub struct DevicePatternArgs {
    #[command(flatten)]
    pub target: PatternSlotArgs,

    #[command(flatten)]
    pub midi: MidiDeviceArgs,
}

#[derive(Args, Debug, Clone)]
pub struct MidiRenderArgs {
    /// MIDI export tempo in BPM (defaults to UI_DEFAULT_BPM from TD3_CONFIG.env)
    #[arg(long = "bpm")]
    pub bpm: Option<u32>,

    /// MIDI ticks per quarter note (defaults to MIDI_EXPORT_PPQN from TD3_CONFIG.env)
    #[arg(long = "ppqn")]
    pub ppqn: Option<u16>,

    /// MIDI channel (1-16) (defaults to MIDI_EXPORT_CHANNEL from TD3_CONFIG.env)
    #[arg(long = "mid-channel")]
    pub mid_channel: Option<u8>,

    /// Semitone offset added to exported MIDI notes (defaults to MIDI_EXPORT_OCTAVE_OFFSET from TD3_CONFIG.env)
    #[arg(long = "mid-octave-offset")]
    pub mid_octave_offset: Option<i8>,

    /// Velocity used for accented notes in MIDI export (defaults to MIDI_EXPORT_ACCENT_VELOCITY from TD3_CONFIG.env)
    #[arg(long = "mid-accent-velocity")]
    pub mid_accent_velocity: Option<u8>,

    /// Velocity used for non-accented notes in MIDI export (defaults to MIDI_EXPORT_NORMAL_VELOCITY from TD3_CONFIG.env)
    #[arg(long = "mid-normal-velocity")]
    pub mid_normal_velocity: Option<u8>,

    /// MIDI slide rendering mode: td3 | generic | none (defaults to MIDI_EXPORT_SLIDE_MODE from TD3_CONFIG.env)
    #[arg(long = "mid-slide")]
    pub mid_slide: Option<MidiSlideMode>,

    /// Repeat the pattern N times in MIDI export (defaults to MIDI_EXPORT_LOOP_COUNT from TD3_CONFIG.env)
    #[arg(long = "loop")]
    pub loop_count: Option<u32>,

    /// Target exported length in bars for MIDI export.
    /// Overrides --loop when both are provided.
    #[arg(long = "bars")]
    pub bars: Option<u32>,
}

/// Arguments for the export subcommand (export from device).
#[derive(Args, Debug, Clone)]
pub struct ExportArgs {
    #[command(flatten)]
    pub device: DevicePatternArgs,

    /// Output file (single-file export). Omit for full pack into G1-P1A/ folder.
    #[arg(long = "output", short = 'o')]
    pub output: Option<String>,

    /// Export format(s): syx,toml,steps,json,mid,txt (comma-separated, default: all)
    #[arg(long = "format")]
    pub format: Option<String>,

    #[command(flatten)]
    pub render: MidiRenderArgs,
}

/// Arguments for the import subcommand (push to device).
#[derive(Args, Debug, Clone)]
pub struct ImportArgs {
    #[command(flatten)]
    pub device: DevicePatternArgs,

    /// File to import and push (required)
    #[arg(long = "input", short = 'i')]
    pub input: String,
}

/// Arguments for the convert subcommand (pure file-to-file, no device).
#[derive(Args, Debug, Clone)]
pub struct ConvertArgs {
    /// Source file (.syx, .toml, .json, .steps.txt, .mid, .seq)
    pub input: String,
    /// Destination file - output format is inferred from the extension
    pub output: String,

    #[command(flatten)]
    pub render: MidiRenderArgs,
}

/// Arguments for the extract-bank subcommand (`.sqs` -> folder tree).
#[derive(Args, Debug, Clone)]
pub struct ExtractBankArgs {
    /// Input `.sqs` full-bank file
    pub input: String,
    /// Output folder (will be created; refuses to overwrite unless --force)
    pub output: String,
    /// Overwrite an existing output folder
    #[arg(long = "force")]
    pub force: bool,
}

/// Arguments for the pack-bank subcommand (folder tree -> `.sqs`).
#[derive(Args, Debug, Clone)]
pub struct PackBankArgs {
    /// Input folder containing 64 per-pattern subfolders
    pub input: String,
    /// Output `.sqs` file (refuses to overwrite unless --force)
    pub output: String,
    /// Overwrite an existing output file
    #[arg(long = "force")]
    pub force: bool,
}

/// Arguments for the import-bank subcommand (import `.sqs` to device).
#[derive(Args, Debug, Clone)]
pub struct ImportBankArgs {
    /// Source `.sqs` full-bank file
    #[arg(long = "input", short = 'i')]
    pub input: String,

    /// Comma-separated target addresses, e.g. `1-1A,2-3B,4-8A`
    /// (case-insensitive; omit for full bank)
    #[arg(long = "partial")]
    pub partial: Option<String>,

    /// Force-write patterns detected as silent (all-REST). Default: skip them.
    #[arg(long = "include-silent")]
    pub include_silent: bool,

    /// Directory to write the pre-import backup zip. Defaults to current dir.
    #[arg(long = "backup-dir")]
    pub backup_dir: Option<String>,

    #[command(flatten)]
    pub midi: MidiDeviceArgs,
}

/// Arguments for the control (web UI) subcommand.
#[derive(Args, Debug, Clone)]
pub struct ControlArgs {
    /// Scratch pattern slot, e.g. G1P1A or G1-P1A (defaults to UI_SCRATCH_PATTERN from TD3_CONFIG.env)
    #[arg(long = "scratch-pattern")]
    pub scratch_pattern: Option<String>,

    /// HTTP server port (defaults to WEB_PORT from TD3_CONFIG.env)
    #[arg(long = "port")]
    pub port: Option<u16>,

    /// HTTP server bind address (defaults to WEB_BIND from TD3_CONFIG.env)
    #[arg(long = "bind")]
    pub bind: Option<String>,

    /// Directory to write the pre-UI-session backup zip (defaults to BACKUP_DIR_PATH from TD3_CONFIG.env)
    #[arg(long = "backup-dir")]
    pub backup_dir: Option<String>,

    #[command(flatten)]
    pub midi: MidiDeviceArgs,
}
