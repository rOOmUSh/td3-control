# CLI

## Purpose

The `td3-control` command line interface is the scriptable side of the project.

Use it for:

- listing MIDI ports
- exporting one TD-3 pattern from the device
- importing one pattern file to the device
- converting pattern files without a device
- extracting and packing full-bank files
- importing full banks to the TD-3 with a mandatory backup
- starting the local web UI

The CLI and web UI share the same Rust format, protocol, and validation code.

## Configuration Precedence

Runtime values are resolved in this order:

`CLI flag -> TD3_CONFIG.env -> bundled default template`

That means a flag such as `--midi-in`, `--port`, or `--bpm` overrides the matching value from `TD3_CONFIG.env` for that command only.

## Common MIDI Flags

Device-facing commands accept these MIDI options:

| Flag | Meaning |
| --- | --- |
| `--midi-in <name>` | MIDI input port name or substring |
| `--midi-out <name>` | MIDI output port name or substring |
| `--timeout-ms <ms>` | SysEx request timeout in milliseconds |
| `--strict-device-name` | Require exact case-sensitive MIDI port names |
| `--retries <n>` | Retry count for probe/download timeout paths |

Uploads are not retried automatically. That is deliberate: retrying a write after an uncertain device state can create partial-write ambiguity.

## List Ports

```sh
cargo run -- list-ports
```

This prints MIDI output ports and MIDI input ports. Use the visible port names with `MIDI_PORT_SUBSTRING`, `--midi-in`, or `--midi-out`.

## Start The Web UI

```sh
cargo run -- control
```

Useful flags:

| Flag | Meaning |
| --- | --- |
| `--scratch-pattern G1P2A` | Scratch slot used by live update and audition paths |
| `--port 3030` | HTTP server port |
| `--bind 127.0.0.1` | HTTP bind address |
| `--backup-dir ./backups` | Directory for the pre-UI full-bank backup |

Example:

```sh
cargo run -- control --scratch-pattern G1P2A --port 3030
```

In `control` mode, the app attempts a full-bank backup before the UI can write to the device. If the TD-3 is not found, the UI starts in offline mode. Other failures still abort startup.

## Export One Device Pattern

```sh
cargo run -- export G1P1A --output G1-P1A.steps.txt
```

The positional address accepts these forms:

- `G1P1A`
- `G1-P1A`
- lowercase variants such as `g1p1a`
- a quoted space-tolerant form such as `"G1 P1A"`

The address means:

- group: `G1` through `G4`
- pattern number: `P1` through `P8`
- side: `A` or `B`

If `--output` is present, the output format is inferred from the file extension.

Supported single-pattern output extensions:

| Extension | Format |
| --- | --- |
| `.syx` | TD-3 SysEx payload |
| `.toml` | Versioned TOML pattern file |
| `.json` | Versioned JSON pattern file |
| `.steps.txt` | Human-readable step text |
| `.mid` | Standard MIDI file |
| `.seq` | SynthTribe-style sequence file |
| `.pat` | ABL3 pattern text |
| `.rbs` | ReBirth song file with one populated slot |

If `--output` is omitted, export creates a package folder named from the device address, for example `PATTERN_G1-P1A/`.

## Export Format Selection

For package export, `--format` accepts a comma-separated list:

```sh
cargo run -- export G1-P1A --format syx,toml,steps,json,mid,seq,pat,rbs
```

Accepted tokens:

- `syx`
- `toml`
- `steps`
- `json`
- `mid`
- `seq`
- `pat`
- `rbs`

Use `steps` for `.steps.txt`. The token is not `txt`.

## MIDI Render Options

MIDI export options apply to `.mid` output from `export`, `convert`, and package flows:

| Flag | Meaning |
| --- | --- |
| `--bpm <n>` | Tempo |
| `--ppqn <n>` | Ticks per quarter note |
| `--mid-channel <1-16>` | MIDI channel |
| `--mid-octave-offset <n>` | Semitone offset |
| `--mid-accent-velocity <0-127>` | Accent velocity |
| `--mid-normal-velocity <0-127>` | Normal note velocity |
| `--mid-slide td3|generic|none` | Slide rendering mode |
| `--loop <n>` | Repeat pattern N times |
| `--bars <n>` | Target exported length in bars; overrides `--loop` |

Slide modes:

- `td3`: render TD-3-style overlapping note timing for slides
- `generic`: render a more general MIDI slide approximation
- `none`: ignore slide flags in MIDI output

## Import One Pattern To Device

```sh
cargo run -- import G2P3B --input pattern.steps.txt
```

The input format is inferred from the extension. Supported input extensions:

- `.syx`
- `.toml`
- `.json`
- `.steps.txt`
- `.mid`
- `.seq`
- `.pat`
- `.rbs`

For `.rbs` single-pattern import, the CLI reads the primary ReBirth slot. Use full-bank extraction when you need all slots.

For `.mid` import, the TD-3 is monophonic. If a MIDI file contains multiple note candidates on the same step, the CLI asks which pitch to keep.

## Convert Without A Device

```sh
cargo run -- convert input.seq output.steps.txt
```

`convert` opens no MIDI ports. It imports the source file into the internal pattern model, validates it, then exports using the destination extension.

Examples:

```sh
cargo run -- convert pattern.steps.txt pattern.mid --bpm 132 --loop 4
cargo run -- convert pattern.mid pattern.toml --mid-octave-offset 12
cargo run -- convert pattern.pat pattern.rbs
```

## Extract A Full Bank

```sh
cargo run -- extract-bank backup.sqs extracted-bank
```

This converts a `.sqs` full-bank file into a folder tree of 64 per-pattern folders. It refuses to overwrite an existing output folder unless `--force` is provided.

```sh
cargo run -- extract-bank backup.sqs extracted-bank --force
```

## Pack A Full Bank

```sh
cargo run -- pack-bank extracted-bank new-bank.sqs
```

This packs a 64-folder extraction tree back into `.sqs`. It refuses to overwrite an existing output file unless `--force` is provided.

```sh
cargo run -- pack-bank extracted-bank new-bank.sqs --force
```

## Import A Full Bank To The TD-3

```sh
cargo run -- import-bank --input new-bank.sqs --backup-dir ./backups
```

`import-bank` is a device-write workflow. It creates a pre-write backup before uploading bank data.

Useful flags:

| Flag | Meaning |
| --- | --- |
| `--partial 1-1A,2-3B` | Write only selected target slots |
| `--include-silent` | Force-write slots detected as silent |
| `--backup-dir <dir>` | Where to write the mandatory pre-import backup |

By default, silent slots are skipped to avoid erasing useful device data with empty patterns by accident.

## Safety Notes

- `control` uses the configured scratch slot for live update and audition.
- `import` writes exactly one device slot.
- `import-bank` can write many device slots and always backs up first.
- `--partial` narrows `import-bank`; it does not disable validation.
- Uploads are not retried automatically.
- Never treat a printed success line as device success unless the command completed without a protocol error.

## See Also

- [FORMATS](FORMATS.md)
- [SETTINGS](SETTINGS.md)
- [MAIN](MAIN.md)
- [BANK](BANK.md)
