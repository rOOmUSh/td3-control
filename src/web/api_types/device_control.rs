use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Device controls: POST /api/device/filter-cutoff, /api/device/pitch-bend
// ---------------------------------------------------------------------------

/// Value for one device control. `value` is 0 through 127 for filter
/// cutoff and 0 through 16383 for pitch bend; the handler rejects
/// anything larger. `midi_channel` is 1 through 16 and defaults to the
/// configured device channel when omitted.
#[derive(Deserialize)]
pub struct DeviceControlRequest {
    pub value: u32,
    #[serde(default, alias = "midiChannel")]
    pub midi_channel: Option<u32>,
}

/// Per-step Filter Cutoff lane for LIVE (device-sequenced) playback.
/// `cutoffs` is exactly 16 values of 0 through 127, or absent to stop
/// sending while keeping the cycle timing. `active_steps` (1 through 16)
/// and `triplet` must mirror the pattern the device is playing.
/// `at_cycle_boundary` defers the switch to the next pattern wrap.
#[derive(Deserialize)]
pub struct StepLaneRequest {
    #[serde(default)]
    pub cutoffs: Option<Vec<u32>>,
    #[serde(alias = "activeSteps")]
    pub active_steps: u32,
    #[serde(default)]
    pub triplet: bool,
    #[serde(default, alias = "midiChannel")]
    pub midi_channel: Option<u32>,
    #[serde(default, alias = "atCycleBoundary")]
    pub at_cycle_boundary: bool,
}

#[derive(Serialize, Deserialize)]
pub struct StepLaneResponse {
    pub ok: bool,
}

/// `ok` means the message bytes were accepted by the MIDI output port.
/// Neither message has a device reply, so no stronger confirmation exists.
#[derive(Serialize, Deserialize)]
pub struct DeviceControlResponse {
    pub ok: bool,
    pub value: u32,
}
