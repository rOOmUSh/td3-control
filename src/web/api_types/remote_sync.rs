use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RemoteSyncCommandKind {
    Play,
    Stop,
    Bpm,
    Triplet,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSyncCommand {
    pub command: RemoteSyncCommandKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub centibpm: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_epoch_micros: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub triplet: Option<bool>,
    /// Whether a `triplet` command should write the 12-step morph
    /// projection rather than only setting the device's triplet flag. The
    /// two produce different patterns, so a peer that is not told which one
    /// the sender used will play something else. Absent from an older peer,
    /// which is read as the flag-only behaviour it had.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub morph: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSyncRelayRequest {
    #[serde(default)]
    pub port: Option<u32>,
    #[serde(default)]
    pub ports: Option<Vec<u32>>,
    pub command: RemoteSyncCommandKind,
    #[serde(default)]
    pub centibpm: Option<u32>,
    #[serde(default)]
    pub target_epoch_micros: Option<u64>,
    #[serde(default)]
    pub triplet: Option<bool>,
    #[serde(default)]
    pub morph: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSyncProbeRequest {
    #[serde(default)]
    pub port: Option<u32>,
    #[serde(default)]
    pub ports: Option<Vec<u32>>,
}

impl RemoteSyncRelayRequest {
    pub fn command_payload(&self) -> RemoteSyncCommand {
        RemoteSyncCommand {
            command: self.command.clone(),
            centibpm: self.centibpm,
            target_epoch_micros: self.target_epoch_micros,
            triplet: self.triplet,
            morph: self.morph,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSyncRelayResult {
    pub port: u16,
    pub ok: bool,
    pub queued: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Whether the peer had a browser waiting on its poll when the command
    /// arrived. `Some(false)` means the command was queued for a peer whose
    /// UI is not attached, which is worth reporting but is not a failure:
    /// the command is held and the next poll drains it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub listening: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSyncProbeResult {
    pub port: u16,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Round trip of the probe request in microseconds, measured around the
    /// HTTP call to the peer. Present only for a probe that succeeded: a
    /// failure's duration is the timeout, which says nothing about the link.
    /// This is the server to server hop, which is the one a relayed command
    /// takes, so it is the number a start delay has to cover.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rtt_micros: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSyncCommandResponse {
    pub ok: bool,
    pub queued: bool,
    /// Whether a browser was waiting on this server's poll when the command
    /// was accepted. Absent from an older peer's reply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub listening: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub results: Vec<RemoteSyncRelayResult>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSyncProbeResponse {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub results: Vec<RemoteSyncProbeResult>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSyncPollResponse {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<RemoteSyncCommand>,
}
