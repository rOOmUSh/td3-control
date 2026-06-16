use crate::error::Td3Error;

#[test]
fn sysex_lock_poisoned_error_includes_operation_name() {
    let err = crate::midi_exchange_lock::sysex_lock_poisoned_error("pattern download");

    match err {
        Td3Error::Midi(message) => {
            assert_eq!(message, "MIDI SysEx lock poisoned for pattern download");
        }
        other => panic!("expected MIDI error, got {other:?}"),
    }
}
