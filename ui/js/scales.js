// Scale bank for the note randomizer.
// Loaded from /api/config/scales (config/scales-config.json on disk, with embedded scales-defaults.json fallback).

import { api } from './api.js';

// Runtime state - populated by loadScales()
let SCALES = [];
let TAG_GROUPS = [];

/** Load scales from backend config. Call before using any other export. */
export async function loadScales() {
    const config = await api.getScalesConfig();
    SCALES = config.scales || [];
    TAG_GROUPS = config.tag_groups || [];
}

/** Populate a <select> element with grouped scale options. */
export function populateScaleSelect(selectEl) {
    selectEl.innerHTML = '';
    for (const group of TAG_GROUPS) {
        const matching = SCALES.filter(s => s.tags.includes(group.tag));
        if (matching.length === 0) continue;
        const optgroup = document.createElement('optgroup');
        optgroup.label = group.label;
        for (const scale of matching) {
            const opt = document.createElement('option');
            opt.value = scale.id;
            opt.textContent = scale.name;
            optgroup.appendChild(opt);
        }
        selectEl.appendChild(optgroup);
    }
}

/** Get a scale by ID. */
export function getScale(id) {
    return SCALES.find(s => s.id === id) || SCALES[0];
}

/** All scales loaded from config. */
export function getAllScales() { return SCALES; }

/** Tag groups loaded from config. */
export function getTagGroups() { return TAG_GROUPS; }

/**
 * Generate allowed note indices (0-12 TD-3 range) from root + scale.
 * root: 0-11 (C=0, C#=1, ...)
 * Returns array of TD-3 note indices (0-12) that fall within the scale.
 */
export function scaleNotes(root, scale) {
    const allowed = new Set();
    for (const interval of scale.intervals) {
        const pitch = (root + interval) % 12;
        allowed.add(pitch);
    }
    // Always include 12 (C^) if 0 (C) is in the scale
    const result = [];
    for (let n = 0; n <= 12; n++) {
        if (allowed.has(n % 12)) result.push(n);
    }
    return result;
}
