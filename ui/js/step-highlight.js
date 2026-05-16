const ACTIVE_STEP_CLASSES = [
    'step-active',
    'step-pulse',
    'led-glow-green-bright',
    'bg-primary-fixed',
];

export function restoreStepHighlight(card) {
    if (!card) return;

    card.classList.remove(...ACTIVE_STEP_CLASSES);

    const origBg = card.dataset.origBg;
    if (origBg) {
        origBg.split(' ').filter(Boolean).forEach((cls) => card.classList.add(cls));
    }

    delete card.dataset.origBg;
}

export function applyStepHighlight(card) {
    if (!card) return;

    const bgClasses = [...card.classList].filter((cls) => cls.startsWith('bg-'));
    card.dataset.origBg = bgClasses.join(' ');
    bgClasses.forEach((cls) => card.classList.remove(cls));
    card.classList.add(...ACTIVE_STEP_CLASSES);
}
