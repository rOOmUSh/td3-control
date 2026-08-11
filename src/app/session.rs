use std::time::Duration;

use crate::config::Config;
use crate::error::Td3Error;
use crate::midi_io;
use crate::pattern::Pattern;
use crate::td3_protocol;

/// Owns the CLI's MIDI transport lifetime: input listener guard, output
/// connection, receive inbox, and shared retry/timeout policy.
pub(super) struct CliDeviceSession {
    _input_guard: midir::MidiInputConnection<()>,
    output: midir::MidiOutputConnection,
    inbox: std::sync::mpsc::Receiver<Vec<u8>>,
    retries: u32,
    timeout: Duration,
}

impl CliDeviceSession {
    pub(super) fn open(config: &Config) -> Result<Self, Td3Error> {
        let (output, inbox, input_guard) = midi_io::open_device_with_retry(
            &config.midi.output_port_name,
            &config.midi.input_port_name,
            config.midi.strict_name_match,
            "td3-control-cli-input",
            "td3-control-cli-output",
        )?;

        let mut session = Self {
            _input_guard: input_guard,
            output,
            inbox,
            retries: config.midi.retry_count,
            timeout: config.midi.request_timeout,
        };
        session.print_device_identity()?;
        Ok(session)
    }

    fn print_device_identity(&mut self) -> Result<(), Td3Error> {
        let info = self.probe_identity()?;
        eprintln!(
            "Device: {}, firmware {}",
            info.product_name, info.firmware_version
        );
        Ok(())
    }

    fn probe_identity(&mut self) -> Result<td3_protocol::DeviceInfo, Td3Error> {
        let retries = self.retries;
        let timeout = self.timeout;
        td3_protocol::with_retry(retries, "device probe", || {
            td3_protocol::probe_device(&mut self.output, &self.inbox, timeout)
        })
    }

    pub(super) fn download_pattern(
        &mut self,
        patgroup: u8,
        slot: u8,
        side: u8,
    ) -> Result<(Vec<u8>, Pattern), Td3Error> {
        let retries = self.retries;
        let timeout = self.timeout;
        td3_protocol::with_retry(retries, "pattern download", || {
            td3_protocol::download_pattern(
                &mut self.output,
                &self.inbox,
                patgroup,
                slot,
                side,
                timeout,
            )
        })
    }

    pub(super) fn upload_pattern(
        &mut self,
        pattern: &Pattern,
        patgroup: u8,
        slot: u8,
        side: u8,
    ) -> Result<(), Td3Error> {
        td3_protocol::upload_pattern(
            &mut self.output,
            &self.inbox,
            pattern,
            patgroup,
            slot,
            side,
            self.timeout,
        )
    }

    pub(super) fn set_sync_source(
        &mut self,
        source: td3_protocol::SyncSource,
    ) -> Result<(), Td3Error> {
        td3_protocol::set_sync_source(&mut self.output, &self.inbox, source, self.timeout)
    }

    pub(super) fn bank_device(&mut self) -> crate::bank::import::MidiBankDevice<'_> {
        crate::bank::import::MidiBankDevice {
            out_conn: &mut self.output,
            rx: &self.inbox,
            retries: self.retries,
            timeout: self.timeout,
        }
    }
}
