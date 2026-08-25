use std::net::TcpListener;

use crate::launcher::web_port::{
    first_available_web_port, parse_web_port, validate_web_port, WebPortStatus,
};

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
    with_free_port(|port| {
        let status = validate_web_port("127.0.0.1", &port.to_string());
        status.is_available().then_some(())
    });
}

#[test]
fn startup_web_port_uses_available_preferred_port() {
    with_free_port(|port| {
        let expected = WebPortStatus::Available {
            bind: "127.0.0.1".to_string(),
            port,
        };
        (first_available_web_port("127.0.0.1", port) == expected).then_some(())
    });
}

#[test]
fn startup_web_port_skips_occupied_port() {
    for attempt in 0..PORT_ATTEMPTS {
        let (occupied, next_port) = bind_occupied_port_with_available_next();
        let occupied_port = occupied.local_addr().unwrap().port();

        let status = first_available_web_port("127.0.0.1", occupied_port);
        drop(occupied);

        let expected = WebPortStatus::Available {
            bind: "127.0.0.1".to_string(),
            port: next_port,
        };
        if status == expected {
            return;
        }
        // The adjacent port was taken between the probe and the call. Try
        // another pair rather than loosening the assertion.
        assert!(
            attempt + 1 < PORT_ATTEMPTS,
            "no adjacent port stayed free across {PORT_ATTEMPTS} attempts, last status {status:?}"
        );
    }
}

#[test]
fn web_port_availability_reports_occupied_port() {
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

/// How many times a port-dependent assertion is retried with a fresh port.
const PORT_ATTEMPTS: usize = 16;

/// Run `attempt` against a port that was free a moment ago, retrying with a
/// fresh port until it succeeds.
///
/// Binding to port 0 and releasing yields a port the OS has just handed out,
/// but nothing stops another process claiming it before the assertion runs,
/// and on Windows a just-closed socket can linger in TIME_WAIT. Retrying
/// keeps the assertions exact rather than loosening them to tolerate a race,
/// which is what made these fail intermittently in CI.
fn with_free_port<T>(attempt: impl Fn(u16) -> Option<T>) -> T {
    for _ in 0..PORT_ATTEMPTS {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind an ephemeral port");
        let port = listener.local_addr().expect("read the bound port").port();
        drop(listener);

        if let Some(value) = attempt(port) {
            return value;
        }
    }
    panic!("no ephemeral port stayed free across {PORT_ATTEMPTS} attempts");
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
