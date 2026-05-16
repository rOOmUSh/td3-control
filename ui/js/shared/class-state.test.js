// Tests for applyState: verify that state-dependent classes swap in
// and out correctly, structural classes are never touched, and shared
// classes between states survive a cross-state repaint.

import { applyState } from './class-state.js';

function makeEl(className) {
    // Minimal DOMTokenList polyfill so the test runs under plain node
    // (no jsdom). Matches the subset of classList we rely on: add,
    // remove, contains.
    const list = new Set((className || '').split(/\s+/).filter(Boolean));
    return {
        classList: {
            add: (c) => list.add(c),
            remove: (c) => list.delete(c),
            contains: (c) => list.has(c),
            toArray: () => [...list],
        },
        get className() { return [...list].join(' '); },
    };
}

function assert(cond, msg) {
    if (!cond) throw new Error('assertion failed: ' + msg);
}

// --- binary state: active/inactive (group, side buttons) ---------------

{
    const btn = makeEl('flex-1 h-10 text-xs rounded-lg tactile-button');
    const states = {
        active: ['bg-highest', 'text-dim', 'border-b-2', 'border-fixed', 'font-black'],
        inactive: ['bg-low', 'text-variant', 'font-bold', 'hover:bg-high'],
    };

    applyState(btn, states, 'active');
    assert(btn.classList.contains('bg-highest'), 'active bg added');
    assert(btn.classList.contains('font-black'), 'active font added');
    assert(!btn.classList.contains('bg-low'), 'inactive bg absent');
    assert(!btn.classList.contains('font-bold'), 'inactive font absent');
    assert(btn.classList.contains('h-10'), 'structural h-10 preserved');
    assert(btn.classList.contains('rounded-lg'), 'structural rounded preserved');

    applyState(btn, states, 'inactive');
    assert(btn.classList.contains('bg-low'), 'inactive bg added');
    assert(btn.classList.contains('font-bold'), 'inactive font added');
    assert(!btn.classList.contains('bg-highest'), 'active bg removed');
    assert(!btn.classList.contains('font-black'), 'active font removed');
    assert(btn.classList.contains('h-10'), 'structural h-10 still preserved');
}

// --- tri-state: scratch/active/default (pattern buttons) ---------------
// Scratch and active share `font-black` and `border-b-2`; default uses
// `font-bold`. Verify shared classes are NOT stripped when swapping
// between scratch and active.

{
    const btn = makeEl('h-9 text-xs rounded-lg tactile-button');
    const states = {
        scratch: ['bg-red-900', 'text-red-100', 'border-b-2', 'border-red-500', 'font-black'],
        active:  ['bg-highest', 'text-dim', 'border-b-2', 'border-fixed', 'font-black'],
        default: ['bg-low', 'text-variant', 'font-bold', 'hover:bg-high'],
    };

    applyState(btn, states, 'scratch');
    assert(btn.classList.contains('bg-red-900'), 'scratch bg added');
    assert(btn.classList.contains('border-b-2'), 'shared border added');
    assert(btn.classList.contains('font-black'), 'shared font added');

    applyState(btn, states, 'active');
    assert(btn.classList.contains('bg-highest'), 'active bg added');
    assert(!btn.classList.contains('bg-red-900'), 'scratch bg removed');
    assert(!btn.classList.contains('border-red-500'), 'scratch border color removed');
    assert(btn.classList.contains('border-b-2'), 'shared border-b-2 still present');
    assert(btn.classList.contains('font-black'), 'shared font-black still present');
    assert(btn.classList.contains('border-fixed'), 'active border color added');

    applyState(btn, states, 'default');
    assert(btn.classList.contains('bg-low'), 'default bg added');
    assert(btn.classList.contains('font-bold'), 'default font added');
    assert(!btn.classList.contains('font-black'), 'font-black removed (not in default)');
    assert(!btn.classList.contains('border-b-2'), 'border-b-2 removed (not in default)');

    applyState(btn, states, 'active');
    assert(btn.classList.contains('font-black'), 'font-black restored on re-activate');
    assert(btn.classList.contains('border-b-2'), 'border-b-2 restored on re-activate');
    assert(!btn.classList.contains('font-bold'), 'default font removed');
    assert(btn.classList.contains('h-9'), 'structural h-9 preserved across all swaps');
}

// --- idempotent: applying the same state twice is a no-op ---------------

{
    const btn = makeEl('h-8 rounded-lg');
    const states = {
        on:  ['bg-a', 'text-a'],
        off: ['bg-b', 'text-b', 'hover:bg-c'],
    };
    applyState(btn, states, 'on');
    const first = btn.classList.toArray().slice().sort().join(' ');
    applyState(btn, states, 'on');
    const second = btn.classList.toArray().slice().sort().join(' ');
    assert(first === second, 'applying same state twice is idempotent');
}

// --- unknown activeKey: all known state classes removed -----------------

{
    const btn = makeEl('h-8');
    const states = {
        on: ['bg-a'],
        off: ['bg-b'],
    };
    applyState(btn, states, 'on');
    assert(btn.classList.contains('bg-a'), 'on bg added');
    applyState(btn, states, 'nonexistent');
    assert(!btn.classList.contains('bg-a'), 'unknown key clears all states');
    assert(!btn.classList.contains('bg-b'), 'unknown key clears all states (off too)');
    assert(btn.classList.contains('h-8'), 'structural preserved when no state applies');
}

console.log('class-state.test.js: all assertions passed');
