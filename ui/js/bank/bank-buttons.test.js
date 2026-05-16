import { bankButton, menuButton } from './bank-buttons.js';

globalThis.document = {
    createElement(tagName) {
        return makeElement(tagName);
    },
};

function makeElement(tagName) {
    return {
        tagName,
        type: '',
        className: '',
        title: '',
        textContent: '',
        style: {},
        children: [],
        attributes: {},
        listeners: {},
        appendChild(child) {
            this.children.push(child);
            return child;
        },
        setAttribute(name, value) {
            this.attributes[name] = String(value);
        },
        addEventListener(type, handler) {
            this.listeners[type] = handler;
        },
        click(event = makeClickEvent()) {
            this.listeners.click?.(event);
            return event;
        },
    };
}

function makeClickEvent() {
    return {
        defaultPrevented: false,
        propagationStopped: false,
        preventDefault() {
            this.defaultPrevented = true;
        },
        stopPropagation() {
            this.propagationStopped = true;
        },
    };
}

function assert(cond, msg) {
    if (!cond) throw new Error('assertion failed: ' + msg);
}

{
    let clicked = false;
    let passedButton = null;
    const btn = bankButton({
        icon: 'delete',
        label: 'Delete',
        className: 'tactile-button',
        title: 'Delete item',
        ariaLabel: 'Delete item A',
        danger: true,
        preventDefault: true,
        stopPropagation: true,
        onClick: (ev, el) => {
            clicked = ev.defaultPrevented && ev.propagationStopped;
            passedButton = el;
        },
    });

    assert(btn.type === 'button', 'bankButton creates button type');
    assert(btn.className === 'bank-toolbar-btn tactile-button danger', 'bankButton class list');
    assert(btn.title === 'Delete item', 'bankButton title');
    assert(btn.attributes['aria-label'] === 'Delete item A', 'bankButton aria label');
    assert(btn.children.length === 2, 'bankButton icon and label nodes');
    assert(btn.children[0].className === 'material-symbols-outlined', 'bankButton icon class');
    assert(btn.children[0].textContent === 'delete', 'bankButton icon text');
    assert(btn.children[1].textContent === 'Delete', 'bankButton label text');

    btn.click();
    assert(clicked, 'bankButton click flags applied before handler');
    assert(passedButton === btn, 'bankButton passes button to handler');
}

{
    let clicks = 0;
    const btn = bankButton({
        label: 'Confirm',
        active: true,
        onClick: () => { clicks += 1; },
    });

    assert(btn.className === 'bank-toolbar-btn is-active', 'bankButton active class');
    btn.click();
    assert(clicks === 1, 'bankButton click handler fires once');
}

{
    let clicked = false;
    const btn = menuButton('info', 'Open details', () => { clicked = true; });

    assert(btn.type === 'button', 'menuButton creates button type');
    assert(btn.className === '', 'menuButton leaves menu styling to CSS parent');
    assert(btn.children.length === 2, 'menuButton icon and label nodes');
    assert(btn.children[0].style.fontSize === '0.95rem', 'menuButton icon size');
    assert(btn.children[1].textContent === 'Open details', 'menuButton label text');
    btn.click();
    assert(clicked, 'menuButton click handler fires');
}

console.log('bank-buttons.test.js: all assertions passed');
