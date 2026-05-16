// Bassline preview - in-browser WebAudio player for supporting bassline patterns.
//
// Purpose: let the user hear a generated bassline without the TD-3 connected.
// Not aiming for TD-3 emulation - just note confirmation.
//
// Pure-ish module: no DOM access, no IndexedDB, no fetch. Owns a single
// WebAudio context and a single master gain. Only one preview at a time;
// calling previewBassline() while another preview is playing stops the
// previous one first.
//
// Pitch math mirrors src/formats/mid.rs so what the user hears matches what
// the exported .mid file will play, plus a +12 semitone bump so the default
// TD-3 low C is audible on laptop speakers (MIDI exporter default puts C at
// MIDI 24 / 32.7 Hz, effectively sub-bass - we shift up one octave for
// audition).

const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B', 'C^'];

// Match src/formats/mid.rs defaults, plus audition bump.
const MIDI_EXPORT_OCTAVE_OFFSET = 12;
const AUDITION_BUMP_SEMITONES = 12;
const PREVIEW_OCTAVE_OFFSET = MIDI_EXPORT_OCTAVE_OFFSET + AUDITION_BUMP_SEMITONES; // 24

// Envelope constants (seconds).
const ATTACK_SEC = 0.005;
const RELEASE_SEC = 0.080;
const MIN_SUSTAIN_SEC = 0.010;

// Gain levels (0..1, before master gain).
const ACCENT_PEAK = 1.0;
const NORMAL_PEAK = 0.6;

// Module-level singleton state.
let audioContext = null;
let masterGain = null;
let masterGainValue = 0.3;
let activeSession = null; // { voices: OscillatorNode[], gains: GainNode[], stepTimers: number[], endTimer: number, onEnd }

/**
 * Test-only seam: inject a custom AudioContext factory.
 * @param {() => AudioContext} factory
 */
let contextFactory = null;
export function __setAudioContextFactory(factory) {
    contextFactory = factory;
    // Force a fresh context on next preview so the new factory is used.
    if (audioContext) {
        try { audioContext.close(); } catch { /* ignore */ }
    }
    audioContext = null;
    masterGain = null;
}

function ensureContext() {
    if (audioContext) return;
    const Ctor = contextFactory
        ? contextFactory
        : (typeof window !== 'undefined' && (window.AudioContext || window.webkitAudioContext));
    if (!Ctor) {
        throw new Error('bassline-preview: WebAudio is not available in this environment');
    }
    audioContext = contextFactory ? contextFactory() : new Ctor();
    masterGain = audioContext.createGain();
    masterGain.gain.value = masterGainValue;
    masterGain.connect(audioContext.destination);
}

/**
 * Convert a TD-3 step (note name + transpose) to a MIDI pitch using the
 * same formula as the MIDI exporter, then bump one octave up for audition.
 *
 * @param {{note:string, transpose:'NORMAL'|'UP'|'DOWN'}} step
 * @returns {number} MIDI note number (0..127 clamped)
 */
export function stepToPreviewMidi(step) {
    const idx = NOTE_NAMES.indexOf(step.note);
    if (idx < 0) {
        throw new Error(`bassline-preview: unknown note name "${step.note}"`);
    }
    let transposeOct = 0;
    if (step.transpose === 'UP') transposeOct = 12;
    else if (step.transpose === 'DOWN') transposeOct = -12;
    const td3Pitch = 12 + idx + transposeOct;
    const midi = td3Pitch + PREVIEW_OCTAVE_OFFSET;
    return Math.max(0, Math.min(127, midi));
}

/**
 * Convert a MIDI note number to Hz.
 * @param {number} midi
 * @returns {number}
 */
export function midiToHz(midi) {
    return 440 * Math.pow(2, (midi - 69) / 12);
}

/**
 * Seconds per step, matching the MIDI exporter timing:
 *   non-triplet: 16th-note grid  → 60/bpm/4
 *   triplet:     triplet-8th grid → 60/bpm/6
 *
 * @param {number} bpm
 * @param {boolean} triplet
 * @returns {number}
 */
export function stepDurationSec(bpm, triplet) {
    if (typeof bpm !== 'number' || !isFinite(bpm) || bpm <= 0) {
        throw new Error(`bassline-preview: bpm must be a positive number, got ${bpm}`);
    }
    const divisor = triplet ? 6 : 4;
    return 60 / bpm / divisor;
}

/**
 * Build the list of timed events for a pattern. Rests are skipped. Ties in
 * V1 bassline output should not occur, but we handle them defensively by
 * merging the tied step into the previous note's duration.
 *
 * @param {Object} pattern
 * @param {number} stepSec
 * @returns {Array<{stepIndex:number, startOffset:number, duration:number, midi:number, accent:boolean}>}
 */
export function buildNoteEvents(pattern, stepSec) {
    const events = [];
    const steps = pattern.steps;
    for (let i = 0; i < steps.length; i++) {
        const s = steps[i];
        const isRest = s.time === 'REST' || s.time === 'TIE_REST';
        const isTie = s.time === 'TIE' || s.time === 'TIE_REST';

        if (isTie && !isRest && events.length > 0) {
            const prev = events[events.length - 1];
            if (prev.stepIndex + Math.round(prev.duration / stepSec) === i) {
                prev.duration += stepSec;
                continue;
            }
        }
        if (isRest) continue;

        events.push({
            stepIndex: i,
            startOffset: i * stepSec,
            duration: stepSec,
            midi: stepToPreviewMidi(s),
            accent: s.accent === true,
        });
    }
    return events;
}

/**
 * Schedule one voice (oscillator + per-voice gain with ADSR) for a single note.
 * The voice is connected to the shared masterGain.
 *
 * @param {AudioContext} ctx
 * @param {AudioNode} master
 * @param {{startAt:number, duration:number, midi:number, accent:boolean}} ev
 * @returns {{osc:OscillatorNode, gain:GainNode}}
 */
function scheduleVoice(ctx, master, ev) {
    const osc = ctx.createOscillator();
    osc.type = 'sawtooth';
    osc.frequency.setValueAtTime(midiToHz(ev.midi), ev.startAt);

    const gain = ctx.createGain();
    const peak = ev.accent ? ACCENT_PEAK : NORMAL_PEAK;

    const sustainSec = Math.max(MIN_SUSTAIN_SEC, ev.duration - RELEASE_SEC);
    const attackEnd = ev.startAt + ATTACK_SEC;
    const releaseStart = ev.startAt + sustainSec;
    const endAt = releaseStart + RELEASE_SEC;

    gain.gain.setValueAtTime(0.0001, ev.startAt);
    gain.gain.exponentialRampToValueAtTime(peak, attackEnd);
    gain.gain.setValueAtTime(peak, releaseStart);
    gain.gain.exponentialRampToValueAtTime(0.0001, endAt);

    osc.connect(gain);
    gain.connect(master);

    osc.start(ev.startAt);
    osc.stop(endAt + 0.01);
    return { osc, gain };
}

function clearActiveSession() {
    if (!activeSession) return;
    for (const t of activeSession.stepTimers) clearTimeout(t);
    if (activeSession.endTimer != null) clearTimeout(activeSession.endTimer);
    activeSession = null;
}

/**
 * Stop any currently playing preview. Safe to call when nothing is playing.
 * Fires the current session's onEnd callback with reason 'stopped'.
 */
export function stopPreview() {
    if (!activeSession) return;
    const session = activeSession;
    clearActiveSession();

    if (audioContext) {
        const now = audioContext.currentTime;
        for (let i = 0; i < session.gains.length; i++) {
            const g = session.gains[i];
            const o = session.voices[i];
            try {
                g.gain.cancelScheduledValues(now);
                g.gain.setValueAtTime(Math.max(0.0001, g.gain.value), now);
                g.gain.exponentialRampToValueAtTime(0.0001, now + RELEASE_SEC);
                o.stop(now + RELEASE_SEC + 0.01);
            } catch { /* already stopped */ }
        }
    }

    if (typeof session.onEnd === 'function') {
        try { session.onEnd({ reason: 'stopped' }); } catch { /* swallow */ }
    }
}

/**
 * Preview a bassline pattern. Only one preview plays at a time; calling this
 * while another preview is active stops the previous one first.
 *
 * @param {Object} pattern
 * @param {Object} opts
 * @param {number} opts.bpm
 * @param {boolean} [opts.triplet]       Defaults to pattern.triplet.
 * @param {(stepIndex:number) => void} [opts.onStep]  Fires when each active step starts.
 * @param {(info:{reason:string}) => void} [opts.onEnd]  Fires when playback ends (naturally or stopped).
 * @returns {{durationSec:number, eventCount:number}}
 */
export function previewBassline(pattern, opts = {}) {
    if (!pattern || !Array.isArray(pattern.steps) || pattern.steps.length !== 16) {
        throw new Error('bassline-preview: pattern must have a 16-length steps array');
    }
    const { bpm, onStep, onEnd } = opts;
    const triplet = typeof opts.triplet === 'boolean' ? opts.triplet : (pattern.triplet === true);

    if (activeSession) stopPreview();

    ensureContext();

    const stepSec = stepDurationSec(bpm, triplet);
    const events = buildNoteEvents(pattern, stepSec);
    const patternDurSec = pattern.steps.length * stepSec;

    const ctxStart = audioContext.currentTime + 0.05; // lead-in
    const voices = [];
    const gains = [];
    for (const e of events) {
        const { osc, gain } = scheduleVoice(audioContext, masterGain, {
            startAt: ctxStart + e.startOffset,
            duration: e.duration,
            midi: e.midi,
            accent: e.accent,
        });
        voices.push(osc);
        gains.push(gain);
    }

    const stepTimers = [];
    if (typeof onStep === 'function') {
        const nowMs = () => performance.now();
        const startWallMs = nowMs() + 50;
        for (const e of events) {
            const delay = Math.max(0, (startWallMs + e.startOffset * 1000) - nowMs());
            stepTimers.push(setTimeout(() => {
                try { onStep(e.stepIndex); } catch { /* swallow */ }
            }, delay));
        }
    }

    const session = { voices, gains, stepTimers, endTimer: null, onEnd };
    session.endTimer = setTimeout(() => {
        if (activeSession !== session) return;
        activeSession = null;
        if (typeof onEnd === 'function') {
            try { onEnd({ reason: 'ended' }); } catch { /* swallow */ }
        }
    }, Math.ceil(patternDurSec * 1000) + 100);

    activeSession = session;

    return { durationSec: patternDurSec, eventCount: events.length };
}

/**
 * Whether a preview is currently playing.
 * @returns {boolean}
 */
export function isPreviewing() {
    return activeSession !== null;
}

/**
 * Set the shared preview gain (0..1). Applies immediately if a context exists
 * and is remembered for future previews.
 *
 * @param {number} v
 */
export function setPreviewGain(v) {
    if (typeof v !== 'number' || !isFinite(v)) {
        throw new Error(`bassline-preview: gain must be a finite number, got ${v}`);
    }
    const clamped = Math.max(0, Math.min(1, v));
    masterGainValue = clamped;
    if (masterGain && audioContext) {
        const now = audioContext.currentTime;
        masterGain.gain.cancelScheduledValues(now);
        masterGain.gain.setValueAtTime(clamped, now);
    }
}

/**
 * @returns {number}
 */
export function getPreviewGain() {
    return masterGainValue;
}

/**
 * Test-only: reset module state. Closes the audio context and clears the
 * active session.
 */
export function __resetForTests() {
    if (activeSession) clearActiveSession();
    activeSession = null;
    if (audioContext) {
        try { audioContext.close(); } catch { /* ignore */ }
    }
    audioContext = null;
    masterGain = null;
    masterGainValue = 0.3;
    contextFactory = null;
}
