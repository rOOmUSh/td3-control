use crate::browser::{
    auto_open_browser_requested_from, control_url, AUTO_OPEN_BROWSER_ENV, SKIP_SCRATCH_CONFIRM_ENV,
};

#[test]
fn auto_open_browser_env_accepts_only_one() {
    assert!(auto_open_browser_requested_from(Some("1")));
    assert!(!auto_open_browser_requested_from(None));
    assert!(!auto_open_browser_requested_from(Some("0")));
    assert!(!auto_open_browser_requested_from(Some("true")));
    assert!(!auto_open_browser_requested_from(Some("")));
}

#[test]
fn control_url_uses_resolved_bind_and_port() {
    assert_eq!(control_url("127.0.0.1", 3030), "http://127.0.0.1:3030");
    assert_eq!(control_url("localhost", 4040), "http://localhost:4040");
}

#[test]
fn launcher_browser_env_names_are_stable() {
    assert_eq!(AUTO_OPEN_BROWSER_ENV, "TD3_AUTO_OPEN_BROWSER");
    assert_eq!(SKIP_SCRATCH_CONFIRM_ENV, "TD3_SKIP_SCRATCH_CONFIRM");
}
