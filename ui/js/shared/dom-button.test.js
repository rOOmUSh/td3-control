import { createButton, appendButtonContent } from './dom-button.js';

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
        disabled: false,
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
    let passedEvent = null;
    let passedButton = null;
    const btn = createButton({
        className: 'semantic-action danger',
        icon: 'delete',
        label: 'Delete',
        title: 'Delete pattern',
        ariaLabel: 'Delete pattern P1',
        disabled: true,
        preventDefault: true,
        stopPropagation: true,
        onClick: (ev, el) => {
            passedEvent = ev;
            passedButton = el;
        },
    });

    assert(btn.type === 'button', 'button type is explicit');
    assert(btn.className === 'semantic-action danger', 'className applied');
    assert(btn.title === 'Delete pattern', 'title applied');
    assert(btn.attributes['aria-label'] === 'Delete pattern P1', 'aria label applied');
    assert(btn.disabled === true, 'disabled applied');
    assert(btn.children.length === 2, 'icon and label appended');
    assert(btn.children[0].className === 'material-symbols-outlined', 'default icon class');
    assert(btn.children[0].textContent === 'delete', 'icon text');
    assert(btn.children[1].textContent === 'Delete', 'label text');

    const ev = btn.click();
    assert(passedEvent === ev, 'event passed to handler');
    assert(passedButton === btn, 'button passed to handler');
    assert(ev.defaultPrevented, 'preventDefault applied');
    assert(ev.propagationStopped, 'stopPropagation applied');
}

{
    const btn = makeElement('button');
    appendButtonContent(btn, {
        icon: 'info',
        label: 'Open details',
        iconSize: '0.95rem',
    });

    assert(btn.children.length === 2, 'appendButtonContent appends both nodes');
    assert(btn.children[0].style.fontSize === '0.95rem', 'icon size applied');
    assert(btn.children[1].textContent === 'Open details', 'label appended');
}

console.log('dom-button.test.js: all assertions passed');
