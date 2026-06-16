# MAIN

## Purpose

The main page at `/` is the operational center of the app. It is where direct TD-3 control, pattern editing, multipattern work, import/export, and handoff into other parts of the system come together.

This page is not just a single-pattern editor. It is a performance-aware workspace built around a scratch slot, a multi-pattern canvas, and fast movement between device data, generated material, and stored library items.

## What Happens On Startup

When the app starts in `control` mode:

1. `TD3_CONFIG.env` is loaded or created.
2. The resolved config decides the MIDI port matching, bind address, UI defaults, scratch pattern, backup directory, and library paths.
3. If `UI_AUTO_CONNECT_TO_MIDI=1`, the app tries to perform a full-bank pre-UI backup.
   - If the TD-3 is found, the backup runs and the user is warned that the scratch pattern will be overwritten.
   - If the TD-3 is not found, the app enters offline mode: the backup and scratch warning are skipped, and the UI displays an OFFLINE banner instead.
   - Other failures (timeout, busy port, malformed reply, disk error) still abort startup.
4. If `UI_AUTO_CONNECT_TO_MIDI=0`, startup MIDI probing and the pre-UI backup are skipped until you connect manually from the UI.
5. When the device is found and reachable during startup, the app reads its current MIDI sync source and forces it to USB so the UI's transport buttons drive the device by default.
6. The Axum server starts and serves the UI plus the JSON API.

That sequence matters. With startup MIDI enabled, the app is designed so the backup is created before the interactive session can write anything to the device, and offline startup never silently bypasses a real device error.

## Core Workflow

The main page is built around a simple loop:

1. connect to the TD-3
2. load or import patterns
3. edit one or many patterns in the multipattern canvas
4. preview or live-send through the scratch slot
5. export, push to progression mode, or save to the bank

The UI keeps this loop local. There is no external service and no cloud dependency.

## Main Responsibilities

### 1. Device control

The page can:

- connect to the TD-3
- read device status
- start and stop the MIDI transport
- control BPM
- preview notes
- load from and save to device slots
- switch the device's MIDI sync source between INT, USB, DIN, and TRIG from a four-pill column in the transport bar

The MIDI status indicator next to the connect button is tri-state:

- grey when offline or no device is connected
- yellow when a device is connected but its sync source is not USB, so the UI cannot drive transport
- green when the device is connected and synced to USB

The server side for this lives primarily under `src/web/handlers.rs`. Sync-source changes are served by `POST /api/midi/sync-source` and are disabled while the app is in offline mode.

### 2. Multipattern editing

The visible sequencer grid on the main page is powered by the multipattern modules under `ui/js/multipattern/`.

Instead of treating the current pattern as a single mutable buffer, the page keeps a session of up to 64 patterns with:

- one focused pattern
- zero or more checked patterns for bulk operations
- per-session timelines
- per-session clipboard and undo/redo history

This is the part of the app that makes the main page much more than a simple TD-3 slot editor.

### 3. Import and export

The main page supports direct import of:

- `.toml`
- `.json`
- `.steps.txt`
- `.pat`
- `.seq`
- `.mid`
- `.sqs`
- `.rbs`

The `.sqs` and `.rbs` paths go through a bank-picker flow so the user can select one or more slots from a full-bank file.

Export from the main page is format-driven and aimed at fast round-tripping and sharing.

### 4. Musical assistance

The main page also provides higher-level musical tools:

- randomization
- key detection
- ranked scale suggestions
- Magic randomizer integration
- send-to-progression handoff

That makes it the place where raw pattern editing and more compositional workflows meet.

## Why The Scratch Slot Exists

The TD-3 only gives the software one real place to audition and update data on the hardware: a writable device slot. The app formalizes that into a configured scratch slot.

The scratch slot is used for:

- live update
- pattern preview
- bank audition
- progression preview
- transport-driven playback

By naming the slot explicitly in config and warning the user on startup, the app makes the tradeoff visible instead of hiding it.

## Relationship To The Other Pages

### Main -> Progressions

The main page can hand off the current pattern plus root/scale context into `/progression.html`. Progression mode then treats that pattern as P1 and derives or regenerates the rest of the chain.

### Main -> Bank

The main page can save generated or edited material into the Bank so it stops being just session state and becomes part of the persistent catalog.

### Main -> CLI

The UI is only one front end. The same Rust core also powers the command-line flows for conversion, bank extraction, bank packing, and device import/export.

## What Makes This Page Important

The main page is where the project's three ideas first become visible together:

- direct device control
- multipattern thinking
- safe bridge into larger workflows

If you understand the main page, the rest of the app makes much more sense.

## See Also

- [MULTIPATTERN TECHNOLOGY](MULTIPATTERN_TECHNOLOGY.md)
- [PROGRESSIONS](PROGRESSIONS.md)
- [BANK](BANK.md)
- [CLI](CLI.md)
- [FORMATS](FORMATS.md)
- [SETTINGS](SETTINGS.md)
- [TECHNICAL ARCHITECTURE](TECHNICAL_ARCHITECTURE.md)
