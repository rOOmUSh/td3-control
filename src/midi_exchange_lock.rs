//! Cross-process guard for MIDI SysEx request/response exchanges.
//!
//! TD-3-family devices share the same SysEx manufacturer and device header.
//! Serializing request/response exchanges prevents two local td3-control
//! processes from issuing indistinguishable pattern requests at the same time.

use std::time::Duration;

use crate::error::Td3Error;

#[cfg(all(windows, not(test)))]
pub(crate) struct SysexExchangeGuard {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(all(windows, not(test)))]
pub(crate) fn acquire(operation: &str, timeout: Duration) -> Result<SysexExchangeGuard, Td3Error> {
    use windows_sys::Win32::Foundation::{
        CloseHandle, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows_sys::Win32::System::Threading::{CreateMutexW, WaitForSingleObject};

    let name: Vec<u16> = "Local\\td3-control-midi-sysex-exchange\0"
        .encode_utf16()
        .collect();

    // SAFETY: security attributes are null, initial owner is false, and
    // `name` is a null-terminated UTF-16 string that lives through the call.
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    if handle.is_null() {
        return Err(Td3Error::Midi(format!(
            "failed to create MIDI SysEx lock for {}",
            operation
        )));
    }

    // SAFETY: `handle` is either a valid mutex handle from CreateMutexW or
    // the function returned above. The timeout is bounded to Win32's u32 API.
    let wait_status = unsafe { WaitForSingleObject(handle, duration_to_wait_ms(timeout)) };
    match wait_status {
        WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(SysexExchangeGuard { handle }),
        WAIT_TIMEOUT => {
            // SAFETY: `handle` is a valid mutex handle and is not owned here.
            unsafe {
                let _ = CloseHandle(handle);
            }
            Err(Td3Error::Timeout {
                operation: format!("MIDI SysEx lock for {}", operation),
            })
        }
        other => {
            // SAFETY: `handle` is a valid mutex handle and is not owned here.
            unsafe {
                let _ = CloseHandle(handle);
            }
            Err(Td3Error::Midi(format!(
                "failed to acquire MIDI SysEx lock for {}: wait status {}",
                operation, other
            )))
        }
    }
}

#[cfg(all(windows, not(test)))]
impl Drop for SysexExchangeGuard {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::ReleaseMutex;

        // SAFETY: `handle` is a mutex handle acquired by this guard.
        unsafe {
            let _ = ReleaseMutex(self.handle);
            let _ = CloseHandle(self.handle);
        }
    }
}

#[cfg(all(windows, not(test)))]
fn duration_to_wait_ms(duration: Duration) -> u32 {
    let millis = duration.as_millis();
    if millis == 0 {
        0
    } else {
        millis.min(u32::MAX as u128) as u32
    }
}

#[cfg(any(not(windows), test))]
pub(crate) struct SysexExchangeGuard {
    _guard: std::sync::MutexGuard<'static, ()>,
}

#[cfg(any(not(windows), test))]
pub(crate) fn acquire(operation: &str, _timeout: Duration) -> Result<SysexExchangeGuard, Td3Error> {
    static SYSEX_EXCHANGE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let guard = SYSEX_EXCHANGE_LOCK
        .lock()
        .map_err(|_| sysex_lock_poisoned_error(operation))?;
    Ok(SysexExchangeGuard { _guard: guard })
}

#[cfg(any(not(windows), test))]
pub(crate) fn sysex_lock_poisoned_error(operation: &str) -> Td3Error {
    let mut message = String::from("MIDI SysEx lock poisoned for ");
    message.push_str(operation);
    Td3Error::Midi(message)
}
