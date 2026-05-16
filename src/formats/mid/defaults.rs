use std::sync::OnceLock;

/// Fallback BPM used when the runtime value from TD3_CONFIG.env has not been
/// installed yet, such as unit tests or library use outside the app binary.
pub const DEFAULT_BPM_FALLBACK: u32 = 120;

static DEFAULT_BPM_CELL: OnceLock<u32> = OnceLock::new();

/// Install the env-driven default BPM.
pub fn set_default_bpm(bpm: u32) {
    let _ = DEFAULT_BPM_CELL.set(bpm);
}

/// Current default BPM.
pub fn default_bpm() -> u32 {
    DEFAULT_BPM_CELL
        .get()
        .copied()
        .unwrap_or(DEFAULT_BPM_FALLBACK)
}

pub const DEFAULT_PPQN: u16 = 480;
pub const DEFAULT_MIDI_CHANNEL: u8 = 1;
pub const DEFAULT_MIDI_OCTAVE_OFFSET: i8 = 0;
pub const DEFAULT_MIDI_ACCENT_VELOCITY: u8 = 110;
pub const DEFAULT_MIDI_NORMAL_VELOCITY: u8 = 78;
pub const DEFAULT_MIDI_LOOP_COUNT: u32 = 1;
pub(crate) const TD3_MIDI_BASE_PITCH: i16 = 24;
pub(crate) const TD3_MIDI_TOP_PITCH: i16 = TD3_MIDI_BASE_PITCH + 36;
