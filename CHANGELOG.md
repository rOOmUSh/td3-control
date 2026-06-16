# Changelog

## v1.1.2 - 2026-06-16

### Added

- Added a startup launcher flow for selecting the scratch slot, MIDI input, MIDI output, and web UI port before starting the control server.
- Added support for running separate local TD-3 and TD-3-MO control instances with explicit per-device MIDI routing and separate web ports.
- Added macOS release setup instructions covering quarantine removal, local signing, MIDI port verification, and direct control startup commands.
- Added multi-port Remote Sync so one control UI can fan out Play, Stop, BPM, and Triplet commands to multiple local td3-control slave instances.
- Added comma-separated and whitespace-separated remote port lists with duplicate removal, per-port probe results, and automatic migration from the existing single-port setting.

### Changed

- Changed scheduled Remote Sync Play so every configured slave and the local transport use the same `targetEpochMicros`, with partial failures reported by port.

### Fixed

- Fixed startup behavior when multiple TD-3-family devices are connected so each instance uses the selected input and output ports instead of relying on broad device-name matching.
- Fixed cross-process MIDI SysEx handling so concurrent local app instances do not consume each other's device replies during startup and pattern transfer work.
- Fixed launcher-started macOS sessions so the control child runs from the release folder and opens the default browser after the web server is ready.
- Fixed startup auto-connect handling so disabled auto-connect starts the web UI without probing MIDI or running the pre-UI backup.
- Fixed the main page export format menu so moving the pointer from the EXPORT button into the format list no longer crosses a dead hover gap that can close the menu before selection.

### Maintenance

- Updated Rust dependencies reported by Dependabot: `log` to `0.4.32` and `rusqlite` to `0.40.1`.

### Compatibility

- Confirmed support on TD-3 firmware: v1.2.6, v1.3.7 and TD-3-MO v2.0.1
- Pattern file formats, SysEx byte layout, existing CLI commands, and UI storage schemas are unchanged.

## v1.1.1 - 2026-05-28

### Fixed

- Fixed checked-pattern playback ordering so newly checked patterns join by pattern index instead of being appended to the end of the checked timeline.
- Fixed active playback queue replacement when a queued checked pattern is unchecked before wrap. The next still-checked pattern is now queued immediately for both Live Update ON scratch saves and Live Update OFF host audition.
- Fixed single-slot checked playback joining a multi-slot checked loop so the audible pattern tracker stays aligned with the pattern actually playing.

## v1.1.0 - 2026-05-27

### Added

- Added non-saving host audition for hardware playback. When Live Update is off, or a row has `NO SAVE` checked, the app plays timed MIDI notes without writing the scratch slot or starting the TD-3 sequencer.
- Added local Remote Sync for starting playtime on second local td3-control instance from the bottom toolbar so two connected synths start playing simultaneously with mirrored Stop, BPM and Triplet mode changes for two local app instances.
- Added multi-pattern `.rbs` export for checked patterns or all patterns.
- Added bulk Bank selection for visible items, snapshots, and imported folder batches, including selected-record deletion.
- Added checkbox to select all patterns in the main section.
- Added a duplicate gate for derived .pat and .mid files when matching native truth files exist nearby.

### Changed

- Changed the default multi-pattern A/B slot assignment mode to serial order.
- Changed the main reset button to reset checked patterns when any are selected, or all patterns when nothing is checked.
- Changed multi-pattern import and export to work with checked pattern selections and multiple imported files.
- Pattern-row button and bottom toolbar screenshots were updated.

### Fixed

- Fixed `.steps.txt` import so patterns with fewer active steps only require rows inside the declared active-step range.
- Fixed timeline playback tracking so active-step and Triplet timing follow the pattern that is actually audible during queued pattern changes.
- Fixed Live Update ON so the focused active pattern is saved to the scratch slot before regular Live Update playback.
- Fixed Live Update OFF so scratch-slot saving stops and host audition behavior resumes.
- Fixed active-step checks so missing rows inside the active range still fail.
- Fixed duplicate import priority so native formats are preferred before derived or lossy formats.
- Prevented lower-fidelity .pat and .mid files from becoming the canonical imported item before native backup files.
- Skipped oversized app-owned JSON and TOML scan candidates during folder indexing. JSON scan candidates larger than 2550 bytes are skipped. TOML scan candidates larger than 1900 bytes are skipped.

### Known issues

- Remote Sync does not guarantee continued sync when two devices play patterns with different active-step counts and the Triplet mode is toggled ON and OFF; stop playback and press Play again to realign them.
