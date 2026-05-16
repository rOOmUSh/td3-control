mod defaults;
mod events;
mod note;
mod options;
mod timeline;
mod timing;
mod writer;

#[allow(unused_imports)]
pub use defaults::{
    default_bpm, set_default_bpm, DEFAULT_BPM_FALLBACK, DEFAULT_MIDI_ACCENT_VELOCITY,
    DEFAULT_MIDI_CHANNEL, DEFAULT_MIDI_LOOP_COUNT, DEFAULT_MIDI_NORMAL_VELOCITY,
    DEFAULT_MIDI_OCTAVE_OFFSET, DEFAULT_PPQN,
};
pub(crate) use defaults::{TD3_MIDI_BASE_PITCH, TD3_MIDI_TOP_PITCH};
#[allow(unused_imports)]
pub(crate) use events::{encode_vlq, TimedMidiEvent};
pub use options::{MidiExportOptions, MidiSlideMode};
#[allow(unused_imports)]
pub(crate) use timeline::build_timeline;
pub use writer::export;
