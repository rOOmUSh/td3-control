export function bindPointerPressActivation(button, activate) {
    let pointerActivated = false;

    button.addEventListener('pointerdown', (event) => {
        if (!isPrimaryButton(event)) return;
        pointerActivated = true;
        activate(event);
    });

    button.addEventListener('click', (event) => {
        if (!isPrimaryButton(event)) return;
        if (pointerActivated && event.detail !== 0) {
            pointerActivated = false;
            return;
        }
        pointerActivated = false;
        activate(event);
    });
}

function isPrimaryButton(event) {
    return event.button == null || event.button === 0;
}
