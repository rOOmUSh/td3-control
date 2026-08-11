//! Raw winmm probe used when a MIDI open fails.
//!
//! `midir` reports a connect failure as one opaque string, so every
//! cause looks identical: a device another program holds, a device the
//! driver will not allocate, and a bad device index all arrive as
//! "could not create Windows MM MIDI input port". The distinction is the
//! `MMSYSERR` code that `midir` discards.
//!
//! This calls the same Windows API directly to recover that code, and
//! lists what winmm itself reports so its device table can be compared
//! against the one the port name was chosen from. The four entry points
//! are declared here rather than pulled from a binding crate because
//! `windows-sys` 0.61 no longer ships them.

#[cfg(windows)]
mod winmm {
    /// `MAXPNAMELEN` from mmsyscom.h.
    pub const MAX_PNAME_LEN: usize = 32;

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct MidiInCapsW {
        pub w_mid: u16,
        pub w_pid: u16,
        pub v_driver_version: u32,
        pub sz_pname: [u16; MAX_PNAME_LEN],
        pub dw_support: u32,
    }

    #[link(name = "winmm")]
    extern "system" {
        pub fn midiInGetNumDevs() -> u32;
        pub fn midiInGetDevCapsW(device_id: usize, caps: *mut MidiInCapsW, caps_size: u32) -> u32;
        pub fn midiInOpen(
            handle: *mut isize,
            device_id: u32,
            callback: usize,
            instance: usize,
            flags: u32,
        ) -> u32;
        pub fn midiInClose(handle: isize) -> u32;
    }
}

#[cfg(windows)]
pub fn probe_input_open(port_name: &str) -> String {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    let mut report = String::new();
    unsafe {
        let count = winmm::midiInGetNumDevs();
        report.push_str(&format!("winmm sees {} MIDI input device(s):", count));

        let mut matched: Option<u32> = None;
        for index in 0..count {
            let mut caps: winmm::MidiInCapsW = std::mem::zeroed();
            let result = winmm::midiInGetDevCapsW(
                index as usize,
                &mut caps,
                std::mem::size_of::<winmm::MidiInCapsW>() as u32,
            );
            if result != 0 {
                report.push_str(&format!(" [{} caps error {}]", index, result));
                continue;
            }
            let end = caps
                .sz_pname
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(caps.sz_pname.len());
            let name = OsString::from_wide(&caps.sz_pname[..end])
                .to_string_lossy()
                .into_owned();
            report.push_str(&format!(" [{} {:?}]", index, name));
            if matched.is_none() && name == port_name {
                matched = Some(index);
            }
        }

        if matched.is_none() {
            report.push_str(&format!(
                "; winmm has no device named {:?}, so the name was matched against a \
                 different device table than the one the open uses",
                port_name
            ));
        }

        // Try every input device, not only the one that was wanted. A
        // process that can open none of them has a problem of its own; a
        // process that opens the others has a problem with this device.
        report.push_str("; direct midiInOpen:");
        for index in 0..count {
            let mut handle: isize = 0;
            let result = winmm::midiInOpen(&mut handle, index, 0, 0, 0);
            report.push_str(&format!(
                " [{} -> {} {}]",
                index,
                result,
                describe_mmsyserr(result)
            ));
            if result == 0 {
                winmm::midiInClose(handle);
            }
        }
    }
    report
}

#[cfg(windows)]
fn describe_mmsyserr(code: u32) -> &'static str {
    match code {
        0 => "MMSYSERR_NOERROR, the open succeeded",
        1 => "MMSYSERR_ERROR, unspecified",
        2 => "MMSYSERR_BADDEVICEID, the index is out of range",
        3 => "MMSYSERR_NOTENABLED",
        4 => "MMSYSERR_ALLOCATED, another client holds the device",
        5 => "MMSYSERR_INVALHANDLE",
        6 => "MMSYSERR_NODRIVER",
        7 => "MMSYSERR_NOMEM, could not allocate or lock memory",
        8 => "MMSYSERR_NOTSUPPORTED",
        10 => "MMSYSERR_INVALFLAG",
        11 => "MMSYSERR_INVALPARAM",
        12 => "MMSYSERR_HANDLEBUSY",
        _ => "unrecognised MMSYSERR",
    }
}

#[cfg(not(windows))]
pub fn probe_input_open(_port_name: &str) -> String {
    "raw MIDI probe is Windows only".to_string()
}
