// SPDX-License-Identifier: AGPL-3.0-or-later
//! Calendar dates and wall-clock times for GhulCal, without a date library.
//!
//! Events use *floating* local time ("10:00 on 13 Nov" stays 10:00 wherever the device is), which
//! is what people mean for appointments and keeps series stable across daylight saving changes.
//! Text forms: date `YYYY-MM-DD`, date and time `YYYY-MM-DDTHH:MM`.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub y: i32,
    pub m: u32,
    pub d: u32,
}

/// Date plus minutes since midnight (0..1440).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DateTime {
    pub date: Date,
    pub min: u32,
}

pub fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(y) => 29,
        2 => 28,
        _ => 0,
    }
}

impl Date {
    pub fn new(y: i32, m: u32, d: u32) -> Option<Date> {
        if !(1..=12).contains(&m) || d == 0 || d > days_in_month(y, m) || !(1..=9999).contains(&y) {
            return None;
        }
        Some(Date { y, m, d })
    }

    /// Days since 1970-01-01 (proleptic Gregorian calendar).
    pub fn days(self) -> i64 {
        let y = if self.m <= 2 { self.y as i64 - 1 } else { self.y as i64 };
        let era = if y >= 0 { y } else { y - 399 } / 400;
        let yoe = y - era * 400;
        let m = self.m as i64;
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + self.d as i64 - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146097 + doe - 719468
    }

    pub fn from_days(z: i64) -> Date {
        let z = z + 719468;
        let era = if z >= 0 { z } else { z - 146096 } / 146097;
        let doe = z - era * 146097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let y = (yoe + era * 400 + if m <= 2 { 1 } else { 0 }) as i32;
        Date { y, m, d }
    }

    pub fn add_days(self, n: i64) -> Date {
        Date::from_days(self.days() + n)
    }

    /// Same day `n` months later; the day is clamped to the month's length (31 Jan + 1 = 28/29 Feb).
    pub fn add_months(self, n: i32) -> Date {
        let total = self.y * 12 + self.m as i32 - 1 + n;
        let y = total.div_euclid(12);
        let m = (total.rem_euclid(12) + 1) as u32;
        Date { y, m, d: self.d.min(days_in_month(y, m)) }
    }

    /// 0 = Monday … 6 = Sunday.
    pub fn weekday(self) -> u32 {
        (self.days() + 3).rem_euclid(7) as u32
    }

    /// ISO 8601 week number.
    pub fn iso_week(self) -> u32 {
        let thursday = self.add_days(3 - self.weekday() as i64);
        let jan1 = Date { y: thursday.y, m: 1, d: 1 };
        ((thursday.days() - jan1.days()) / 7 + 1) as u32
    }

    pub fn parse(s: &str) -> Option<Date> {
        let s = s.trim();
        let b = s.as_bytes();
        if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
            return None;
        }
        Date::new(s[0..4].parse().ok()?, s[5..7].parse().ok()?, s[8..10].parse().ok()?)
    }

    /// Compact iCalendar form `YYYYMMDD`.
    pub fn parse_compact(s: &str) -> Option<Date> {
        if s.len() < 8 || !s.as_bytes()[..8].iter().all(u8::is_ascii_digit) {
            return None;
        }
        Date::new(s[0..4].parse().ok()?, s[4..6].parse().ok()?, s[6..8].parse().ok()?)
    }

    pub fn compact(self) -> String {
        format!("{:04}{:02}{:02}", self.y, self.m, self.d)
    }

    /// Easter Sunday (Gregorian, anonymous algorithm).
    pub fn easter(y: i32) -> Date {
        let a = y % 19;
        let b = y / 100;
        let c = y % 100;
        let d = b / 4;
        let e = b % 4;
        let f = (b + 8) / 25;
        let g = (b - f + 1) / 3;
        let h = (19 * a + b - d - g + 15) % 30;
        let i = c / 4;
        let k = c % 4;
        let l = (32 + 2 * e + 2 * i - h - k) % 7;
        let m = (a + 11 * h + 22 * l) / 451;
        let month = (h + l - 7 * m + 114) / 31;
        let day = (h + l - 7 * m + 114) % 31 + 1;
        Date { y, m: month as u32, d: day as u32 }
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.y, self.m, self.d)
    }
}

impl DateTime {
    pub fn new(date: Date, hour: u32, minute: u32) -> Option<DateTime> {
        (hour < 24 && minute < 60).then_some(DateTime { date, min: hour * 60 + minute })
    }

    /// Minutes since 1970-01-01 00:00 (floating).
    pub fn minutes(self) -> i64 {
        self.date.days() * 1440 + self.min as i64
    }

    pub fn from_minutes(m: i64) -> DateTime {
        DateTime { date: Date::from_days(m.div_euclid(1440)), min: m.rem_euclid(1440) as u32 }
    }

    pub fn add_minutes(self, n: i64) -> DateTime {
        DateTime::from_minutes(self.minutes() + n)
    }

    pub fn hour(self) -> u32 {
        self.min / 60
    }

    pub fn minute(self) -> u32 {
        self.min % 60
    }

    /// `YYYY-MM-DDTHH:MM`; a plain date means midnight.
    pub fn parse(s: &str) -> Option<DateTime> {
        let s = s.trim();
        let date = Date::parse(s.get(..10)?)?;
        if s.len() == 10 {
            return Some(DateTime { date, min: 0 });
        }
        let t = s.get(11..)?;
        let (h, m) = t.split_once(':')?;
        DateTime::new(date, h.parse().ok()?, m.get(..2)?.parse().ok()?)
    }

    /// iCalendar `YYYYMMDDTHHMMSS` (a trailing `Z` is ignored by the caller's choice).
    pub fn parse_compact(s: &str) -> Option<DateTime> {
        let date = Date::parse_compact(s)?;
        let t = s.get(9..)?;
        if s.as_bytes().get(8) != Some(&b'T') || t.len() < 4 {
            return None;
        }
        // `get`: a file may carry any characters here, also ones of several bytes.
        DateTime::new(date, t.get(0..2)?.parse().ok()?, t.get(2..4)?.parse().ok()?)
    }

    pub fn compact(self) -> String {
        format!("{}T{:02}{:02}00", self.date.compact(), self.hour(), self.minute())
    }
}

impl fmt::Display for DateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}T{:02}:{:02}", self.date, self.hour(), self.minute())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn odd_characters_are_no_time() {
        for s in ["20261113Tä000", "20261113T0ä00", "20261113T00ä0", "20261113Tääää", "2026111äT1000", "20261113T😀"]
        {
            assert_eq!(DateTime::parse_compact(s), None, "{s}");
        }
        assert!(DateTime::parse_compact("20261113T1000").is_some());
    }

    #[test]
    fn round_trips_and_weekdays() {
        for z in [-800_000i64, -1, 0, 1, 11_000, 20_769, 2_900_000] {
            assert_eq!(Date::from_days(z).days(), z);
        }
        let d = Date::parse("2026-11-12").unwrap();
        assert_eq!(d.weekday(), 3); // Thursday
        assert_eq!(d.iso_week(), 46);
        assert_eq!(Date::parse("2027-01-01").unwrap().iso_week(), 53);
        assert_eq!(Date::parse("2026-01-31").unwrap().add_months(1).to_string(), "2026-02-28");
        assert_eq!(Date::parse("2028-02-29").unwrap().add_months(12).to_string(), "2029-02-28");
        assert_eq!(Date::easter(2026).to_string(), "2026-04-05");
        assert_eq!(Date::easter(2027).to_string(), "2027-03-28");
        assert!(Date::parse("2026-02-30").is_none());
        let t = DateTime::parse("2026-11-12T23:30").unwrap();
        assert_eq!(t.add_minutes(45).to_string(), "2026-11-13T00:15");
        assert_eq!(DateTime::parse_compact("20261112T093000Z").unwrap().to_string(), "2026-11-12T09:30");
        assert_eq!(t.compact(), "20261112T233000");
    }
}
