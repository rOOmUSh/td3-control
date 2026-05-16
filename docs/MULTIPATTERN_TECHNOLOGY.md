# MULTIPATTERN TECHNOLOGY

## Why It Exists

A TD-3 is slot-based, but composition often is not.

The multipattern system exists to let the app work with a session of patterns instead of one isolated slot at a time. That is what enables sequence building, grouped editing, timeline playback, batch transforms, and handoff into progression and bank workflows.

## The Core Data Model

The main multipattern state lives in `ui/js/multipattern/multipattern-state.js`.

At a high level it keeps:

- `patterns`: the current pattern list, capped at 64 entries
- `focusedIdx`: the active pattern for direct editing
- `checkedSet`: a selection set for bulk actions
- `timelineDefault`: the normal session timeline
- `timelineChecked`: the alternate timeline used when any patterns are checked
- `abMode`: playback ordering mode
- `viewport`: visibility filter state
- `clipboard`: a persisted full-pattern clipboard

The design goal is compatibility without throwing away older single-pattern assumptions. Many callers can still ask for "the current pattern", but the state layer now knows that "current" may be one pattern inside a larger session.

## Dual Timeline Playback

One of the most important ideas in the multipattern system is the dual timeline model.

### Default timeline

When no patterns are checked, playback uses `timelineDefault`.

That timeline behaves like the normal session arrangement. Add, duplicate, and delete operations keep it coherent as the visible pattern list changes.

### Checked timeline

When one or more patterns are checked, playback switches to `timelineChecked`.

This turns selection into a temporary performance arrangement:

- checking a pattern appends it once
- unchecking a pattern removes all of its entries
- the checked timeline persists, so a user can return to it later

This allows two valid ways to work:

- maintain the main arrangement
- build a temporary arrangement from selected patterns without destroying the first one

## Structural Editing Rules

The state module has explicit semantics for structural operations.

### Add

Adding a pattern appends a default pattern, focuses it, and appends it to the default timeline.

### Duplicate

Duplicating inserts the copy next to its source and remaps internal indexes so focus, checks, and timelines still point at the intended material.

### Delete

Deleting compacts the pattern list and timeline references. The system always keeps at least one pattern alive, so deleting the final remaining pattern resets it instead of creating an empty session.

### Move

Reordering patterns changes the visible slot order without rewriting pattern content. The timeline keeps slot-number semantics, which means drag-to-reorder becomes a fast way to redefine playback order.

## Focused Editing vs Bulk Editing

The multipattern system separates two ideas:

- focused editing for note-level work
- checked selection for batch work

That split matters because it keeps keyboard editing, clipboard, and note toggles simple while still enabling bulk shift, transpose, duplication, deletion, and device push flows.

## Persistence Strategy

The multipattern session is persisted in `sessionStorage`.

That includes:

- pattern data
- focus
- checked selection
- timelines
- playback-related UI state

The clipboard is stored separately. There is also migration logic for older single-pattern session data so previous sessions do not become unusable when the internal shape evolves.

## Undo, Redo, and History Boundaries

Undo/redo is session-level, not step-level in isolation.

The app records logical snapshots of the multipattern session after edit bursts. This is why grouped actions like bulk transpose or a single structural move can behave like one undoable event instead of dozens of micro-events.

## Device Awareness

The multipattern system is not purely visual. It is designed around the TD-3's real constraints.

### Scratch slot awareness

A fetched scratch-slot descriptor is threaded into the state so preview, push, and slot-badge calculations stay grounded in the actual device session.

### Live update

When live update is enabled, edits are debounced and sent to the scratch slot. The user edits the session model, and the scratch slot becomes the live hardware mirror of the current focused pattern.

### Playback coordination

Multipattern playback and structural changes are integrated so timeline changes can requeue what the scratch slot should receive next.

## Import and Load Behavior

The multipattern system supports two broad device/file entry modes:

- append one or more patterns to the current session
- replace the whole session with a loaded set

That distinction is critical. Importing one interesting pattern from a bank file should not destroy the current working set, but a full "load all" operation should be able to replace the session deliberately.

## Why This Matters

Without multipattern technology, the app would still be useful, but it would remain mostly a device editor.

With it, the app becomes a composition surface:

- session-based instead of slot-based
- arrangement-aware instead of pattern-isolated
- compatible with performance, generation, and library workflows

That is one of the major reasons this project feels like a workstation rather than a format converter with a UI.

## See Also

- [MAIN](MAIN.md)
- [PROGRESSIONS](PROGRESSIONS.md)
- [TECHNICAL ARCHITECTURE](TECHNICAL_ARCHITECTURE.md)
