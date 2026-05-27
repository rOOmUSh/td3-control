//! Propellerhead ReBirth RB-338 `.rbs` song file reader and writer.
//!
//! An `.rbs` song embeds two TB-303 synth chunks, each holding 32 patterns
//! in 4 banks with 8 slots per bank. The module exposes the file as a flat
//! 64-pattern bank while preserving all non-pattern bytes during serialization.

mod bank;
mod chunks;
mod record;
mod single;
mod song;

pub use bank::{export_bank, import_bank};
pub use single::{export_single, export_single_at, import_single};
pub use song::{index_for, RbsSong};

/// IFF chunk ID for a TB-303 device.
pub(super) const CHUNK_303: &[u8; 4] = b"303 ";

/// Expected active-step count for generated silent slots.
pub(super) const DEFAULT_ACTIVE_STEPS: u8 = 16;

pub const DEVICES: usize = 2;
pub const GROUPS_PER_DEVICE: usize = 4;
pub const SLOTS_PER_GROUP: usize = 8;
pub const SLOTS_PER_DEVICE: usize = GROUPS_PER_DEVICE * SLOTS_PER_GROUP;
pub const TOTAL_SLOTS: usize = SLOTS_PER_DEVICE * DEVICES;
pub const STEPS_PER_PATTERN: usize = 16;

/// Bytes per pattern record: 2-byte header plus 16 2-byte steps.
pub const RECORD_LEN: usize = 2 + STEPS_PER_PATTERN * 2;

/// Bytes of synth-knob config before the 32 pattern records in a `303 ` chunk.
pub const CONFIG_LEN: usize = 9;

/// Expected payload size of a `303 ` chunk.
pub const CHUNK_303_PAYLOAD_LEN: usize = CONFIG_LEN + SLOTS_PER_DEVICE * RECORD_LEN;

/// Bundled default template used for fresh `.rbs` writes.
pub const DEFAULT_TEMPLATE: &[u8] = include_bytes!("../rbs_template.rbs");
