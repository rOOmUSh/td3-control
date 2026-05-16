# TECHNICAL ARCHITECTURE

## High-Level Shape

The project has four major layers:

1. Rust core domain and format code
2. Rust web/API layer
3. static frontend pages and state modules
4. SQLite plus sidecar persistence for the Bank

The same binary can act as:

- a CLI tool
- a local web server
- a device controller
- a bank import/export pipeline

That shared-core approach is one of the strongest design choices in the repo.

## Rust Core

The Rust side owns:

- CLI parsing
- MIDI device access
- TD-3 protocol exchange
- format parsing and serialization
- bank backup and bank package logic
- local catalog persistence

Important modules include:

- `src/config.rs`
- `src/app.rs`
- `src/td3_protocol.rs`
- `src/formats/*`
- `src/bank/*`
- `src/library/*`

## Runtime Configuration

`TD3_CONFIG.env` is the main runtime contract.

Config precedence is:

`CLI flag -> TD3_CONFIG.env -> bundled template`

This config controls:

- MIDI port matching
- timeout and retry behavior
- bind address and port
- scratch pattern
- UI defaults
- export defaults
- bank database path
- backup path
- sidecar path

The app writes the config file automatically on first run if it does not already exist.

## Web Server Layer

The web server lives in `src/web/mod.rs`.

It:

- initializes the library store
- builds a snapshot of UI config
- auto-connects to MIDI when possible
- serves HTML pages
- serves static frontend assets
- exposes JSON APIs under `/api`

The HTML pages are served through small handlers so the app can inject resolved config into the first paint instead of forcing the browser to bootstrap from a later fetch.

### API stability

The JSON routes under `/api` are the local web UI's internal API. They are documented here as architecture, not as a stable third-party integration contract.

The route groups currently cover:

- MIDI status, port listing, connect, disconnect, and sync source
- pattern load, save, import, export, parse-bank, preview, and export-pool
- transport start, stop, and BPM
- keyboard, scales, progression, and runtime config
- progression package export
- Bank items, snapshots, tags, scans, imports, compare, merge, related, duplicates, and audition
- Control-page queue handoff from the Bank

If external API stability becomes a release goal, add a dedicated `API.md` with versioning and compatibility rules before encouraging external clients to depend on these routes.

## MIDI And Transport Model

The device model is more subtle than "open a port and send bytes".

### Session ownership

When idle, the app keeps a direct `MidiOutputConnection` in the active session.

### Playing state

When transport playback starts, a dedicated clock runner thread takes ownership of that output connection.

### Why that matters

The system still needs to support SysEx operations while playback is active, especially for progression mode where the next pattern may need to be queued before the device wraps.

That is why `with_sender` in `src/web/handlers.rs` can route output through either:

- the direct MIDI connection
- the clock runner's queued sender

This is one of the most important internal details in the project because it keeps live playback and live pattern updates from fighting each other.

### Sync source as device state

The TD-3's sequencer clock source (INT, USB, DIN, TRIG) is read from the device on every connect using SysEx command `0x75` (Get Configuration) and written using command `0x1B` (Set Sequencer Clock Source). The session state caches the current value so the UI can render the correct pill selection and the tri-state indicator without re-querying.

`td3-control control` startup forces the source to USB so the UI's transport buttons drive the device by default. Runtime changes from the UI go through `POST /api/midi/sync-source`.

### Offline mode

When the pre-UI backup fails specifically with `PortNotFound`, `app::try_pre_ui_backup` maps the error to `Ok(None)` so `main` falls through to `web::start_server` with no active session. Other backup failures (timeout, busy, malformed reply, disk error) still abort startup. The web layer renders an OFFLINE banner instead of the scratch-pattern prompt and disables the transport, sync-source, and device-only handlers until a device appears.

## Scratch-Slot Contract

The scratch slot is the core hardware bridge.

Many features depend on it:

- main-page live update
- progression preview
- bank item audition
- note preview and transport workflows

By centralizing those actions around a declared scratch slot, the app avoids pretending it can do "temporary" hardware playback without using real device memory.

## Format Pipeline

The app has a broad format surface.

The `src/formats/` modules cover:

- TD-3 SysEx payloads
- JSON and TOML representations
- human-readable `steps.txt`
- MIDI import/export
- `.seq`
- `.pat`
- `.rbs`
- `.sqs`

That allows the same pattern model to move between:

- device memory
- text-based editing
- DAW workflows
- ReBirth and SynthTribe related formats
- bank backup packages

## Bank Persistence Strategy

The Bank uses a hybrid persistence model:

- SQLite is the authoritative metadata store
- sidecar files hold the 112-byte pattern payloads

This split gives the app two benefits:

- structured querying for items, snapshots, batches, tags, and relations
- fast raw-payload access for replay, compare, duplicate analysis, and export

The code explicitly documents SQLite as the primary source of truth while still maintaining an in-memory mirror for some mutation-heavy paths.

## Snapshot And Backup Safety

The app takes atomicity seriously in a few critical places.

### Pre-UI and pre-import backups

Bank backups are built entirely in memory, written as `.tmp`, flushed with `sync_all`, renamed atomically, hashed, and then renamed again to include a short SHA-256 marker.

### Progression package export

Progression ZIP export follows the same style:

- build full archive in memory
- write temp file
- flush
- rename to final archive

This keeps the filesystem from ending up with half-written deliverables after crashes.

## Frontend Architecture

The frontend is intentionally modular rather than framework-heavy.

Key characteristics:

- page-specific entry files
- explicit state modules
- small helper modules for transport, history, preview, export, and import flows
- sessionStorage and localStorage for lightweight persistence

The main page, progression page, and bank page all have different state models because they solve different problems.

That separation is a strength, not a weakness.

## Multipattern As An Architectural Pivot

The multipattern system is not only a feature. It changes the architecture of the main page.

Instead of a page built around one mutable pattern buffer, the main page becomes a stateful session with:

- structure
- selection semantics
- dual playback timelines
- import and replacement rules
- cross-page handoff

That is why so many higher-level flows become possible after it lands.

## Progression Package Layout

The progression package exporter in `src/web/package_export.rs` assembles a ZIP from:

- 4 acid patterns
- 4 active basslines
- optionally all 20 archetype basslines for combined bank exports

Combined formats place those patterns into deterministic slot layouts:

- acid patterns on A-side or Device 1
- basslines on B-side or Device 2

That mapping is what lets a generated progression become a reusable bank-like artifact rather than just a bag of unrelated files.

## Why The Architecture Works

The project works because it keeps the difficult boundaries explicit:

- config is explicit
- scratch-slot ownership is explicit
- playback ownership is explicit
- bank persistence is explicit
- atomic write paths are explicit

That clarity is what lets a relatively small codebase cover live device control, file conversion, pattern generation, and library management without collapsing into one large unstructured tool.

## See Also

- [MAIN](MAIN.md)
- [MULTIPATTERN TECHNOLOGY](MULTIPATTERN_TECHNOLOGY.md)
- [PROGRESSIONS](PROGRESSIONS.md)
- [BANK](BANK.md)
- [CLI](CLI.md)
- [FORMATS](FORMATS.md)
- [SETTINGS](SETTINGS.md)
- [FAQ](FAQ.md)
