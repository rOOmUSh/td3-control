const FORMAT_RULES = [
    { suffix: '.steps.txt', format: 'steps', binary: false, bank: false },
    { suffix: '.midi', format: 'mid', binary: true, bank: false },
    { suffix: '.toml', format: 'toml', binary: false, bank: false },
    { suffix: '.json', format: 'json', binary: false, bank: false },
    { suffix: '.txt', format: 'steps', binary: false, bank: false },
    { suffix: '.pat', format: 'pat', binary: false, bank: false },
    { suffix: '.seq', format: 'seq', binary: true, bank: false },
    { suffix: '.mid', format: 'mid', binary: true, bank: false },
    { suffix: '.sqs', format: 'sqs', binary: true, bank: true },
    { suffix: '.rbs', format: 'rbs', binary: true, bank: true },
];

export function detectImportFormat(filename) {
    if (typeof filename !== 'string' || filename.length === 0) {
        return { format: null, binary: false, bank: false, error: 'unsupported' };
    }
    const lower = filename.toLowerCase();
    const rule = FORMAT_RULES.find(entry => lower.endsWith(entry.suffix));
    if (!rule) {
        return { format: null, binary: false, bank: false, error: 'unsupported' };
    }
    return {
        format: rule.format,
        binary: rule.binary,
        bank: rule.bank,
        error: null,
    };
}

export function unsupportedImportMessage() {
    return 'Unsupported file type (use .toml, .json, .steps.txt, .pat, .seq, .mid, .sqs, or .rbs)';
}
