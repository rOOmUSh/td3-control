// Minimal DOM stand-ins for knob control tests run under plain Node.

export function fakeStorage(initial = {}) {
    const values = new Map(Object.entries(initial));
    return {
        getItem(key) { return values.has(key) ? values.get(key) : null; },
        setItem(key, value) { values.set(key, String(value)); },
    };
}

export function fakeClassList(initial = []) {
    const classes = new Set(initial);
    return {
        contains(name) { return classes.has(name); },
        toggle(name, force) {
            if (force === undefined ? !classes.has(name) : force) classes.add(name);
            else classes.delete(name);
        },
    };
}

export function fakeEventTarget() {
    const listeners = new Map();
    return {
        addEventListener(type, listener) {
            if (!listeners.has(type)) listeners.set(type, new Set());
            listeners.get(type).add(listener);
        },
        removeEventListener(type, listener) {
            listeners.get(type)?.delete(listener);
        },
        dispatch(type, event = {}) {
            event.preventDefault ||= () => { event.defaultPrevented = true; };
            for (const listener of listeners.get(type) || []) listener(event);
            return event;
        },
    };
}

export function fakeElement(initialClasses = []) {
    return {
        ...fakeEventTarget(),
        attributes: {},
        classList: fakeClassList(initialClasses),
        style: {},
        textContent: '',
        setAttribute(name, value) { this.attributes[name] = String(value); },
    };
}
