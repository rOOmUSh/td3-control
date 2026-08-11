export function restoreStepHighlight(card) {
    if (!card) return;
    card.classList.remove('step-active');
}

export function applyStepHighlight(card) {
    if (!card) return;
    card.classList.add('step-active');
}
