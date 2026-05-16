// Shared harmonic map built from the acid progression generator's output.
// Used as the single source of truth for root/scale/profile/degree info
// by the bassline generator (and future chord generator).
//
// Pure module - no DOM, no state, no IO.

import { degreeToPitchClass } from './progression/progression-generator.js';

/**
 * @typedef {Object} HarmonicMap
 * @property {string} packageId
 * @property {string} createdAt     ISO timestamp
 * @property {number|null} seed     RNG seed used (for reproducibility)
 * @property {number} root          0..11
 * @property {string} scaleId
 * @property {string} scaleName
 * @property {string} profile       safe | dark | tension | jazz
 * @property {number[]} degrees     length 4, 1-based scale degrees
 * @property {Array<{patternIndex:number,degree:number,centerPc:number}>} centers
 * @property {number[]} timeline
 * @property {number[]} anchorSteps always [0, 4, 8, 12]
 * @property {number[]} scaleIntervals
 * @property {string[]} scaleTags
 */

/**
 * Build a harmonic map. Accepts the same values the acid generator already
 * resolved (scale, profile, degrees) plus a seed + packageId for replay.
 *
 * @param {Object} params
 * @param {string} params.packageId
 * @param {number|null} params.seed
 * @param {number} params.root
 * @param {{id:string,name:string,intervals:number[],tags?:string[]}} params.scale
 * @param {string} params.profile
 * @param {number[]} params.degrees
 * @param {number[]} [params.timeline]
 * @returns {HarmonicMap}
 */
export function buildHarmonicMap({ packageId, seed, root, scale, profile, degrees, timeline }) {
    if (typeof root !== 'number' || root < 0 || root > 11) {
        throw new Error(`buildHarmonicMap: root must be 0..11, got ${root}`);
    }
    if (!scale || !Array.isArray(scale.intervals) || scale.intervals.length === 0) {
        throw new Error('buildHarmonicMap: scale is required with non-empty intervals');
    }
    if (!Array.isArray(degrees) || degrees.length !== 4) {
        throw new Error('buildHarmonicMap: degrees must be a length-4 array');
    }
    if (typeof profile !== 'string' || profile.length === 0) {
        throw new Error('buildHarmonicMap: profile is required');
    }

    const centers = degrees.map((degree, patternIndex) => ({
        patternIndex,
        degree,
        centerPc: degreeToPitchClass(root, scale, degree),
    }));

    return {
        packageId: packageId ?? null,
        createdAt: new Date().toISOString(),
        seed: seed ?? null,
        root,
        scaleId: scale.id ?? '',
        scaleName: scale.name ?? '',
        profile,
        degrees: [...degrees],
        centers,
        timeline: Array.isArray(timeline) ? [...timeline] : [],
        anchorSteps: [0, 4, 8, 12],
        scaleIntervals: [...scale.intervals],
        scaleTags: Array.isArray(scale.tags) ? [...scale.tags] : [],
    };
}
