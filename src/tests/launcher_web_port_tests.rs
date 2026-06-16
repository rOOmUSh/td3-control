use std::net::TcpListener;
use std::sync::{Mutex, MutexGuard};

use crate::launcher::web_port::{
    first_available_web_port, parse_web_port, validate_web_port, WebPortStatus,
};

static PORT_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn web_port_parser_rejects_invalid_input() {
    for raw in ["", " ", "abc", "-1", "12.5", "0", "65536"] {
        assert!(parse_web_port(raw).is_err(), "accepted invalid port {raw}");
    }
}

#[test]
fn web_port_parser_accepts_boundary_values() {
    assert_eq!(parse_web_port("1"), Ok(1));
    assert_eq!(parse_web_port("65535"), Ok(65_535));
}

#[test]
fn web_port_availability_reports_free_port() {
    let _guard = port_test_guard();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let status = validate_web_port("127.0.0.1", &port.to_string());

    assert!(status.is_available(), "expected available, got {status:?}");
}

#[test]
fn startup_web_port_uses_available_preferred_port() {
    let _guard = port_test_guard();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let status = first_available_web_port("127.0.0.1", port);

    assert_eq!(
        status,
        WebPortStatus::Available {
            bind: "127.0.0.1".to_string(),
            port
        }
    );
}

#[test]
fn startup_web_port_skips_occupied_port() {
    let _guard = port_test_guard();
    let (occupied, next_port) = bind_occupied_port_with_available_next();
    let occupied_port = occupied.local_addr().unwrap().port();

    let status = first_available_web_port("127.0.0.1", occupied_port);

    assert_eq!(
        status,
        WebPortStatus::Available {
            bind: "127.0.0.1".to_string(),
            port: next_port
        }
    );
    drop(occupied);
}

#[test]
fn web_port_availability_reports_occupied_port() {
    let _guard = port_test_guard();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let status = validate_web_port("127.0.0.1", &port.to_string());

    assert_eq!(
        status,
        WebPortStatus::Occupied {
            bind: "127.0.0.1".to_string(),
            port
        }
    );
    drop(listener);
}

#[test]
fn web_port_availability_rejects_invalid_bind_address() {
    let status = validate_web_port("not a bind address", "3030");

    assert!(matches!(status, WebPortStatus::Invalid { .. }));
}

#[test]
fn startup_web_port_keeps_invalid_bind_error() {
    let status = first_available_web_port("not a bind address", 3030);

    assert!(matches!(status, WebPortStatus::Invalid { .. }));
}

fn bind_occupied_port_with_available_next() -> (TcpListener, u16) {
    for _ in 0..100 {
        let occupied = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = occupied.local_addr().unwrap().port();
        if port == u16::MAX {
            continue;
        }
        let next_port = port + 1;
        match TcpListener::bind(("127.0.0.1", next_port)) {
            Ok(next) => {
                drop(next);
                return (occupied, next_port);
            }
            Err(_) => continue,
        }
    }
    panic!("could not find adjacent test ports");
}

fn port_test_guard() -> MutexGuard<'static, ()> {
    PORT_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
