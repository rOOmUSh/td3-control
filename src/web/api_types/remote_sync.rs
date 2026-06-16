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
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSyncProbeResult {
    pub port: u16,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSyncCommandResponse {
    pub ok: bool,
    pub queued: bool,
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
