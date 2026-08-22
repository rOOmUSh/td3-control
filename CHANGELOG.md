# Changelog

## v1.3.0 - 2026-08-21

### Added

- Added a `CUTOFF` knob to the transport bar for the TD-3-MO. The MO accepts Filter Cutoff as MIDI CC 74 over USB, and the knob sends exactly that message, on the channel chosen with `CH`, through `POST /api/device/filter-cutoff`. The knob appears whenever the connected device reports firmware `2.0.1`, the TD-3-MO firmware that accepts the message, and is hidden on any other firmware; the flag travels as `device_controls` in the `/api/status` and `/api/midi/connect` replies. The value is `0` to `127`. Sends are throttled to one per 30 ms during a drag and always end on the final value, and each reply reflects the real port write: while idle the bytes go straight to the port, during `LIVE` playback through the clock thread, and during non-saving host audition through the audition thread, which writes them between its scheduled notes without disturbing their timing.
- Added a per-pattern step lane drawer on the Control and Progression pages. A thin handle under every pattern card slides open a green box of sixteen small knobs per lane, each aligned under its step column with a digit readout coloured from red at the minimum through a dimmed white at the centre to bright green at the maximum. The `CUTOFF` lane, shown while the device reports firmware `2.0.1`, sends Filter Cutoff (CC 74, `0` to `127`, default `64`) at the start of every step; the `GATE` lane, shown while `LIVE` is off, sets each step's ordinary note length (`1` to `100` percent, default `50`). Each lane has a horizontal ON/OFF switch and starts off, a `RAND` button that fills its sixteen steps with random values and switches it on, and an all-steps ratio knob that scales every step relative to its own value toward the top or bottom and restores the stored values at centre. In `NO-LIVE` mode the lanes travel with every audition request as `stepCutoffs` and `stepGates` and a change lands on the running audition's next update, at every `TRIPLET` morph amount: the morph moves timing only, each step's values travel with their step, a retired step's cutoff leaves with its note, the per-step gate feeds the morph's gate compensation, and the drawer cells move with the step cards while staying editable; in `LIVE` mode the clock thread emits the cutoff lane on the MIDI Clock pulse that starts each step, switching with the playing pattern at once and with a timeline pre-load at the next wrap, through `POST /api/transport/step-lane`. On a device that accepts CC 74, turning a cutoff knob is also sent immediately. Lane values stay with the pattern for the browser session and never reach device memory, the bank, or exports. A card whose device and mode leave no lane to show (a regular TD-3 in `LIVE` mode) has no handle.
- Added StepDSL v1.1 (`format=td3-stepdsl-v1.1`). Every `.steps.txt` written by the app now carries the per-step `CUTOFF` and `GATE` values as `|CO:n|GT:n` on each active row, the lane switches as `pattern_co_lane` and `pattern_gt_lane`, the TRIPLET morph amount as `triplet_morph` and `triplet_morph_percentage`, and the LIVE button as `live_update`. Importing or pasting such a file restores the lanes and switches on the new card, sets the TRIPLET amount when the pattern can use it, and switches LIVE to the file's state. An untouched lane exports the transport-bar value on every step; the ratio knob is not exported. Import is lenient: out-of-range values clamp, an unusable lane falls back to v1 behaviour, unknown header keys are ignored under the new tag, the cutoff lane is dropped unless the connected device accepts CC 74, and a morph amount is ignored for a pattern whose active steps are not a multiple of four. Every existing `td3-stepdsl-v1` file still reads unchanged.
- Added `POST /api/device/pitch-bend` (14-bit Pitch Bend, `0` to `16383`, center `8192`) and a matching `BEND` knob module. The knob is commented out of the transport bar because the device showed no audible response to Pitch Bend; the endpoint and module stay in place.
- Added the TD-3-MO product name to the names the protocol session accepts. Previously a device answering `TD-3-MO` to the product-name query was rejected with a device mismatch and could not connect at all.

### Compatibility

- `StatusResponse` gains an optional `device_controls` field, present only while connected; `ConnectResponse` gains a required boolean of the same name. Old clients ignore both. No pattern, file, or SysEx format changes.
- `.steps.txt` files are now written with the `td3-stepdsl-v1.1` tag, nine header lines, and `|CO:n|GT:n` on every row. Readers built before this release reject the new tag; this app reads both. `POST /api/pattern/export` accepts an optional `stepsMeta` object for `steps_txt`, `POST /api/pattern/export-pool` an optional `stepsMeta` list, and `POST /api/pattern/import` returns `stepsMeta` for a v1.1 document.
- The host-audition requests (`/api/pattern/audition`, `/update`, `/queue-next-cycle`) accept optional `stepCutoffs` and `stepGates` arrays of exactly 16 values; omitted fields keep the previous behaviour. Patterns in `sessionStorage` may carry a `lanes` object; the server and every export ignore it, and a pattern without it resolves to the defaults.

## v1.2.0 - 2026-08-10

Everything in this release is new since v1.1.2.

### Added

- Added a `GATE` control for host audition, the Live Update OFF playback path, which previously held every ordinary note for a fixed half step. A footer knob now sets that length anywhere from `1` to `100` percent of the step, and the value is shared by the Control and Progression pages. Changing it mid-playback does not restart or reschedule anything: the note already sounding reaches its scheduled Note Off and later notes take the new length, so the cadence and the step highlighting carry on undisturbed. Row preview uses the same value, and a row marked `NO SAVE` keeps using it even with Live Update on, where the footer control is hidden. Ties, rests, slides and accents keep their own timing rules; only ordinary notes are affected.
- Added triplet morph, a continuous sweep from a straight pattern to its triplet form. With Live Update off, a `TRIPLET` knob morphs a 16-step pattern into its 12-step triplet over `0` to `100`, and every intermediate amount is a real schedule played on the device rather than a picture on screen: the four beat anchors hold, the remaining cells slide from their sixteenth positions toward the triplet grid in proportion to the amount, and the bar keeps its length throughout. A beat has four source cells but only three triplet targets, so one cell of each beat converges on the neighbour it collides into and leaves once the sweep passes `80` percent, with the survivor taking over the slot so the pulse holds and only the note count drops. The audible gate widens through the sweep to compensate for the device ringing a fixed time past every Note Off, which would otherwise thin the phrase out as the cells widen; the gate setting itself is untouched. With Live Update on, a yellow `MORPH` checkbox next to the `TRIPLET` button, and one on each pattern row, makes `TRIPLET` write the 12-step triplet projection to the device instead of only setting the native triplet flag. That result is an ordinary 12-step pattern that saves, exports and uploads through the existing paths, and switching `TRIPLET` back off restores the original sixteen steps with their notes at their original indexes. Available on the Control and Progression pages, for 4, 8, 12 and 16 step sources, with a `0`/`100` toggle for jumping between straight and full triplet without sweeping, survivor editing at the endpoint, and a derived view that moves each step card and its `UP`/`DN`/`SL`/`AC` block together without touching the stored pattern.
- Added a device MIDI channel setting, because host audition and the keyboard note preview address the TD-3 with channel-voice messages that a device configured for another channel discards. A `CH` selector sits in the transport bar between `LIVE` and `REMOTE` whenever Live Update is off, and `MIDI_DEVICE_CHANNEL` in the `MIDI & DEVICE` settings section supplies the value it starts on. The selector applies for the session without editing a file or restarting: the channel travels with each audition and preview request, so a change takes effect on the next one while playback continues. The choice is shared by the Control and Progression pages, and the control is hidden in LIVE mode because device playback carries no channel. The default is `1`, and a configuration file written before the key existed loads unchanged on that default.
- Added tempo metadata to StepDSL. Saved and exported StepDSL files now carry the BPM they were made at, through the CLI, the control workflows, bank snapshot exports, and progression package files, and an import reads it back instead of falling back to a default. Short patterns write only their active rows.
- Added named batched pattern downloads, so exporting several patterns at once produces individually named files rather than one opaque batch.

### Changed

- Changed host-audition schedule replacement to carry stable per-event identities. An event already played in the current cycle is never replayed when its timing moves, and an update that cannot be applied safely mid-cycle is deferred whole to the next cycle instead of dropping or firing an event late.

### Fixed

- Fixed NO-LIVE audition and note preview producing no sound on a TD-3 set to any MIDI channel other than `1`. Both paths encoded channel `1` with no way to change it, and a TD-3 discards channel-voice messages addressed elsewhere, so the app sequenced correctly and the device stayed silent. LIVE playback hid the fault, because it writes the pattern over SysEx and drives the device sequencer with MIDI realtime Start, Clock and Stop, none of which carry a channel: the same device played in LIVE and produced nothing in NO-LIVE, which made the fault look specific to one operating system. Measured on a TD-3-MO set to channel `3`, the host emitted a correct `192` bytes per `4` seconds, exactly `16` Note On and `16` Note Off per cycle, while the recording sat at the `-70.3 dB` noise floor against `-7.3 dB` for the same pattern in LIVE mode. The configured channel now reaches every channel-voice message the app sends, including the shutdown Note Off sweep and the All Notes Off that ends an audition, so a note sounded on channel N is always stopped on channel N rather than left ringing. With the channel set correctly the same measurement reads `-7.31 dB`, matching LIVE.
- Fixed a cycle-timing defect where every mid-cycle schedule update re-derived the loop origin from the wall clock, folding scheduler latency into the grid. Sweeping a control during playback accumulated a permanent offset; hardware capture measured a `110 ms` step after one sweep. An update whose cycle length is unchanged now preserves the loop origin exactly, and only a real tempo change re-anchors it. Verified over a `4` minute `16` second capture: no accumulation, residual timing flat across the whole take.
- Fixed step highlighting drifting away from what the device was playing. Highlighting now follows the MIDI clock rather than a free-running page timer, no-live audition rollovers are synchronized to the audition cycle, and a backgrounded browser tab keeps advancing the highlight instead of stalling and jumping on return.
- Fixed the pattern card list rebuilding itself for changes that do not affect any card, a cost carried since the multi-pattern UI was introduced. Every state change rebuilt every element, so the work scaled with the number of patterns loaded and repeated for notifications that changed nothing a card is drawn from. With 64 patterns that is `17,792` elements and a measured `246.85 ms` per rebuild, which a control sweep asks for around 25 times a second: the work queued faster than it could complete and the tab grew to roughly `800 MB` of collectable garbage, while the same eager rebuilding made the cards flicker once per MIDI status poll. Changes that no card renders from are now skipped, and the rebuilds that remain are coalesced to one per animation frame. Cost per step at 64 patterns fell to `11.78 ms`, a sustained 600-step sweep peaks at `19.2 MB`, and the poll no longer redraws anything.
- Fixed a failed MIDI port open reporting nothing useful. The driver error is now surfaced with the code the operating system actually returned, a transient open is retried briefly before giving up, every device query is bounded instead of able to wait forever, and a startup failure is written to `td3-control-startup-error.log` beside the executable with the error, exit code, command line and working directory. A launcher-started session that died immediately previously left no window and no message.
- Fixed the app refusing to start on a host with no MIDI subsystem, such as a headless machine or a container. It now reports no ports and starts in offline mode.
- Fixed StepDSL `active_steps` handling so short patterns only require and export active rows.

### Compatibility

- Confirmed support on TD-3 firmware: v1.2.6, v1.3.7 and TD-3-MO v2.0.1
- Pattern file formats, SysEx byte layout, and existing CLI commands are unchanged. With `MIDI_DEVICE_CHANNEL` left at its default of `1`, host audition emits the same bytes as previous releases.
- StepDSL gained an optional tempo field. Files written by earlier versions load unchanged, and a file carrying tempo stays readable by a reader that ignores it.
- UI storage gained two session-scoped additions for the triplet work, both optional and both validated on read: a `td3_triplet_morph_session_v1` key holding the transient sweep state, and a `td3_triplet_morph_send_v1` key holding the sixteen-step sources a `MORPH` projection can be undone back to. The main `td3_multipattern` blob gained a `tripletMorphSendFlags` field, and the device channel is remembered under `td3_midi_channel`. A session written by an earlier version loads unchanged, a malformed or unrecognised payload is discarded rather than trusted, and nothing here reaches saved pattern files.

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
