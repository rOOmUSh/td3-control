use super::*;

pub fn sanitize_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_was_under = false;
    let bad = |c: char| matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|');
    for c in s.chars() {
        if bad(c) || c.is_whitespace() {
            if !prev_was_under {
                out.push('_');
                prev_was_under = true;
            }
        } else {
            out.push(c);
            prev_was_under = false;
        }
    }
    if out.is_empty() {
        return "progression".to_string();
    }
    out
}

pub(super) fn timestamp_compact(now: SystemTime) -> String {
    let (y, mo, d, h, mi, s) = to_ymdhms(now);
    format!("{:04}-{:02}-{:02}_{:02}-{:02}-{:02}", y, mo, d, h, mi, s)
}

pub(super) fn timestamp_iso(now: SystemTime) -> String {
    let (y, mo, d, h, mi, s) = to_ymdhms(now);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, mo, d, h, mi, s)
}

fn to_ymdhms(now: SystemTime) -> (i64, u64, u64, u64, u64, u64) {
    let secs = now
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let s = secs % 60;
    let mi = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = (secs / 86400) as i64;

    let z = days + 719468;
    let era = if z >= 0 {
        z / 146097
    } else {
        (z - 146096) / 146097
    };
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36525 - doe / 146096) / 365;
    let y_base = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y_base + 1 } else { y_base };

    (year, month, d, h, mi, s)
}
