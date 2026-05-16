# FORMATS

## Purpose

The project moves TD-3 patterns between device memory, text files, DAWs, legacy tools, full-bank files, and the local Bank library.

This page describes which formats exist and where they are used.

## Format Summary

| Extension | Scope | Typical Use |
| --- | --- | --- |
| `.syx` | Single pattern | TD-3 SysEx payload import/export |
| `.toml` | Single pattern | Human-editable structured pattern file |
| `.json` | Single pattern | Structured interchange and internal re-import |
| `.steps.txt` | Single pattern | Compact human-readable step text |
| `.mid` | Single pattern | DAW import/export |
| `.seq` | Single pattern | SynthTribe-style sequence exchange |
| `.pat` | Single pattern | ABL3 pattern text |
| `.rbs` | Single pattern or bank-like song | ReBirth-compatible song/pattern exchange |
| `.sqs` | Full bank | TD-3/SynthTribe full-bank exchange |

## Workflow Support Matrix

| Workflow | Supported Formats |
| --- | --- |
| CLI `export --output` | `.syx`, `.toml`, `.json`, `.steps.txt`, `.mid`, `.seq`, `.pat`, `.rbs` |
| CLI `export` package folder | `syx`, `toml`, `steps`, `json`, `mid`, `seq`, `pat`, `rbs` |
| CLI `import` | `.syx`, `.toml`, `.json`, `.steps.txt`, `.mid`, `.seq`, `.pat`, `.rbs` |
| CLI `convert` | any supported single-pattern input to supported single-pattern output |
| CLI `extract-bank` | `.sqs` to a 64-slot folder tree |
| CLI `pack-bank` | 64-slot folder tree to `.sqs` |
| CLI `import-bank` | `.sqs` to TD-3 |
| Main page import/export | single-pattern formats plus bank-picker flows for `.sqs` and `.rbs` |
| Progression package export | `mid`, `steps_txt`, `seq`, `pat`, `rbs`, `json`, `toml`, `combined.rbs`, `combined.sqs` |
| Bank ingest | pattern files and bank files supported by the ingest pipeline |
| Snapshot export | selected snapshot slots to pattern files |

## `.syx`

`.syx` is the direct TD-3 SysEx representation for one pattern.

Use it when you want the closest single-pattern device exchange format. The protocol layer validates the payload before turning it into a `Pattern`.

## `.toml` And `.json`

TOML and JSON are versioned structured formats.

They include:

- format tag
- format version
- device tag
- active step count
- triplet flag
- all 16 step entries

These formats are stricter than ad hoc text dumps. Unknown fields and invalid step data are rejected.

## `.steps.txt`

`.steps.txt` is the compact human-readable step format.

It is intended for quick inspection, copy/paste, and editing. The CLI format token is `steps`.

Do not use `txt` as a CLI format token. The file extension is `.steps.txt`, but the token is `steps`.

## `.mid`

MIDI export is for DAW workflows.

Export behavior is affected by:

- BPM
- PPQN
- MIDI channel
- octave offset
- normal and accent velocities
- slide rendering mode
- loop count or target bars

MIDI import maps a monophonic MIDI phrase back into a TD-3 pattern. If a CLI `.mid` import sees multiple pitch candidates on one step, it prompts for the note to keep because the TD-3 pattern model is monophonic.

## `.seq`

`.seq` is a SynthTribe-style sequence format for single patterns.

Use it when moving patterns between TD-3 tooling that understands SynthTribe-like sequence files.

## `.pat`

`.pat` is an ABL3 pattern-style text format.

The importer/exporter preserves the TD-3 pattern as far as that format allows, with explicit validation on row lengths and field values.

## `.rbs`

`.rbs` is ReBirth-compatible.

In single-pattern CLI export, the pattern is placed into a ReBirth song at the corresponding TD-3-style slot:

- A-side maps to Device 1
- B-side maps to Device 2
- every other slot stays silent

In single-pattern CLI import, the primary slot is read. For full-bank-style `.rbs` workflows, use the UI bank picker or bank conversion paths.

Progression combined `.rbs` export uses a larger layout:

- acid patterns on Device 1
- basslines on Device 2

## `.sqs`

`.sqs` is a full-bank format.

It contains 64 TD-3 pattern slots. It is used by:

- bank extraction
- bank packing
- full-bank import to device
- backups
- combined progression package export

Progression combined `.sqs` export places:

- acid patterns on A-side
- basslines on B-side

## Progression Package Layout

Progression package ZIP files are created by the backend and written atomically.

The root folder inside the ZIP is:

```text
TD-3 Patterns Progression/
```

Per-pattern exports are organized by progression position:

```text
TD-3 Patterns Progression/
  P1/
    P1.mid
    P1.steps.txt
    P1.seq
    P1.pat
    P1.rbs
    P1.json
    P1.toml
    P1_BASSLINE/
      P1_BASSLINE.mid
      P1_BASSLINE.steps.txt
      ...
  P2/
  P3/
  P4/
  combined.rbs
  combined.sqs
```

Only selected formats are included.

When all 20 bassline archetypes are present, combined bank exports lay them out position-major by archetype:

```text
P1 pedal, P1 rootPulse, P1 offbeat, P1 shadow, P1 arpeggio,
P2 pedal, ...
P4 arpeggio
```

## Round-Trip Expectations

The safest round trips are the formats closest to the internal TD-3 pattern model:

- `.syx`
- `.toml`
- `.json`
- `.steps.txt`

MIDI, ReBirth, ABL3, and SynthTribe formats are useful interchange formats, but some concepts are represented differently across tools. The importers validate what they can and reject malformed or unsupported data instead of guessing silently.

## See Also

- [CLI](CLI.md)
- [PROGRESSIONS](PROGRESSIONS.md)
- [BANK](BANK.md)
- [TECHNICAL ARCHITECTURE](TECHNICAL_ARCHITECTURE.md)

