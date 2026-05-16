# PROGRESSIONS

## What Progression Mode Does

Progression mode at `/progression.html` turns one pattern idea into a four-pattern phrase.

It is built for a workflow where the user wants related musical material, not four unrelated random patterns. The system therefore treats the progression as a connected chain with shared scale context, profile rules, anchor points, and derived endings.

## The Standard Generator

The standard generator lives mainly in `ui/js/progression/progression-generator.js`.

Its model is:

1. choose a progression profile for the selected scale
2. choose a four-degree template from that profile
3. generate P1 as the base pattern
4. derive P2-P4 from the previous pattern rather than regenerating blindly

This makes the result feel like one phrase with movement instead of four separate random outputs.

## Profile Resolution

The generator does not treat every scale the same.

It resolves a profile by:

- direct `scale_profiles[scale.id]` overrides from config
- fallback to tag-based profile priority
- final fallback to `"safe"`

That gives the scale config real influence over how progression material is shaped.

## Degree Templates

After profile resolution, the generator picks a four-degree preset such as:

- `[1, 4, 5, 1]`
- `[1, 6, 7, 1]`

Those degree templates define the center movement of the progression.

The output label then reflects the translated pitch centers, so the package and UI can show a musically meaningful summary of what was generated.

## How P1 Is Built

P1 is not random note spray.

The base-pattern generator uses:

- the chosen root and scale
- a center degree
- anchor steps
- note density
- slide density
- accent density

Anchor positions are biased toward center-supporting notes. The generator then fills the body with a mix of:

- step motion
- repeats
- limited leaps

and verifies that the pattern still confirms its tonal center strongly enough.

## How P2-P4 Are Derived

P2, P3, and P4 are derived by remapping and mutating the previous pattern.

The derivation step includes:

- anchor remap to the new center
- confirmation that enough anchors still support the center
- controlled mutation of non-anchor notes
- ending rewrite so the phrase leads toward the next pattern center
- small accent and slide variation

This is why the results usually feel connected: contour and phrase memory survive the handoff from one pattern to the next.

## P1-Locked Regeneration

Progression mode also supports a "keep P1, regenerate P2-P4" workflow.

That is useful when:

- the user already likes the opening idea
- the user wants alternative harmonic motion around the same first pattern
- the pattern was sent from the Main page and should remain intact

This feature reuses the same sibling-derivation chain instead of inventing a second system.

## Magic Progression Mode

The optional Magic path uses a stricter candidate pipeline from `ui/js/magic-randomizer/`.

That pipeline:

1. analyzes the chosen scale
2. generates candidate pitch sequences
3. validates them
4. scores them
5. repairs near misses
6. overlays slides and accents

In progression mode, each pattern can target a different center pitch class while staying inside the same overall scale context.

## Bassline Generation

Progression mode does not stop at four acid patterns. It also generates supporting basslines.

The v2 bassline generator builds all five archetypes for each progression position:

- pedal
- rootPulse
- offbeat
- shadow
- arpeggio

Each archetype is a deterministic function of:

- the acid pattern
- the harmonic center
- scale intervals
- extracted pattern features
- RNG

That gives the UI a full `5 x 4` bassline matrix, not just one supporting line per slot.

## Preview and Playback

The progression page offers multiple listening paths:

- TD-3 preview of a selected acid pattern
- TD-3 preview of a selected bassline archetype
- WebAudio MIDI preview of basslines
- timeline playback for the full progression

All preview paths are coordinated so only one preview mode is active at a time.

## Export and Packaging

Progression packages are exported as ZIP archives by the backend module in `src/web/package_export.rs`.

Per-pattern exports can include:

- `mid`
- `steps_txt`
- `seq`
- `pat`
- `rbs`
- `json`
- `toml`

Combined exports can also include:

- `combined.rbs`
- `combined.sqs`

The combined formats place acid patterns and basslines into specific bank layouts so the package can represent a whole phrase, not just independent files.

The ZIP root folder is:

```text
TD-3 Patterns Progression/
```

Each progression position gets its own folder. The selected active bassline for that position is nested below it:

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

Only the selected formats are written. The backend builds the ZIP in memory, writes a temporary file, flushes it, then renames it into place so a crash does not leave a half-written archive with the final filename.

### Combined RBS Layout

`combined.rbs` places the progression into a ReBirth-style two-device layout:

- acid patterns go to Device 1
- basslines go to Device 2

When all twenty bassline archetypes are present, Device 2 is filled position-major by archetype:

```text
P1 pedal, P1 rootPulse, P1 offbeat, P1 shadow, P1 arpeggio,
P2 pedal, ...
P4 arpeggio
```

### Combined SQS Layout

`combined.sqs` places the progression into a TD-3-style 64-slot bank layout:

- acid patterns go to A-side slots
- basslines go to B-side slots

With all archetypes present, the twenty bassline variants are laid out sequentially from `G1P1B`.

## Push To TD-3

The `PUSH TO TD-3` button writes the four acid progression patterns to the device.

Target placement is deterministic:

1. Start at the currently selected group, pattern number, and side.
2. Stay within that same group and side.
3. Walk forward through pattern numbers.
4. Wrap from P8 back to P1.
5. Skip the configured scratch slot if it falls in the target range.
6. Collect exactly four target slots.

Examples with scratch slot `G1P2A`:

| Selected slot | Write targets |
| --- | --- |
| `G1P1A` | `G1P1A`, `G1P3A`, `G1P4A`, `G1P5A` |
| `G1P3A` | `G1P3A`, `G1P4A`, `G1P5A`, `G1P6A` |
| `G1P8A` | `G1P8A`, `G1P1A`, `G1P3A`, `G1P4A` |

The confirmation modal previews the exact target addresses before writing. The scratch skip prevents a progression push from clobbering the live audition buffer during the same session.

## Bank Snapshot Push

Progression mode can push its output into the Bank in two ways:

- one pattern plus its five bassline archetypes
- the full four-pattern progression plus all twenty bassline variants

This is implemented in `ui/js/progression/progression-bank-snapshot.js`, which maps the musical structure into deterministic snapshot slot layouts.

## Why This Part Of The App Matters

Progression mode is where the project stops being only a TD-3 editor and becomes a phrase-generation environment.

It captures a specific design goal:

- keep the hardware workflow close
- keep the results musically related
- keep the output portable into exports, snapshots, and later reuse

## See Also

- [MAIN](MAIN.md)
- [MULTIPATTERN TECHNOLOGY](MULTIPATTERN_TECHNOLOGY.md)
- [BANK](BANK.md)
- [FORMATS](FORMATS.md)
- [SETTINGS](SETTINGS.md)
- [TECHNICAL ARCHITECTURE](TECHNICAL_ARCHITECTURE.md)
