// Usage: node ui/js/shared/device-label.test.js

import {
    MO_LABEL,
    deviceLabel,
    deviceLabelWithFirmware,
    firmwareMajor,
    isMoFirmware,
} from './device-label.js';

let passed = 0;
let failed = 0;

function assert(condition, message) {
    if (!condition) {
        console.error(`  FAIL: ${message}`);
        failed += 1;
        return;
    }
    passed += 1;
}

function test(name, fn) {
    try {
        fn();
        console.log(`  ok: ${name}`);
    } catch (error) {
        console.error(`  FAIL: ${name}: ${error.stack || error.message}`);
        failed += 1;
    }
}

test('firmwareMajor reads the leading version number', () => {
    assert(firmwareMajor('2.0.1') === 2, '2.0.1');
    assert(firmwareMajor('1.2.0') === 1, '1.2.0');
    assert(firmwareMajor('10.0.0') === 10, 'two digit major');
    assert(firmwareMajor('  2.0.1') === 2, 'leading space');
    assert(firmwareMajor('3') === 3, 'major only');
});

test('firmwareMajor rejects what it cannot read', () => {
    assert(firmwareMajor(undefined) === null, 'undefined');
    assert(firmwareMajor(null) === null, 'null');
    assert(firmwareMajor('') === null, 'empty');
    assert(firmwareMajor('unknown') === null, 'non-numeric');
    assert(firmwareMajor('v2.0.1') === null, 'leading v is not a number');
    assert(firmwareMajor(2) === null, 'a number is not a firmware string');
});

test('a 2.x firmware is a TD-3-MO', () => {
    assert(isMoFirmware('2.0.1') === true, '2.0.1, the shipped MO firmware');
    assert(isMoFirmware('2.0.0') === true, '2.0.0');
    assert(isMoFirmware('2.1.4') === true, 'a later 2.x');
    assert(isMoFirmware('3.0.0') === true, 'anything above 2');
});

test('a 1.x firmware is not', () => {
    assert(isMoFirmware('1.0.0') === false, '1.0.0');
    assert(isMoFirmware('1.9.9') === false, '1.9.9');
    assert(isMoFirmware('0.9.0') === false, '0.9.0');
});

test('an unreadable firmware is not assumed to be a MO', () => {
    assert(isMoFirmware(undefined) === false, 'undefined');
    assert(isMoFirmware('') === false, 'empty');
    assert(isMoFirmware('unknown') === false, 'non-numeric');
});

test('deviceLabel renames a 2.x device whatever it called itself', () => {
    // The MO answers the product-name query with "TD-3", which is the whole
    // reason this exists.
    assert(deviceLabel('TD-3', '2.0.1') === MO_LABEL, 'a MO reporting TD-3');
    assert(deviceLabel('TD-3-MO', '2.0.1') === MO_LABEL, 'a MO reporting TD-3-MO');
});

test('deviceLabel leaves a 1.x device alone', () => {
    assert(deviceLabel('TD-3', '1.2.0') === 'TD-3', 'base model keeps its name');
});

test('deviceLabel falls back to the reported name when firmware is unusable', () => {
    assert(deviceLabel('TD-3', undefined) === 'TD-3', 'no firmware');
    assert(deviceLabel('TD-3', 'unknown') === 'TD-3', 'unreadable firmware');
    assert(deviceLabel('SOMETHING ELSE', '') === 'SOMETHING ELSE', 'unknown device');
    assert(deviceLabel(undefined, undefined) === '', 'nothing to show');
    assert(deviceLabel('', '1.0.0') === '', 'empty name stays empty');
});

test('deviceLabelWithFirmware formats the status line', () => {
    assert(deviceLabelWithFirmware('TD-3', '2.0.1') === 'TD-3-MO v2.0.1', 'MO line');
    assert(deviceLabelWithFirmware('TD-3', '1.2.0') === 'TD-3 v1.2.0', 'base line');
    assert(deviceLabelWithFirmware('TD-3', undefined) === 'TD-3 v?', 'absent firmware');
    assert(deviceLabelWithFirmware(undefined, undefined) === '', 'nothing to show');
});

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
