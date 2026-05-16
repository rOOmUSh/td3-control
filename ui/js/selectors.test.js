import * as selectors from './selectors.js';

function classListFrom(className) {
    const tokens = new Set((className || '').split(/\s+/).filter(Boolean));
    return {
        add: (...classes) => classes.forEach((c) => tokens.add(c)),
        remove: (...classes) => classes.forEach((c) => tokens.delete(c)),
        contains: (c) => tokens.has(c),
    };
}

function makeButton(dataset) {
    return {
        dataset,
        classList: classListFrom('rounded-lg tactile-button'),
        closest(selector) {
            if (selector === '[data-group]' && this.dataset.group) return this;
            if (selector === '[data-pattern]' && this.dataset.pattern) return this;
            if (selector === '[data-side]' && this.dataset.side) return this;
            return null;
        },
    };
}

function makeContainer(id) {
    return {
        id,
        handler: null,
        addEventListener(event, handler) {
            if (event === 'click') this.handler = handler;
        },
        click(target) {
            if (!this.handler) throw new Error(`${id} click handler missing`);
            this.handler({ target });
        },
    };
}

function assert(cond, msg) {
    if (!cond) throw new Error('assertion failed: ' + msg);
}

const groupContainer = makeContainer('group-buttons');
const patternContainer = makeContainer('pattern-buttons');
const sideContainer = makeContainer('side-buttons');

const groupButtons = [1, 2, 3, 4].map((n) => makeButton({ group: String(n) }));
const patternButtons = [1, 2, 3, 4, 5, 6, 7, 8].map((n) => makeButton({ pattern: String(n) }));
const sideButtons = ['A', 'B'].map((s) => makeButton({ side: s }));

globalThis.document = {
    getElementById(id) {
        if (id === 'group-buttons') return groupContainer;
        if (id === 'pattern-buttons') return patternContainer;
        if (id === 'side-buttons') return sideContainer;
        return null;
    },
    querySelectorAll(selector) {
        if (selector === '#group-buttons [data-group]') return groupButtons;
        if (selector === '#pattern-buttons [data-pattern]') return patternButtons;
        if (selector === '#side-buttons [data-side]') return sideButtons;
        return [];
    },
};

const state = {
    group: 1,
    pattern: 1,
    side: 'A',
    getGroup() { return this.group; },
    setGroup(group) { this.group = group; },
    getPatternNum() { return this.pattern; },
    setPatternNum(pattern) { this.pattern = pattern; },
    getSide() { return this.side; },
    setSide(side) { this.side = side; },
};

selectors.init(state);
selectors.setScratch(1, 1, 'A');

assert(patternButtons[0].classList.contains('is-scratch'), 'scratch pattern starts red on matching group');

groupContainer.click(groupButtons[1]);
assert(state.group === 2, 'group click updates state');
assert(!patternButtons[0].classList.contains('is-scratch'), 'scratch red clears on non-scratch group');
assert(patternButtons[0].classList.contains('is-active'), 'selected pattern keeps active color');

groupContainer.click(groupButtons[0]);
assert(state.group === 1, 'group returns to scratch group');
assert(patternButtons[0].classList.contains('is-scratch'), 'scratch red returns on scratch group');

sideContainer.click(sideButtons[1]);
assert(state.side === 'B', 'side click updates state');
assert(!patternButtons[0].classList.contains('is-scratch'), 'scratch red clears on non-scratch side');

sideContainer.click(sideButtons[0]);
assert(state.side === 'A', 'side returns to scratch side');
assert(patternButtons[0].classList.contains('is-scratch'), 'scratch red returns on scratch side');

console.log('selectors.test.js: all assertions passed');
