//! Text-to-value rules the importers need: lenient numeric parsing and ISO-8601
//! dates. Kept here so every importer reads values the same way.

/// `String::getIntValue`: optional leading whitespace and sign, then digits. 0 when there are none.
pub fn int_value(text: &str) -> i64 {
    let s = text.trim_start();
    let (negative, digits_part) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let digits: &str = {
        let end = digits_part
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(digits_part.len());
        &digits_part[..end]
    };
    if digits.is_empty() {
        return 0;
    }
    let magnitude = digits.parse::<i64>().unwrap_or(i64::MAX);
    if negative {
        -magnitude
    } else {
        magnitude
    }
}

/// `String::getDoubleValue`: the longest leading number, or 0.0 when there is none.
pub fn double_value(text: &str) -> f64 {
    let s = text.trim_start().as_bytes();
    let mut end = 0;
    if end < s.len() && (s[end] == b'+' || s[end] == b'-') {
        end += 1;
    }
    let int_start = end;
    while end < s.len() && s[end].is_ascii_digit() {
        end += 1;
    }
    let mut digits = end - int_start;
    if end < s.len() && s[end] == b'.' {
        let frac_start = end + 1;
        let mut frac_end = frac_start;
        while frac_end < s.len() && s[frac_end].is_ascii_digit() {
            frac_end += 1;
        }
        digits += frac_end - frac_start;
        if digits > 0 {
            end = frac_end;
        }
    }
    if digits == 0 {
        return 0.0;
    }
    if end < s.len() && (s[end] == b'e' || s[end] == b'E') {
        let mut exp_end = end + 1;
        if exp_end < s.len() && (s[exp_end] == b'+' || s[exp_end] == b'-') {
            exp_end += 1;
        }
        let exp_digits_start = exp_end;
        while exp_end < s.len() && s[exp_end].is_ascii_digit() {
            exp_end += 1;
        }
        if exp_end > exp_digits_start {
            end = exp_end;
        }
    }
    std::str::from_utf8(&s[..end])
        .ok()
        .and_then(|t| t.parse::<f64>().ok())
        .unwrap_or(0.0)
}

/// Milliseconds since the Unix epoch for an ISO-8601 date or date-time (`2020-03-04`,
/// `2020-03-04T05:06:07Z`, `2020-03-04 05:06:07+02:00`). A missing zone reads as UTC. 0 when the
/// text does not parse.
pub fn iso8601_ms(text: &str) -> i64 {
    parse_iso8601(text.trim()).unwrap_or(0)
}

fn parse_iso8601(s: &str) -> Option<i64> {
    let date = s.get(..10)?;
    let (year, month, day) = (
        date.get(0..4)?.parse::<i64>().ok()?,
        date.get(5..7)?.parse::<i64>().ok()?,
        date.get(8..10)?.parse::<i64>().ok()?,
    );
    if date.as_bytes().get(4) != Some(&b'-') || date.as_bytes().get(7) != Some(&b'-') {
        return None;
    }
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let mut rest = &s[10..];
    let mut seconds_of_day = 0i64;
    let mut millis = 0i64;
    if let Some(time) = rest.strip_prefix(['T', ' ']) {
        let hh = time.get(0..2)?.parse::<i64>().ok()?;
        let mm = time.get(3..5)?.parse::<i64>().ok()?;
        if time.as_bytes().get(2) != Some(&b':') {
            return None;
        }
        let mut after = &time[5..];
        let mut ss = 0;
        if let Some(sec) = after.strip_prefix(':') {
            ss = sec.get(0..2)?.parse::<i64>().ok()?;
            after = &sec[2..];
            if let Some(frac) = after.strip_prefix('.') {
                let digits: String = frac.chars().take_while(|c| c.is_ascii_digit()).collect();
                let take = digits.len().min(3);
                if take > 0 {
                    let scale = 10i64.pow(3 - take as u32);
                    millis = digits[..take].parse::<i64>().ok()? * scale;
                }
                after = &frac[digits.len()..];
            }
        }
        seconds_of_day = hh * 3600 + mm * 60 + ss;
        rest = after;
    }
    let mut offset_seconds = 0i64;
    if rest != "Z" && !rest.is_empty() {
        let sign = rest.chars().next().filter(|c| *c == '+' || *c == '-')?;
        let zone = rest[1..].replace(':', "");
        if zone.len() != 4 {
            return None;
        }
        let oh = zone[0..2].parse::<i64>().ok()?;
        let om = zone[2..4].parse::<i64>().ok()?;
        offset_seconds = (oh * 3600 + om * 60) * if sign == '+' { 1 } else { -1 };
    }
    let days = days_from_civil(year, month, day);
    let total = days * 86_400 + seconds_of_day - offset_seconds;
    Some(total * 1000 + millis)
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_and_double_follow_juce_leniency() {
        assert_eq!(int_value("  -12abc"), -12);
        assert_eq!(int_value("abc"), 0);
        assert_eq!(double_value("215.5x"), 215.5);
        assert_eq!(double_value("1e3"), 1000.0);
        assert_eq!(double_value("."), 0.0);
    }

    #[test]
    fn iso8601_dates_convert_to_epoch_millis() {
        assert_eq!(iso8601_ms("1970-01-01T00:00:01Z"), 1000);
        assert_eq!(iso8601_ms("2020-03-04T05:06:07Z"), 1_583_298_367_000);
        assert_eq!(iso8601_ms("2020-03-04T05:06:07+01:00"), 1_583_294_767_000);
        assert!(iso8601_ms("2022-05-06") > 0);
        assert_eq!(iso8601_ms("not a date"), 0);
    }
}
