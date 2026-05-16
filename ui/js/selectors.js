// Group, Pattern, and Side selector controls.
// The sidebar is fully interactive for load/save.
// The scratch pattern button is highlighted red to indicate the play target.
//
// Shared between index.html and progression.html. The caller injects the
// page's state module (state.js or progression-state.js) via init(state);
// both expose the same getGroup/setGroup/getPatternNum/setPatternNum/
// getSide/setSide surface, so this module does not need to know which
// page it's running on.
//
// Class ownership: HTML partials in ui/partials/sidebar/*.html own the
// structural classes (sizing, typography, rounded/tactile). This module
// only toggles the state-dependent subset via applyState() from
// shared/class-state.js - no more `el.className = "<full string>"`
// which silently overrode HTML-side size changes (e.g. h-8 loaded,
// flipped to h-10 on first repaint).

import { applyState } from './shared/class-state.js';

let state = null;

// Scratch slot coordinates (set from server on init)
let scratchGroup = null;
let scratchPattern = null;
let scratchSide = null;

// State-class maps. Classes listed here are the only ones this module
// adds/removes; everything else stays under HTML control.

const GROUP_STATES = {
    active:   ['is-active'],
    inactive: ['is-inactive'],
};

const SIDE_STATES = {
    active:   ['is-side-active'],
    inactive: ['is-inactive'],
};

// Pattern is tri-state: scratch (red, takes priority over active/default),
// active (currently selected but not the scratch slot), default.
const PATTERN_STATES = {
    scratch: ['is-scratch'],
    active:  ['is-active'],
    default: ['is-inactive'],
};

export function setScratch(group, pattern, side) {
    scratchGroup = group;
    scratchPattern = pattern;
    scratchSide = side;
    updateGroupButtons();
    updatePatternButtons();
    updateSideButtons();
}

export function init(stateModule) {
    state = stateModule;
    initGroupButtons();
    initPatternButtons();
    initSideButtons();
}

function initGroupButtons() {
    const container = document.getElementById('group-buttons');
    container.addEventListener('click', (e) => {
        const btn = e.target.closest('[data-group]');
        if (!btn) return;
        state.setGroup(parseInt(btn.dataset.group));
        updateGroupButtons();
        updatePatternButtons();
    });
    updateGroupButtons();
}

function initPatternButtons() {
    const container = document.getElementById('pattern-buttons');
    container.addEventListener('click', (e) => {
        const btn = e.target.closest('[data-pattern]');
        if (!btn) return;
        state.setPatternNum(parseInt(btn.dataset.pattern));
        updatePatternButtons();
    });
    updatePatternButtons();
}

function initSideButtons() {
    const container = document.getElementById('side-buttons');
    container.addEventListener('click', (e) => {
        const btn = e.target.closest('[data-side]');
        if (!btn) return;
        state.setSide(btn.dataset.side);
        updateSideButtons();
        updatePatternButtons();
    });
    updateSideButtons();
}

function updateGroupButtons() {
    const btns = document.querySelectorAll('#group-buttons [data-group]');
    const current = state.getGroup();
    btns.forEach(btn => {
        const key = parseInt(btn.dataset.group) === current ? 'active' : 'inactive';
        applyState(btn, GROUP_STATES, key);
    });
}

function updatePatternButtons() {
    const btns = document.querySelectorAll('#pattern-buttons [data-pattern]');
    const currentGroup = state.getGroup();
    const currentSide = state.getSide();
    const currentNum = state.getPatternNum();
    btns.forEach(btn => {
        const num = parseInt(btn.dataset.pattern);
        const isScratch = scratchGroup !== null
            && currentGroup === scratchGroup
            && num === scratchPattern
            && currentSide === scratchSide;
        const isActive = num === currentNum;
        // Scratch takes visual priority over active; both override default.
        const key = isScratch ? 'scratch' : (isActive ? 'active' : 'default');
        applyState(btn, PATTERN_STATES, key);
    });
}

function updateSideButtons() {
    const btns = document.querySelectorAll('#side-buttons [data-side]');
    const current = state.getSide();
    btns.forEach(btn => {
        const key = btn.dataset.side === current ? 'active' : 'inactive';
        applyState(btn, SIDE_STATES, key);
    });
}
