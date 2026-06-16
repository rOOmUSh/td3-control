//! Web UI port validation and launcher preflight bind checks.

use std::io;
use std::net::{SocketAddr, TcpListener};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebPortStatus {
    Available {
        bind: String,
        port: u16,
    },
    Invalid {
        message: String,
    },
    Occupied {
        bind: String,
        port: u16,
    },
    Unavailable {
        bind: String,
        port: u16,
        message: String,
    },
}

impl WebPortStatus {
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available { .. })
    }

    pub fn port(&self) -> Option<u16> {
        match self {
            Self::Available { port, .. }
            | Self::Occupied { port, .. }
            | Self::Unavailable { port, .. } => Some(*port),
            Self::Invalid { .. } => None,
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::Available { bind, port } => format!("Web UI port available at {}:{}", bind, port),
            Self::Invalid { message } => message.clone(),
            Self::Occupied { bind, port } => {
                format!("Web UI port {}:{} is already in use", bind, port)
            }
            Self::Unavailable {
                bind,
                port,
                message,
            } => format!("Web UI port {}:{} is unavailable: {}", bind, port, message),
        }
    }
}

pub fn parse_web_port(raw: &str) -> Result<u16, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Web UI port is required".to_string());
    }
    let parsed: u32 = trimmed
        .parse()
        .map_err(|_| format!("Web UI port must be an integer in 1..=65535, got '{}'", raw))?;
    if !(1..=65_535).contains(&parsed) {
        return Err(format!(
            "Web UI port {} is out of range, expected 1..=65535",
            parsed
        ));
    }
    Ok(parsed as u16)
}

pub fn validate_web_port(bind: &str, raw: &str) -> WebPortStatus {
    match parse_web_port(raw) {
        Ok(port) => check_web_port_available(bind, port),
        Err(message) => WebPortStatus::Invalid { message },
    }
}

pub fn first_available_web_port(bind: &str, preferred: u16) -> WebPortStatus {
    let mut port = preferred;
    loop {
        let status = check_web_port_available(bind, port);
        match status {
            WebPortStatus::Available { .. } => return status,
            WebPortStatus::Occupied { .. } if port < u16::MAX => {
                port = port.saturating_add(1);
            }
            _ => return status,
        }
    }
}

pub fn check_web_port_available(bind: &str, port: u16) -> WebPortStatus {
    let addr: SocketAddr = match format!("{}:{}", bind, port).parse() {
        Ok(addr) => addr,
        Err(error) => {
            return WebPortStatus::Invalid {
                message: format!("invalid bind address '{}': {}", bind, error),
            };
        }
    };

    match TcpListener::bind(addr) {
        Ok(listener) => {
            drop(listener);
            WebPortStatus::Available {
                bind: bind.to_string(),
                port,
            }
        }
        Err(error) if error.kind() == io::ErrorKind::AddrInUse => WebPortStatus::Occupied {
            bind: bind.to_string(),
            port,
        },
        Err(error) => WebPortStatus::Unavailable {
            bind: bind.to_string(),
            port,
            message: error.to_string(),
        },
    }
}
