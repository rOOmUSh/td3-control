const ROW_BTN_BASE = 'td3-row-btn tactile-button';

export const ROW_BTN_NEUTRAL = rowButtonClass('neutral');
export const ROW_BTN_SL = rowButtonClass('slide');
export const ROW_BTN_AC = rowButtonClass('accent');
export const ROW_BTN_UD = rowButtonClass('updown');
export const ROW_BTN_SHIFT = rowButtonClass('shift');
export const ROW_BTN_BANK = rowButtonClass('bank');
export const ROW_BTN_DANGER = rowButtonClass('danger');
export const ROW_BTN_TRIPLET = rowButtonClass('triplet');
export const ROW_DISABLED = 'td3-row-btn-disabled';
export const ROW_COL_LABEL = 'text-[0.7rem] font-black text-on-surface-variant tracking-wider text-center';
export const ROW_NUM_LABEL = 'text-[0.7rem] font-black text-on-surface-variant text-center';
export const ROW_PIPE = '<div class="self-stretch w-px bg-outline-variant opacity-40"></div>';
export const TD3_CHECKBOX = 'td3-checkbox';
// Amber tick box for mode checkboxes, matching the sidebar MAGIC toggle.
// Pairs with a `peer sr-only` input inside the same label.
export const ROW_MORPH_BOX = 'w-3 h-3 rounded-sm border-2 border-on-surface-variant '
    + 'bg-transparent inline-flex items-center justify-center text-[0.55rem] font-black '
    + 'leading-none text-black transition-colors peer-checked:bg-amber-400 '
    + "peer-checked:border-amber-400 after:content-[''] peer-checked:after:content-['V']";

export function rowButtonClass(variant = 'neutral', extra = '') {
    const parts = [ROW_BTN_BASE, `td3-row-btn-${variant}`];
    if (extra) parts.push(extra);
    return parts.join(' ');
}
