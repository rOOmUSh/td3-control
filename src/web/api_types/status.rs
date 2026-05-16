use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct StatusResponse {
    pub connected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firmware: Option<String>,
    pub playing: bool,
    /// Truncated integer BPM. Kept for wire compatibility with older
    /// clients; new clients should read `centibpm` for 0.01 precision.
    pub bpm: u32,
    /// Current tempo in centi-BPM (BPM x 100). Exposes the full
    /// fractional tempo the clock thread is running at.
    pub centibpm: u32,
    /// Wire identifier of the device's clock source: "int", "din", "usb", or "trig".
    /// `None` when no MIDI session is open.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync_source: Option<String>,
}

#[derive(Deserialize)]
pub struct SetSyncSourceRequest {
    pub source: String,
}

#[derive(Serialize)]
pub struct SetSyncSourceResponse {
    pub source: String,
}

// ---------------------------------------------------------------------------
// Ports
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct PortsResponse {
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

// ---------------------------------------------------------------------------
// Connect / Disconnect
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ConnectRequest {
    pub in_port: Option<String>,
    pub out_port: Option<String>,
}

#[derive(Serialize)]
pub struct ConnectResponse {
    pub product_name: String,
    pub firmware: String,
}

#[derive(Serialize, Deserialize)]
pub struct DisconnectResponse {
    pub disconnected: bool,
}
