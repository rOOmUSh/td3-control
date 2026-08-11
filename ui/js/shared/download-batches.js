const DOWNLOAD_BATCH_SIZE = 10;

function waitForMilliseconds(delayMs) {
    return new Promise(resolve => setTimeout(resolve, delayMs));
}

export async function runDownloadBatches(
    items,
    downloadItem,
    delayMs,
    wait = waitForMilliseconds,
) {
    const safeItems = Array.isArray(items) ? items : [];
    const safeDelayMs = Number.isFinite(Number(delayMs))
        ? Math.max(0, Number(delayMs))
        : 0;

    for (let start = 0; start < safeItems.length; start += DOWNLOAD_BATCH_SIZE) {
        const end = Math.min(start + DOWNLOAD_BATCH_SIZE, safeItems.length);
        for (let index = start; index < end; index++) {
            await downloadItem(safeItems[index], index);
        }
        if (end < safeItems.length && safeDelayMs > 0) {
            await wait(safeDelayMs);
        }
    }
}
