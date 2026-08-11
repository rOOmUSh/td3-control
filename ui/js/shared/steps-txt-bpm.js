const MIN_CENTIBPM = 2000;
const MAX_CENTIBPM = 30000;

export function parseBpmCentibpm(raw) {
    if (typeof raw !== 'string' || raw.length === 0) {
        throw new Error(`invalid bpm '${String(raw ?? '')}'`);
    }

    const firstDot = raw.indexOf('.');
    const wholeRaw = firstDot === -1 ? raw : raw.slice(0, firstDot);
    const fractionRaw = firstDot === -1 ? '' : raw.slice(firstDot + 1);
    if (fractionRaw.includes('.')
        || !/^\d+$/.test(wholeRaw)
        || fractionRaw.length > 2
        || !/^\d*$/.test(fractionRaw)) {
        throw new Error(`invalid bpm '${raw}'`);
    }

    const whole = Number(wholeRaw);
    if (!Number.isSafeInteger(whole)) {
        throw new Error(`invalid bpm '${raw}'`);
    }
    const fraction = fractionRaw.length === 0
        ? 0
        : Number(fractionRaw.padEnd(2, '0'));
    const centibpm = whole * 100 + fraction;
    validateCentibpm(centibpm, `bpm '${raw}'`);
    return centibpm;
}

export function bpmValueToCentibpm(bpm) {
    if (typeof bpm !== 'number' || !Number.isFinite(bpm)) {
        throw new Error('bpm must be a finite number');
    }
    return parseBpmCentibpm(String(bpm));
}

export function formatBpmCentibpm(centibpm) {
    validateCentibpm(centibpm, 'centibpm');
    const whole = Math.floor(centibpm / 100);
    const fraction = centibpm % 100;
    if (fraction === 0) return String(whole);
    if (fraction % 10 === 0) return `${whole}.${fraction / 10}`;
    return `${whole}.${String(fraction).padStart(2, '0')}`;
}

function validateCentibpm(centibpm, label) {
    if (!Number.isInteger(centibpm)
        || centibpm < MIN_CENTIBPM
        || centibpm > MAX_CENTIBPM) {
        throw new Error(`${label} must be ${MIN_CENTIBPM}-${MAX_CENTIBPM} centibpm`);
    }
}
