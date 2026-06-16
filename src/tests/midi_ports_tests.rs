use crate::midi_ports;

#[test]
fn clean_names_removes_empty_names_and_deduplicates() {
    let names = vec![
        "TD-3".to_string(),
        " ".to_string(),
        "TD-3-MO".to_string(),
        "TD-3".to_string(),
        String::new(),
    ];

    assert_eq!(
        midi_ports::clean_names(names),
        vec!["TD-3".to_string(), "TD-3-MO".to_string()]
    );
}
