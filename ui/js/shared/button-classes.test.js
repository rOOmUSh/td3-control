import {
    ROW_BTN_NEUTRAL,
    ROW_BTN_SL,
    ROW_BTN_AC,
    ROW_BTN_UD,
    ROW_BTN_SHIFT,
    ROW_BTN_BANK,
    ROW_BTN_DANGER,
    ROW_BTN_TRIPLET,
    ROW_DISABLED,
    TD3_CHECKBOX,
    rowButtonClass,
} from './button-classes.js';

function assert(cond, msg) {
    if (!cond) throw new Error('assertion failed: ' + msg);
}

assert(ROW_BTN_NEUTRAL === 'td3-row-btn tactile-button td3-row-btn-neutral', 'neutral row class');
assert(ROW_BTN_SL === 'td3-row-btn tactile-button td3-row-btn-slide', 'slide row class');
assert(ROW_BTN_AC === 'td3-row-btn tactile-button td3-row-btn-accent', 'accent row class');
assert(ROW_BTN_UD === 'td3-row-btn tactile-button td3-row-btn-updown', 'updown row class');
assert(ROW_BTN_SHIFT === 'td3-row-btn tactile-button td3-row-btn-shift', 'shift row class');
assert(ROW_BTN_BANK === 'td3-row-btn tactile-button td3-row-btn-bank', 'bank row class');
assert(ROW_BTN_DANGER === 'td3-row-btn tactile-button td3-row-btn-danger', 'danger row class');
assert(ROW_BTN_TRIPLET === 'td3-row-btn tactile-button td3-row-btn-triplet', 'triplet row class');
assert(ROW_DISABLED === 'td3-row-btn-disabled', 'disabled row class');
assert(TD3_CHECKBOX === 'td3-checkbox', 'shared checkbox class');
assert(
    rowButtonClass('neutral', 'w-full') === 'td3-row-btn tactile-button td3-row-btn-neutral w-full',
    'extra class append',
);

console.log('button-classes.test.js: all assertions passed');
