// Parses the toolbar search box into structured filter fragments.
// Supported tokens (anywhere in the string, space-separated):
//   tag:foo                  -> filter.tag = 'foo'
//   scale:phrygian           -> filter.scale = 'phrygian'
//   root:D                   -> filter.root = 'D'
//   slot:G2P4B               -> filter.slot_key = 'G2P4B'
//   snapshot:"My snapshot"   -> filter.snapshot_id = 'My snapshot' (caller resolves by name)
//   favorite                 -> filter.favorite = true
//   format:seq               -> filter.format = 'seq'
// Any remaining whitespace-separated tokens become free text (filter.search).
//
// Quoted strings (double or single quotes) keep spaces intact. Unknown
// prefixes fall back into free text so the user isn't silently trapped.

const KNOWN_KEYS = new Set(['tag', 'scale', 'root', 'slot', 'snapshot', 'format']);

export function parseQuery(input) {
    const out = {
        text: '',
        tags: [],
        scale: undefined,
        root: undefined,
        slot: undefined,
        snapshot: undefined,
        favorite: false,
        format: undefined,
    };
    if (!input || !input.trim()) return out;

    const tokens = tokenize(input);
    const textTokens = [];

    for (const tok of tokens) {
        const lower = tok.toLowerCase();
        if (lower === 'favorite' || lower === 'favourite' || lower === 'fav') {
            out.favorite = true;
            continue;
        }
        const colon = tok.indexOf(':');
        if (colon > 0) {
            const key = tok.slice(0, colon).toLowerCase();
            let value = tok.slice(colon + 1);
            value = stripQuotes(value);
            if (KNOWN_KEYS.has(key) && value) {
                if (key === 'tag') out.tags.push(value);
                else if (key === 'scale') out.scale = value;
                else if (key === 'root') out.root = value;
                else if (key === 'slot') out.slot = value;
                else if (key === 'snapshot') out.snapshot = value;
                else if (key === 'format') out.format = value;
                continue;
            }
        }
        textTokens.push(stripQuotes(tok));
    }

    out.text = textTokens.join(' ').trim();
    return out;
}

/**
 * Merge a parsed query into an ItemFilter-shaped object (mutates a copy).
 * `resolveSnapshot` is optional and, when provided, converts a snapshot name
 * into its snapshot_id; otherwise the raw name is passed through unchanged.
 */
export function mergeQueryIntoFilter(filter, parsed, resolveSnapshot) {
    const next = { ...filter };
    next.search = parsed.text || '';
    // Tags: only the first tag token is applied because the backend filter
    // has a single `tag` slot. Extra tags are appended to free text so they
    // still bias matching.
    if (parsed.tags.length > 0) {
        next.tag = parsed.tags[0];
        if (parsed.tags.length > 1) {
            const extra = parsed.tags.slice(1).map((t) => `tag:${t}`).join(' ');
            next.search = next.search ? `${next.search} ${extra}` : extra;
        }
    } else {
        next.tag = undefined;
    }
    next.scale    = parsed.scale    ?? undefined;
    next.root     = parsed.root     ?? undefined;
    next.slot_key = parsed.slot     ?? undefined;
    next.format   = parsed.format   ?? undefined;
    next.favorite = parsed.favorite ? true : undefined;

    if (parsed.snapshot) {
        next.snapshot_id = resolveSnapshot ? resolveSnapshot(parsed.snapshot) : parsed.snapshot;
    } else {
        next.snapshot_id = undefined;
    }
    return next;
}

function tokenize(input) {
    const out = [];
    let buf = '';
    let quote = null;
    for (let i = 0; i < input.length; i++) {
        const c = input[i];
        if (quote) {
            buf += c;
            if (c === quote) quote = null;
            continue;
        }
        if (c === '"' || c === "'") {
            quote = c;
            buf += c;
            continue;
        }
        if (/\s/.test(c)) {
            if (buf) { out.push(buf); buf = ''; }
            continue;
        }
        buf += c;
    }
    if (buf) out.push(buf);
    return out;
}

function stripQuotes(s) {
    if (s.length >= 2) {
        const first = s[0];
        const last = s[s.length - 1];
        if ((first === '"' || first === "'") && first === last) {
            return s.slice(1, -1);
        }
    }
    return s;
}
