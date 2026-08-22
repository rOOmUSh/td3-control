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

Two document tags are read: `td3-stepdsl-v1` and `td3-stepdsl-v1.1`. Every new file is written as v1.1. A v1 document is everything tagged `td3-stepdsl-v1`, with or without a `bpm` line and with either all sixteen rows or only the active rows. (Earlier documentation called the `bpm` and short-row extension "v1.1" while the tag stayed `td3-stepdsl-v1`; from this release the v1.1 name belongs to the tag below.)

A complete v1.1 document looks like this:

```text
format=td3-stepdsl-v1.1
active_steps=3
triplet_time=off
triplet_morph=off
triplet_morph_percentage=0
bpm=128
live_update=off
pattern_co_lane=on
pattern_gt_lane=off

01  G:---:N|CO:40|GT:50
02  G:D--:N|CO:90|GT:50
03  G:---:T|CO:127|GT:50

# NOTE:TAS:TIME|CO:cutoff|GT:gate
# transpose: U|D|-
# accent: A|-
# slide: S|-
# time: N|T|R|TR
# Cutoff Control | CO:0-127
# Gate Control | GT:1-100
# Lanes | pattern_co_lane, pattern_gt_lane: on/off
# Live Update | live_update: on/off
```

### v1.1 fields

A row is `NN NOTE:TAS:TIME` followed by zero or more `|KEY:VALUE` fields in any order. `CO` is the Filter Cutoff (MIDI CC 74) for the step, `0` through `127`; `GT` is the ordinary-note gate for the step, `1` through `100` percent. Writers emit both on every active row. The header keys are:

- `pattern_co_lane`, `pattern_gt_lane`: `on` or `off`, the lane switches of the step lane drawer.
- `triplet_morph`: `on` or `off`, and `triplet_morph_percentage`: `0` through `100`, the page's TRIPLET morph amount at export time.
- `live_update`: `on` or `off`, the LIVE button at export time.

Export values: a lane that is on writes its stored per-step values; a lane that is off and never edited writes the transport-bar `CUTOFF` or `GATE` value on every step; a lane that is off but edited writes its stored values with the switch off. The all-steps ratio knob is not written. Files written by the CLI, bank extract and backup, snapshot export, and package export carry `CO:64`, `GT:50`, both lanes `off`, morph `off`, and `live_update=off`.

### v1.1 import rules

The browser import and paste paths, and the CLI, read both tags. Everything the spec below leaves out behaves as v1, so an old reader's expectations hold:

- Any tag other than the two above is an error.
- Under v1.1 an unrecognised `key=value` header line is ignored. Under v1 it is rejected as before.
- A `CO` or `GT` value out of range is clamped to the nearest valid value (`GT:128` reads as `100`, `GT:0` as `1`, `CO:200` as `127`). A non-numeric value, or an active row that lacks the field while another has it, makes that lane absent for the whole document. Rows beyond `active_steps` never decide this; their values are kept when present.
- The lane switch is the header key when present. Without it, all active rows equal means off and any difference means on.
- `triplet_morph` counts only as `on` with a usable percentage; an unusable `on`/`off` value leaves the key absent.
- In the browser: the cutoff lane is dropped (defaults, switch off) unless the connected device accepts Filter Cutoff (a TD-3-MO on firmware `2.0.1`); the morph keys set the page's TRIPLET amount only for a straight pattern whose `active_steps` is a multiple of four; `live_update` switches the LIVE button through its normal path. The ratio knobs come back at centre. A card's PASTE FULL restores the lanes only. The CLI and the library import read the pattern and ignore the rest, as they ignore `bpm`.

New files saved by the CLI or browser include `bpm`. BPM accepts up to two decimal places, such as `128`, `128.3`, or `128.37`, and must be between `20.00` and `300.00`. The value is stored and transferred as integer centi-BPM, so decimal parsing does not depend on floating-point rounding.

Readers also accept legacy documents without `bpm`. Importing one in the browser leaves the current session BPM unchanged. Rows `01` through `active_steps` are required, while later rows are optional. New saves emit only the active rows. Legacy documents that contain all 16 rows remain valid, including documents whose inactive rows contain data.

BPM is document and playback-session metadata. It is not part of the TD-3 hardware `Pattern` or its SysEx payload. Uploading a `.steps.txt` file writes only the pattern data to the device. A device download cannot recover tempo, so CLI saves use `--bpm`, then `UI_DEFAULT_BPM`, then the bundled default of 120.

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

