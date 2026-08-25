// The model name shown for a connected device.
//
// A TD-3-MO answers the SysEx product-name query with "TD-3", the same
// string the base model returns, so the reported name alone cannot tell the
// two apart. The firmware can: the MO ships 2.x, the base model 1.x. Any
// firmware whose major version is 2 or above is therefore labelled TD-3-MO.
//
// This is display only. Whether the device accepts Filter Cutoff and Pitch
// Bend is decided separately and exactly, by `supports_device_controls` in
// `src/web/device_capabilities.rs`, which matches firmware 2.0.1 and nothing
// else. A future 2.1 would read as a TD-3-MO here while still being refused
// those messages until that rule is widened deliberately, with device
// evidence.

/** Product name shown for a device whose firmware is 2.x or later. */
export const MO_LABEL = 'TD-3-MO';

/** Lowest firmware major version that identifies a TD-3-MO. */
export const MO_MAJOR_VERSION = 2;

/**
 * Major version of a firmware string such as `"2.0.1"`, or null when the
 * string is missing or does not start with a number.
 */
export function firmwareMajor(firmware) {
    if (typeof firmware !== 'string') return null;
    const match = /^\s*(\d+)/.exec(firmware);
    if (!match) return null;
    return Number.parseInt(match[1], 10);
}

/** True when `firmware` identifies a TD-3-MO. */
export function isMoFirmware(firmware) {
    const major = firmwareMajor(firmware);
    return major !== null && major >= MO_MAJOR_VERSION;
}

/**
 * The model name to show. Falls back to whatever the device reported when
 * the firmware is missing or unreadable, so an unknown device is never
 * relabelled on a guess.
 */
export function deviceLabel(productName, firmware) {
    if (isMoFirmware(firmware)) return MO_LABEL;
    return typeof productName === 'string' && productName ? productName : '';
}

/**
 * The `NAME vX.Y.Z` line shown beside the connection light. An absent
 * firmware renders as `?`, matching what the label did before.
 */
export function deviceLabelWithFirmware(productName, firmware) {
    const name = deviceLabel(productName, firmware);
    if (!name) return '';
    return `${name} v${firmware || '?'}`;
}
