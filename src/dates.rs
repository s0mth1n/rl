//! Calendar and date math without external crates: civil calendar conversions.
//! Howard Hinnant's days_from_civil / civil_from_days algorithms (public domain).

pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = ((m + 9) % 12) as i64;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub fn days_in_month(y: i64, m: u32) -> u32 {
    (if m == 12 {
        days_from_civil(y + 1, 1, 1) - days_from_civil(y, 12, 1)
    } else {
        days_from_civil(y, m + 1, 1) - days_from_civil(y, m, 1)
    }) as u32
}

/// 0 = Monday.
pub fn weekday(z: i64) -> u32 {
    ((z + 3).rem_euclid(7)) as u32
}

pub const WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];
pub const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Local offset from UTC in seconds, via the date command (for example +0530 becomes 19800).
pub fn utc_offset_secs() -> i64 {
    let out = std::process::Command::new("date")
        .arg("+%z")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    let t = out.trim();
    let (sign, digits) = match t.as_bytes().first() {
        Some(b'+') => (1, &t[1..]),
        Some(b'-') => (-1, &t[1..]),
        _ => (1, t),
    };
    let n: i64 = digits.parse().unwrap_or(0);
    sign * ((n / 100) * 3600 + (n % 100) * 60)
}

/// Local today as (year, month, day), via the date command so the zone is right.
pub fn today() -> (i64, u32, u32) {
    let out = std::process::Command::new("date")
        .arg("+%Y-%m-%d")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    let p: Vec<i64> = out
        .trim()
        .split('-')
        .filter_map(|x| x.parse().ok())
        .collect();
    if p.len() == 3 {
        (p[0], p[1] as u32, p[2] as u32)
    } else {
        civil_from_days(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64)
                / 86400,
        )
    }
}

/// Add n months to a civil date, clamping the day (Jan 31 + 1 month is Feb 28).
fn add_months((y, m, d): (i64, u32, u32), n: i64) -> i64 {
    let t = (y * 12 + m as i64 - 1) + n;
    let (yy, mm) = (t.div_euclid(12), t.rem_euclid(12) as u32 + 1);
    days_from_civil(yy, mm, d.min(days_in_month(yy, mm)))
}

fn weekday_name(s: &str) -> Option<u32> {
    WEEKDAYS
        .iter()
        .position(|w| w.to_lowercase().starts_with(s) && s.len() >= 3)
        .map(|i| i as u32)
}
fn month_name(s: &str) -> Option<u32> {
    MONTHS
        .iter()
        .position(|m| m.to_lowercase().starts_with(s) && s.len() >= 3)
        .map(|i| i as u32 + 1)
}

/// Date phrases: "in 3 weeks", "5 days ago", "in 2 months" (calendar-precise), "next friday",
/// "last monday", "dec 25", "25 dec 2027", "tomorrow", or "2026-12-25".
pub fn parse_phrase(q: &str, from: (i64, u32, u32)) -> Option<i64> {
    let q = q.trim().to_lowercase();
    let base = days_from_civil(from.0, from.1, from.2);
    let w: Vec<&str> = q.split_whitespace().collect();
    let num = |s: &str| s.parse::<i64>().ok();
    match w.as_slice() {
        ["in", n, u] => {
            let n = num(n)?;
            match *u {
                "day" | "days" => Some(base + n),
                "week" | "weeks" => Some(base + 7 * n),
                "month" | "months" => Some(add_months(from, n)),
                "year" | "years" => Some(add_months(from, 12 * n)),
                _ => None,
            }
        }
        [n, u, "ago"] => {
            let n = num(n)?;
            match *u {
                "day" | "days" => Some(base - n),
                "week" | "weeks" => Some(base - 7 * n),
                "month" | "months" => Some(add_months(from, -n)),
                "year" | "years" => Some(add_months(from, -12 * n)),
                _ => None,
            }
        }
        ["next", wd] => weekday_name(wd).map(|t| {
            let d = (t as i64 - weekday(base) as i64).rem_euclid(7);
            base + if d == 0 { 7 } else { d }
        }),
        ["last", wd] => weekday_name(wd).map(|t| {
            let d = (weekday(base) as i64 - t as i64).rem_euclid(7);
            base - if d == 0 { 7 } else { d }
        }),
        ["today"] => Some(base),
        ["tomorrow"] => Some(base + 1),
        ["yesterday"] => Some(base - 1),
        [m, d] => month_name(m)
            .and_then(|mm| {
                num(d).filter(|dd| (1..=31).contains(dd)).map(|dd| {
                    let mut y = from.0;
                    if (mm, dd as u32) < (from.1, from.2) {
                        y += 1;
                    }
                    days_from_civil(y, mm, dd as u32)
                })
            })
            .or_else(|| {
                month_name(d).and_then(|mm| {
                    num(m).filter(|dd| (1..=31).contains(dd)).map(|dd| {
                        let mut y = from.0;
                        if (mm, dd as u32) < (from.1, from.2) {
                            y += 1;
                        }
                        days_from_civil(y, mm, dd as u32)
                    })
                })
            }),
        [m, d, y] => {
            let (mm, dd, yy) = month_name(m)
                .and_then(|mm| num(d).map(|dd| (mm, dd, num(y))))
                .or_else(|| month_name(d).and_then(|mm| num(m).map(|dd| (mm, dd, num(y)))))?;
            let yy = yy?;
            if !(1..=31).contains(&dd) || yy < 1 {
                return None;
            }
            Some(days_from_civil(yy, mm, dd as u32))
        }
        _ => {
            let p: Vec<i64> = q.split('-').filter_map(|x| x.parse().ok()).collect();
            if p.len() == 3 && (1..=12).contains(&p[1]) {
                Some(days_from_civil(p[0], p[1] as u32, p[2] as u32))
            } else {
                None
            }
        }
    }
}

/// A month grid as text lines: a header then one line per week, today bracketed.
pub fn month_lines(y: i64, m: u32, today: (i64, u32, u32)) -> Vec<String> {
    let first = weekday(days_from_civil(y, m, 1));
    let n = days_in_month(y, m);
    let mut lines = vec!["Mo Tu We Th Fr Sa Su".to_string()];
    let mut line = "    ".repeat(first as usize);
    for d in 1..=n {
        let cell = if (y, m, d) == today {
            format!("[{d:2}]")
        } else {
            format!(" {d:2} ")
        };
        line.push_str(&cell);
        if (first + d).is_multiple_of(7) {
            lines.push(std::mem::take(&mut line));
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn civil_round_trip() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(weekday(0), 3); // Thursday
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(
            civil_from_days(days_from_civil(2026, 11, 11)),
            (2026, 11, 11)
        );
        assert_eq!(days_in_month(2026, 2), 28);
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2026, 4), 30);
    }
    #[test]
    fn phrases() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let t = (2026, 11, 11);
        assert_eq!(
            parse_phrase("in 3 weeks", t).map(civil_from_days),
            Some((2026, 12, 2))
        );
        assert_eq!(
            parse_phrase("5 days ago", t).map(civil_from_days),
            Some((2026, 11, 6))
        );
        assert_eq!(
            parse_phrase("2026-12-25", t).map(civil_from_days),
            Some((2026, 12, 25))
        );
        assert_eq!(
            parse_phrase("tomorrow", t).map(civil_from_days),
            Some((2026, 11, 12))
        );
        assert_eq!(parse_phrase("gibberish", t), None);
        // Calendar-precise months clamp the day, weekdays wrap, month names take an optional year.
        let jan31 = (2026, 1, 31);
        assert_eq!(
            parse_phrase("in 1 month", jan31).map(civil_from_days),
            Some((2026, 2, 28))
        );
        assert_eq!(
            parse_phrase("in 13 months", t).map(civil_from_days),
            Some((2027, 12, 11))
        );
        assert_eq!(
            parse_phrase("1 year ago", t).map(civil_from_days),
            Some((2025, 11, 11))
        );
        assert_eq!(parse_phrase("next friday", t).map(weekday), Some(4));
        assert_eq!(
            parse_phrase("next friday", t).map(civil_from_days),
            Some((2026, 11, 13))
        );
        assert_eq!(
            parse_phrase("last monday", t).map(civil_from_days),
            Some((2026, 11, 9))
        );
        assert_eq!(
            parse_phrase("dec 25", t).map(civil_from_days),
            Some((2026, 12, 25))
        );
        assert_eq!(
            parse_phrase("25 jan", t).map(civil_from_days),
            Some((2027, 1, 25))
        );
        assert_eq!(
            parse_phrase("march 3 2028", t).map(civil_from_days),
            Some((2028, 3, 3))
        );
    }
    #[test]
    fn grid() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let lines = month_lines(2026, 11, (2026, 11, 11));
        assert_eq!(lines[0], "Mo Tu We Th Fr Sa Su");
        assert!(lines[3].contains("[11]"), "{}", lines[3]);
        assert_eq!(lines[1].trim(), "1");
    }
}
