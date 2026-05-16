// Single-item audition button. Dropped in on every surface where a single
// pattern is visible: card, table row, drawer action row, snapshot grid cell,
// duplicate member chip, related representative tile, imported-entry row.
//
// Owns its own tiny pub/sub for the "which item is playing" state. We keep
// it *out* of the global bank-state bus on purpose: play/stop is the one
// interaction that happens independently of the library filter, sidebar, or
// selection - routing it through `setState` would cause every subscribed
// view (Related, Duplicates, etc.) to full-rebuild on each click, which
// caused a visible flicker and a needless re-fetch of /api/bank/related.
//
// Each button created here subscribes on mount; when it's removed from the
// DOM the subscription self-cleans on the next broadcast.

import { bankApi } from './bank-api.js';
import { toast } from './bank-toast.js';
import { envInt } from '../td3-env.js';

// Audition BPM defaults are sourced from window.TD3_CONFIG_ENV - the
// inline config block in every served HTML page guarantees uiDefaultBpm
// is set before any module evaluates. No JS-side fallback literal here.
const ENV_DEFAULT_BPM = envInt('uiDefaultBpm');
const MIN_BPM = 20;
const MAX_BPM = 300;

function envDefaultBpm() {
    return ENV_DEFAULT_BPM;
}

let _playingItemId = null;
const _playListeners = new Set();

// Audition BPM is shared across every per-item play button and the Bank
// footer's BPM knob. Kept here (rather than in the global bank-state bus)
// so that adjusting BPM doesn't trigger full view re-renders - same
// reasoning as playingItemId. The footer knob calls setBankBpm(), which
// repaints all BPM subscribers and, if an audition is already live, sends
// /api/transport/bpm so the device retunes on the fly.
// Lazy-init: null means "not read yet, resolve from TD3_CONFIG.env on
// first access". Once loadAppConfig has resolved, uiDefaultBpm is used;
// otherwise DEFAULT_BPM_FALLBACK.
let _currentBpm = null;
const _bpmListeners = new Set();

export function getPlayingItemId() {
    return _playingItemId;
}

function setPlayingItemId(id) {
    if (_playingItemId === id) return;
    _playingItemId = id;
    for (const fn of [..._playListeners]) {
        try { fn(_playingItemId); }
        catch (e) { console.error('bank-play listener error:', e); }
    }
}

function subscribePlay(fn) {
    _playListeners.add(fn);
    return () => _playListeners.delete(fn);
}

export function getBankBpm() {
    if (_currentBpm === null) {
        _currentBpm = Math.max(MIN_BPM, Math.min(MAX_BPM, envDefaultBpm()));
    }
    return _currentBpm;
}

export async function setBankBpm(bpm) {
    const clamped = Math.max(MIN_BPM, Math.min(MAX_BPM, Math.floor(Number(bpm) || 0)));
    if (clamped === _currentBpm) return;
    _currentBpm = clamped;
    for (const fn of [..._bpmListeners]) {
        try { fn(_currentBpm); }
        catch (e) { console.error('bank-play bpm listener error:', e); }
    }
    // If audition is live, retune the running clock. Failure is non-fatal
    // (e.g. device disconnect mid-session) - the next play click will
    // re-send BPM anyway as part of the start handshake.
    if (_playingItemId !== null) {
        try { await bankApi.transportBpm(_currentBpm); }
        catch (e) { console.warn('bank-play: live bpm update failed:', e); }
    }
}

export function subscribeBpm(fn) {
    _bpmListeners.add(fn);
    return () => _bpmListeners.delete(fn);
}

/**
 * Build a play/stop button bound to a specific LibraryItem.
 *
 * @param {string} itemId  LibraryItem.item_id to audition.
 * @param {object} [opts]
 *   - size: 'sm' | 'md' (default 'sm'). Controls padding + icon size.
 *   - bpm:  Per-button BPM override. When omitted, the shared footer BPM
 *     (getBankBpm()) is read at click time, so turning the footer knob
 *     retunes every button without needing to rebuild the views.
 *   - title: tooltip override.
 *   - showLabel: if true, appends a PLAY/STOP text label next to the icon
 *     (useful for the drawer action row which uses labelled buttons).
 *   - onBeforePlay: optional hook fired after click is accepted, before network.
 *   - stopPropagation: stop the click from bubbling to a parent (card/row
 *     click handlers usually open a drawer, which we don't want here).
 *     Defaults to true.
 * @returns {HTMLButtonElement}
 */
export function makePlayButton(itemId, opts = {}) {
    const { size = 'sm', bpm, title, showLabel = false, onBeforePlay, stopPropagation = true } = opts;

    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = `bank-play-btn bank-play-${size}`;
    if (showLabel) btn.classList.add('bank-play-labelled');
    btn.dataset.itemId = itemId;

    const icon = document.createElement('span');
    icon.className = 'material-symbols-outlined';
    btn.appendChild(icon);

    let labelEl = null;
    if (showLabel) {
        labelEl = document.createElement('span');
        labelEl.className = 'bank-play-label';
        btn.appendChild(labelEl);
    }

    const applyState = (playingId) => {
        const isPlaying = playingId === itemId;
        paintIcon(btn, icon, isPlaying, labelEl);
        btn.setAttribute('aria-label', title || (isPlaying ? 'Stop playback' : 'Play pattern on device'));
        btn.title = title || (isPlaying ? 'Stop playback on device' : 'Play this pattern on the TD-3');
    };
    applyState(_playingItemId);

    const unsub = subscribePlay((playingId) => {
        // When the button's host node is gone, clean ourselves up so the
        // listener set doesn't grow without bound. `isConnected` flips to
        // false as soon as the DOM tree containing the button is detached.
        if (!btn.isConnected) { unsub(); return; }
        applyState(playingId);
    });

    btn.addEventListener('click', async (ev) => {
        if (stopPropagation) {
            ev.preventDefault();
            ev.stopPropagation();
        }
        if (btn.disabled) return;
        btn.disabled = true;
        try {
            if (_playingItemId === itemId) {
                await bankApi.stopPlayback();
                setPlayingItemId(null);
                toast('Playback stopped', 'info');
            } else {
                if (typeof onBeforePlay === 'function') onBeforePlay(itemId);
                const effectiveBpm = bpm ?? getBankBpm();
                await bankApi.playItem(itemId, effectiveBpm);
                setPlayingItemId(itemId);
                toast(`Playing on device (${effectiveBpm} BPM)`, 'success');
            }
        } catch (e) {
            toast(`Play failed: ${e.message}`, 'error');
            // Best-effort: re-sync what the server actually thinks is playing
            // so the button doesn't get stuck showing the wrong icon.
            try {
                const { item_id } = await bankApi.getPlaying();
                setPlayingItemId(item_id || null);
            } catch { /* ignore */ }
        } finally {
            btn.disabled = false;
        }
    });

    return btn;
}

function paintIcon(btn, iconEl, playing, labelEl) {
    if (playing) {
        iconEl.textContent = 'stop';
        btn.classList.add('is-playing');
        if (labelEl) labelEl.textContent = 'STOP';
    } else {
        iconEl.textContent = 'play_arrow';
        btn.classList.remove('is-playing');
        if (labelEl) labelEl.textContent = 'PLAY';
    }
}

/**
 * Fetch the current server-side audition state and seed the local tracker.
 * Called once on Bank UI boot so reloads don't stall with a stale icon.
 */
export async function hydratePlayingState() {
    try {
        const { item_id } = await bankApi.getPlaying();
        setPlayingItemId(item_id || null);
    } catch (e) {
        // Non-fatal - just log. A missing device or a fresh server returns
        // { item_id: null } anyway; a true network error would surface
        // elsewhere in reloadAll().
        console.warn('bank-play: failed to hydrate playing state:', e);
    }
}
