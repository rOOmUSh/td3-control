use crate::error::Td3Error;
use crate::midi_io;

#[test]
fn ensure_port_name_available_accepts_strict_exact_match() {
    let ports = vec!["TD-3".to_string(), "TD-3-MO".to_string()];

    assert!(midi_io::ensure_port_name_available(&ports, "TD-3-MO", true).is_ok());
}

#[test]
fn ensure_port_name_available_rejects_strict_partial_match() {
    let ports = vec!["TD-3".to_string(), "TD-3-MO".to_string()];

    match midi_io::ensure_port_name_available(&ports, "TD", true) {
        Ok(()) => panic!("expected strict exact match to reject ambiguous partial query"),
        Err(Td3Error::PortNotFound {
            port_name,
            available,
        }) => {
            assert_eq!(port_name, "TD");
            assert_eq!(available, "TD-3, TD-3-MO");
        }
        Err(other) => panic!("expected PortNotFound, got {:?}", other),
    }
}

#[test]
fn ensure_port_name_available_accepts_non_strict_substring() {
    let ports = vec!["TD-3".to_string(), "TD-3-MO".to_string()];

    assert!(midi_io::ensure_port_name_available(&ports, "TD-3", false).is_ok());
}
