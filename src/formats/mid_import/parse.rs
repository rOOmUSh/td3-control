use crate::error::Td3Error;

// ---------------------------------------------------------------------------
// Raw MIDI file parsing
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub(super) struct ParsedSmf {
    pub(super) ppqn: u32,
    pub(super) events: Vec<TimedEvent>,
    /// Largest tick observed across all parsed events (including meta and
    /// EOT). Used to derive `active_steps` when trailing ties/rests leave
    /// the last onset earlier than the intended pattern length.
    pub(super) last_tick: u32,
}

/// Channel-voice events we care about. Everything else (CC, meta, sysex,
/// program change, pressure, pitch bend) is parsed for length-tracking and
/// then discarded.
#[derive(Debug, Clone, Copy)]
pub(super) enum MidiEvent {
    NoteOn { pitch: u8, velocity: u8 },
    NoteOff { pitch: u8 },
}

#[derive(Debug, Clone, Copy)]
pub(super) struct TimedEvent {
    pub(super) tick: u32,
    pub(super) event: MidiEvent,
}

pub(super) fn parse_smf(bytes: &[u8]) -> Result<ParsedSmf, Td3Error> {
    if bytes.len() < 14 || &bytes[0..4] != b"MThd" {
        return Err(Td3Error::FormatError("missing MThd header".to_string()));
    }
    let header_len = read_u32_be(&bytes[4..8])? as usize;
    if header_len < 6 || 8 + header_len > bytes.len() {
        return Err(Td3Error::FormatError("truncated MThd chunk".to_string()));
    }
    let format = read_u16_be(&bytes[8..10])?;
    let ntrks = read_u16_be(&bytes[10..12])? as usize;
    let division = read_u16_be(&bytes[12..14])?;
    if division & 0x8000 != 0 {
        return Err(Td3Error::FormatError(
            "SMPTE timing is not supported (only PPQN)".to_string(),
        ));
    }
    let ppqn = division as u32;
    if ppqn == 0 {
        return Err(Td3Error::FormatError("PPQN=0 is invalid".to_string()));
    }
    if format > 2 {
        return Err(Td3Error::FormatError(format!(
            "unknown SMF format {} (expected 0, 1, or 2)",
            format
        )));
    }

    // Parse all tracks; merge note events. Format 1 stores tracks in
    // parallel - we union them because TD-3 is a single monophonic voice.
    let mut events: Vec<TimedEvent> = Vec::new();
    let mut last_tick: u32 = 0;
    let mut cursor = 8 + header_len;
    for _ in 0..ntrks {
        if cursor + 8 > bytes.len() {
            return Err(Td3Error::FormatError("truncated track header".to_string()));
        }
        if &bytes[cursor..cursor + 4] != b"MTrk" {
            return Err(Td3Error::FormatError("expected MTrk chunk".to_string()));
        }
        let track_len = read_u32_be(&bytes[cursor + 4..cursor + 8])? as usize;
        let track_start = cursor + 8;
        let track_end = track_start
            .checked_add(track_len)
            .ok_or_else(|| Td3Error::FormatError("track length overflow".to_string()))?;
        if track_end > bytes.len() {
            return Err(Td3Error::FormatError("truncated MTrk data".to_string()));
        }
        let track_last_tick = parse_track(&bytes[track_start..track_end], &mut events)?;
        last_tick = last_tick.max(track_last_tick);
        cursor = track_end;
    }

    // Stable sort so ties at the same tick keep track order.
    events.sort_by_key(|e| e.tick);
    Ok(ParsedSmf {
        ppqn,
        events,
        last_tick,
    })
}

fn parse_track(track: &[u8], out: &mut Vec<TimedEvent>) -> Result<u32, Td3Error> {
    let mut i = 0;
    let mut tick = 0u32;
    let mut running_status: Option<u8> = None;

    while i < track.len() {
        let (delta, consumed) = read_vlq(&track[i..])?;
        i += consumed;
        tick = tick
            .checked_add(delta)
            .ok_or_else(|| Td3Error::FormatError("tick overflow parsing track".to_string()))?;

        if i >= track.len() {
            return Err(Td3Error::FormatError("truncated event in MTrk".to_string()));
        }

        let mut status = track[i];
        let data_start;
        if status < 0x80 {
            // Running status: reuse previous channel-voice status, and the
            // byte we just peeked is actually the first data byte.
            status = running_status.ok_or_else(|| {
                Td3Error::FormatError("running status with no prior status byte".to_string())
            })?;
            data_start = i;
        } else {
            data_start = i + 1;
        }

        if status == 0xFF {
            // Meta event: 0xFF, type, VLQ length, data. Drop entirely but
            // keep parsing so we advance past it correctly.
            if data_start + 1 > track.len() {
                return Err(Td3Error::FormatError("truncated meta event".to_string()));
            }
            let (mlen, mc) = read_vlq(&track[data_start + 1..])?;
            let end = data_start
                .checked_add(1 + mc + mlen as usize)
                .ok_or_else(|| Td3Error::FormatError("meta length overflow".to_string()))?;
            if end > track.len() {
                return Err(Td3Error::FormatError(
                    "meta event extends past MTrk end".to_string(),
                ));
            }
            i = end;
            // Meta events do not update running status.
            continue;
        }

        if status == 0xF0 || status == 0xF7 {
            // SysEx / escape: VLQ length, data. Drop and advance.
            let (slen, sc) = read_vlq(&track[data_start..])?;
            let end = data_start
                .checked_add(sc + slen as usize)
                .ok_or_else(|| Td3Error::FormatError("sysex length overflow".to_string()))?;
            if end > track.len() {
                return Err(Td3Error::FormatError(
                    "sysex event extends past MTrk end".to_string(),
                ));
            }
            i = end;
            running_status = None;
            continue;
        }

        // Channel-voice message. Updates running status.
        running_status = Some(status);
        let high = status & 0xF0;
        let data_bytes = match high {
            0x80 | 0x90 | 0xA0 | 0xB0 | 0xE0 => 2,
            0xC0 | 0xD0 => 1,
            _ => {
                return Err(Td3Error::FormatError(format!(
                    "unknown MIDI status byte 0x{:02X}",
                    status
                )));
            }
        };
        if data_start + data_bytes > track.len() {
            return Err(Td3Error::FormatError(
                "truncated channel-voice event".to_string(),
            ));
        }

        match high {
            0x90 => {
                let pitch = track[data_start];
                let velocity = track[data_start + 1];
                if pitch > 127 || velocity > 127 {
                    return Err(Td3Error::FormatError("MIDI data byte > 127".to_string()));
                }
                if velocity == 0 {
                    // 0x90 with velocity 0 is a note-off by convention.
                    out.push(TimedEvent {
                        tick,
                        event: MidiEvent::NoteOff { pitch },
                    });
                } else {
                    out.push(TimedEvent {
                        tick,
                        event: MidiEvent::NoteOn { pitch, velocity },
                    });
                }
            }
            0x80 => {
                let pitch = track[data_start];
                let vel = track[data_start + 1];
                if pitch > 127 || vel > 127 {
                    return Err(Td3Error::FormatError("MIDI data byte > 127".to_string()));
                }
                out.push(TimedEvent {
                    tick,
                    event: MidiEvent::NoteOff { pitch },
                });
            }
            _ => {
                // Ignore CC, program change, pressure, pitch bend - they
                // don't influence step derivation.
            }
        }

        i = data_start + data_bytes;
    }

    Ok(tick)
}

fn read_u32_be(slice: &[u8]) -> Result<u32, Td3Error> {
    if slice.len() < 4 {
        return Err(Td3Error::FormatError("expected 4 bytes".to_string()));
    }
    Ok(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_u16_be(slice: &[u8]) -> Result<u16, Td3Error> {
    if slice.len() < 2 {
        return Err(Td3Error::FormatError("expected 2 bytes".to_string()));
    }
    Ok(u16::from_be_bytes([slice[0], slice[1]]))
}

fn read_vlq(slice: &[u8]) -> Result<(u32, usize), Td3Error> {
    let mut value: u32 = 0;
    for (idx, b) in slice.iter().take(5).enumerate() {
        value = (value << 7) | ((b & 0x7F) as u32);
        if b & 0x80 == 0 {
            return Ok((value, idx + 1));
        }
    }
    Err(Td3Error::FormatError(
        "VLQ longer than 5 bytes or truncated".to_string(),
    ))
}
