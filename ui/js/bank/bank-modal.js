// Shared modal dialog builder for the Bank UI.
//
// Usage:
//   const close = openModal({
//       title: 'Scan Folder',
//       body: myFragmentOrElement,
//       primaryLabel: 'Scan',
//       onPrimary: async () => { ... },    // may return a Promise; any throw
//                                          // is caught and shown via toast
//       secondaryLabel: 'Cancel',           // default 'Cancel'
//       onSecondary: () => { ... },         // optional; default just closes
//   });
//
// The returned `close()` function tears down the modal. Esc closes.
// Clicking the backdrop closes. Focus is given to the first focusable node
// inside `body` on open; previous focus is restored on close.
//
// Reuses the existing `.bank-modal-backdrop` / `.bank-modal` CSS.
// Primary/secondary buttons get `bank-toolbar-btn` so they match the rest
// of the toolbar.

import { toast } from './bank-toast.js';
import { bankButton } from './bank-buttons.js';

let modalSequence = 0;

/**
 * Opens a modal and returns a `close()` function.
 *
 * @param {Object} opts
 * @param {string} opts.title - Header text.
 * @param {HTMLElement} opts.body - Body content.
 * @param {string} [opts.primaryLabel] - Primary button label. If omitted the
 *   primary button is suppressed (info-only dialogs).
 * @param {Function} [opts.onPrimary] - Async or sync handler. If it throws or
 *   the returned promise rejects, the error is toasted and the modal stays
 *   open. If it resolves, the modal closes.
 * @param {string} [opts.secondaryLabel] - Secondary button label. Default 'Cancel'.
 * @param {Function} [opts.onSecondary] - Secondary click handler. If omitted
 *   the modal just closes.
 * @param {boolean} [opts.secondaryDanger] - Style the secondary button red.
 * @param {string} [opts.secondaryClassName] - Replace the secondary button's
 *   default Bank classes with an app-specific class list.
 * @param {string} [opts.primaryClassName] - Replace the primary button's
 *   default Bank classes with an app-specific class list.
 * @param {boolean|Function} [opts.primaryEnabled] - Initial boolean or a
 *   predicate re-evaluated when the body receives input or change events.
 * @param {string} [opts.size] - 'default' | 'wide'.
 * @returns {Function} close
 */
export function openModal({
    title,
    body,
    primaryLabel,
    onPrimary,
    secondaryLabel = 'Cancel',
    onSecondary,
    secondaryDanger = false,
    secondaryClassName = '',
    primaryClassName = '',
    primaryEnabled = true,
    size = 'default',
    noScrim = false,
    danger = false,
} = {}) {
    if (!(body instanceof HTMLElement)) {
        throw new Error('openModal: body must be an HTMLElement');
    }
    const previouslyFocused = document.activeElement;

    const backdrop = document.createElement('div');
    backdrop.className = 'bank-modal-backdrop' + (noScrim ? ' no-scrim' : '');

    const modal = document.createElement('div');
    modal.className = 'bank-modal';
    modal.setAttribute('role', 'dialog');
    modal.setAttribute('aria-modal', 'true');
    if (size === 'wide') {
        modal.style.minWidth = '560px';
    }

    // Header row with title + close button.
    const header = document.createElement('div');
    header.style.display = 'flex';
    header.style.alignItems = 'flex-start';
    header.style.justifyContent = 'space-between';
    header.style.gap = '0.75rem';
    header.style.marginBottom = '0.5rem';

    const h = document.createElement('h3');
    h.id = `bank-modal-title-${++modalSequence}`;
    h.textContent = typeof title === 'string' ? title : 'Dialog';
    modal.setAttribute('aria-labelledby', h.id);
    header.appendChild(h);

    const closeX = bankButton({
        icon: 'close',
        ariaLabel: 'Close',
        onClick: () => close(),
    });
    header.appendChild(closeX);
    modal.appendChild(header);

    // Body container.
    const bodyWrap = document.createElement('div');
    bodyWrap.className = 'bank-modal-body';
    bodyWrap.appendChild(body);
    modal.appendChild(bodyWrap);

    // Footer with actions.
    const actions = document.createElement('div');
    actions.className = 'bank-modal-actions';

    const secondary = bankButton({
        label: secondaryLabel || 'Cancel',
        danger: secondaryDanger,
        onClick: () => {
            try { onSecondary?.(); } catch (e) { toast(e.message || String(e), 'error'); }
            close();
        },
    });
    if (secondaryClassName) secondary.className = secondaryClassName;
    actions.appendChild(secondary);

    let primary = null;
    if (primaryLabel) {
        primary = bankButton({
            label: primaryLabel,
            danger,
            active: !danger,
            onClick: async (ev, btn) => {
                if (!onPrimary) { close(); return; }
                btn.disabled = true;
                try {
                    const res = onPrimary();
                    if (res && typeof res.then === 'function') await res;
                    close();
                } catch (e) {
                    toast(e.message || String(e), 'error');
                    btn.disabled = false;
                }
            },
        });
        if (primaryClassName) primary.className = primaryClassName;
        actions.appendChild(primary);

        const refreshPrimaryEnabled = () => {
            const enabled = typeof primaryEnabled === 'function'
                ? primaryEnabled()
                : primaryEnabled;
            primary.disabled = !enabled;
        };
        bodyWrap.addEventListener('input', refreshPrimaryEnabled);
        bodyWrap.addEventListener('change', refreshPrimaryEnabled);
        refreshPrimaryEnabled();
    }

    modal.appendChild(actions);
    backdrop.appendChild(modal);

    // Click outside closes. We check target strict-equal to the backdrop so
    // clicks inside the modal don't bubble and dismiss unexpectedly.
    backdrop.addEventListener('click', (ev) => {
        if (ev.target === backdrop) close();
    });

    // Esc closes. Scoped to keydown on document while open.
    const keyHandler = (ev) => {
        if (ev.key === 'Escape') {
            ev.stopPropagation();
            close();
        } else if (ev.key === 'Tab') {
            const focusableElements = [...modal.querySelectorAll(
                'input, textarea, select, button, a[href], [tabindex]:not([tabindex="-1"])'
            )].filter((element) => !element.disabled && element.tabIndex >= 0);
            if (focusableElements.length === 0) {
                ev.preventDefault();
                return;
            }
            const first = focusableElements[0];
            const last = focusableElements[focusableElements.length - 1];
            const active = document.activeElement;
            if (ev.shiftKey && (active === first || !modal.contains(active))) {
                ev.preventDefault();
                last.focus();
            } else if (!ev.shiftKey && (active === last || !modal.contains(active))) {
                ev.preventDefault();
                first.focus();
            }
        } else if (ev.key === 'Enter' && primary && !ev.shiftKey) {
            // Only fire default submit if the active element is an input that
            // isn't a textarea - matches typical browser form behavior.
            const el = document.activeElement;
            const tag = el && el.tagName;
            if (tag === 'INPUT') {
                ev.preventDefault();
                primary.click();
            }
        }
    };
    document.addEventListener('keydown', keyHandler, true);

    document.body.appendChild(backdrop);

    // Focus first focusable element inside body on open.
    const focusable = bodyWrap.querySelector(
        'input, textarea, select, button, [tabindex]:not([tabindex="-1"])'
    );
    if (focusable && typeof focusable.focus === 'function') {
        try { focusable.focus(); } catch { /* ignore */ }
    } else {
        try { closeX.focus(); } catch { /* ignore */ }
    }

    let closed = false;
    function close() {
        if (closed) return;
        closed = true;
        document.removeEventListener('keydown', keyHandler, true);
        try { backdrop.remove(); } catch { /* ignore */ }
        if (previouslyFocused && typeof previouslyFocused.focus === 'function') {
            try { previouslyFocused.focus(); } catch { /* ignore */ }
        }
    }

    return close;
}

/**
 *
 * @param {Object} opts
 * @param {string} [opts.title='Confirm']
 * @param {string} opts.message            Body text. Newlines become <br>.
 * @param {string} [opts.okLabel='OK']
 * @param {string} [opts.cancelLabel='Cancel']
 * @param {boolean} [opts.danger=false]    Style the OK button as destructive.
 * @returns {Promise<boolean>} resolves true when OK was clicked, false on Cancel / Esc / outside click.
 */
export function confirmModal({
    title = 'Confirm',
    message = '',
    okLabel = 'OK',
    cancelLabel = 'Cancel',
    danger = false,
} = {}) {
    return new Promise((resolve) => {
        const body = document.createElement('div');
        body.className = 'bank-confirm-body';
        for (const line of String(message).split(/\r?\n/)) {
            const p = document.createElement('p');
            p.textContent = line;
            body.appendChild(p);
        }

        let answered = false;
        const close = openModal({
            title,
            body,
            primaryLabel: okLabel,
            secondaryLabel: cancelLabel,
            noScrim: true,
            danger,
            onPrimary: () => {
                answered = true;
                resolve(true);
            },
            onSecondary: () => {
                if (!answered) {
                    answered = true;
                    resolve(false);
                }
            },
        });

        // Also catch the Esc / click-outside paths, which don't fire
        // onSecondary. Wrap `close` so we resolve(false) exactly once.
        const originalClose = close;
        const wrappedClose = () => {
            if (!answered) {
                answered = true;
                resolve(false);
            }
            originalClose();
        };
        // The backdrop listener already calls close() via openModal's scope,
        // so hook into the DOM mutation: when the backdrop is removed, flush
        // the answer.
        const obs = new MutationObserver(() => {
            if (!document.body.contains(body)) {
                obs.disconnect();
                if (!answered) {
                    answered = true;
                    resolve(false);
                }
            }
        });
        obs.observe(document.body, { childList: true, subtree: true });
        void wrappedClose;
    });
}

/**
 * Inline replacement for `window.prompt`. Resolves to the entered string
 * when OK is clicked, or `null` when the user cancels / dismisses.
 *
 * @param {Object} opts
 * @param {string} [opts.title='Enter value']
 * @param {string} [opts.label='']
 * @param {string} [opts.defaultValue='']
 * @param {string} [opts.placeholder='']
 * @param {string} [opts.okLabel='OK']
 * @param {string} [opts.cancelLabel='Cancel']
 * @param {string} [opts.inputId='']
 * @param {number} [opts.inputMaxLength=0] - Maximum input characters. Zero
 *   leaves the input unrestricted.
 * @param {boolean} [opts.cancelDanger=false]
 * @param {string} [opts.cancelClassName='']
 * @param {string} [opts.okClassName='']
 * @param {boolean} [opts.noScrim=true] - Preserve the legacy transparent,
 *   non-interactive backdrop unless a blocking dialog is requested.
 * @param {Function} [opts.isValueAllowed] - Enables OK when this predicate
 *   returns true for the current input value.
 * @returns {Promise<string | null>}
 */
export function promptModal({
    title = 'Enter value',
    label = '',
    defaultValue = '',
    placeholder = '',
    okLabel = 'OK',
    cancelLabel = 'Cancel',
    inputId = '',
    inputMaxLength = 0,
    cancelDanger = false,
    cancelClassName = '',
    okClassName = '',
    noScrim = true,
    isValueAllowed = () => true,
} = {}) {
    return new Promise((resolve) => {
        const body = document.createElement('div');
        body.className = 'bank-confirm-body';

        let labelElement = null;
        if (label) {
            labelElement = document.createElement('label');
            labelElement.textContent = label;
            body.appendChild(labelElement);
        }
        const input = document.createElement('input');
        input.type = 'text';
        if (inputId) {
            input.id = inputId;
            if (labelElement) labelElement.htmlFor = inputId;
        }
        if (Number.isInteger(inputMaxLength) && inputMaxLength > 0) {
            input.maxLength = inputMaxLength;
        }
        input.value = defaultValue || '';
        if (placeholder) input.placeholder = placeholder;
        input.style.width = '100%';
        body.appendChild(input);

        let answered = false;
        openModal({
            title,
            body,
            primaryLabel: okLabel,
            secondaryLabel: cancelLabel,
            secondaryDanger: cancelDanger,
            secondaryClassName: cancelClassName,
            primaryClassName: okClassName,
            primaryEnabled: () => isValueAllowed(input.value),
            noScrim,
            onPrimary: () => {
                answered = true;
                resolve(input.value);
            },
            onSecondary: () => {
                if (!answered) {
                    answered = true;
                    resolve(null);
                }
            },
        });

        const obs = new MutationObserver(() => {
            if (!document.body.contains(body)) {
                obs.disconnect();
                if (!answered) {
                    answered = true;
                    resolve(null);
                }
            }
        });
        obs.observe(document.body, { childList: true, subtree: true });
    });
}
