# FAQ

## What is the difference between Main, Progressions, and Bank?

- `Main` is the live control and multipattern workspace.
- `Progressions` is the four-pattern phrase generator and bassline package workflow.
- `Bank` is the persistent library, snapshot browser, and import/analysis system.

## Do I need a TD-3 connected all the time?

No.

The control UI itself now starts in offline mode when no TD-3 is detected. The pre-UI bank backup is skipped, the scratch-pattern warning is replaced by an OFFLINE banner, and the UI loads normally for editing, generation, library, snapshots, and file workflows.

You need the hardware for:

- live control
- direct device import/export
- transport playback
- preview and audition workflows
- changing the device's MIDI sync source from the UI

You do not need it for:

- file conversion
- bank extraction and packing
- many bank management tasks
- reading and organizing existing library data
- pattern editing, randomization, progression generation, and bassline export

A real device error other than "device not found" (timeout, busy port, malformed reply, disk error) still aborts startup rather than silently degrading to offline mode.

## My UI's PLAY button does not move the TD-3 even though it is connected.

The TD-3's sequencer follows the UI only when its MIDI sync source is set to USB. The transport bar has an INT / USB / DIN / TRIG pill column for changing this from the UI. The MIDI status indicator next to the connect button is tri-state:

- grey when offline or no device is connected
- yellow when the device is connected but its sync source is not USB, so transport buttons cannot drive it
- green when the device is connected and synced to USB

`td3-control control` forces the device to USB on startup so the default state is "UI drives transport". The pill column lets you flip the device into INT, DIN, or TRIG without leaving the UI, and back to USB to regain transport control. The pill column is disabled while the app is in offline mode.

## What is the scratch pattern?

The scratch pattern is the device slot the app is allowed to overwrite during live work.

It is used for:

- live update
- preview
- audition
- playback handoff

The scratch slot is configurable with `UI_SCRATCH_PATTERN` and is intentionally surfaced to the user so it is never a hidden destructive behavior.

## Will the app overwrite my device data?

It can overwrite the configured scratch slot during normal operation.

For higher-risk operations, the app includes safety steps:

- the control UI creates a full-bank backup before the session starts
- `import-bank` takes a mandatory pre-write backup
- the startup flow warns explicitly about scratch-slot use

## Where are backups stored?

By default in `./backups`, controlled by `BACKUP_DIR_PATH`.

The app writes backup zips there before:

- control UI sessions
- full-bank import operations

Those backups can later be synced into the Bank as snapshot records.

## Where is the Bank data stored?

By default:

- SQLite catalog: `ui/config/bank-library.sqlite3`
- sidecar payloads: `ui/config/bank-library-patterns/`

Both are configurable through `TD3_CONFIG.env`.

## Which file formats are supported?

Across the full app, depending on workflow, the project supports:

- `.syx`
- `.toml`
- `.json`
- `.steps.txt`
- `.mid`
- `.seq`
- `.pat`
- `.rbs`
- `.sqs`

The exact available formats depend on whether you are in:

- CLI conversion
- Main-page import/export
- Progression package export
- Bank ingest or snapshot export

## Why is there both a CLI and a web UI?

Because the project solves two different categories of problems.

The web UI is better for:

- editing
- live playback
- progression work
- bank browsing

The CLI is better for:

- scriptable conversion
- device pull/push from the terminal
- extract/pack workflows
- quick port inspection

Both use the same Rust core.

## Is the app Windows-only?

No. Windows and macOS are both supported.

Windows-specific support in the repo:

- Windows startup scripts (`start_server.bat`, `run_full_test.bat`,
  `ui_tests.bat`)
- 1 ms multimedia-timer resolution, `THREAD_PRIORITY_TIME_CRITICAL`, and a
  high-resolution waitable timer for the MIDI clock thread
- native folder-picker driven by `IFileOpenDialog` for the Bank page

macOS-specific support in the repo:

- Mach `THREAD_TIME_CONSTRAINT_POLICY` applied to the MIDI clock thread for
  soft real-time scheduling under load
- native folder-picker driven by AppleScript's `choose folder` through
  `osascript` for the Bank page

Linux and other Unix targets compile and run, but the Bank BROWSE button is
not wired up and the clock thread runs at default scheduling priority. The
sleep + spin-tail fallback in the clock loop still keeps tick jitter low on
modern kernels.

## How do imports avoid filling the Bank with duplicates?

The Bank stores content hashes and sidecar payloads, then uses duplicate analysis and cluster logic to skip or classify repeated material.

That lets repeated scans stay useful instead of flooding the library with obvious copies.

## Can I use the Bank without trusting the device preview path?

Yes.

You can use it as a metadata and file-management system only. Auditioning through the scratch slot is a separate capability, not a requirement for using snapshots, tags, imports, or compare tools.

## Where should I start if I want to understand the internals?

Read these in order:

1. [MAIN](MAIN.md)
2. [MULTIPATTERN TECHNOLOGY](MULTIPATTERN_TECHNOLOGY.md)
3. [PROGRESSIONS](PROGRESSIONS.md)
4. [BANK](BANK.md)
5. [TECHNICAL ARCHITECTURE](TECHNICAL_ARCHITECTURE.md)
