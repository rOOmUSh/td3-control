// Tiny toast helper. Drops a styled node into #bank-toast-stack and
// auto-removes after 3s. Never uses innerHTML with user data.

const STACK_ID = 'bank-toast-stack';
const AUTO_DISMISS_MS = 3000;

export function toast(message, kind = 'info') {
    const stack = document.getElementById(STACK_ID);
    if (!stack) {
        // Fallback for pages that forgot to include the stack - log instead.
        console[kind === 'error' ? 'error' : 'log'](`[toast:${kind}]`, message);
        return;
    }
    const el = document.createElement('div');
    el.className = `bank-toast ${safeKind(kind)}`;
    el.setAttribute('role', 'status');
    el.textContent = typeof message === 'string' ? message : String(message);
    stack.appendChild(el);
    setTimeout(() => {
        el.style.opacity = '0';
        el.style.transition = 'opacity 0.2s ease';
        setTimeout(() => el.remove(), 220);
    }, AUTO_DISMISS_MS);
}

function safeKind(k) {
    return k === 'success' || k === 'error' || k === 'info' ? k : 'info';
}
