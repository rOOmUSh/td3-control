export function createButton({
    className = '',
    icon,
    label,
    title,
    ariaLabel,
    disabled = false,
    onClick,
    preventDefault = false,
    stopPropagation = false,
    iconClassName = 'material-symbols-outlined',
    iconSize,
} = {}) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = className;
    btn.disabled = !!disabled;
    if (title) btn.title = title;
    if (ariaLabel) btn.setAttribute('aria-label', ariaLabel);
    appendButtonContent(btn, { icon, label, iconClassName, iconSize });
    if (onClick) {
        btn.addEventListener('click', (ev) => {
            if (preventDefault) ev.preventDefault();
            if (stopPropagation) ev.stopPropagation();
            onClick(ev, btn);
        });
    }
    return btn;
}

export function appendButtonContent(btn, {
    icon,
    label,
    iconClassName = 'material-symbols-outlined',
    iconSize,
} = {}) {
    if (icon) {
        const ic = document.createElement('span');
        ic.className = iconClassName;
        if (iconSize) ic.style.fontSize = iconSize;
        ic.textContent = icon;
        btn.appendChild(ic);
    }
    if (label) {
        const lbl = document.createElement('span');
        lbl.textContent = label;
        btn.appendChild(lbl);
    }
}
