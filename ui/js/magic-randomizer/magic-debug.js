// Debug report for one magic-randomize run. Prints a single console
// summary describing the pipeline outcome - useful while tuning weights
// or investigating.
//
// The orchestrator builds the report; this module just formats and
// prints it.

const STRONG_BEATS = [0, 4, 8, 12];

/**
 * Build a debug report object from one full magic run.
 *
 * @param {object} input              { rootPc, scaleName, mode, attempts, ... }
 * @param {object} winner             chosen { score, breakdown, metrics, candidate, actions }
 * @param {object[]} rejected         rejected attempts [{ reasons }]
 * @returns {object} report object
 */
export function buildDebugReport(input, winner, rejected) {
    const m = winner ? winner.metrics : null;
    const c = winner ? winner.candidate : null;
    const noteList = c ? c.steps.map(s => `${s.note}/${s.transpose[0]}/${s.time === 'REST' ? '·' : '·'}`) : [];

    return {
        rootPc:           input.rootPc,
        scaleName:        input.scaleName,
        mode:             input.mode,                 // 'full' | 'slice' | 'progression'
        attempts:         input.attempts,
        accepted:         winner ? winner.score : null,
        winnerBreakdown:  winner ? winner.breakdown : null,
        rejectSummary:    summarizeReasons(rejected),
        repairActions:    winner ? (winner.actions || []) : [],
        metrics: m && {
            activeCount:        m.activeCount,
            rootCount:          m.rootCount,
            distinctPcs:        m.distinctPcs,
            strongStable:       m.strongStableCount,
            maxRunLen:          m.maxRunLen,
            maxPcCount:         m.maxPcCount,
            maxAbsPitchCount:   m.maxAbsPitchCount,
            largestLeap:        m.largestLeap,
            loopMovement:       m.loopMovement,
            registerSpan:       m.registerMax - m.registerMin,
            movementBuckets:    m.movementBuckets,
        },
        finalNotes:       noteList,
        finalSteps:       c ? c.steps : [],
    };
}

function summarizeReasons(rejected) {
    if (!rejected || rejected.length === 0) return {};
    const counts = {};
    for (const r of rejected) {
        for (const reason of (r.reasons || [])) {
            counts[reason] = (counts[reason] || 0) + 1;
        }
    }
    return counts;
}

/**
 * Print the debug report to console.log under a single grouped block.
 * Safe in environments without console.group - falls back to a flat dump.
 */
export function printDebugReport(report, label = 'magic-randomizer') {
    if (!report) return;
    const head = `[${label}] ${report.scaleName ?? '?'} root=${report.rootPc} mode=${report.mode} attempts=${report.attempts} score=${report.accepted ?? '-'}`;
    if (typeof console.groupCollapsed === 'function') {
        console.groupCollapsed(head);
    } else {
        console.log(head);
    }
    if (report.metrics) console.log('metrics:', report.metrics);
    if (report.winnerBreakdown) console.log('breakdown:', report.winnerBreakdown);
    if (report.repairActions && report.repairActions.length) console.log('repair:', report.repairActions);
    if (report.rejectSummary && Object.keys(report.rejectSummary).length) {
        console.log('rejects:', report.rejectSummary);
    }
    if (typeof console.groupEnd === 'function') console.groupEnd();
}

export { STRONG_BEATS };
