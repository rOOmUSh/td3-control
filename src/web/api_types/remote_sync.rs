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
    pub port: u16,
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
    pub port: u16,
}

impl From<RemoteSyncRelayRequest> for RemoteSyncCommand {
    fn from(req: RemoteSyncRelayRequest) -> Self {
        Self {
            command: req.command,
            centibpm: req.centibpm,
            target_epoch_micros: req.target_epoch_micros,
            triplet: req.triplet,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSyncCommandResponse {
    pub ok: bool,
    pub queued: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSyncProbeResponse {
    pub ok: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSyncPollResponse {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<RemoteSyncCommand>,
}
