//! Local wall-clock formatting for message timestamps without a date crate.
//! The UTC offset is read once per process.

use std::sync::OnceLock;

/// Days since 1970-01-01 → (year, month 1-12, day 1-31) (H. Hinnant's
/// `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[cfg(windows)]
fn system_offset_secs() -> i64 {
    use windows::Win32::System::SystemInformation::{GetLocalTime, GetSystemTime};
    // SAFETY: both calls only return a SYSTEMTIME by value.
    let (local, utc) = unsafe { (GetLocalTime(), GetSystemTime()) };
    // Day of month and wall time are enough: the two readings differ by < 2 days.
    let minutes = |day: u16, hour: u16, minute: u16| {
        i64::from(day) * 1440 + i64::from(hour) * 60 + i64::from(minute)
    };
    let mut diff =
        minutes(local.wDay, local.wHour, local.wMinute) - minutes(utc.wDay, utc.wHour, utc.wMinute);
    // Month rollover between the readings (e.g. local 1st vs UTC 31st).
    if diff > 1440 {
        diff -= i64::from(utc.wDay) * 1440;
    } else if diff < -1440 {
        diff += i64::from(local.wDay) * 1440;
    }
    diff * 60
}

#[cfg(not(windows))]
fn system_offset_secs() -> i64 {
    let out = std::process::Command::new("date").arg("+%z").output();
    let text = out
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    parse_offset(text.trim()).unwrap_or(0)
}

/// `+0130` / `-0500` → seconds east of UTC.
#[cfg_attr(windows, allow(dead_code))]
fn parse_offset(s: &str) -> Option<i64> {
    let (sign, digits) = match s.as_bytes().first()? {
        b'+' => (1, &s[1..]),
        b'-' => (-1, &s[1..]),
        _ => return None,
    };
    if digits.len() != 4 {
        return None;
    }
    let h: i64 = digits[..2].parse().ok()?;
    let m: i64 = digits[2..].parse().ok()?;
    Some(sign * (h * 3600 + m * 60))
}

fn offset_secs() -> i64 {
    static OFFSET: OnceLock<i64> = OnceLock::new();
    *OFFSET.get_or_init(system_offset_secs)
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// `HH:MM` for today, `Mon D, HH:MM` this year, `Mon D YYYY, HH:MM` otherwise.
pub fn format_with_offset(ms: u64, now_ms: u64, offset_secs: i64) -> String {
    let local = |t: u64| (t / 1000) as i64 + offset_secs;
    let (secs, now) = (local(ms), local(now_ms));
    let (day, now_day) = (secs.div_euclid(86_400), now.div_euclid(86_400));
    let tod = secs.rem_euclid(86_400);
    let hm = format!("{:02}:{:02}", tod / 3600, (tod % 3600) / 60);
    if day == now_day {
        return hm;
    }
    let (y, m, d) = civil_from_days(day);
    let month = MONTHS[(m - 1) as usize];
    if y == civil_from_days(now_day).0 {
        format!("{month} {d}, {hm}")
    } else {
        format!("{month} {d} {y}, {hm}")
    }
}

pub fn format_timestamp(ms: u64) -> String {
    format_with_offset(ms, now_ms(), offset_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2026-10-14T09:05:00Z
    const T: u64 = 1_791_968_700_000;

    #[test]
    fn civil_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_740), (2026, 10, 14));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
    }

    #[test]
    fn today_shows_time_only_in_local_offset() {
        assert_eq!(format_with_offset(T, T + 60_000, 0), "09:05");
        assert_eq!(format_with_offset(T, T, 2 * 3600), "11:05");
        assert_eq!(format_with_offset(T, T, -(5 * 3600 + 30 * 60)), "03:35");
    }

    #[test]
    fn other_days_include_date_and_year_when_different() {
        assert_eq!(format_with_offset(T, T + 86_400_000, 0), "Oct 14, 09:05");
        assert_eq!(
            format_with_offset(T, T + 400 * 86_400_000, 0),
            "Oct 14 2026, 09:05"
        );
    }

    #[test]
    fn offsets_parse() {
        assert_eq!(parse_offset("+0130"), Some(5400));
        assert_eq!(parse_offset("-0500"), Some(-18_000));
        assert_eq!(parse_offset("Z"), None);
    }
}
