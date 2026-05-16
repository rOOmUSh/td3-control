# BANK

## What The Bank Is

The Bank page at `/bank.html` is the app's persistent pattern library.

It is not just a file browser. It is a local catalog with:

- items
- snapshots
- tags
- import batches
- duplicate analysis
- related-group analysis
- compare and merge helpers
- fast replay through cached pattern payloads

The Bank turns generated, imported, and device-derived material into something searchable and reusable.

## Storage Model

The Bank is backed by two storage layers:

- a SQLite database at `LIBRARY_DATABASE_PATH`
- a pattern sidecar directory at `PATTERN_SIDECAR_DIR`

SQLite stores the catalog metadata. The sidecar directory stores the raw 112-byte TD-3 pattern payloads used for:

- replay
- compare
- duplicate detection
- snapshot export

This split keeps the catalog structured while preserving fast access to the device-ready pattern body.

## Main Domain Objects

The key domain types live in `src/library/model.rs`.

### LibraryItem

A single catalog entry for one pattern-sized unit of content.

Items track metadata such as:

- display name
- source kind
- source path
- tags
- favorite/archive state
- snapshot linkage
- format
- root and scale metadata
- duplicate status
- analysis status
- content hash

### Snapshot

A snapshot is a named 64-slot bank view with an origin, description, and optional backup path.

Snapshot origins include:

- backup
- imported
- manual
- merge

### SnapshotSlot

Every snapshot has stable slot rows, including explicit empty slots. That is what lets the UI render a consistent 64-cell grid whether the source came from a device backup, a package push, or a manual snapshot.

## Import Pipeline

The Bank can ingest:

- files directly
- folder scans
- backup zips synced from the backup directory

The ingest pipeline records import batches so the user can review what happened later.

Each imported file can end up as:

- imported item
- duplicate skipped
- unsupported
- failed

The pipeline is intentionally visible. The app does not pretend every import succeeded silently.

## Importing And Scanning

The Bank toolbar has two import paths:

- `FOLDER SCAN` scans a folder and can recurse into subfolders.
- `IMPORT` accepts absolute file paths, one per line.

Folder scan supports a live progress display. While the server walks and parses files, the UI reports how many files were found and how many have been parsed. The scan result is recorded as an import batch so the user can inspect it later in the Imported Folders view.

The Browse button is platform-specific:

- Windows uses the native folder picker.
- macOS uses AppleScript's `choose folder`.
- Linux and other Unix builds currently return an error for Browse, so paste a folder path manually.

Failed imports are not hidden. The Failed Imports view and retry-failed flow let the user retry a batch after fixing paths, permissions, or malformed source files.

## Search And Filters

The search box supports both free text and structured tokens.

Supported tokens:

| Token | Meaning |
| --- | --- |
| `tag:acid` | Filter by tag |
| `scale:phrygian` | Filter by scale |
| `root:D` | Filter by root |
| `slot:G2P4B` | Filter by source slot |
| `snapshot:"April backup"` | Filter by snapshot name or ID |
| `favorite` | Show favorites |
| `format:seq` | Filter by stored/imported format |

Examples:

```text
tag:acid scale:phrygian favorite
snapshot:"April backup" slot:G1P2A
format:seq root:D
```

The sidebar also exposes focused library views:

- All Items
- Snapshots
- Imported Folders
- Related Groups
- Duplicates
- Favorites
- Needs Review
- Failed Imports

The toolbar can switch between card and table views, enable dense mode, open the filter panel, compare two selected items, create a merge plan, and clear the current selection.

For a full public guide to every Bank button, menu action, modal button, and button-like control, see [BANK BUTTONS](BANK_BUTTONS.md).

## Duplicate And Related Analysis

The Bank computes two important derived views:

### Duplicates

Duplicate detection uses content hashes and cluster logic to identify exact or near-equivalent material.

### Related groups

Related views are broader than duplicates. They are meant to surface musically or structurally connected items rather than only exact sameness.

Together these views help answer:

- what do I already have?
- what is too similar?
- what belongs to the same idea family?

## Snapshot Workflows

Snapshots are central because they give the Bank a bank-shaped memory.

You can use snapshots to:

- store imported banks
- keep generated progression packs
- preserve pre-UI and pre-import backups
- compare two banks slot by slot
- export selected snapshot slots into pattern files

The Bank also supports slot movement and slot deletion inside snapshots, which makes snapshots more than static archives.

## Snapshot Editing

Snapshots are always rendered as 64-slot grids. Empty slots are explicit, not missing rows.

Snapshot editing supports:

- creating manual snapshots
- adding items to a snapshot
- renaming or updating snapshot metadata
- deleting selected slots
- moving a slot into an empty destination
- swapping two occupied slots
- exporting selected slots as pattern files
- deleting snapshots

Slot operations validate the slot key and preserve the 64-slot grid shape. Deleting a slot clears that slot inside the snapshot; it does not mean every source file on disk is deleted.

## Add To Control

Bank items and snapshot slots can be appended to the Control page's multipattern canvas.

This handoff uses a server-side queue:

1. Bank resolves selected items or slots to decoded patterns.
2. The patterns are posted to `/api/control/queue/append`.
3. A live Control tab is notified through `BroadcastChannel`.
4. The Control page drains `/api/control/queue/consume` exactly once.

The queue is capped at 64 patterns to match the Control page. Overflow is reported as dropped patterns. Add To Control does not write to the TD-3 by itself; it only appends patterns to the Control workspace.

## Auditioning Through The Device

A Bank item can be played directly on the TD-3.

This works by:

1. reading the cached 112-byte payload from the sidecar
2. uploading it to the configured scratch slot
3. starting the transport

That gives the Bank page true hardware audition, not just metadata browsing.

## Safety And Ownership Rules

The Bank is careful about ownership boundaries.

- deleting a snapshot does not mean deleting every source file on disk
- deleting an import batch removes catalog state, not the original files
- sidecar cleanup is best-effort because the catalog is the source of truth
- imported snapshots and manually created snapshots are treated differently

Those rules matter because the Bank is both a creative tool and a local archive.

## Why The Bank Matters

Without the Bank, the rest of the app would still be useful for live control and generation, but it would be much easier to lose track of valuable patterns over time.

The Bank is what makes the project scale from:

- one session

to:

- a long-lived personal pattern library with history, snapshots, and structure

## See Also

- [MAIN](MAIN.md)
- [BANK BUTTONS](BANK_BUTTONS.md)
- [PROGRESSIONS](PROGRESSIONS.md)
- [CLI](CLI.md)
- [FORMATS](FORMATS.md)
- [SETTINGS](SETTINGS.md)
- [TECHNICAL ARCHITECTURE](TECHNICAL_ARCHITECTURE.md)
- [FAQ](FAQ.md)
