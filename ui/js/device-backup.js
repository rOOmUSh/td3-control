// Device bank backup and restore.
//
// On MIDI connect: automatically dump all 64 patterns (G1-4, P1-8, A/B)
// into IndexedDB. Restore writes them all back to the device.

import { api } from './api.js';
import * as history from './history.js';

let setStatus = () => {};
let onBackupDone = () => {};

/**
 * Initialize with callbacks.
 * @param {function} statusFn - status log writer
 * @param {function} backupDoneFn - called with (success: boolean) when backup finishes
 */
export function init(statusFn, backupDoneFn) {
    setStatus = statusFn;
    onBackupDone = backupDoneFn || (() => {});
}

/**
 * Backup all 64 patterns from the device into IndexedDB.
 * Called automatically after MIDI connection is established.
 * @param {string} firmware - firmware version for tagging the backup
 */
export async function backupDevice(firmware) {
    const patterns = [];
    let errors = 0;

    setStatus('Backing up device bank...');

    for (let g = 1; g <= 4; g++) {
        for (let p = 1; p <= 8; p++) {
            for (const s of ['A', 'B']) {
                try {
                    const data = await api.loadPattern(g, p, s);
                    patterns.push({ group: g, pattern: p, side: s, data });
                    const count = patterns.length;
                    if (count % 8 === 0) {
                        setStatus(`Backing up device bank... ${count}/64`);
                    }
                } catch (err) {
                    errors++;
                    console.error(`Backup failed for G${g}-P${p}${s}:`, err.message);
                }
            }
        }
    }

    if (errors > 0) {
        setStatus(`Backup incomplete: ${patterns.length}/64 patterns saved, ${errors} errors`);
        onBackupDone(false);
        return;
    }

    try {
        await history.storeBackup(patterns, firmware);
        setStatus(`Device bank stored successfully (${patterns.length} patterns)`);
        onBackupDone(true);
    } catch (err) {
        setStatus('Backup storage error: ' + err.message);
        onBackupDone(false);
    }
}

/**
 * Ensure a backup exists. If no backup is stored in IndexedDB, read all
 * 64 patterns from the device and store them. Safe to call on every page
 * load - skips if a backup already exists.
 * @param {string} firmware - firmware version for tagging
 */
export async function ensureBackup(firmware) {
    const exists = await history.hasBackup();
    if (exists) return;
    await backupDevice(firmware);
}

/**
 * Restore the most recent backup to the device.
 * Writes all 64 patterns back sequentially.
 */
export async function restoreDevice() {
    const backup = await history.getLatestBackup();
    if (!backup) {
        setStatus('No backup found');
        return;
    }

    setStatus('Restoring device bank...');
    let done = 0;
    let errors = 0;

    for (const entry of backup.patterns) {
        try {
            await api.savePattern(entry.group, entry.pattern, entry.side, entry.data);
            done++;
            if (done % 8 === 0) {
                setStatus(`Restoring ${done}/${backup.patterns.length}...`);
            }
        } catch (err) {
            errors++;
            console.error(`Restore failed for G${entry.group}-P${entry.pattern}${entry.side}:`, err.message);
        }
    }

    if (errors > 0) {
        setStatus(`Restore incomplete: ${done} written, ${errors} errors`);
    } else {
        setStatus(`Bank restored successfully (${done} patterns)`);
    }
}
