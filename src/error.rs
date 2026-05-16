use thiserror::Error;

#[derive(Error, Debug)]
pub enum Td3Error {
    #[error("MIDI timeout while waiting for {operation} (is the TD-3 connected and powered on?)")]
    Timeout { operation: String },

    #[error("invalid sysex: payload too short (expected at least {expected}, got {actual})")]
    PayloadTooShort { expected: usize, actual: usize },

    #[error("invalid sysex: payload length mismatch (expected exactly {expected}, got {actual})")]
    InvalidPayloadLength { expected: usize, actual: usize },

    #[error("invalid sysex: wrong message ID (expected 0x78, got 0x{actual:02x})")]
    WrongMessageId { actual: u8 },

    #[error("invalid transpose value: {0}")]
    InvalidTranspose(u8),

    #[error("invalid accent value: {0}")]
    InvalidAccent(u8),

    #[error("invalid slide value: {0}")]
    InvalidSlide(u8),

    #[error("invalid time value: {0}")]
    InvalidTime(u16),

    #[error("invalid active steps: {value} (must be 1..=16)")]
    InvalidActiveSteps { value: u8 },

    #[error("invalid note at step {step}: {value} (must be 0..=12)")]
    InvalidNote { step: usize, value: u8 },

    #[error("invalid pattern address: group={patgroup}, slot={slot}, side={side} (expected group 0..=3, slot 0..=7, side 0..=1)")]
    InvalidPatternAddress { patgroup: u8, slot: u8, side: u8 },

    #[error("invalid sysex nibble in {field}: 0x{value:02x}")]
    InvalidNibble { field: &'static str, value: u8 },

    #[error("invalid sysex flag in {field}: 0x{value:02x}")]
    InvalidFlag { field: &'static str, value: u8 },

    #[error("MIDI port '{port_name}' not found (available: {available}). Use 'list-ports' to see all ports")]
    PortNotFound {
        port_name: String,
        available: String,
    },

    #[error("MIDI error: {0}")]
    Midi(String),

    #[error("sysex response error: {0}")]
    SysexResponse(String),

    #[error("upload failed: {0}")]
    UploadFailed(String),

    #[error("device mismatch: expected '{expected}', got '{actual}'. Use --strict-device-name to override")]
    DeviceMismatch { expected: String, actual: String },

    #[error("format error: {0}")]
    FormatError(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("UTF-8 error: {0}")]
    Utf8(#[from] std::str::Utf8Error),

    #[error("invalid argument: {0}")]
    CliError(String),

    #[error("invalid bank address '{0}' (expected G-P<side>, e.g. 1-1A or 2-3B)")]
    BankAddressInvalid(String),

    #[error("duplicate bank address '{0}' in --partial list")]
    BankAddressDuplicate(String),

    #[error("bank folder incomplete - missing subfolder(s): {0}")]
    BankFolderIncomplete(String),

    #[error("bank backup failed: {0}")]
    BankBackupFailed(String),

    #[error("bank import aborted by user before any device write")]
    BankImportAborted,

    #[error(
        "TD-3 control UI is already running on {bind}:{port}.\n       Close the other instance or change WEB_PORT in TD3_CONFIG.env."
    )]
    InstanceRunning { bind: String, port: u16 },

    #[error(
        "could not open TD-3 MIDI port (device busy).\n       Another td3-control instance may be holding the port.\n       Original driver error: {driver_error}"
    )]
    DeviceBusy { driver_error: String },

    #[error("{0}")]
    Other(String),
}
