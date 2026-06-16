use crate::error::Td3Error;

pub(crate) const AUTO_OPEN_BROWSER_ENV: &str = "TD3_AUTO_OPEN_BROWSER";
pub(crate) const SKIP_SCRATCH_CONFIRM_ENV: &str = "TD3_SKIP_SCRATCH_CONFIRM";

pub(crate) fn auto_open_browser_requested() -> bool {
    auto_open_browser_requested_from(std::env::var(AUTO_OPEN_BROWSER_ENV).ok().as_deref())
}

pub(crate) fn auto_open_browser_requested_from(value: Option<&str>) -> bool {
    value == Some("1")
}

pub(crate) fn control_url(bind_address: &str, listen_port: u16) -> String {
    format!("http://{}:{}", bind_address, listen_port)
}

pub(crate) fn open_default_browser(url: &str) -> Result<(), Td3Error> {
    #[cfg(target_os = "macos")]
    {
        open_default_browser_macos(url)
    }

    #[cfg(not(target_os = "macos"))]
    {
        webbrowser::open(url).map_err(|error| {
            Td3Error::Other(format!("failed to open browser at {}: {}", url, error))
        })
    }
}

#[cfg(target_os = "macos")]
fn open_default_browser_macos(url: &str) -> Result<(), Td3Error> {
    let output = std::process::Command::new("/usr/bin/open")
        .arg(url)
        .output()
        .map_err(|error| {
            Td3Error::Other(format!(
                "failed to start /usr/bin/open for {}: {}",
                url, error
            ))
        })?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = stderr.trim();
    if detail.is_empty() {
        return Err(Td3Error::Other(format!(
            "/usr/bin/open failed for {} with status {}",
            url, output.status
        )));
    }

    Err(Td3Error::Other(format!(
        "/usr/bin/open failed for {} with status {}: {}",
        url, output.status, detail
    )))
}
