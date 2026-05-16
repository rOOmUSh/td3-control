// Tiny RNG facade so candidate generation is fully deterministic in tests
// and reproducible in production runs (the same seed → same melody).
// Same shape as progression-generator.js's RNG.

export function createRng(seed) {
    if (seed == null) {
        return { next: () => Math.random() };
    }
    let s = seed | 0;
    return {
        next() {
            s |= 0; s = s + 0x6D2B79F5 | 0;
            let t = Math.imul(s ^ s >>> 15, 1 | s);
            t = t + Math.imul(t ^ t >>> 7, 61 | t) ^ t;
            return ((t ^ t >>> 14) >>> 0) / 4294967296;
        },
    };
}

export function pickWeighted(rng, items, weights) {
    let total = 0;
    for (const w of weights) total += w;
    if (total <= 0) return items[Math.floor(rng.next() * items.length)];
    let r = rng.next() * total;
    for (let i = 0; i < items.length; i++) {
        r -= weights[i];
        if (r <= 0) return items[i];
    }
    return items[items.length - 1];
}

export function pickOne(rng, items) {
    if (!items || items.length === 0) return null;
    return items[Math.floor(rng.next() * items.length)];
}

export function shuffleInPlace(arr, rng) {
    for (let i = arr.length - 1; i > 0; i--) {
        const j = Math.floor(rng.next() * (i + 1));
        [arr[i], arr[j]] = [arr[j], arr[i]];
    }
    return arr;
}
