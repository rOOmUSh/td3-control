import { createButton } from '../shared/dom-button.js';

export function bankButton({
    icon,
    label,
    className = '',
    title,
    ariaLabel,
    danger = false,
    active = false,
    onClick,
    preventDefault = false,
    stopPropagation = false,
} = {}) {
    return createButton({
        icon,
        label,
        title,
        ariaLabel,
        className: buttonClass(className, { danger, active }),
        onClick,
        preventDefault,
        stopPropagation,
    });
}

export function menuButton(icon, label, onClick) {
    return createButton({
        icon,
        label,
        iconSize: '0.95rem',
        onClick,
    });
}

function buttonClass(extra, { danger, active }) {
    const parts = ['bank-toolbar-btn'];
    if (extra) parts.push(extra);
    if (danger) parts.push('danger');
    if (active) parts.push('is-active');
    return parts.join(' ');
}
