//! API route handlers.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::extract::rejection::JsonRejection;
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::error::Td3Error;
use crate::formats;
use crate::midi_session::{establish_td3_midi_session, Td3MidiSessionConfig};
use crate::td3_protocol;

use super::api_types::*;
use super::clock;
use super::config_storage;
use super::state::{AppState, ClockState, ConfigState, MidiSession, MidiState, PlaybackState};
use super::user_config::{KeyboardConfig, ProgressionConfig, ScalesConfig, UserConfigFile};
use crate::midi_io::SysexSender;

// ---------------------------------------------------------------------------
// Error type for handlers
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum AppError {
    BadRequest(String),
    Conflict(String),
    Internal(String),
    NotFound(String),
    Midi(Td3Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            AppError::Midi(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        };
        (status, Json(ErrorBody { error: message })).into_response()
    }
}

impl From<Td3Error> for AppError {
    fn from(err: Td3Error) -> Self {
        AppError::Midi(err)
    }
}

fn json_payload<T>(
    payload: Result<Json<T>, JsonRejection>,
    name: &'static str,
) -> Result<T, AppError> {
    payload
        .map(|Json(req)| req)
        .map_err(|err| AppError::BadRequest(format!("invalid {} JSON: {}", name, err)))
}

mod audition;
mod config;
mod connect;
mod pattern;
mod scratch;
mod transport;

pub use audition::*;
pub use config::*;
pub use connect::*;
pub use pattern::*;
pub use scratch::*;
pub(crate) use transport::*;
