// Pure helpers for multipattern timeline playback. Extracted so they can
// be unit-tested without the DOM, state store, or MIDI API.
//
// Timeline semantics (mirror the progression page):
//   timeline[i] is a 1-based pattern number (1..N), or 0 for an empty slot.
//   Playback walks non-empty slots in index order, wrapping at the end.
//   When the *same* pattern number repeats across consecutive slots the
//   device keeps looping its buffer (no SysEx save); only *different*
//   patterns trigger a pre-load to the scratch slot.

/**
 * Find the first non-empty timeline slot starting from 0. Returns -1 when
 * every slot is empty (nothing to play).
 *
 * @param {number[]} tl
 * @returns {number}
 */
export function firstTimelinePos(tl) {
    if (!Array.isArray(tl)) return -1;
    for (let i = 0; i < tl.length; i += 1) {
        if (tl[i] >= 1) return i;
    }
    return -1;
}

/**
 * Find the next non-empty timeline slot strictly after `pos`, wrapping
 * around to 0 if necessary. Returns -1 when no non-empty slot exists.
 * This matches `findNextNonEmpty` in progression-transport but is expressed
 * without bounding the pattern number to 1..4 - the main page supports
 * up to 64 patterns.
 *
 * @param {number[]} tl
 * @param {number} pos
 * @returns {number}
 */
export function nextTimelinePos(tl, pos) {
    if (!Array.isArray(tl) || tl.length === 0) return -1;
    const len = tl.length;
    for (let i = 1; i <= len; i += 1) {
        const candidate = (pos + i) % len;
        if (tl[candidate] >= 1) return candidate;
    }
    return -1;
}

/**
 * Advance a cursor to the next non-empty slot whose pattern number matches
 * `devicePatIdx + 1`, searching forward (strictly) from `from` and wrapping.
 * Used at the wrap boundary so the cursor ends up on a slot that matches
 * the pattern the device just loaded from its scratch buffer - which, in
 * the uninterrupted case, is the slot `nextTimelinePos(tl, from)` would
 * pick anyway, but in the interrupted case (a checkbox override queued a
 * different pattern) it can be a completely different slot.
 *
 * Fallback: when the device pattern isn't present in the timeline at all
 * (e.g. the user unchecked it and its entries were stripped from the
 * checked timeline), behave like {@link nextTimelinePos} - advance to the
 * next non-empty slot and let the next cycle re-sync.
 *
 * @param {number[]} tl
 * @param {number} from               0-based cursor position to search past
 * @param {number|null} devicePatIdx  0-based pattern index the device is
 *                                    looping (null → fall back to nextTimelinePos)
 * @returns {number}                  matching slot index, or -1 if timeline empty
 */
export function advanceCursorToDevicePattern(tl, from, devicePatIdx) {
    if (!Array.isArray(tl) || tl.length === 0) return -1;
    if (devicePatIdx === null || devicePatIdx === undefined || devicePatIdx < 0) {
        return nextTimelinePos(tl, from);
    }
    const target = devicePatIdx + 1;
    const len = tl.length;
    const base = (from >= 0 && from < len) ? from : -1;
    for (let i = 1; i <= len; i += 1) {
        const pos = ((base + i) % len + len) % len;
        if (tl[pos] === target) return pos;
    }
    return nextTimelinePos(tl, from);
}

/**
 * Decide whether an immediate scratch save is required after a structural
 * state change during play. Returns true when the cursor's slot points at a
 * pattern that differs from what is currently held in the scratch buffer,
 * meaning the device would otherwise swap to the wrong pattern at the next
 * wrap.
 *
 * The classic miss this prevents: the active timeline switches from a
 * checkbox-driven arrangement (e.g. [4]) back to the default arrangement
 * (e.g. [1,1,1,1]) after the user unchecks every pattern. The pre-load
 * "next slot == current slot" short-circuit would otherwise skip writing
 * P1 to the scratch, so the device keeps looping the previously-scratched
 * P4 buffer indefinitely. With this helper the caller forces a P1 save the
 * moment the uncheck lands, and the device swaps cleanly at the next wrap.
 *
 * @param {number[]} tl
 * @param {number} cursor              0-based timeline cursor position
 * @param {number|null|undefined} queuedPatIdx  0-based pattern idx currently
 *                                    held in scratch (null/undefined when
 *                                    scratch state is unknown, in which case
 *                                    a save is forced)
 * @returns {boolean}
 */
export function needsImmediateScratchSave(tl, cursor, queuedPatIdx) {
    if (!Array.isArray(tl)) return false;
    if (!Number.isInteger(cursor) || cursor < 0 || cursor >= tl.length) return false;
    const cursorNum = tl[cursor];
    if (!Number.isInteger(cursorNum) || cursorNum < 1) return false;
    if (queuedPatIdx === null || queuedPatIdx === undefined) return true;
    return cursorNum !== queuedPatIdx + 1;
}

/**
 * Decide whether a host-sequenced no-save transport needs to replace the
 * active audition schedule at a wrap boundary.
 *
 * LIVE UPDATE ON uses scratch pre-load plus device clock, so host audition
 * updates are only valid in audition mode with MIDI connected. Repeated
 * timeline slots for the same pattern do not need a schedule replacement.
 *
 * @param {boolean} liveUpdate
 * @param {boolean} connected
 * @param {boolean} auditionMode
 * @param {number|null|undefined} previousPatIdx
 * @param {number|null|undefined} nextPatIdx
 * @returns {boolean}
 */
export function shouldUpdateHostAuditionPattern(
    liveUpdate,
    connected,
    auditionMode,
    previousPatIdx,
    nextPatIdx,
) {
    return !liveUpdate
        && connected
        && auditionMode
        && Number.isInteger(previousPatIdx)
        && Number.isInteger(nextPatIdx)
        && nextPatIdx >= 0
        && nextPatIdx !== previousPatIdx;
}

/**
 * Count the non-empty slots in the timeline. Used for user-facing loop
 * counters ("loop 3/12"). Empty (0) slots are skipped.
 *
 * @param {number[]} tl
 * @returns {number}
 */
export function countNonEmpty(tl) {
    if (!Array.isArray(tl)) return 0;
    let n = 0;
    for (let i = 0; i < tl.length; i += 1) if (tl[i] >= 1) n += 1;
    return n;
}

/**
 * Build a sequenced timeline fill from a source array of 1-based pattern
 * numbers, repeating it to match `length`. Empty sources yield a timeline
 * of zeros. Used by FILL SEQUENCE / FILL CHECKED.
 *
 * @param {number[]} source  1-based pattern numbers (may be empty)
 * @param {number} length
 * @returns {number[]}
 */
export function repeatFill(source, length) {
    if (!Array.isArray(source) || source.length === 0) {
        return Array.from({ length }, () => 0);
    }
    return Array.from({ length }, (_, i) => source[i % source.length]);
}

/**
 * Produce a random timeline fill. Each slot picks a pattern from
 * `patternCount` uniformly. For `patternCount <= 0` the fill is all-empty.
 *
 * @param {number} patternCount   current N (1..64)
 * @param {number} length         timeline length
 * @param {() => number} rand     deterministic RNG for tests (defaults to Math.random)
 * @returns {number[]}
 */
export function randomFill(patternCount, length, rand = Math.random) {
    if (!Number.isInteger(patternCount) || patternCount <= 0) {
        return Array.from({ length }, () => 0);
    }
    return Array.from({ length }, () => 1 + Math.floor(rand() * patternCount));
}

/**
 * Golden-angle HSL color walk. Used to give each row a stable, visually
 * distinct hue when N grows past the ~8-color point where a fixed palette
 * starts repeating obviously.
 *
 * @param {number} idx   0-based row index
 * @param {number} [saturation=70]
 * @param {number} [lightness=55]
 * @returns {string}  "hsl(H, S%, L%)" string
 */
export function hslForIndex(idx, saturation = 70, lightness = 55) {
    const hue = (idx * 137.508) % 360;
    return `hsl(${hue}, ${saturation}%, ${lightness}%)`;
}
