use crate::launcher::device_options::{
    exact_name_present, select_configured_port, ConfigPortSelection,
};
use crate::launcher::midi_probe::PortListing;
use crate::launcher::startup_state;

fn names(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

#[test]
fn strict_config_selects_exact_name_only() {
    let ports = names(&["TD-3", "USB TD-3"]);

    assert_eq!(
        select_configured_port(&ports, "TD-3", true),
        ConfigPortSelection::Single("TD-3".to_string())
    );
}

#[test]
fn strict_config_rejects_partial_name() {
    let ports = names(&["USB TD-3"]);

    assert_eq!(
        select_configured_port(&ports, "TD-3", true),
        ConfigPortSelection::None
    );
}

#[test]
fn substring_config_selects_single_match() {
    let ports = names(&["Other", "USB TD-3"]);

    assert_eq!(
        select_configured_port(&ports, "td-3", false),
        ConfigPortSelection::Single("USB TD-3".to_string())
    );
}

#[test]
fn substring_config_reports_td3_and_td3_mo_ambiguity() {
    let ports = names(&["USB TD-3", "USB TD-3-MO"]);

    assert_eq!(
        select_configured_port(&ports, "TD-3", false),
        ConfigPortSelection::Ambiguous(vec!["USB TD-3".to_string(), "USB TD-3-MO".to_string(),])
    );
}

#[test]
fn empty_config_selects_nothing() {
    let ports = names(&["TD-3"]);

    assert_eq!(
        select_configured_port(&ports, "  ", false),
        ConfigPortSelection::None
    );
}

#[test]
fn exact_name_present_requires_full_name() {
    let ports = names(&["USB TD-3"]);

    assert!(exact_name_present(&ports, "USB TD-3"));
    assert!(!exact_name_present(&ports, "TD-3"));
}

#[test]
fn startup_state_allows_env_default_when_no_exact_ports_selected() {
    let ports = PortListing {
        inputs: names(&["TD-3 In"]),
        outputs: names(&["TD-3 Out"]),
    };

    assert!(startup_state::current_midi_choice(&ports, &None, &None).is_ok());
}

#[test]
fn startup_state_rejects_half_selected_midi_pair() {
    let ports = PortListing {
        inputs: names(&["TD-3 In"]),
        outputs: names(&["TD-3 Out"]),
    };
    let input = Some("TD-3 In".to_string());

    let result = startup_state::current_midi_choice(&ports, &input, &None);

    assert!(result.is_err());
}

#[test]
fn startup_state_rejects_selected_port_that_disappeared() {
    let ports = PortListing {
        inputs: names(&["TD-3 In"]),
        outputs: names(&["TD-3 Out"]),
    };
    let input = Some("Other In".to_string());
    let output = Some("TD-3 Out".to_string());

    let result = startup_state::current_midi_choice(&ports, &input, &output);

    assert!(result.is_err());
}
