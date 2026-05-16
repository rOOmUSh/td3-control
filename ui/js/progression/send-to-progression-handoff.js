// SEND TO PROGRESSION - handoff consumer.
//
// The main page writes a one-shot blob to sessionStorage under
// `td3_progression_handoff` and navigates to /progression.html. On progression
// boot, this module reads that blob, installs the sent pattern as P1, derives
// P2..P4 via the shared `deriveSiblings` chain, and clears the handoff key.
//
// The handoff is time-bounded so a forgotten click from a previous page visit
// can't ambush the user's progression state much later. The blob is removed
// unconditionally on read - even on validation failure - so a malformed entry
// never sticks around to retry.

const HANDOFF_KEY = 'td3_progression_handoff';
const FRESHNESS_MS = 30_000;

/**
 * Pull and validate the handoff blob. Returns the blob on success, or null
 * when absent / stale / malformed. Removes the sessionStorage entry on every
 * read (one-shot semantics).
 */
function readHandoff() {
    let raw;
    try {
        raw = sessionStorage.getItem(HANDOFF_KEY);
    } catch {
        return null;
    }
    if (!raw) return null;
    try { sessionStorage.removeItem(HANDOFF_KEY); } catch { /* quota/private */ }

    let blob;
    try { blob = JSON.parse(raw); } catch { return null; }
    if (!blob || typeof blob !== 'object') return null;

    const { p1, root, scale, sentAt } = blob;
    if (!p1 || !Array.isArray(p1.steps) || p1.steps.length !== 16) return null;
    if (typeof root !== 'number' || root < 0 || root > 11) return null;
    if (typeof scale !== 'string' || !scale) return null;
    if (typeof sentAt !== 'number') return null;
    if (Date.now() - sentAt > FRESHNESS_MS) return null;
    return blob;
}

/**
 * Consume the SEND TO PROGRESSION handoff, if present. Writes P1 verbatim and
 * derives P2..P4 using the progression page's own config. Must be called once
 * during progression-main init, after scales have been loaded (so scale-id
 * lookup resolves) but before the first sequencer render.
 *
 * @param {object} deps
 * @param {object} deps.state - progression-state module
 * @param {(id: string) => object} deps.getScale - scale lookup by id
 * @param {object} deps.progressionConfig - config object (from api.getProgressionConfig)
 * @param {Function} deps.deriveSiblings - from progression-generator
 * @param {Function} deps.createRng
 * @param {Function} deps.resolveProfile
 * @param {Function} deps.chooseProgressionDegrees
 * @param {(msg: string, kind?: string) => void} [deps.toast]
 * @returns {null | {patterns, root, scale, profile, degrees, label}} the
 *   derived progression data on success, or null when no handoff was consumed.
 *   Callers use this to drive the shared bassline + package persistence path
 *   so SEND TO PROGRESSION lands in the same post-RANDOMIZE state (package
 *   persisted, SAVE PACKAGE enabled).
 */
export function consumeSendToProgressionHandoff(deps) {
    const {
        state, getScale, progressionConfig,
        deriveSiblings, createRng, resolveProfile, chooseProgressionDegrees,
        toast: toastFn,
    } = deps;

    const blob = readHandoff();
    if (!blob) return null;

    const scale = getScale(blob.scale);
    if (!scale) return null;

    const rng = createRng(null);
    const profile = resolveProfile(scale, progressionConfig);
    const degrees = chooseProgressionDegrees(profile, progressionConfig, rng);
    const anchorSteps = progressionConfig.anchor_steps || [0, 4, 8, 12];

    const patterns = deriveSiblings(blob.p1, {
        root: blob.root, scale, degrees, anchorSteps,
        config: progressionConfig, rng, profile,
    });

    state.setPatterns(patterns);
    state.setProgressionRoot(blob.root);
    state.setProgressionScaleId(blob.scale);
    state.setProgressionDegrees(degrees);
    // Label mirrors the generate path for session persistence.
    const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];
    const rootName = NOTE_NAMES[blob.root] || '';
    const label = `${rootName} ${scale.name} - from main pattern`;
    state.setProgressionLabel(label);

    if (toastFn) toastFn('Derived from main pattern', 'info');
    return { patterns, root: blob.root, scale, profile, degrees, label };
}
