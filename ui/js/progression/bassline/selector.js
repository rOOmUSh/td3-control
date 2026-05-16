// Archetype selector - picks the "default" archetype for a generated bassline
// set based on P1 features. The UI still exposes all 5 archetypes so the user
// can audition; selector only seeds which chip the UI lights up first.
//
// Heuristic (empirical, tuned against the user's C# minor jam pattern):
//
//   density < 0.35                 → ARPEGGIO  (sparse lead wants busy bass)
//   density < 0.55                 → ROOT_PULSE (medium lead pairs with anchored pulse)
//   density < 0.75 AND anchorsActive >= 3  → SHADOW  (busy + grounded lead → follow gestures)
//   density < 0.75                 → OFFBEAT_RESPONSE (busy lead with weak anchors → fill gaps)
//   density >= 0.75                → PEDAL    (wall-to-wall lead → bass anchors hard)
//
// High syncopation flips any choice into PEDAL because syncopated leads are
// rhythmically unstable and the bass must provide the metric spine.

export const ARCHETYPE_KEYS = Object.freeze(['pedal', 'rootPulse', 'offbeat', 'shadow', 'arpeggio']);

export function selectDefaultArchetype(features) {
    if (!features) return 'rootPulse';
    const { density, anchorsActive, syncopation } = features;

    // Very syncopated leads need the bass to lock the grid. Overrides density.
    if (syncopation >= 0.6) return 'pedal';

    if (density < 0.35) return 'arpeggio';
    if (density < 0.55) return 'rootPulse';
    if (density < 0.75) {
        return anchorsActive >= 3 ? 'shadow' : 'offbeat';
    }
    return 'pedal';
}
