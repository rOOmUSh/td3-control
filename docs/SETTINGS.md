# SETTINGS

## Purpose

The Settings page at `/settings.html` edits the local configuration that shapes the web UI, randomizer, scale system, keyboard mappings, and export defaults.

It has three major areas:

- Keyboard Mapping
- Scales
- Config sections backed by `TD3_CONFIG.env`

Settings are local files. There is no cloud account, and settings are not synced between app folders.

## Keyboard Mapping

The Keyboard Mapping tab edits `config/keyboard-config.json` (created on first save). Until then, the embedded `keyboard-defaults.json` is served.

It controls:

- note-entry keys
- action keys
- keyboard edit behavior used by the Control page

Use this when your keyboard layout or muscle memory does not match the default TD-3 editing keys.

The reset button reloads defaults from the embedded `keyboard-defaults.json`.

## Scales

The Scales tab edits `config/scales-config.json` (created on first save). Until then, the embedded `scales-defaults.json` is served.

Scale definitions affect:

- randomization
- key detection and scale ranking
- Progression profile resolution
- Magic randomizer candidate generation
- root/scale selectors across the app

The reset button reloads defaults from the embedded `scales-defaults.json`.

Scale IDs should be stable. Other config can refer to them, and progression profile overrides can be keyed by scale ID.

## Runtime Config Editor

The Config section edits `TD3_CONFIG.env` through the web UI.

The server exposes only known editable keys. Unknown keys are rejected on write, and each value is validated by type, range, or option set before the file is updated.

Writes are atomic:

1. stage new content in a temporary file
2. keep a `.bak` backup of the previous file
3. replace `TD3_CONFIG.env`

Most runtime config changes take effect after restarting the app. The page says this explicitly because the server and page state are initialized from config at startup.

## Config Sections

### MIDI And Device

Controls MIDI port discovery and request behavior:

- `MIDI_PORT_SUBSTRING`
- `MIDI_STRICT_NAME_MATCH`
- `MIDI_TIMEOUT_MS`
- `MIDI_RETRIES`
- `MIDI_DEVICE_CHANNEL`

Use strict matching when multiple devices have confusingly similar names. Keep retries conservative because TD-3 writes must not be repeated blindly.

`MIDI_DEVICE_CHANNEL` is the MIDI channel, 1 through 16, that the connected TD-3 listens on. It must match the channel set on the device. Two paths send channel-voice messages and are silent on a mismatch:

- NO-LIVE audition, where the app sequences Note On/Off from the host
- the keyboard note preview

LIVE playback is unaffected either way. It writes the pattern over SysEx and drives the device sequencer with MIDI realtime Start, Clock, and Stop, none of which carry a channel. A device left on a non-matching channel therefore plays in LIVE mode and stays silent in NO-LIVE, which is the symptom this setting resolves.

The default is 1. Configuration files written before this key existed load unchanged and use that default.

The transport bar carries a `CH` selector next to `GATE` that sets the same channel for the session, without editing this file or restarting. It appears whenever Live Update is off, starts on the `MIDI_DEVICE_CHANNEL` value, and is shared by the Control and Progression pages. Use this setting for the channel a device is normally on, and the selector when switching between devices on different channels.

The device does not report its own channel in a form the app can rely on. A `Get Configuration` SysEx exchange returns a `MIDI In Channel` field, but on TD-3 and TD-3-MO units whose channel was set in SynthTribe that field reads `0x00` while the device answers only on its configured channel, so the value cannot be trusted for this purpose and the channel is set here instead.

### Web Server

Controls local server and scratch-slot behavior:

- `WEB_PORT`
- `WEB_BIND`
- `UI_SCRATCH_PATTERN`
- `UI_AUTO_CONNECT_TO_MIDI`
- `UI_AUTO_SET_LIVE_UPDATE`

`UI_SCRATCH_PATTERN` is device-facing. The slot named here can be overwritten by live update, preview, Bank audition, and Progression preview flows.

### Sequencer Defaults

Controls startup defaults:

- `UI_DEFAULT_BPM`
- `UI_DEFAULT_TRIPLET`
- `UI_MAX_BANK_HISTORY_SIZE`

`UI_DEFAULT_BPM` is also used by preview and audition paths where no more specific tempo is selected.

### Randomizer

Controls default randomizer state:

- `UI_RAND_DEFAULT_ROOT`
- `UI_RAND_DEFAULT_SCALE`
- `UI_RAND_NOTE_PERCENT`
- `UI_RAND_SLIDE_PERCENT`
- `UI_RAND_ACC_PERCENT`

The default scale must match a scale ID after normalization. Normalization lowercases and converts spaces to underscores.

### Progression

Controls progression timing behavior:

- `PROGRESSION_NEXT_PATTERN_SAVE_STEP`

This value decides when the progression playback logic preloads or saves the next pattern in its local workflow.

### Bank And Library

Controls persistent library storage:

- `LIBRARY_DATABASE_PATH`
- `BACKUP_DIR_PATH`
- `PATTERN_SIDECAR_DIR`

`LIBRARY_DATABASE_PATH` points to the SQLite catalog. `PATTERN_SIDECAR_DIR` stores device-ready pattern payloads used by compare, replay, duplicate detection, and snapshot export.

### MIDI Export

Controls MIDI file rendering:

- `MIDI_EXPORT_CHANNEL`
- `MIDI_EXPORT_PPQN`
- `MIDI_EXPORT_OCTAVE_OFFSET`
- `MIDI_EXPORT_NORMAL_VELOCITY`
- `MIDI_EXPORT_ACCENT_VELOCITY`
- `MIDI_EXPORT_SLIDE_MODE`
- `MIDI_EXPORT_LOOP_COUNT`

These defaults are shared by CLI and web export paths where applicable.

### Pattern Export

Controls browser downloads from the Main page:

- `UI_PATTERN_EXPORT_NAME_PROMPT`
- `UI_PATTERN_EXPORT_BATCH_DELAY_MS`

`UI_PATTERN_EXPORT_NAME_PROMPT` defaults to `1`. When enabled, choosing an export format opens a dialog for a pattern-set name. When disabled, one local timestamp is used for all files in that export.

`UI_PATTERN_EXPORT_BATCH_DELAY_MS` is the pause between groups of 10 individual pattern downloads. It accepts `0` through `60000` milliseconds and defaults to `2000`. Set it to `0` to disable the pause. Combined RBS exports create one file and do not use this delay.

Both settings are read when the server starts. Restart the app after changing either value.

## Safety Notes

- Changing the scratch pattern changes which hardware slot preview and live-update flows use.
- Changing `WEB_BIND` to `0.0.0.0` makes the local web server reachable from other machines on the network.
- Changing library paths can make an existing library appear empty until the path is restored.
- Resetting a Config section restores bundled defaults for that section only.
- Resetting Keyboard or Scales uses their dedicated defaults files, not `TD3_CONFIG.env`.

## See Also

- [CLI](CLI.md)
- [FORMATS](FORMATS.md)
- [MAIN](MAIN.md)
- [PROGRESSIONS](PROGRESSIONS.md)
- [BANK](BANK.md)

