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
pub(crate) use events::{
    control_change_event, encode_vlq, note_off_event, note_on_event, TimedMidiEvent,
    ORDER_CONTROL_CHANGE, ORDER_NOTE_OFF, ORDER_NOTE_ON, ORDER_SLIDE_NOTE_OFF, ORDER_SLIDE_NOTE_ON,
};
pub use options::{MidiExportOptions, MidiSlideMode};
pub use timeline::StepLanes;
pub(crate) use timeline::FILTER_CUTOFF_CC;
#[allow(unused_imports)]
pub(crate) use timeline::{build_timeline, build_timeline_with_gate, build_timeline_with_lanes};
pub use writer::export;
