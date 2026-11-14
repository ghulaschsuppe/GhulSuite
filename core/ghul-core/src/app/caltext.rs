// SPDX-License-Identifier: AGPL-3.0-or-later
//! GhulCal text formats: iCalendar (.ics) import/export, quick add, public holidays.
//!
//! Everything here works on plain text and the calendar types of [`super::cal`]; nothing touches
//! a vault. Times are floating local wall-clock times (see [`super::caldate`]).

use super::cal::{CalEvent, CalRepeat};
use super::caldate::{days_in_month, is_leap, Date, DateTime};

// =============================================================================================
// iCalendar export
// =============================================================================================

/// Writes the events as an RFC 5545 `VCALENDAR` (CRLF line endings, folded lines).
pub fn ics_export(events: &[CalEvent], calendar_name: &str) -> String {
    let now =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    ics_export_at(events, calendar_name, now)
}

/// [`ics_export`] with a fixed "now" (seconds since the Unix epoch, UTC) for `DTSTAMP`.
pub(crate) fn ics_export_at(events: &[CalEvent], calendar_name: &str, now_secs: i64) -> String {
    let stamp = utc_stamp(now_secs);
    let mut out = String::new();
    let mut line = |s: &str| push_folded(&mut out, s);
    line("BEGIN:VCALENDAR");
    line("VERSION:2.0");
    line("PRODID:-//GhulSuite//GhulCal//EN");
    line("CALSCALE:GREGORIAN");
    if !calendar_name.is_empty() {
        line(&format!("X-WR-CALNAME:{}", ics_escape(calendar_name)));
    }
    for ev in events {
        for l in event_lines(ev, &stamp) {
            line(&l);
        }
    }
    line("END:VCALENDAR");
    out
}

/// The content lines of one `VEVENT` (unfolded); empty if the event has no valid start.
fn event_lines(ev: &CalEvent, stamp: &str) -> Vec<String> {
    let mut l = Vec::new();
    let uid = if ev.id.is_empty() { random_hex(16) } else { ev.id.clone() };
    if ev.all_day {
        let Some(start) = ev.start.get(..10).and_then(Date::parse) else {
            return l;
        };
        let last = ev.end.get(..10).and_then(Date::parse).filter(|d| *d >= start).unwrap_or(start);
        l.push("BEGIN:VEVENT".into());
        l.push(format!("UID:{}@ghulsuite", ics_escape(&uid)));
        l.push(format!("DTSTAMP:{stamp}"));
        l.push(format!("DTSTART;VALUE=DATE:{}", start.compact()));
        l.push(format!("DTEND;VALUE=DATE:{}", last.add_days(1).compact()));
        if let Some(rule) = rrule_text(ev.repeat, &ev.repeat_until, None) {
            l.push(format!("RRULE:{rule}"));
        }
        let ex: Vec<String> = exception_dates(&ev.exceptions).iter().map(|d| d.compact()).collect();
        if ev.repeat != CalRepeat::None && !ex.is_empty() {
            l.push(format!("EXDATE;VALUE=DATE:{}", ex.join(",")));
        }
    } else {
        let Some(start) = DateTime::parse(&ev.start) else {
            return l;
        };
        let end = DateTime::parse(&ev.end).filter(|e| *e >= start).unwrap_or(start.add_minutes(60));
        l.push("BEGIN:VEVENT".into());
        l.push(format!("UID:{}@ghulsuite", ics_escape(&uid)));
        l.push(format!("DTSTAMP:{stamp}"));
        // A series from another time zone is written on the clock of its zone.
        let zone = if ev.zone.is_empty() { String::new() } else { format!(";TZID={}", ev.zone) };
        l.push(format!("DTSTART{zone}:{}", start.compact()));
        l.push(format!("DTEND{zone}:{}", end.compact()));
        if let Some(rule) = rrule_text(ev.repeat, &ev.repeat_until, Some(start)) {
            l.push(format!("RRULE:{rule}"));
        }
        let ex: Vec<String> =
            exception_dates(&ev.exceptions).iter().map(|d| DateTime { date: *d, min: start.min }.compact()).collect();
        if ev.repeat != CalRepeat::None && !ex.is_empty() {
            l.push(format!("EXDATE{zone}:{}", ex.join(",")));
        }
    }
    l.push(format!("SUMMARY:{}", ics_escape(&ev.title)));
    if !ev.location.is_empty() {
        l.push(format!("LOCATION:{}", ics_escape(&ev.location)));
    }
    if !ev.notes.is_empty() {
        l.push(format!("DESCRIPTION:{}", ics_escape(&ev.notes)));
    }
    if ev.birth_year > 0 {
        l.push(format!("X-GHUL-BIRTH-YEAR:{}", ev.birth_year));
    }
    for &r in &ev.reminders {
        l.push("BEGIN:VALARM".into());
        l.push("ACTION:DISPLAY".into());
        l.push(format!("DESCRIPTION:{}", ics_escape(&ev.title)));
        if ev.all_day {
            // "Minutes before 09:00 of the first day" relative to the start at midnight.
            l.push(format!("TRIGGER:{}", fmt_duration(540 - r as i64)));
        } else if r == 0 {
            l.push("TRIGGER:PT0S".into());
        } else {
            l.push(format!("TRIGGER:-PT{r}M"));
        }
        l.push("END:VALARM".into());
    }
    l.push("END:VEVENT".into());
    l
}

/// `RRULE` value without the property name. `timed_start` selects the `UNTIL` form: a date for
/// all-day events, a floating local date-time (end of that day) for timed ones (RFC 5545 wants
/// `UNTIL` to have the same value type as `DTSTART`).
fn rrule_text(repeat: CalRepeat, until: &str, timed_start: Option<DateTime>) -> Option<String> {
    let mut rule = match repeat {
        CalRepeat::None => return None,
        CalRepeat::Daily => "FREQ=DAILY".to_string(),
        CalRepeat::Weekdays => "FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR".to_string(),
        CalRepeat::Weekly => "FREQ=WEEKLY".to_string(),
        CalRepeat::Biweekly => "FREQ=WEEKLY;INTERVAL=2".to_string(),
        CalRepeat::Monthly => "FREQ=MONTHLY".to_string(),
        CalRepeat::Yearly => "FREQ=YEARLY".to_string(),
    };
    if let Some(u) = until.get(..10).and_then(Date::parse) {
        match timed_start {
            Some(_) => rule.push_str(&format!(";UNTIL={}T235959", u.compact())),
            None => rule.push_str(&format!(";UNTIL={}", u.compact())),
        }
    }
    Some(rule)
}

fn exception_dates(ex: &[String]) -> Vec<Date> {
    let mut v: Vec<Date> = ex.iter().filter_map(|s| s.get(..10).and_then(Date::parse)).collect();
    v.sort();
    v.dedup();
    v
}

/// Signed RFC 5545 duration in hours/minutes (`PT9H`, `-PT15H`, `-PT1H30M`, `PT0S`).
fn fmt_duration(minutes: i64) -> String {
    if minutes == 0 {
        return "PT0S".into();
    }
    let sign = if minutes < 0 { "-" } else { "" };
    let m = minutes.unsigned_abs();
    let (h, mm) = (m / 60, m % 60);
    let mut s = format!("{sign}PT");
    if h > 0 {
        s.push_str(&format!("{h}H"));
    }
    if mm > 0 {
        s.push_str(&format!("{mm}M"));
    }
    s
}

/// `YYYYMMDDTHHMMSSZ` for seconds since the epoch.
fn utc_stamp(secs: i64) -> String {
    let day = secs.div_euclid(86_400);
    let s = secs.rem_euclid(86_400);
    format!("{}T{:02}{:02}{:02}Z", Date::from_days(day).compact(), s / 3600, s / 60 % 60, s % 60)
}

fn random_hex(bytes: usize) -> String {
    use rand_core::{OsRng, RngCore};
    let mut b = vec![0u8; bytes];
    OsRng.fill_bytes(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Escapes a TEXT value (`\`, `;`, `,`, newlines).
pub(crate) fn ics_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.push_str("\\\\"),
            ';' => out.push_str("\\;"),
            ',' => out.push_str("\\,"),
            '\r' => {
                if chars.peek() != Some(&'\n') {
                    out.push_str("\\n");
                }
            }
            '\n' => out.push_str("\\n"),
            _ => out.push(c),
        }
    }
    out
}

/// Reverses [`ics_escape`]; unknown escapes keep the escaped character.
pub(crate) fn ics_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') | Some('N') => out.push('\n'),
            Some(o) => out.push(o),
            None => out.push('\\'),
        }
    }
    out
}

/// Appends one content line folded at 75 octets (never inside a UTF-8 character) plus CRLF.
fn push_folded(out: &mut String, line: &str) {
    let mut count = 0;
    for ch in line.chars() {
        let len = ch.len_utf8();
        if count + len > 75 {
            out.push_str("\r\n ");
            count = 1;
        }
        out.push(ch);
        count += len;
    }
    out.push_str("\r\n");
}

// =============================================================================================
// iCalendar import
// =============================================================================================

/// One content line: `NAME;PARAM=VALUE:value`.
#[derive(Clone, Debug, Default)]
struct Prop {
    name: String,
    params: Vec<(String, String)>,
    value: String,
}

impl Prop {
    fn param(&self, key: &str) -> Option<&str> {
        self.params.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
}

fn parse_prop(line: &str) -> Option<Prop> {
    let mut in_quotes = false;
    let mut colon = None;
    for (i, c) in line.char_indices() {
        match c {
            '"' => in_quotes = !in_quotes,
            ':' if !in_quotes => {
                colon = Some(i);
                break;
            }
            _ => {}
        }
    }
    let colon = colon?;
    let head = &line[..colon];
    let value = line[colon + 1..].to_string();
    let mut parts = Vec::new();
    let mut cur = String::new();
    in_quotes = false;
    for c in head.chars() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                cur.push(c);
            }
            ';' if !in_quotes => parts.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    parts.push(cur);
    let name = parts[0].trim().to_ascii_uppercase();
    if name.is_empty() {
        return None;
    }
    let params = parts[1..]
        .iter()
        .filter_map(|p| {
            let (k, v) = p.split_once('=')?;
            Some((k.trim().to_ascii_uppercase(), v.trim().trim_matches('"').to_string()))
        })
        .collect();
    Some(Prop { name, params, value })
}

/// Splits into lines (CRLF or LF) and joins folded continuation lines.
fn unfold(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        if raw.starts_with(' ') || raw.starts_with('\t') {
            if let Some(prev) = lines.last_mut() {
                prev.push_str(&raw[1..]);
                continue;
            }
        }
        if !raw.trim().is_empty() {
            lines.push(raw.to_string());
        }
    }
    lines
}

#[derive(Default)]
struct RawEvent {
    uid: String,
    summary: String,
    location: String,
    description: String,
    dtstart: Option<Prop>,
    dtend: Option<Prop>,
    duration: Option<String>,
    rrule: Option<String>,
    exdates: Vec<Prop>,
    alarms: Vec<Option<Prop>>,
    birth_year: u32,
    recurrence_id: Option<Prop>,
    cancelled: bool,
}

/// A parsed `DTSTART`/`DTEND`/… value as local wall-clock time.
#[derive(Clone, Copy, Debug)]
struct IcsTime {
    dt: DateTime,
    is_date: bool,
}

/// Parses one date or date-time value as local wall-clock time of this device: `Z` (UTC) and
/// times with a `TZID` are converted with the offsets valid on that day; a time without either
/// (or with a zone nobody knows) is taken as it stands.
fn parse_ics_time(value: &str, value_param: Option<&str>, tzid: Option<&str>, z: &Zones) -> Option<IcsTime> {
    let v = value.trim();
    if !v.is_ascii() || v.len() < 8 {
        return None;
    }
    let date_only = value_param.is_some_and(|p| p.eq_ignore_ascii_case("DATE"))
        || (v.len() == 8 && v.bytes().all(|b| b.is_ascii_digit()));
    if date_only || v.as_bytes().get(8) != Some(&b'T') {
        let date = Date::parse_compact(v)?;
        return Some(IcsTime { dt: DateTime { date, min: 0 }, is_date: true });
    }
    let mut dt = DateTime::parse_compact(v)?;
    if v.ends_with('Z') || v.ends_with('z') {
        dt = z.utc_to_device(dt);
    } else if let Some(id) = tzid {
        dt = z.zone_to_device(id, dt);
    }
    Some(IcsTime { dt, is_date: false })
}

fn prop_time(p: &Prop, z: &Zones) -> Option<IcsTime> {
    parse_ics_time(&p.value, p.param("VALUE"), p.param("TZID"), z)
}

// ---------------------------------------------------------------------------------------------
// Time zones
// ---------------------------------------------------------------------------------------------

/// The time zone database of the device, provided by the app, the core carries none of its own.
#[uniffi::export(with_foreign)]
pub trait TimeZones: Send + Sync {
    /// UTC offset in minutes of `zone` (IANA name; empty: the zone of this device) at the UTC
    /// instant `utc_minutes` (minutes since 1970-01-01T00:00Z). `None` if the zone is unknown.
    fn offset_at_utc(&self, zone: String, utc_minutes: i64) -> Option<i32>;
    /// The same for a wall-clock time in that zone.
    fn offset_at_local(&self, zone: String, local_minutes: i64) -> Option<i32>;
}

/// A device that is always at the same offset and knows no other zone.
pub(crate) struct FixedOffset(pub i32);

impl TimeZones for FixedOffset {
    fn offset_at_utc(&self, zone: String, _: i64) -> Option<i32> {
        zone.is_empty().then_some(self.0)
    }
    fn offset_at_local(&self, zone: String, _: i64) -> Option<i32> {
        zone.is_empty().then_some(self.0)
    }
}

const MAX_OFFSET: i32 = 24 * 60;
// A file full of time zone rules must not make the import slow: the full history of a real zone
// (tzurl.org) has up to about 150 blocks and 300 single dates.
const MAX_FILE_ZONES: usize = 64;
const MAX_OBSERVANCES: usize = 256;
const MAX_SINGLE_ONSETS: usize = 1024;

/// On which day of a year an observance (standard or daylight time) begins.
#[derive(Clone, Debug)]
enum Onset {
    /// The `nth` weekday (0 = Monday) of the month; negative counts from the end.
    Weekday {
        month: u32,
        nth: i32,
        weekday: u32,
    },
    Day {
        month: u32,
        day: u32,
    },
    /// The weekday that falls on one of these days of the month ("Sunday on or after the 8th");
    /// negative days count from the end of the month.
    WeekdayAmong {
        month: u32,
        days: Vec<i32>,
        weekday: u32,
    },
    /// The same with days of the year.
    WeekdayAmongYearDays {
        days: Vec<i32>,
        weekday: u32,
    },
}

impl Onset {
    fn in_year(&self, y: i32) -> Option<Date> {
        match self {
            Onset::Day { month, day } => Date::new(y, *month, *day),
            Onset::Weekday { month, nth, weekday } => nth_weekday(y, *month, *nth, *weekday),
            Onset::WeekdayAmong { month, days, weekday } => {
                if !(1..=12).contains(month) {
                    return None;
                }
                let len = days_in_month(y, *month) as i32;
                days.iter()
                    .filter_map(|d| Date::new(y, *month, u32::try_from(if *d > 0 { *d } else { len + 1 + *d }).ok()?))
                    .find(|d| d.weekday() == *weekday)
            }
            Onset::WeekdayAmongYearDays { days, weekday } => {
                let first = Date::new(y, 1, 1)?;
                let len = if is_leap(y) { 366 } else { 365 };
                days.iter()
                    .filter(|d| (1..=len).contains(&d.abs()))
                    .map(|d| first.add_days(if *d > 0 { *d as i64 - 1 } else { (len + *d) as i64 }))
                    .find(|d| d.weekday() == *weekday)
            }
        }
    }
}

/// `STANDARD` or `DAYLIGHT` inside a `VTIMEZONE`.
#[derive(Clone, Debug)]
struct Observance {
    /// First onset, wall-clock time.
    start: DateTime,
    from: i32,
    to: i32,
    yearly: Option<Onset>,
    /// Last onset of the yearly rule, as minutes since 1970 in UTC.
    until_utc: Option<i64>,
    extra: Vec<DateTime>,
}

impl Observance {
    /// The latest onset at or before `local` (minutes since 1970).
    fn onset_before(&self, local: i64) -> Option<i64> {
        let first = self.start.minutes();
        let mut best = (first <= local).then_some(first);
        let mut take = |m: i64| {
            if m >= first && m <= local && best.map_or(true, |b| m > b) {
                best = Some(m);
            }
        };
        for x in &self.extra {
            take(x.minutes());
        }
        if let Some(rule) = &self.yearly {
            // The last onset before `local` is in that year or the one before, or, if the rule
            // has ended long ago, around its last year.
            let year = DateTime::from_minutes(local).date.y;
            let last = self.until_utc.map_or(year, |u| DateTime::from_minutes(u).date.y.min(year));
            for y in [last - 1, last, last + 1, year - 1, year] {
                if let Some(d) = rule.in_year(y) {
                    let at = DateTime { date: d, min: self.start.min }.minutes();
                    // The clock shows the old offset until the onset.
                    if self.until_utc.map_or(true, |u| at - self.from as i64 <= u) {
                        take(at);
                    }
                }
            }
        }
        best
    }
}

fn nth_weekday(y: i32, month: u32, nth: i32, weekday: u32) -> Option<Date> {
    if !(1..=12).contains(&month) || nth == 0 || nth.abs() > 5 || weekday > 6 {
        return None;
    }
    let len = days_in_month(y, month);
    let day = if nth > 0 {
        let first = Date::new(y, month, 1)?;
        1 + (weekday + 7 - first.weekday()) % 7 + (nth as u32 - 1) * 7
    } else {
        let last = Date::new(y, month, len)?;
        let back = (last.weekday() + 7 - weekday) % 7 + (nth.unsigned_abs() - 1) * 7;
        if back >= len {
            return None;
        }
        len - back
    };
    Date::new(y, month, day)
}

/// A `VTIMEZONE` of the file: used for zone names the device does not know (Windows names of
/// Outlook, made-up names).
#[derive(Clone, Debug, Default)]
struct FileZone {
    id: String,
    observances: Vec<Observance>,
}

impl FileZone {
    fn offset_at_local(&self, local: i64) -> Option<i32> {
        let current = self
            .observances
            .iter()
            .filter_map(|o| o.onset_before(local).map(|at| (at, o.to)))
            .max_by_key(|(at, _)| *at)
            .map(|(_, to)| to);
        // Before the first onset the zone was at the offset it then left.
        current.or_else(|| self.observances.iter().min_by_key(|o| o.start.minutes()).map(|o| o.from))
    }
}

/// `+0100`, `-0530`, `+013000` in minutes.
fn parse_utc_offset(s: &str) -> Option<i32> {
    let s = s.trim();
    let (sign, digits) = match s.as_bytes().first()? {
        b'+' => (1, &s[1..]),
        b'-' => (-1, &s[1..]),
        _ => return None,
    };
    if digits.len() < 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (h, m): (i32, i32) = (digits[0..2].parse().ok()?, digits[2..4].parse().ok()?);
    (h <= 23 && m <= 59).then_some(sign * (h * 60 + m))
}

/// What is collected of a `STANDARD`/`DAYLIGHT` block before it is complete.
#[derive(Default)]
struct RawObservance {
    start: Option<String>,
    from: Option<i32>,
    to: Option<i32>,
    rrule: Option<String>,
    rdates: Vec<String>,
}

impl RawObservance {
    fn finish(self) -> Option<Observance> {
        let start = DateTime::parse_compact(self.start?.trim())?;
        let to = self.to?;
        let from = self.from.unwrap_or(to);
        let mut o = Observance { start, from, to, yearly: None, until_utc: None, extra: Vec::new() };
        o.extra = self.rdates.iter().filter_map(|v| DateTime::parse_compact(v.trim())).collect();
        if let Some(rule) = self.rrule {
            let list = |v: &str| -> Option<Vec<i32>> { v.split(',').take(31).map(|d| d.trim().parse().ok()).collect() };
            let (mut yearly, mut month) = (false, start.date.m);
            let (mut byday, mut month_days, mut year_days) = (None, None, None);
            for part in rule.split(';') {
                let Some((k, v)) = part.split_once('=') else { continue };
                let v = v.trim().to_ascii_uppercase();
                match k.trim().to_ascii_uppercase().as_str() {
                    "FREQ" => yearly = v == "YEARLY",
                    "BYMONTH" => month = v.parse().ok()?,
                    "BYMONTHDAY" => month_days = Some(list(&v)?),
                    "BYYEARDAY" => year_days = Some(list(&v)?),
                    "BYDAY" if v.is_ascii() && v.len() >= 2 => {
                        let (n, wd) = v.split_at(v.len() - 2);
                        let weekday = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"].iter().position(|d| *d == wd)?;
                        let nth: Option<i32> = if n.is_empty() { None } else { Some(n.parse().ok()?) };
                        byday = Some((nth, weekday as u32));
                    }
                    // In UTC ("…Z"); a plain date counts to the end of that day.
                    "UNTIL" => {
                        o.until_utc = match DateTime::parse_compact(&v) {
                            Some(dt) if v.ends_with('Z') => Some(dt.minutes()),
                            Some(dt) => Some(dt.minutes() - from as i64),
                            None => Date::parse_compact(&v).map(|d| d.days() * 1440 + 1439 + MAX_OFFSET as i64),
                        }
                    }
                    _ => {}
                }
            }
            if yearly {
                o.yearly = Some(match (byday, month_days, year_days) {
                    (Some((None, weekday)), Some(days), _) => Onset::WeekdayAmong { month, days, weekday },
                    (Some((None, weekday)), None, Some(days)) => Onset::WeekdayAmongYearDays { days, weekday },
                    (Some((nth, weekday)), _, _) => Onset::Weekday { month, nth: nth.unwrap_or(1), weekday },
                    (None, Some(days), _) => Onset::Day { month, day: u32::try_from(*days.first()?).ok()? },
                    (None, None, _) => Onset::Day { month, day: start.date.d },
                });
            }
        }
        Some(o)
    }
}

/// A wall-clock time in `zone` as wall-clock time of this device; `None` if the device does not
/// know the zone.
pub(crate) fn wall_to_device(device: &dyn TimeZones, zone: &str, local: DateTime) -> Option<DateTime> {
    let m = local.minutes();
    let utc = m - device.offset_at_local(zone.to_owned(), m)?.clamp(-MAX_OFFSET, MAX_OFFSET) as i64;
    let own = device.offset_at_utc(String::new(), utc)?.clamp(-MAX_OFFSET, MAX_OFFSET);
    Some(DateTime::from_minutes(utc + own as i64))
}

/// The names a `TZID` may stand for: itself, and without the prefix some programs put before it
/// ("/mozilla.org/20050126_1/Europe/Berlin").
fn zone_names(tzid: &str) -> Vec<String> {
    let tzid = tzid.trim().trim_matches('"');
    if tzid.is_empty() || tzid.len() > 100 {
        return Vec::new();
    }
    let parts: Vec<&str> = tzid.split('/').filter(|p| !p.is_empty()).collect();
    let mut names = vec![tzid.to_owned()];
    for n in [3, 2] {
        if tzid.starts_with('/') && parts.len() > n {
            names.push(parts[parts.len() - n..].join("/"));
        }
    }
    names
}

/// Windows names of time zones (Outlook, Exchange) and the zone of the time zone database they
/// stand for. Only used when the rules sent along in the file agree with that zone.
const WINDOWS_ZONES: &[(&str, &str)] = &[
    ("W. Europe Standard Time", "Europe/Berlin"),
    ("Central Europe Standard Time", "Europe/Budapest"),
    ("Central European Standard Time", "Europe/Warsaw"),
    ("Romance Standard Time", "Europe/Paris"),
    ("GMT Standard Time", "Europe/London"),
    ("Greenwich Standard Time", "Atlantic/Reykjavik"),
    ("FLE Standard Time", "Europe/Helsinki"),
    ("GTB Standard Time", "Europe/Bucharest"),
    ("E. Europe Standard Time", "Europe/Chisinau"),
    ("Russian Standard Time", "Europe/Moscow"),
    ("Turkey Standard Time", "Europe/Istanbul"),
    ("Israel Standard Time", "Asia/Jerusalem"),
    ("Egypt Standard Time", "Africa/Cairo"),
    ("South Africa Standard Time", "Africa/Johannesburg"),
    ("W. Central Africa Standard Time", "Africa/Lagos"),
    ("E. Africa Standard Time", "Africa/Nairobi"),
    ("Arab Standard Time", "Asia/Riyadh"),
    ("Arabian Standard Time", "Asia/Dubai"),
    ("Iran Standard Time", "Asia/Tehran"),
    ("Pakistan Standard Time", "Asia/Karachi"),
    ("India Standard Time", "Asia/Kolkata"),
    ("SE Asia Standard Time", "Asia/Bangkok"),
    ("China Standard Time", "Asia/Shanghai"),
    ("Singapore Standard Time", "Asia/Singapore"),
    ("Taipei Standard Time", "Asia/Taipei"),
    ("Tokyo Standard Time", "Asia/Tokyo"),
    ("Korea Standard Time", "Asia/Seoul"),
    ("W. Australia Standard Time", "Australia/Perth"),
    ("Cen. Australia Standard Time", "Australia/Adelaide"),
    ("E. Australia Standard Time", "Australia/Brisbane"),
    ("AUS Eastern Standard Time", "Australia/Sydney"),
    ("New Zealand Standard Time", "Pacific/Auckland"),
    ("Hawaiian Standard Time", "Pacific/Honolulu"),
    ("Alaskan Standard Time", "America/Anchorage"),
    ("Pacific Standard Time", "America/Los_Angeles"),
    ("US Mountain Standard Time", "America/Phoenix"),
    ("Mountain Standard Time", "America/Denver"),
    ("Central Standard Time", "America/Chicago"),
    ("Central Standard Time (Mexico)", "America/Mexico_City"),
    ("Canada Central Standard Time", "America/Regina"),
    ("Eastern Standard Time", "America/New_York"),
    ("SA Pacific Standard Time", "America/Bogota"),
    ("Atlantic Standard Time", "America/Halifax"),
    ("Newfoundland Standard Time", "America/St_Johns"),
    ("E. South America Standard Time", "America/Sao_Paulo"),
    ("Argentina Standard Time", "America/Argentina/Buenos_Aires"),
];

/// Converts the times of a file into wall-clock time of this device.
struct Zones<'a> {
    device: &'a dyn TimeZones,
    file: Vec<FileZone>,
}

impl Zones<'_> {
    fn utc_to_device(&self, utc: DateTime) -> DateTime {
        let m = utc.minutes();
        let off = self.device.offset_at_utc(String::new(), m).unwrap_or(0).clamp(-MAX_OFFSET, MAX_OFFSET);
        DateTime::from_minutes(m + off as i64)
    }

    /// A wall-clock time in the zone `tzid`. An unknown zone leaves the time as it is.
    fn zone_to_device(&self, tzid: &str, local: DateTime) -> DateTime {
        let m = local.minutes();
        let known = zone_names(tzid).into_iter().find_map(|n| self.device.offset_at_local(n, m));
        let off = known.or_else(|| self.file_offset(tzid, m));
        match off {
            Some(o) => self.utc_to_device(DateTime::from_minutes(m - o.clamp(-MAX_OFFSET, MAX_OFFSET) as i64)),
            None => local,
        }
    }
}

/// Signed RFC 5545 duration (`-PT15M`, `P1D`, `-P1W`, `PT1H30M`) in minutes; seconds are cut.
pub(crate) fn parse_duration(s: &str) -> Option<i64> {
    const CAP: i64 = 20 * 366 * 1440 * 60; // seconds, keeps all date arithmetic sane
    let s = s.trim();
    let (neg, rest) = match s.as_bytes().first()? {
        b'-' => (true, &s[1..]),
        b'+' => (false, &s[1..]),
        _ => (false, s),
    };
    let rest = rest.strip_prefix('P').or_else(|| rest.strip_prefix('p'))?;
    let mut secs: i64 = 0;
    let mut num: Option<i64> = None;
    let mut time_part = false;
    let mut any = false;
    for c in rest.chars() {
        match c.to_ascii_uppercase() {
            d @ '0'..='9' => {
                let n = num.unwrap_or(0).checked_mul(10)?.checked_add((d as u8 - b'0') as i64)?;
                if n > CAP {
                    return None;
                }
                num = Some(n);
            }
            'T' if num.is_none() => time_part = true,
            u => {
                let n = num.take()?;
                let mult = match (u, time_part) {
                    ('W', false) => 7 * 86_400,
                    ('D', false) => 86_400,
                    ('H', true) => 3600,
                    ('M', true) => 60,
                    ('S', true) => 1,
                    _ => return None,
                };
                secs = secs.checked_add(n.checked_mul(mult)?)?;
                if secs > CAP {
                    return None;
                }
                any = true;
            }
        }
    }
    if !any || num.is_some() {
        return None;
    }
    let min = secs / 60;
    Some(if neg { -min } else { min })
}

/// Reads an `.ics` file on a device that is always `utc_offset_minutes` ahead of UTC.
pub fn ics_import(text: &str, utc_offset_minutes: i32) -> Result<Vec<CalEvent>, String> {
    ics_import_zoned(text, &FixedOffset(utc_offset_minutes))
}

/// Reads an `.ics` file; all times become wall-clock time of this device (see [`TimeZones`]).
/// Ids and calendar ids of the result are empty: the caller assigns them.
pub fn ics_import_zoned(text: &str, device: &dyn TimeZones) -> Result<Vec<CalEvent>, String> {
    let mut zones = Zones { device, file: Vec::new() };
    let mut zone: Option<FileZone> = None;
    let mut observance: Option<RawObservance> = None;
    let mut seen_calendar = false;
    let mut stack: Vec<String> = Vec::new();
    let mut cur: Option<RawEvent> = None;
    let mut alarm: Option<Option<Prop>> = None;
    let mut raws: Vec<RawEvent> = Vec::new();

    for line in unfold(text) {
        let Some(p) = parse_prop(&line) else {
            continue;
        };
        match p.name.as_str() {
            "BEGIN" => {
                let comp = p.value.trim().to_ascii_uppercase();
                match comp.as_str() {
                    "VCALENDAR" => seen_calendar = true,
                    "VEVENT" if seen_calendar && cur.is_none() => cur = Some(RawEvent::default()),
                    "VALARM" if stack.last().map(String::as_str) == Some("VEVENT") && cur.is_some() => {
                        alarm = Some(None)
                    }
                    "VTIMEZONE" if cur.is_none() => zone = Some(FileZone::default()),
                    "STANDARD" | "DAYLIGHT" if zone.is_some() => observance = Some(RawObservance::default()),
                    _ => {}
                }
                stack.push(comp);
            }
            "END" => {
                let comp = p.value.trim().to_ascii_uppercase();
                if let Some(pos) = stack.iter().rposition(|c| *c == comp) {
                    stack.truncate(pos);
                }
                match comp.as_str() {
                    "VALARM" => {
                        if let (Some(ev), Some(a)) = (cur.as_mut(), alarm.take()) {
                            ev.alarms.push(a);
                        }
                    }
                    "VEVENT" => {
                        alarm = None;
                        if let Some(ev) = cur.take() {
                            raws.push(ev);
                        }
                    }
                    "STANDARD" | "DAYLIGHT" => {
                        if let (Some(z), Some(o)) = (zone.as_mut(), observance.take().and_then(RawObservance::finish)) {
                            let singles: usize = z.observances.iter().map(|x| x.extra.len()).sum();
                            if z.observances.len() < MAX_OBSERVANCES && singles + o.extra.len() <= MAX_SINGLE_ONSETS {
                                z.observances.push(o);
                            }
                        }
                    }
                    "VTIMEZONE" => {
                        observance = None;
                        if let Some(z) = zone.take() {
                            if !z.id.is_empty() && !z.observances.is_empty() && zones.file.len() < MAX_FILE_ZONES {
                                zones.file.push(z);
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => match (stack.last().map(String::as_str), cur.as_mut()) {
                (Some("VEVENT"), Some(ev)) => event_prop(ev, p),
                (Some("VALARM"), Some(_)) => {
                    if let Some(a) = alarm.as_mut() {
                        if p.name == "TRIGGER" {
                            *a = Some(p);
                        }
                    }
                }
                (Some("VTIMEZONE"), None) => {
                    if let (Some(z), "TZID") = (zone.as_mut(), p.name.as_str()) {
                        z.id = p.value.trim().to_string();
                    }
                }
                (Some("STANDARD" | "DAYLIGHT"), None) => {
                    if let Some(o) = observance.as_mut() {
                        match p.name.as_str() {
                            "DTSTART" => o.start = Some(p.value),
                            "TZOFFSETFROM" => o.from = parse_utc_offset(&p.value),
                            "TZOFFSETTO" => o.to = parse_utc_offset(&p.value),
                            "RRULE" => o.rrule = Some(p.value),
                            "RDATE" if o.rdates.len() < MAX_SINGLE_ONSETS => {
                                o.rdates.extend(p.value.split(',').take(MAX_SINGLE_ONSETS).map(str::to_owned))
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            },
        }
    }
    if !seen_calendar {
        return Err("Not an iCalendar file (no VCALENDAR)".into());
    }

    // Master events first, then moved/cancelled single occurrences (RECURRENCE-ID).
    let mut out: Vec<(String, CalEvent)> = Vec::new();
    let mut overrides: Vec<(String, Option<Date>, Option<CalEvent>)> = Vec::new();
    for raw in raws {
        let rid = raw.recurrence_id.as_ref().and_then(|p| prop_time(p, &zones)).map(|t| t.dt.date);
        let is_override = raw.recurrence_id.is_some();
        let cancelled = raw.cancelled;
        let uid = raw.uid.clone();
        let ev = finish_event(raw, &zones);
        if is_override {
            overrides.push((uid, rid, if cancelled { None } else { ev }));
        } else if let Some(ev) = ev {
            if !cancelled {
                out.push((uid, ev));
            }
        }
    }
    for (uid, rid, ev) in overrides {
        if let Some(date) = rid {
            if let Some((_, master)) =
                out.iter_mut().find(|(u, m)| !u.is_empty() && *u == uid && m.repeat != CalRepeat::None)
            {
                let d = date.to_string();
                if !master.exceptions.contains(&d) {
                    master.exceptions.push(d);
                    master.exceptions.sort();
                }
            }
        }
        if let Some(ev) = ev {
            out.push((uid, ev));
        }
    }
    if out.is_empty() {
        return Err("No events found".into());
    }
    Ok(out.into_iter().map(|(_, e)| e).collect())
}

fn event_prop(ev: &mut RawEvent, p: Prop) {
    match p.name.as_str() {
        "UID" => ev.uid = p.value.trim().to_string(),
        "SUMMARY" => ev.summary = ics_unescape(&p.value),
        "LOCATION" => ev.location = ics_unescape(&p.value),
        "DESCRIPTION" => ev.description = ics_unescape(&p.value),
        "DTSTART" => ev.dtstart = Some(p),
        "DTEND" => ev.dtend = Some(p),
        "DURATION" => ev.duration = Some(p.value),
        "RRULE" => ev.rrule = Some(p.value),
        "EXDATE" => ev.exdates.push(p),
        "RECURRENCE-ID" => ev.recurrence_id = Some(p),
        "STATUS" => ev.cancelled = p.value.trim().eq_ignore_ascii_case("CANCELLED"),
        "X-GHUL-BIRTH-YEAR" => {
            ev.birth_year = p.value.trim().parse().ok().filter(|y| (1..=9999).contains(y)).unwrap_or(0)
        }
        _ => {}
    }
}

impl Zones<'_> {
    /// Offset of a zone by the rules the file carries for it.
    fn file_offset(&self, tzid: &str, local: i64) -> Option<i32> {
        let tzid = tzid.trim().trim_matches('"');
        self.file.iter().find(|z| z.id.eq_ignore_ascii_case(tzid)).and_then(|z| z.offset_at_local(local))
    }

    /// The zone of the device's database that `tzid` names: directly, or as a Windows name whose
    /// rules in the file agree with that zone around `start`.
    fn known_name(&self, tzid: &str, start: DateTime) -> Option<String> {
        let m = start.minutes();
        if let Some(n) = zone_names(tzid).into_iter().find(|n| self.device.offset_at_local(n.clone(), m).is_some()) {
            return Some(n);
        }
        let id = tzid.trim().trim_matches('"');
        let name = WINDOWS_ZONES.iter().find(|(w, _)| w.eq_ignore_ascii_case(id))?.1;
        let agrees = [0, 91, 182, 273].iter().all(|days| {
            let at = m + days * 1440;
            let by_device = self.device.offset_at_local(name.to_owned(), at);
            by_device.is_some() && by_device == self.file_offset(id, at)
        });
        agrees.then(|| name.to_owned())
    }

    /// Whether `zone` and the zone of this device show the same time all through the year after
    /// `start` (then the series needs no zone of its own).
    fn like_device(&self, zone: &str, start: DateTime) -> bool {
        (0..9).all(|i| {
            let at = start.minutes() + i * 45 * 1440;
            self.device.offset_at_local(zone.to_owned(), at) == self.device.offset_at_local(String::new(), at)
        })
    }

    fn utc_to_zone(&self, zone: &str, utc: DateTime) -> Option<DateTime> {
        let off = self.device.offset_at_utc(zone.to_owned(), utc.minutes())?.clamp(-MAX_OFFSET, MAX_OFFSET);
        Some(utc.add_minutes(off as i64))
    }

    /// A series whose start names a time zone other than the device's: (zone, start, end) on the
    /// clock of that zone. Converting only its first date would put the later ones an hour off
    /// whenever the two zones change their clocks on different days, or only one of them does.
    fn series_zone(&self, raw: &RawEvent) -> Option<(String, DateTime, DateTime)> {
        raw.rrule.as_ref()?;
        let p = raw.dtstart.as_ref()?;
        let id = p.param("TZID")?;
        let v = p.value.trim();
        if !v.is_ascii() || v.ends_with(['Z', 'z']) || p.param("VALUE").is_some_and(|x| x.eq_ignore_ascii_case("DATE"))
        {
            return None;
        }
        let start = DateTime::parse_compact(v)?;
        let name = self.known_name(id, start)?;
        if self.like_device(&name, start) {
            return None;
        }
        let same_zone = |q: &Prop| {
            q.param("TZID")
                .is_some_and(|x| x.trim().trim_matches('"').eq_ignore_ascii_case(id.trim().trim_matches('"')))
        };
        let end = match (&raw.dtend, raw.duration.as_deref().and_then(parse_duration)) {
            (Some(e), _) if same_zone(e) && e.value.is_ascii() => DateTime::parse_compact(e.value.trim())?,
            (Some(_), _) => return None,
            (None, Some(d)) => start.add_minutes(d),
            (None, None) => start.add_minutes(60),
        };
        Some((name, start, if end > start { end } else { start.add_minutes(60) }))
    }

    /// The day of an `EXDATE` or `UNTIL` value in the zone of the series.
    fn day_in_zone(&self, zone: &str, value: &str) -> Option<Date> {
        let v = value.trim();
        if !v.is_ascii() {
            return None;
        }
        match DateTime::parse_compact(v) {
            Some(dt) if v.ends_with(['Z', 'z']) => Some(self.utc_to_zone(zone, dt)?.date),
            Some(dt) => Some(dt.date),
            None => Date::parse_compact(v),
        }
    }
}

fn finish_event(raw: RawEvent, z: &Zones) -> Option<CalEvent> {
    let st = prop_time(raw.dtstart.as_ref()?, z)?;
    let end = raw.dtend.as_ref().and_then(|p| prop_time(p, z));
    let dur = raw.duration.as_deref().and_then(parse_duration);
    let mut ev = CalEvent {
        title: raw.summary.trim().to_string(),
        location: raw.location.trim().to_string(),
        notes: raw.description.trim().to_string(),
        birth_year: raw.birth_year,
        ..CalEvent::default()
    };
    let start_date = st.dt.date;
    if st.is_date {
        ev.all_day = true;
        let last = match (end, dur) {
            // DTEND is exclusive; a date-time end at a later time still covers its own day.
            (Some(e), _) if e.is_date || e.dt.min == 0 => e.dt.date.add_days(-1),
            (Some(e), _) => e.dt.date,
            (None, Some(d)) if d >= 1440 => start_date.add_days(d / 1440 - 1),
            _ => start_date,
        };
        ev.start = start_date.to_string();
        ev.end = last.max(start_date).to_string();
    } else {
        ev.start = st.dt.to_string();
        let e = match (end, dur) {
            (Some(e), _) => e.dt,
            (None, Some(d)) => st.dt.add_minutes(d),
            _ => st.dt.add_minutes(60),
        };
        let e = if e < st.dt { st.dt.add_minutes(60) } else { e };
        ev.end = e.to_string();
    }
    if let Some(rule) = raw.rrule.as_deref() {
        let (rep, until) = map_rrule(rule, start_date, z);
        ev.repeat = rep;
        if rep != CalRepeat::None {
            ev.repeat_until = until.map(|d| d.to_string()).unwrap_or_default();
        }
    }
    if ev.repeat != CalRepeat::None {
        let mut ex: Vec<String> = raw
            .exdates
            .iter()
            .flat_map(|p| {
                p.value
                    .split(',')
                    .filter_map(|v| parse_ics_time(v, p.param("VALUE"), p.param("TZID"), z))
                    .map(|t| t.dt.date.to_string())
                    .collect::<Vec<_>>()
            })
            .collect();
        ex.sort();
        ex.dedup();
        ev.exceptions = ex;
    }
    let start_min = st.dt.minutes();
    let mut rem: Vec<u32> = raw
        .alarms
        .iter()
        .filter_map(|a| {
            let t = a.as_ref()?;
            let rel = if t.param("VALUE").is_some_and(|v| v.eq_ignore_ascii_case("DATE-TIME")) {
                prop_time(t, z)?.dt.minutes() - start_min
            } else {
                parse_duration(&t.value)?
            };
            let before = if ev.all_day { 540 - rel } else { -rel };
            Some(before.clamp(0, u32::MAX as i64) as u32)
        })
        .filter(|before| *before <= super::cal::MAX_REMINDER_MINUTES)
        .collect();
    rem.sort_unstable_by(|a, b| b.cmp(a));
    rem.dedup();
    ev.reminders = rem;
    if let Some((zone, start, end)) = z.series_zone(&raw).filter(|_| ev.repeat != CalRepeat::None && !ev.all_day) {
        ev.start = start.to_string();
        ev.end = end.to_string();
        let rule = raw.rrule.as_deref().unwrap_or("");
        // The last day: counted from the start in the zone, or the day `UNTIL` has there.
        let until_value =
            rule.split(';').filter_map(|p| p.split_once('=')).find(|(k, _)| k.trim().eq_ignore_ascii_case("UNTIL"));
        let until = match until_value {
            Some((_, v)) => z.day_in_zone(&zone, v),
            None => map_rrule(rule, start.date, z).1,
        };
        ev.repeat_until = until.map(|d| d.to_string()).unwrap_or_default();
        let mut ex: Vec<String> = raw
            .exdates
            .iter()
            .flat_map(|p| {
                p.value.split(',').filter_map(|v| z.day_in_zone(&zone, v)).map(|d| d.to_string()).collect::<Vec<_>>()
            })
            .collect();
        ex.sort();
        ex.dedup();
        ev.exceptions = ex;
        ev.zone = zone;
    }
    Some(ev)
}

/// Maps the supported `RRULE` subset onto [`CalRepeat`]; returns the last day if known.
fn map_rrule(rule: &str, start: Date, z: &Zones) -> (CalRepeat, Option<Date>) {
    let mut freq = "";
    let mut interval = 1u32;
    let mut byday: Vec<String> = Vec::new();
    let mut until: Option<Date> = None;
    let mut count: Option<u32> = None;
    for part in rule.split(';') {
        let Some((k, v)) = part.split_once('=') else {
            continue;
        };
        let v = v.trim();
        match k.trim().to_ascii_uppercase().as_str() {
            "FREQ" => freq = v,
            "INTERVAL" => interval = v.parse().unwrap_or(1),
            "BYDAY" => byday = v.split(',').map(|d| d.trim().to_ascii_uppercase()).collect(),
            "UNTIL" => until = parse_ics_time(v, None, None, z).map(|t| t.dt.date),
            "COUNT" => count = v.parse().ok().filter(|n| (1..=10_000).contains(n)),
            _ => {}
        }
    }
    let workdays = {
        let mut d = byday.clone();
        d.sort();
        d == ["FR", "MO", "TH", "TU", "WE"]
    };
    let rep = match freq.to_ascii_uppercase().as_str() {
        "DAILY" if workdays => CalRepeat::Weekdays,
        "DAILY" => CalRepeat::Daily,
        "WEEKLY" if workdays && interval == 1 => CalRepeat::Weekdays,
        "WEEKLY" if interval == 2 => CalRepeat::Biweekly,
        "WEEKLY" => CalRepeat::Weekly,
        "MONTHLY" => CalRepeat::Monthly,
        "YEARLY" => CalRepeat::Yearly,
        _ => CalRepeat::None,
    };
    if until.is_none() {
        if let Some(n) = count {
            until = count_to_until(rep, start, n).filter(|d| d.y <= 9999);
        }
    }
    (rep, until)
}

/// The date of the `n`-th occurrence (1 = the start itself).
fn count_to_until(rep: CalRepeat, start: Date, n: u32) -> Option<Date> {
    let k = n.saturating_sub(1) as i64;
    Some(match rep {
        CalRepeat::None => return None,
        CalRepeat::Daily => start.add_days(k),
        CalRepeat::Weekly => start.add_days(7 * k),
        CalRepeat::Biweekly => start.add_days(14 * k),
        CalRepeat::Monthly => start.add_months(k as i32),
        CalRepeat::Yearly => start.add_months(12 * k as i32),
        CalRepeat::Weekdays => {
            let mut d = start;
            let mut left = if start.weekday() < 5 { k } else { k + 1 };
            while left > 0 {
                d = d.add_days(1);
                if d.weekday() < 5 {
                    left -= 1;
                }
            }
            d
        }
    })
}

// =============================================================================================
// Quick add
// =============================================================================================

/// Result of [`cal_quick_parse`].
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct CalQuick {
    pub title: String,
    /// `YYYY-MM-DD`
    pub date: String,
    /// `HH:MM` or empty (all-day).
    pub start: String,
    /// `HH:MM` or empty (all-day).
    pub end: String,
    pub all_day: bool,
    pub repeat: CalRepeat,
    /// Text after `#` (without it) or empty; the caller matches it against calendar names.
    pub calendar_hint: String,
}

#[derive(Clone, Debug)]
struct QTok {
    text: String,
    /// Lower case, trailing commas removed.
    key: String,
    /// No whitespace before this token in the input (a split-off range dash or its right side).
    glued: bool,
    used: bool,
}

fn is_dash(c: char) -> bool {
    matches!(c, '-' | '\u{2013}' | '\u{2014}')
}

fn quick_tokens(text: &str) -> Vec<QTok> {
    let mut out = Vec::new();
    let mut push = |s: String, glued: bool| {
        if !s.is_empty() {
            let key = s.trim_end_matches(',').to_lowercase();
            out.push(QTok { text: s, key, glued, used: false });
        }
    };
    for word in text.split_whitespace() {
        if Date::parse(word.trim_end_matches(',')).is_some() {
            push(word.to_string(), false);
            continue;
        }
        // Split "14-15", "18:30-19:30" (also with a long dash), "2-3pm", "6:30pm-7:30pm" into time, dash, time; keep
        // "Team-Call" whole.
        let chars: Vec<char> = word.chars().collect();
        let mut piece = String::new();
        let mut first = true;
        for (i, &c) in chars.iter().enumerate() {
            let split = is_dash(c)
                && i > 0
                && i + 1 < chars.len()
                && chars[i + 1].is_ascii_digit()
                && (chars[i - 1].is_ascii_digit() || parse_time_word(&piece.to_lowercase()).is_some());
            if split {
                push(std::mem::take(&mut piece), !first);
                push(c.to_string(), true);
                first = false;
                // The right side is glued to the dash.
                piece.clear();
            } else {
                piece.push(c);
            }
        }
        push(piece, !first);
    }
    out
}

/// Weekday index (0 = Monday) for German and English names and abbreviations (optional dot).
fn weekday_of(word: &str) -> Option<u32> {
    let w = word.strip_suffix('.').unwrap_or(word);
    Some(match w {
        "montag" | "mo" | "monday" | "mon" => 0,
        "dienstag" | "di" | "tuesday" | "tue" | "tues" => 1,
        "mittwoch" | "mi" | "wednesday" | "wed" => 2,
        "donnerstag" | "do" | "thursday" | "thu" | "thur" | "thurs" => 3,
        "freitag" | "fr" | "friday" | "fri" => 4,
        "samstag" | "sonnabend" | "sa" | "saturday" | "sat" => 5,
        "sonntag" | "so" | "sunday" | "sun" => 6,
        _ => return None,
    })
}

/// Full weekday names only (for the plural "montags" / "mondays").
fn full_weekday_of(word: &str) -> Option<u32> {
    (word.len() >= 6).then(|| weekday_of(word)).flatten()
}

fn next_weekday(today: Date, wd: u32) -> Date {
    today.add_days(((wd + 7 - today.weekday()) % 7) as i64)
}

#[derive(Clone, Copy, Debug)]
struct TimeWord {
    h: u32,
    m: u32,
    colon: bool,
    uhr: bool,
    /// Some(true) = pm, Some(false) = am.
    mer: Option<bool>,
}

impl TimeWord {
    fn explicit(&self) -> bool {
        self.uhr || self.colon || self.mer.is_some()
    }
}

/// `9`, `9:30`, `9.30`, optionally glued to `uhr`, `h`, `am`, `pm`.
fn parse_time_word(k: &str) -> Option<TimeWord> {
    let b = k.as_bytes();
    let mut i = 0;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 || i > 2 {
        return None;
    }
    let h: u32 = k[..i].parse().ok()?;
    let (mut m, mut colon) = (0, false);
    if i + 3 <= b.len()
        && (b[i] == b':' || b[i] == b'.')
        && b[i + 1].is_ascii_digit()
        && b[i + 2].is_ascii_digit()
        && !b.get(i + 3).is_some_and(|c| c.is_ascii_digit())
    {
        colon = b[i] == b':';
        m = k[i + 1..i + 3].parse().ok()?;
        i += 3;
    }
    let (uhr, mer) = match &k[i..] {
        "" => (false, None),
        "uhr" | "h" => (true, None),
        "am" | "a.m." => (false, Some(false)),
        "pm" | "p.m." => (false, Some(true)),
        _ => return None,
    };
    (m < 60).then_some(TimeWord { h, m, colon, uhr, mer })
}

fn time_minutes(t: TimeWord, mer: Option<bool>) -> Option<u32> {
    let mut h = t.h;
    match mer {
        Some(pm) => {
            if h == 0 || h > 12 {
                return None;
            }
            if h == 12 {
                h = 0;
            }
            if pm {
                h += 12;
            }
        }
        None if h > 23 => return None,
        None => {}
    }
    Some(h * 60 + t.m)
}

fn hm(min: u32) -> String {
    format!("{:02}:{:02}", min / 60, min % 60)
}

/// Parses a quick-add line such as "Zahnarzt Fr 10 Uhr" or "Lunch with Ann tomorrow 12:30pm".
/// `today` is `YYYY-MM-DD`, `now_minutes` the current local time in minutes since midnight.
#[uniffi::export]
pub fn cal_quick_parse(text: String, today: String, now_minutes: u32) -> CalQuick {
    let today = Date::parse(&today).unwrap_or_else(system_today);
    let mut t = quick_tokens(&text);
    let n = t.len();
    let free = |t: &[QTok], i: usize| i < t.len() && !t[i].used;

    // #calendar
    let mut hint = String::new();
    if let Some(i) = t.iter().position(|x| x.text.starts_with('#') && x.text.len() > 1) {
        hint = t[i].text[1..].chars().take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_').collect();
        if !hint.is_empty() {
            t[i].used = true;
        }
    }

    // Repetition: "jeden Montag", "every week", "alle 2 Wochen", "täglich", "montags".
    let mut repeat = CalRepeat::None;
    let mut repeat_day: Option<Date> = None;
    for i in 0..n {
        if !free(&t, i) || !matches!(t[i].key.as_str(), "jede" | "jeden" | "jeder" | "jedes" | "every" | "alle") {
            continue;
        }
        if free(&t, i + 2)
            && free(&t, i + 1)
            && matches!(t[i + 1].key.as_str(), "other" | "zweite" | "zweiten" | "2" | "2." | "zwei" | "two")
            && matches!(t[i + 2].key.trim_end_matches('.'), "woche" | "wochen" | "week" | "weeks")
        {
            repeat = CalRepeat::Biweekly;
            t[i].used = true;
            t[i + 1].used = true;
            t[i + 2].used = true;
            break;
        }
        if !free(&t, i + 1) || t[i].key == "alle" {
            continue;
        }
        let w = t[i + 1].key.trim_end_matches('.');
        let rep = match w {
            "tag" | "day" => CalRepeat::Daily,
            "werktag" | "weekday" | "workday" => CalRepeat::Weekdays,
            "woche" | "week" => CalRepeat::Weekly,
            "monat" | "month" => CalRepeat::Monthly,
            "jahr" | "year" => CalRepeat::Yearly,
            _ => match weekday_of(w) {
                Some(wd) => {
                    repeat_day = Some(next_weekday(today, wd));
                    CalRepeat::Weekly
                }
                None => continue,
            },
        };
        repeat = rep;
        t[i].used = true;
        t[i + 1].used = true;
        break;
    }
    if repeat == CalRepeat::None {
        for x in t.iter_mut().filter(|x| !x.used) {
            let rep = match x.key.as_str() {
                "täglich" | "taeglich" | "daily" => CalRepeat::Daily,
                "werktags" | "weekdays" => CalRepeat::Weekdays,
                "wöchentlich" | "woechentlich" | "weekly" => CalRepeat::Weekly,
                "zweiwöchentlich" | "zweiwoechentlich" | "biweekly" | "fortnightly" => CalRepeat::Biweekly,
                "monatlich" | "monthly" => CalRepeat::Monthly,
                "jährlich" | "jaehrlich" | "yearly" | "annually" => CalRepeat::Yearly,
                k => match k.strip_suffix('s').and_then(full_weekday_of) {
                    Some(wd) => {
                        repeat_day = Some(next_weekday(today, wd));
                        CalRepeat::Weekly
                    }
                    None => continue,
                },
            };
            repeat = rep;
            x.used = true;
            break;
        }
    }

    let take_prefix = |t: &mut [QTok], i: usize, words: &[&str]| {
        if i > 0 && !t[i - 1].used && words.contains(&t[i - 1].key.as_str()) {
            t[i - 1].used = true;
        }
    };

    // Explicit date: "18.11.", "18.11.2027", "24.12.27", "2027-11-18".
    let mut date: Option<Date> = None;
    for i in 0..n {
        if t[i].used {
            continue;
        }
        if let Some(d) = parse_quick_date(&t[i].key, today) {
            date = Some(d);
            t[i].used = true;
            take_prefix(&mut t, i, &["am", "on", "ab"]);
            break;
        }
    }

    // Relative day.
    let mut rel: Option<Date> = None;
    for x in t.iter_mut().filter(|x| !x.used) {
        let d = match x.key.as_str() {
            "heute" | "today" => 0,
            "morgen" | "tomorrow" => 1,
            "übermorgen" | "uebermorgen" => 2,
            _ => continue,
        };
        rel = Some(today.add_days(d));
        x.used = true;
        break;
    }

    // Weekday: "Fr", "am Freitag", "next Monday", "kommenden Di.".
    let mut wday: Option<Date> = None;
    for i in 0..n {
        if t[i].used {
            continue;
        }
        if let Some(wd) = weekday_of(&t[i].key) {
            wday = Some(next_weekday(today, wd));
            t[i].used = true;
            take_prefix(&mut t, i, &["am", "nächsten", "naechsten", "kommenden", "on", "next", "this"]);
            break;
        }
    }

    // Times.
    let time_at = |t: &[QTok], i: usize| -> Option<(TimeWord, usize)> {
        if !free(t, i) {
            return None;
        }
        let mut tw = parse_time_word(&t[i].key)?;
        let mut e = i + 1;
        if free(t, e) {
            match t[e].key.as_str() {
                "uhr" | "h" if tw.mer.is_none() && !tw.uhr => {
                    tw.uhr = true;
                    e += 1;
                }
                "pm" | "p.m." if tw.mer.is_none() && !tw.uhr => {
                    tw.mer = Some(true);
                    e += 1;
                }
                // A separate "am" is German for "on the" unless it ends the text.
                "a.m." if tw.mer.is_none() && !tw.uhr => {
                    tw.mer = Some(false);
                    e += 1;
                }
                "am" if tw.mer.is_none() && !tw.uhr && e + 1 == t.len() => {
                    tw.mer = Some(false);
                    e += 1;
                }
                _ => {}
            }
        }
        Some((tw, e))
    };
    let mut start: Option<u32> = None;
    let mut end: Option<u32> = None;
    for i in 0..n {
        if t[i].used {
            continue;
        }
        let pre = matches!(t[i].key.as_str(), "von" | "um" | "ab" | "from" | "at" | "between");
        let j = if pre { i + 1 } else { i };
        let Some((t1, k)) = time_at(&t, j) else {
            continue;
        };
        if !free(&t, k) {
            continue;
        }
        let sep = t[k].key.as_str();
        let dash = sep.chars().count() == 1 && sep.chars().all(is_dash);
        if !dash && !matches!(sep, "bis" | "to" | "until" | "till" | "and") {
            continue;
        }
        let Some((t2, e)) = time_at(&t, k + 1) else {
            continue;
        };
        if !dash && !pre && !t1.explicit() && !t2.explicit() {
            continue;
        }
        let (m1, m2) = match (t1.mer, t2.mer) {
            (None, Some(pm)) => {
                let b = time_minutes(t2, Some(pm));
                let a = time_minutes(t1, Some(pm)).filter(|a| b.is_some_and(|b| *a <= b));
                (a.or_else(|| time_minutes(t1, Some(!pm))), b)
            }
            (Some(pm), None) => {
                let a = time_minutes(t1, Some(pm));
                let b = time_minutes(t2, Some(pm)).filter(|b| a.is_some_and(|a| *b >= a));
                (a, b.or_else(|| time_minutes(t2, Some(!pm))))
            }
            (a, b) => (time_minutes(t1, a), time_minutes(t2, b)),
        };
        let (Some(m1), Some(m2)) = (m1, m2) else {
            continue;
        };
        start = Some(m1);
        end = Some(m2);
        for x in &mut t[i..e] {
            x.used = true;
        }
        break;
    }
    if start.is_none() {
        for i in 0..n {
            if t[i].used {
                continue;
            }
            let pre = matches!(t[i].key.as_str(), "um" | "ab" | "at" | "gegen");
            let j = if pre { i + 1 } else { i };
            let Some((tw, e)) = time_at(&t, j) else {
                continue;
            };
            if !pre && !tw.explicit() {
                continue;
            }
            let Some(m) = time_minutes(tw, tw.mer) else {
                continue;
            };
            start = Some(m);
            for x in &mut t[i..e] {
                x.used = true;
            }
            break;
        }
    }
    let end = start.map(|s| end.unwrap_or(if s >= 23 * 60 { 23 * 60 + 59 } else { s + 60 }));

    let day = date.or(rel).or(wday).or(repeat_day).unwrap_or(match start {
        Some(s) if s < now_minutes => today.add_days(1),
        _ => today,
    });

    CalQuick {
        title: quick_title(&t),
        date: day.to_string(),
        start: start.map(hm).unwrap_or_default(),
        end: end.map(hm).unwrap_or_default(),
        all_day: start.is_none(),
        repeat,
        calendar_hint: hint,
    }
}

fn parse_quick_date(k: &str, today: Date) -> Option<Date> {
    if let Some(d) = Date::parse(k) {
        return Some(d);
    }
    let parts: Vec<&str> = k.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let digits = |s: &str, lens: &[usize]| lens.contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(parts[0], &[1, 2]) || !digits(parts[1], &[1, 2]) || !(parts[2].is_empty() || digits(parts[2], &[2, 4])) {
        return None;
    }
    let (d, m): (u32, u32) = (parts[0].parse().ok()?, parts[1].parse().ok()?);
    if parts[2].is_empty() {
        match Date::new(today.y, m, d) {
            Some(x) if x >= today => Some(x),
            _ => Date::new(today.y + 1, m, d),
        }
    } else {
        let y: i32 = parts[2].parse().ok()?;
        Date::new(if parts[2].len() == 2 { 2000 + y } else { y }, m, d)
    }
}

/// The remaining words: filler words in front of recognised parts (or at the end) are dropped.
fn quick_title(t: &[QTok]) -> String {
    const FILLER: [&str; 9] = ["am", "um", "ab", "von", "bis", "at", "on", "from", "to"];
    let is_filler = |x: &QTok| FILLER.contains(&x.key.as_str()) || x.key.is_empty();
    let mut title = String::new();
    let mut prev_kept: Option<usize> = None;
    for (i, x) in t.iter().enumerate() {
        if x.used {
            continue;
        }
        if is_filler(x) {
            let next = t[i + 1..].iter().find(|y| y.used || !is_filler(y));
            if next.map_or(true, |y| y.used) {
                continue;
            }
        }
        if !title.is_empty() && !(x.glued && prev_kept == Some(i.wrapping_sub(1))) {
            title.push(' ');
        }
        title.push_str(&x.text);
        prev_kept = Some(i);
    }
    let title = title.trim_matches(|c: char| c == ',' || c.is_whitespace());
    let mut c = title.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

fn system_today() -> Date {
    let secs =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    Date::from_days(secs.div_euclid(86_400))
}

// =============================================================================================
// Public holidays
// =============================================================================================

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct CalHoliday {
    /// `YYYY-MM-DD`
    pub date: String,
    pub name: String,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct CalRegion {
    /// `DE-BY`, `AT`, `CH`, … or empty for "no holidays".
    pub code: String,
    pub name: String,
}

const REGIONS: [(&str, &str, &str); 19] = [
    ("", "Keine", "None"),
    ("DE-BW", "Baden-Württemberg", "Baden-Württemberg"),
    ("DE-BY", "Bayern", "Bavaria"),
    ("DE-BE", "Berlin", "Berlin"),
    ("DE-BB", "Brandenburg", "Brandenburg"),
    ("DE-HB", "Bremen", "Bremen"),
    ("DE-HH", "Hamburg", "Hamburg"),
    ("DE-HE", "Hessen", "Hesse"),
    ("DE-MV", "Mecklenburg-Vorpommern", "Mecklenburg-Western Pomerania"),
    ("DE-NI", "Niedersachsen", "Lower Saxony"),
    ("DE-NW", "Nordrhein-Westfalen", "North Rhine-Westphalia"),
    ("DE-RP", "Rheinland-Pfalz", "Rhineland-Palatinate"),
    ("DE-SL", "Saarland", "Saarland"),
    ("DE-SN", "Sachsen", "Saxony"),
    ("DE-ST", "Sachsen-Anhalt", "Saxony-Anhalt"),
    ("DE-SH", "Schleswig-Holstein", "Schleswig-Holstein"),
    ("DE-TH", "Thüringen", "Thuringia"),
    ("AT", "Österreich", "Austria"),
    ("CH", "Schweiz", "Switzerland"),
];

fn is_german(lang: &str) -> bool {
    lang.get(..2).is_some_and(|l| l.eq_ignore_ascii_case("de"))
}

/// Regions with offline public holidays; the first entry (empty code) means none.
#[uniffi::export]
pub fn cal_regions(lang: String) -> Vec<CalRegion> {
    let de = is_german(&lang);
    REGIONS
        .iter()
        .map(|(code, n_de, n_en)| CalRegion { code: code.to_string(), name: if de { n_de } else { n_en }.to_string() })
        .collect()
}

/// Public holidays of a region between `from` and `to` (inclusive, `YYYY-MM-DD`), sorted by date.
#[uniffi::export]
pub fn cal_holidays(region: String, from: String, to: String, lang: String) -> Vec<CalHoliday> {
    let (Some(from), Some(to)) = (Date::parse(&from), Date::parse(&to)) else {
        return Vec::new();
    };
    if from > to || !REGIONS.iter().any(|(c, _, _)| !c.is_empty() && *c == region) {
        return Vec::new();
    }
    let de = is_german(&lang);
    let mut all: Vec<(Date, &'static str)> = Vec::new();
    for y in from.y..=to.y.min(from.y + 200) {
        holidays_of_year(&region, y, de, &mut all);
    }
    all.retain(|(d, _)| *d >= from && *d <= to);
    all.sort_by_key(|(d, _)| *d);
    all.into_iter().map(|(d, n)| CalHoliday { date: d.to_string(), name: n.to_string() }).collect()
}

fn holidays_of_year(region: &str, y: i32, de: bool, out: &mut Vec<(Date, &'static str)>) {
    let easter = Date::easter(y);
    let mut add = |d: Option<Date>, n_de: &'static str, n_en: &'static str| {
        if let Some(d) = d {
            out.push((d, if de { n_de } else { n_en }));
        }
    };
    let fixed = |m: u32, d: u32| Date::new(y, m, d);
    let e = |n: i64| Some(easter.add_days(n));

    match region {
        "AT" => {
            add(fixed(1, 1), "Neujahr", "New Year's Day");
            add(fixed(1, 6), "Heilige Drei Könige", "Epiphany");
            add(e(1), "Ostermontag", "Easter Monday");
            add(fixed(5, 1), "Staatsfeiertag", "National Holiday");
            add(e(39), "Christi Himmelfahrt", "Ascension Day");
            add(e(50), "Pfingstmontag", "Whit Monday");
            add(e(60), "Fronleichnam", "Corpus Christi");
            add(fixed(8, 15), "Mariä Himmelfahrt", "Assumption Day");
            add(fixed(10, 26), "Nationalfeiertag", "National Day");
            add(fixed(11, 1), "Allerheiligen", "All Saints' Day");
            add(fixed(12, 8), "Mariä Empfängnis", "Immaculate Conception");
            add(fixed(12, 25), "Christtag", "Christmas Day");
            add(fixed(12, 26), "Stefanitag", "St. Stephen's Day");
            return;
        }
        "CH" => {
            add(fixed(1, 1), "Neujahr", "New Year's Day");
            add(e(-2), "Karfreitag", "Good Friday");
            add(e(1), "Ostermontag", "Easter Monday");
            add(e(39), "Auffahrt", "Ascension Day");
            add(e(50), "Pfingstmontag", "Whit Monday");
            add(fixed(8, 1), "Bundesfeier", "Swiss National Day");
            add(fixed(12, 25), "Weihnachten", "Christmas Day");
            add(fixed(12, 26), "Stephanstag", "St. Stephen's Day");
            return;
        }
        _ => {}
    }
    let Some(state) = region.strip_prefix("DE-") else {
        return;
    };
    let is = |list: &[&str]| list.contains(&state);

    add(fixed(1, 1), "Neujahr", "New Year's Day");
    if is(&["BW", "BY", "ST"]) {
        add(fixed(1, 6), "Heilige Drei Könige", "Epiphany");
    }
    if (state == "BE" && y >= 2019) || (state == "MV" && y >= 2023) {
        add(fixed(3, 8), "Internationaler Frauentag", "International Women's Day");
    }
    add(e(-2), "Karfreitag", "Good Friday");
    if state == "BB" {
        add(e(0), "Ostersonntag", "Easter Sunday");
    }
    add(e(1), "Ostermontag", "Easter Monday");
    add(fixed(5, 1), "Tag der Arbeit", "Labour Day");
    if state == "BE" && (y == 2020 || y == 2025) {
        add(fixed(5, 8), "Tag der Befreiung", "Liberation Day");
    }
    add(e(39), "Christi Himmelfahrt", "Ascension Day");
    if state == "BB" {
        add(e(49), "Pfingstsonntag", "Whit Sunday");
    }
    add(e(50), "Pfingstmontag", "Whit Monday");
    if is(&["BW", "BY", "HE", "NW", "RP", "SL"]) {
        add(e(60), "Fronleichnam", "Corpus Christi");
    }
    if state == "SL" {
        add(fixed(8, 15), "Mariä Himmelfahrt", "Assumption Day");
    }
    if state == "TH" && y >= 2019 {
        add(fixed(9, 20), "Weltkindertag", "World Children's Day");
    }
    add(fixed(10, 3), "Tag der Deutschen Einheit", "German Unity Day");
    let reformation = y == 2017 || is(&["BB", "MV", "SN", "ST", "TH"]) || (is(&["HB", "HH", "NI", "SH"]) && y >= 2018);
    if reformation {
        add(fixed(10, 31), "Reformationstag", "Reformation Day");
    }
    if is(&["BW", "BY", "NW", "RP", "SL"]) {
        add(fixed(11, 1), "Allerheiligen", "All Saints' Day");
    }
    if state == "SN" {
        // Wednesday before 23 November.
        let nov22 = Date { y, m: 11, d: 22 };
        add(
            Some(nov22.add_days(-(((nov22.weekday() + 7 - 2) % 7) as i64))),
            "Buß- und Bettag",
            "Day of Repentance and Prayer",
        );
    }
    add(fixed(12, 25), "1. Weihnachtstag", "Christmas Day");
    add(fixed(12, 26), "2. Weihnachtstag", "Second Day of Christmas");
}

// =============================================================================================
// Tests
// =============================================================================================

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_794_492_000; // 2026-11-12 14:00:00 UTC

    fn ev(title: &str, all_day: bool, start: &str, end: &str) -> CalEvent {
        CalEvent {
            id: "abc123".into(),
            calendar_id: "cal1".into(),
            title: title.into(),
            all_day,
            start: start.into(),
            end: end.into(),
            ..CalEvent::default()
        }
    }

    fn unfolded(ics: &str) -> Vec<String> {
        unfold(ics)
    }

    fn q(text: &str) -> CalQuick {
        // Thursday, 12 November 2026, 10:00
        cal_quick_parse(text.into(), "2026-11-12".into(), 600)
    }

    // --- export ---------------------------------------------------------------------------

    #[test]
    fn stamp_format() {
        assert_eq!(utc_stamp(NOW), "20261112T140000Z");
        assert_eq!(utc_stamp(0), "19700101T000000Z");
    }

    #[test]
    fn export_timed_event() {
        let mut e = ev("Zahnarzt; Kontrolle, jährlich", false, "2026-11-13T10:00", "2026-11-13T11:30");
        e.location = "Praxis\\Müller".into();
        e.notes = "Zeile 1\nZeile 2".into();
        e.reminders = vec![15, 0];
        e.links = vec![super::super::cal::CalLink { kind: "drive".into(), label: "x".into(), target: "y".into() }];
        let ics = ics_export_at(&[e], "Privat, Familie", NOW);
        assert!(ics.ends_with("END:VCALENDAR\r\n"));
        assert!(!ics.replace("\r\n", "").contains('\n'));
        let l = unfolded(&ics);
        for want in [
            "BEGIN:VCALENDAR",
            "VERSION:2.0",
            "PRODID:-//GhulSuite//GhulCal//EN",
            "X-WR-CALNAME:Privat\\, Familie",
            "UID:abc123@ghulsuite",
            "DTSTAMP:20261112T140000Z",
            "DTSTART:20261113T100000",
            "DTEND:20261113T113000",
            "SUMMARY:Zahnarzt\\; Kontrolle\\, jährlich",
            "LOCATION:Praxis\\\\Müller",
            "DESCRIPTION:Zeile 1\\nZeile 2",
            "TRIGGER:-PT15M",
            "TRIGGER:PT0S",
            "ACTION:DISPLAY",
        ] {
            assert!(l.iter().any(|x| x == want), "missing {want}\n{ics}");
        }
        assert!(!ics.contains("RRULE"));
        assert!(!ics.contains("drive"));
        assert_eq!(l.iter().filter(|x| *x == "BEGIN:VALARM").count(), 2);
    }

    #[test]
    fn export_all_day_with_repeat_and_reminders() {
        let mut e = ev("Urlaub", true, "2026-12-24", "2026-12-26");
        e.repeat = CalRepeat::Yearly;
        e.repeat_until = "2030-12-31".into();
        e.exceptions = vec!["2028-12-24".into(), "2027-12-24".into()];
        e.reminders = vec![0, 1440, 90];
        e.birth_year = 1950;
        let l = unfolded(&ics_export_at(&[e], "", NOW));
        for want in [
            "DTSTART;VALUE=DATE:20261224",
            "DTEND;VALUE=DATE:20261227",
            "RRULE:FREQ=YEARLY;UNTIL=20301231",
            "EXDATE;VALUE=DATE:20271224,20281224",
            "TRIGGER:PT9H",
            "TRIGGER:-PT15H",
            "TRIGGER:PT7H30M",
            "X-GHUL-BIRTH-YEAR:1950",
        ] {
            assert!(l.iter().any(|x| x == want), "missing {want}: {l:?}");
        }
        assert!(!l.iter().any(|x| x.starts_with("X-WR-CALNAME")));
    }

    #[test]
    fn export_repeat_rules() {
        let cases = [
            (CalRepeat::Daily, "RRULE:FREQ=DAILY"),
            (CalRepeat::Weekdays, "RRULE:FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR"),
            (CalRepeat::Weekly, "RRULE:FREQ=WEEKLY"),
            (CalRepeat::Biweekly, "RRULE:FREQ=WEEKLY;INTERVAL=2"),
            (CalRepeat::Monthly, "RRULE:FREQ=MONTHLY"),
            (CalRepeat::Yearly, "RRULE:FREQ=YEARLY"),
        ];
        for (rep, want) in cases {
            let mut e = ev("x", false, "2026-11-13T10:00", "2026-11-13T11:00");
            e.repeat = rep;
            let l = unfolded(&ics_export_at(&[e.clone()], "", NOW));
            assert!(l.iter().any(|x| x == want), "{rep:?}: {l:?}");
            e.repeat_until = "2027-01-31".into();
            e.exceptions = vec!["2026-11-20".into()];
            let l = unfolded(&ics_export_at(&[e], "", NOW));
            assert!(l.iter().any(|x| *x == format!("{want};UNTIL=20270131T235959")), "{l:?}");
            assert!(l.iter().any(|x| x == "EXDATE:20261120T100000"));
        }
    }

    #[test]
    fn export_folds_utf8_safely() {
        let title = "Ä€😀".repeat(40);
        let mut e = ev(&title, false, "2026-11-13T10:00", "");
        e.id.clear();
        let ics = ics_export_at(&[e], "", NOW);
        for line in ics.split("\r\n") {
            assert!(line.len() <= 75, "{} octets: {line}", line.len());
        }
        let l = unfolded(&ics);
        assert!(l.iter().any(|x| *x == format!("SUMMARY:{title}")));
        // Missing end: one hour; empty id: generated UID.
        assert!(l.iter().any(|x| x == "DTEND:20261113T110000"));
        let uid = l.iter().find(|x| x.starts_with("UID:")).unwrap();
        assert_eq!(uid.len(), "UID:".len() + 32 + "@ghulsuite".len());
    }

    #[test]
    fn export_skips_invalid_start() {
        let ics = ics_export_at(&[ev("x", false, "kaputt", "")], "", NOW);
        assert!(!ics.contains("VEVENT"));
    }

    #[test]
    fn escape_round_trip() {
        let s = "a\\b;c,d\ne\r\nf";
        assert_eq!(ics_escape(s), "a\\\\b\\;c\\,d\\ne\\nf");
        assert_eq!(ics_unescape(&ics_escape(s)), "a\\b;c,d\ne\nf");
        assert_eq!(ics_unescape("x\\Ny\\"), "x\ny\\");
    }

    #[test]
    fn export_import_round_trip() {
        let mut a = ev("Weekly, sync; team", false, "2026-11-16T09:00", "2026-11-16T09:30");
        a.repeat = CalRepeat::Weekdays;
        a.repeat_until = "2026-12-18".into();
        a.exceptions = vec!["2026-11-18".into()];
        a.reminders = vec![60, 10];
        a.location = "Raum 4".into();
        a.notes = "Agenda:\n- A\n- B".into();
        let mut b = ev("Oma", true, "2026-12-24", "2026-12-24");
        b.repeat = CalRepeat::Yearly;
        b.reminders = vec![1440, 0];
        b.birth_year = 1940;
        let mut c = ev("Reise", true, "2026-07-01", "2026-07-14");
        c.repeat = CalRepeat::Biweekly;
        let back = ics_import(&ics_export_at(&[a.clone(), b.clone(), c.clone()], "X", NOW), 60).unwrap();
        for (orig, got) in [a, b, c].into_iter().zip(back) {
            let want = CalEvent { id: String::new(), calendar_id: String::new(), links: vec![], ..orig };
            assert_eq!(got, want);
        }
    }

    // --- import ---------------------------------------------------------------------------

    const SAMPLE: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//EN\r\n\
BEGIN:VTIMEZONE\r\nTZID:Europe/Berlin\r\nBEGIN:STANDARD\r\nDTSTART:19701025T030000\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\n\
BEGIN:VEVENT\r\nUID:one\r\nDTSTART;TZID=Europe/Berlin:20261113T100000\r\nDTEND;TZID=\"Europe/Berlin\":20261113T113000\r\n\
SUMMARY:Lange Beschreibung mit\\, Komma und \\; Semikolon und sehr vielen Wo\r\n rtern\r\n\
DESCRIPTION:Zeile 1\\nZeile 2\r\nLOCATION:Büro\r\n\
BEGIN:VALARM\r\nACTION:DISPLAY\r\nTRIGGER:-PT15M\r\nEND:VALARM\r\n\
BEGIN:VALARM\r\nACTION:AUDIO\r\nTRIGGER;RELATED=END:-PT1H30M\r\nEND:VALARM\r\n\
BEGIN:VALARM\r\nTRIGGER:PT5M\r\nEND:VALARM\r\nEND:VEVENT\r\n\
BEGIN:VTODO\r\nDTSTART:20261113T100000\r\nSUMMARY:Aufgabe\r\nEND:VTODO\r\n\
BEGIN:VEVENT\r\nUID:two\r\nDTSTART:20261112T230000Z\r\nDURATION:PT45M\r\nSUMMARY:UTC\r\n\
RRULE:FREQ=WEEKLY;INTERVAL=2;UNTIL=20261231T225959Z\r\nEXDATE:20261126T230000Z,20261210T230000Z\r\nEXDATE:20261224T230000Z\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:three\r\nDTSTART;VALUE=DATE:20261224\r\nDTEND;VALUE=DATE:20261227\r\nSUMMARY:Feiertage\r\n\
RRULE:FREQ=DAILY;COUNT=3\r\nBEGIN:VALARM\r\nTRIGGER:-P1D\r\nEND:VALARM\r\nBEGIN:VALARM\r\nTRIGGER:PT9H\r\nEND:VALARM\r\n\
BEGIN:VALARM\r\nTRIGGER:PT20H\r\nEND:VALARM\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nSUMMARY:No start\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nDTSTART;VALUE=DATE:20270101\r\nSUMMARY:Neujahr\r\nRRULE:FREQ=SECONDLY\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nDTSTART:20270105T080000\r\nSUMMARY:No end\r\nEND:VEVENT\r\n\
END:VCALENDAR\r\n";

    #[test]
    fn import_sample() {
        let v = ics_import(SAMPLE, 60).unwrap();
        assert_eq!(v.len(), 5);
        let a = &v[0];
        assert_eq!(a.title, "Lange Beschreibung mit, Komma und ; Semikolon und sehr vielen Wortern");
        assert_eq!(a.notes, "Zeile 1\nZeile 2");
        assert_eq!(a.location, "Büro");
        assert_eq!((a.start.as_str(), a.end.as_str()), ("2026-11-13T10:00", "2026-11-13T11:30"));
        assert!(!a.all_day);
        assert_eq!(a.reminders, vec![90, 15, 0]);
        assert_eq!((a.id.as_str(), a.calendar_id.as_str()), ("", ""));

        let b = &v[1];
        assert_eq!((b.start.as_str(), b.end.as_str()), ("2026-11-13T00:00", "2026-11-13T00:45"));
        assert_eq!(b.repeat, CalRepeat::Biweekly);
        assert_eq!(b.repeat_until, "2026-12-31");
        assert_eq!(b.exceptions, vec!["2026-11-27", "2026-12-11", "2026-12-25"]);

        let c = &v[2];
        assert!(c.all_day);
        assert_eq!((c.start.as_str(), c.end.as_str()), ("2026-12-24", "2026-12-26"));
        assert_eq!(c.repeat, CalRepeat::Daily);
        assert_eq!(c.repeat_until, "2026-12-26");
        assert_eq!(c.reminders, vec![1980, 0]);

        let d = &v[3];
        assert_eq!((d.start.as_str(), d.end.as_str()), ("2027-01-01", "2027-01-01"));
        assert_eq!(d.repeat, CalRepeat::None);
        assert_eq!(v[4].end, "2027-01-05T09:00");
    }

    #[test]
    fn import_lf_and_rrule_variants() {
        let mk = |rule: &str| {
            format!("BEGIN:VCALENDAR\nBEGIN:VEVENT\nDTSTART:20261112T090000\nRRULE:{rule}\nEND:VEVENT\nEND:VCALENDAR\n")
        };
        let get = |rule: &str| {
            let e = ics_import(&mk(rule), 0).unwrap().remove(0);
            (e.repeat, e.repeat_until)
        };
        assert_eq!(get("FREQ=DAILY"), (CalRepeat::Daily, String::new()));
        assert_eq!(get("FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR"), (CalRepeat::Weekdays, String::new()));
        assert_eq!(get("FREQ=WEEKLY;BYDAY=TH"), (CalRepeat::Weekly, String::new()));
        assert_eq!(get("FREQ=MONTHLY;UNTIL=20270301"), (CalRepeat::Monthly, "2027-03-01".into()));
        assert_eq!(get("FREQ=YEARLY;COUNT=3"), (CalRepeat::Yearly, "2028-11-12".into()));
        assert_eq!(get("FREQ=WEEKLY;COUNT=2"), (CalRepeat::Weekly, "2026-11-19".into()));
        assert_eq!(get("FREQ=MONTHLY;COUNT=2"), (CalRepeat::Monthly, "2026-12-12".into()));
        // Thursday + 5 more weekdays = next Thursday.
        assert_eq!(get("FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR;COUNT=6"), (CalRepeat::Weekdays, "2026-11-19".into()));
        assert_eq!(get("FREQ=HOURLY"), (CalRepeat::None, String::new()));
        assert_eq!(get("FREQ=WEEKLY;COUNT=999999999"), (CalRepeat::Weekly, String::new()));
    }

    #[test]
    fn import_overrides_and_cancelled() {
        let text = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:s\nDTSTART:20261116T090000\nRRULE:FREQ=WEEKLY\nSUMMARY:Serie\nEND:VEVENT\n\
BEGIN:VEVENT\nUID:s\nRECURRENCE-ID:20261123T090000\nDTSTART:20261124T100000\nSUMMARY:Verschoben\nEND:VEVENT\n\
BEGIN:VEVENT\nUID:s\nRECURRENCE-ID:20261130T090000\nSTATUS:CANCELLED\nDTSTART:20261130T090000\nEND:VEVENT\n\
BEGIN:VEVENT\nUID:x\nSTATUS:CANCELLED\nDTSTART:20261130T090000\nEND:VEVENT\nEND:VCALENDAR";
        let v = ics_import(text, 0).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].exceptions, vec!["2026-11-23", "2026-11-30"]);
        assert_eq!(v[1].title, "Verschoben");
        assert_eq!(v[1].repeat, CalRepeat::None);
    }

    #[test]
    fn import_absolute_trigger_and_all_day_without_end() {
        let text = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nDTSTART;VALUE=DATE:20261120\nDURATION:P2D\n\
BEGIN:VALARM\nTRIGGER;VALUE=DATE-TIME:20261119T180000Z\nEND:VALARM\nEND:VEVENT\n\
BEGIN:VEVENT\nDTSTART:20261120\nX-GHUL-BIRTH-YEAR:1988\nEND:VEVENT\nEND:VCALENDAR";
        let v = ics_import(text, 60).unwrap();
        assert_eq!((v[0].start.as_str(), v[0].end.as_str()), ("2026-11-20", "2026-11-21"));
        // 19 Nov 19:00 local = 5 h before midnight = 14 h before 09:00.
        assert_eq!(v[0].reminders, vec![840]);
        assert!(v[1].all_day);
        assert_eq!(v[1].end, "2026-11-20");
        assert_eq!(v[1].birth_year, 1988);
    }

    #[test]
    fn import_errors() {
        assert!(ics_import("", 0).is_err());
        assert!(ics_import("hello world", 0).is_err());
        assert!(ics_import("BEGIN:VEVENT\nDTSTART:20261120\nEND:VEVENT", 0).is_err());
        assert_eq!(ics_import("BEGIN:VCALENDAR\nEND:VCALENDAR", 0).unwrap_err(), "No events found");
    }

    #[test]
    fn durations() {
        assert_eq!(parse_duration("-PT15M"), Some(-15));
        assert_eq!(parse_duration("-P1D"), Some(-1440));
        assert_eq!(parse_duration("-PT1H30M"), Some(-90));
        assert_eq!(parse_duration("PT0S"), Some(0));
        assert_eq!(parse_duration("+P1W"), Some(10080));
        assert_eq!(parse_duration("P1DT2H"), Some(1560));
        assert_eq!(parse_duration("PT90S"), Some(1));
        for bad in ["", "P", "PT", "15M", "P1M", "PT1", "PTH", "P99999999999999999999D", "-", "PÄ"] {
            assert_eq!(parse_duration(bad), None, "{bad}");
        }
        assert_eq!(fmt_duration(540), "PT9H");
        assert_eq!(fmt_duration(-900), "-PT15H");
        assert_eq!(fmt_duration(0), "PT0S");
    }

    #[test]
    fn import_never_panics_on_garbage() {
        let pieces = [
            "BEGIN:VCALENDAR",
            "BEGIN:VEVENT",
            "END:VEVENT",
            "BEGIN:VALARM",
            "END:VALARM",
            "END:VCALENDAR",
            "DTSTART:",
            "DTSTART:2026",
            "DTSTART:20261",
            "DTSTART:20261112T",
            "DTSTART:20261112T1",
            "DTSTART:20261112T0ä0",
            "DTSTART:20261112Tä000",
            "DTSTART;VALUE=DATE:2026ää",
            "DTEND:99991231T235900Z",
            "DTSTART:99991231T235900Z",
            "DTSTART:00000101",
            "DURATION:P99999999W",
            "DURATION:-P1D",
            "RRULE:FREQ=YEARLY;COUNT=10000",
            "RRULE:FREQ=MONTHLY;COUNT=10000",
            "RRULE:FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR;COUNT=10000",
            "RRULE:UNTIL=Z;;=;",
            "EXDATE:,,,",
            "EXDATE;VALUE=DATE:2026111ä",
            "BEGIN:VTIMEZONE",
            "END:VTIMEZONE",
            "BEGIN:DAYLIGHT",
            "END:DAYLIGHT",
            "BEGIN:STANDARD",
            "TZID:X",
            "TZOFFSETTO:+9999",
            "TZOFFSETTO:-0500",
            "TZOFFSETFROM:-",
            "RRULE:FREQ=YEARLY;BYDAY=-9SU;BYMONTH=13",
            "RRULE:FREQ=YEARLY;BYDAY=ä;BYMONTH=x",
            "RRULE:FREQ=YEARLY;BYDAY=5SU;BYMONTH=2;UNTIL=x",
            "RDATE:,,,20261112T100000",
            "DTSTART;TZID=X:20261112T100000",
            "DTSTART;TZID=/a/b/c/d/e:00000101T000000",
            "EXDATE;TZID=X:99991231T235900",
            "DTSTART;TZID=America/New_York:20261112T100000",
            "DTEND;TZID=America/New_York:2026111ä",
            "RRULE:FREQ=WEEKLY;UNTIL=ä",
            "EXDATE;TZID=America/New_York:,,x,20261119T100000Z",
            "TRIGGER:-P",
            "TRIGGER;VALUE=DATE-TIME:x",
            "TRIGGER:PT99999999H",
            ";;;:",
            ":",
            "\"",
            "SUMMARY;X=\"a:b;c\":ok",
            " continuation",
            "\tcont",
            "\\",
            "€€€",
            "😀:😀",
            "\r",
            "",
            "RECURRENCE-ID:20261112",
            "STATUS:CANCELLED",
            "X-GHUL-BIRTH-YEAR:-5",
        ];
        let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut rnd = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..3000 {
            let mut text = String::new();
            let lines = rnd() % 30;
            for _ in 0..lines {
                if rnd() % 5 == 0 {
                    for _ in 0..rnd() % 20 {
                        text.push(char::from_u32((rnd() % 0x3000) as u32).unwrap_or('x'));
                    }
                } else {
                    text.push_str(pieces[(rnd() % pieces.len() as u64) as usize]);
                }
                text.push_str(if rnd() % 2 == 0 { "\r\n" } else { "\n" });
            }
            let off = (rnd() % 3000) as i32 - 1500;
            let _ = ics_import(&text, off);
            let _ = ics_import_zoned(&text, &super::test_zones::World("Europe/Berlin"));
            let _ = cal_quick_parse(text.clone(), "2026-11-12".into(), (rnd() % 2000) as u32);
        }
        let _ = cal_quick_parse("".into(), "garbage".into(), 99999);
    }

    // --- quick add ------------------------------------------------------------------------

    fn timed(title: &str, date: &str, start: &str, end: &str) -> CalQuick {
        CalQuick {
            title: title.into(),
            date: date.into(),
            start: start.into(),
            end: end.into(),
            all_day: false,
            repeat: CalRepeat::None,
            calendar_hint: String::new(),
        }
    }

    #[test]
    fn quick_preview_chips() {
        assert_eq!(q("Zahnarzt Fr 10 Uhr"), timed("Zahnarzt", "2026-11-13", "10:00", "11:00"));
        assert_eq!(
            q("Laufen morgen 18:30-19:30 #sport"),
            CalQuick { calendar_hint: "sport".into(), ..timed("Laufen", "2026-11-13", "18:30", "19:30") }
        );
        assert_eq!(
            q("Team-Call jeden Montag 9 Uhr #arbeit"),
            CalQuick {
                calendar_hint: "arbeit".into(),
                repeat: CalRepeat::Weekly,
                ..timed("Team-Call", "2026-11-16", "09:00", "10:00")
            }
        );
        assert_eq!(
            q("Omas Geburtstag 24.12. #familie"),
            CalQuick {
                title: "Omas Geburtstag".into(),
                date: "2026-12-24".into(),
                start: String::new(),
                end: String::new(),
                all_day: true,
                repeat: CalRepeat::None,
                calendar_hint: "familie".into(),
            }
        );
    }

    #[test]
    fn quick_german() {
        assert_eq!(q("Meeting 14-15 Uhr"), timed("Meeting", "2026-11-12", "14:00", "15:00"));
        assert_eq!(q("Workshop 14:30\u{2013}16:00 übermorgen"), timed("Workshop", "2026-11-14", "14:30", "16:00"));
        assert_eq!(q("Kurs von 9 bis 10 am Dienstag"), timed("Kurs", "2026-11-17", "09:00", "10:00"));
        assert_eq!(q("Elternabend am 18.11. um 19:30"), timed("Elternabend", "2026-11-18", "19:30", "20:30"));
        assert_eq!(q("Abgabe 18.11.2027").date, "2027-11-18");
        assert_eq!(q("Party 24.12.27 ab 20 Uhr"), timed("Party", "2027-12-24", "20:00", "21:00"));
        // Past date without year: next year.
        assert_eq!(q("Neujahrsbrunch 1.1.").date, "2027-01-01");
        assert_eq!(q("Gestern 10.11.").date, "2027-11-10");
        // Today counts as next occurrence of the weekday.
        assert_eq!(q("Sport Do. 18 Uhr").date, "2026-11-12");
        assert_eq!(q("Sport nächsten Do 18 Uhr").title, "Sport");
        let r = q("Yoga jeden Werktag 7:00");
        assert_eq!((r.repeat, r.date.as_str(), r.start.as_str()), (CalRepeat::Weekdays, "2026-11-13", "07:00"));
        assert_eq!(q("Miete monatlich").repeat, CalRepeat::Monthly);
        assert_eq!(q("Backup täglich 23:30").end, "23:59");
        assert_eq!(q("Papiermüll alle 2 Wochen Mi").repeat, CalRepeat::Biweekly);
        let r = q("Chor mittwochs 19 Uhr");
        assert_eq!((r.repeat, r.date.as_str(), r.title.as_str()), (CalRepeat::Weekly, "2026-11-18", "Chor"));
        assert_eq!(q("Zahnarzt, Fr, 10 Uhr").title, "Zahnarzt");
        assert_eq!(q("um 10 Uhr kaffee").title, "Kaffee");
        assert_eq!(q("Termin mit Anna am").title, "Termin mit Anna");
    }

    /// The examples the app itself offers under "Quick add" (today is Thursday, 2026-11-12).
    #[test]
    fn quick_examples_of_the_app() {
        assert_eq!(q("Dentist Fri 10am"), timed("Dentist", "2026-11-13", "10:00", "11:00"));
        assert_eq!(
            q("Run tomorrow 6:30pm-7:30pm #sports"),
            CalQuick { calendar_hint: "sports".into(), ..timed("Run", "2026-11-13", "18:30", "19:30") }
        );
        let r = q("Team call every Monday 9am #work");
        assert_eq!(
            (r.title.as_str(), r.repeat, r.start.as_str(), r.calendar_hint.as_str()),
            ("Team call", CalRepeat::Weekly, "09:00", "work")
        );
        let r = q("Grandma’s birthday 24.12. #family");
        assert_eq!(
            (r.title.as_str(), r.date.as_str(), r.all_day, r.calendar_hint.as_str()),
            ("Grandma’s birthday", "2026-12-24", true, "family")
        );
        assert_eq!(q("Zahnarzt Fr 10 Uhr #privat").calendar_hint, "privat");
        let r = q("Laufen morgen 18:30-19:30 #sport");
        assert_eq!((r.title.as_str(), r.start.as_str(), r.end.as_str()), ("Laufen", "18:30", "19:30"));
        // More ranges with "am"/"pm" or "Uhr" on both sides.
        for (text, start, end) in [
            ("Yoga 6pm-7pm", "18:00", "19:00"),
            ("Lunch 11:30am-1pm", "11:30", "13:00"),
            ("Dienst 10uhr-12uhr", "10:00", "12:00"),
            ("Flug 9:15am\u{2013}11:45am", "09:15", "11:45"),
        ] {
            let r = q(text);
            assert_eq!((r.start.as_str(), r.end.as_str(), r.all_day), (start, end, false), "{text}: {r:?}");
        }
        assert_eq!(q("Team-Call 9-10").title, "Team-Call");
        let r = q("Team-Call jeden Montag 9 Uhr #arbeit");
        assert_eq!((r.title.as_str(), r.repeat, r.start.as_str()), ("Team-Call", CalRepeat::Weekly, "09:00"));
        let r = q("Omas Geburtstag 24.12. #familie");
        assert_eq!((r.title.as_str(), r.date.as_str(), r.all_day), ("Omas Geburtstag", "2026-12-24", true));
    }

    #[test]
    fn quick_english() {
        assert_eq!(q("Lunch with Ann tomorrow 12:30pm"), timed("Lunch with Ann", "2026-11-13", "12:30", "13:30"));
        assert_eq!(q("Call 2-3pm on Monday"), timed("Call", "2026-11-16", "14:00", "15:00"));
        assert_eq!(
            q("Standup every weekday at 9"),
            CalQuick { repeat: CalRepeat::Weekdays, ..timed("Standup", "2026-11-13", "09:00", "10:00") }
        );
        assert_eq!(q("Gym next fri 10am").date, "2026-11-13");
        assert_eq!(q("Gym next fri 10am").start, "10:00");
        assert_eq!(q("Review 11-1pm today"), timed("Review", "2026-11-12", "11:00", "13:00"));
        assert_eq!(q("Party sat 8 pm").start, "20:00");
        assert_eq!(q("Breakfast tomorrow 8 am").start, "08:00");
        assert_eq!(q("Midnight snack 12am").start, "00:00");
        assert_eq!(q("Bills every month").repeat, CalRepeat::Monthly);
        assert_eq!(q("Report weekly").repeat, CalRepeat::Weekly);
        assert_eq!(q("Anniversary every year 3.5.").date, "2027-05-03");
        let r = q("Squash every other week from 6pm to 7:30pm #sport");
        assert_eq!(
            r,
            CalQuick {
                repeat: CalRepeat::Biweekly,
                calendar_hint: "sport".into(),
                ..timed("Squash", "2026-11-12", "18:00", "19:30")
            }
        );
        assert_eq!(q("Talk to Bob at 3pm").title, "Talk to Bob");
        let r = q("Football every sunday");
        assert_eq!((r.repeat, r.date.as_str(), r.all_day), (CalRepeat::Weekly, "2026-11-15", true));
    }

    #[test]
    fn quick_past_time_and_defaults() {
        // 9:00 is before 10:00 now → tomorrow; 11:00 still today.
        assert_eq!(q("Kaffee 9 Uhr").date, "2026-11-13");
        assert_eq!(q("Kaffee 11 Uhr").date, "2026-11-12");
        // An explicit day is kept even when the time has passed.
        assert_eq!(q("Kaffee heute 9 Uhr").date, "2026-11-12");
        let r = q("Notizen");
        assert_eq!((r.date.as_str(), r.all_day, r.start.as_str()), ("2026-11-12", true, ""));
        assert_eq!(q("").title, "");
        assert_eq!(q("  ,  ").title, "");
    }

    #[test]
    fn quick_invalid_and_whole_words() {
        let r = q("Treffen 31.02.");
        assert_eq!((r.date.as_str(), r.title.as_str()), ("2026-11-12", "Treffen 31.02."));
        assert_eq!(q("Sommerfest Domino").title, "Sommerfest Domino");
        assert_eq!(q("Sommerfest Domino").date, "2026-11-12");
        assert_eq!(q("Termin 25 Uhr").start, "");
        assert_eq!(q("Kapitel 9.30").start, "");
        assert!(q("Version 2027").all_day);
        assert_eq!(q("Abend 13pm").start, "");
        assert_eq!(q("Kino 20.30 Uhr").start, "20:30");
        // A plain dash between words is not a time range.
        assert_eq!(q("Team-Call").title, "Team-Call");
        assert_eq!(q("ISO 2026-12-01").date, "2026-12-01");
    }

    // --- holidays -------------------------------------------------------------------------

    fn hol(region: &str, from: &str, to: &str, lang: &str) -> Vec<(String, String)> {
        cal_holidays(region.into(), from.into(), to.into(), lang.into()).into_iter().map(|h| (h.date, h.name)).collect()
    }

    #[test]
    fn nw_2026_exact() {
        let got = hol("DE-NW", "2026-01-01", "2026-12-31", "de");
        let want = [
            ("2026-01-01", "Neujahr"),
            ("2026-04-03", "Karfreitag"),
            ("2026-04-06", "Ostermontag"),
            ("2026-05-01", "Tag der Arbeit"),
            ("2026-05-14", "Christi Himmelfahrt"),
            ("2026-05-25", "Pfingstmontag"),
            ("2026-06-04", "Fronleichnam"),
            ("2026-10-03", "Tag der Deutschen Einheit"),
            ("2026-11-01", "Allerheiligen"),
            ("2026-12-25", "1. Weihnachtstag"),
            ("2026-12-26", "2. Weihnachtstag"),
        ];
        let want: Vec<(String, String)> = want.iter().map(|(d, n)| (d.to_string(), n.to_string())).collect();
        assert_eq!(got, want);
        let en = hol("DE-NW", "2026-01-01", "2026-12-31", "en");
        assert_eq!(en.len(), 11);
        assert_eq!(en[6].1, "Corpus Christi");
    }

    #[test]
    fn state_specials() {
        let has = |r: &str, y: i32, date: &str| {
            hol(r, &format!("{y}-01-01"), &format!("{y}-12-31"), "de").iter().any(|(d, _)| d == date)
        };
        let name = |r: &str, date: &str| hol(r, date, date, "de").first().map(|h| h.1.clone()).unwrap_or_default();
        assert_eq!(name("DE-SN", "2026-11-18"), "Buß- und Bettag");
        assert_eq!(name("DE-SN", "2025-11-19"), "Buß- und Bettag");
        assert!(has("DE-BE", 2025, "2025-05-08"));
        assert!(!has("DE-BE", 2026, "2026-05-08"));
        assert!(!has("DE-BB", 2025, "2025-05-08"));
        assert!(has("DE-TH", 2026, "2026-09-20"));
        assert!(!has("DE-TH", 2018, "2018-09-20"));
        assert!(has("DE-BE", 2026, "2026-03-08"));
        assert!(!has("DE-BE", 2018, "2018-03-08"));
        assert!(has("DE-MV", 2023, "2023-03-08"));
        assert!(!has("DE-MV", 2022, "2022-03-08"));
        assert!(has("DE-BB", 2026, "2026-04-05"));
        assert!(has("DE-BB", 2026, "2026-05-24"));
        assert!(has("DE-SL", 2026, "2026-08-15"));
        assert!(!has("DE-BY", 2026, "2026-08-15"));
        assert!(has("DE-BY", 2026, "2026-01-06"));
        assert!(has("DE-HH", 2018, "2018-10-31"));
        assert!(!has("DE-HH", 2016, "2016-10-31"));
        assert!(has("DE-NW", 2017, "2017-10-31"));
        assert!(!has("DE-NW", 2018, "2018-10-31"));
        assert_eq!(hol("DE-BW", "2026-01-01", "2026-12-31", "de").len(), 12);
        assert_eq!(hol("DE-BE", "2026-01-01", "2026-12-31", "de").len(), 10);
    }

    #[test]
    fn austria_and_switzerland() {
        let at = hol("AT", "2026-01-01", "2026-12-31", "de");
        assert_eq!(at.len(), 13);
        assert!(at.contains(&("2026-10-26".into(), "Nationalfeiertag".into())));
        assert!(at.contains(&("2026-12-08".into(), "Mariä Empfängnis".into())));
        let ch = hol("CH", "2026-01-01", "2026-12-31", "en");
        assert_eq!(ch.len(), 8);
        assert!(ch.contains(&("2026-08-01".into(), "Swiss National Day".into())));
        assert!(ch.contains(&("2026-05-14".into(), "Ascension Day".into())));
    }

    #[test]
    fn range_and_invalid_input() {
        let got = hol("DE-BY", "2026-12-20", "2027-01-10", "de");
        let dates: Vec<&str> = got.iter().map(|(d, _)| d.as_str()).collect();
        assert_eq!(dates, ["2026-12-25", "2026-12-26", "2027-01-01", "2027-01-06"]);
        assert!(hol("", "2026-01-01", "2026-12-31", "de").is_empty());
        assert!(hol("DE-XX", "2026-01-01", "2026-12-31", "de").is_empty());
        assert!(hol("DE-BY", "2027-01-01", "2026-01-01", "de").is_empty());
        assert!(hol("DE-BY", "kaputt", "2026-01-01", "de").is_empty());
        assert!(hol("DE-BY", "0001-01-01", "9999-12-31", "de").len() > 1000);
    }

    #[test]
    fn regions() {
        let de = cal_regions("de".into());
        assert_eq!(de.len(), 19);
        assert_eq!(de[0], CalRegion { code: String::new(), name: "Keine".into() });
        assert_eq!(de.iter().filter(|r| r.code.starts_with("DE-")).count(), 16);
        let en = cal_regions("en-US".into());
        assert!(en.contains(&CalRegion { code: "DE-BY".into(), name: "Bavaria".into() }));
        assert!(en.contains(&CalRegion { code: "AT".into(), name: "Austria".into() }));
        assert_eq!(cal_regions("de-AT".into())[17].name, "Österreich");
    }
}

/// A device with a real time zone database for the tests, reduced to a few zones.
#[cfg(test)]
pub(crate) mod test_zones {
    use super::*;

    const SUNDAY: u32 = 6;

    /// The device is in the named zone. Berlin and New York follow their rules since 2007,
    /// Tokyo never changes its clock.
    pub(crate) struct World(pub &'static str);

    fn at(y: i32, month: u32, nth: i32, hour: i64) -> i64 {
        nth_weekday(y, month, nth, SUNDAY).unwrap().days() * 1440 + hour * 60
    }

    impl World {
        /// Offset of `zone`; `local` tells whether `minutes` is wall-clock time or UTC.
        fn offset(&self, zone: &str, minutes: i64, local: bool) -> Option<i32> {
            let zone = if zone.is_empty() { self.0 } else { zone };
            let y = DateTime::from_minutes(minutes).date.y;
            match zone {
                "Europe/Berlin" => {
                    let (on, off) = if local { (2, 3) } else { (1, 1) };
                    let summer = minutes >= at(y, 3, -1, on) && minutes < at(y, 10, -1, off);
                    Some(if summer { 120 } else { 60 })
                }
                "America/New_York" => {
                    let (on, off) = if local { (2, 2) } else { (7, 6) };
                    let summer = minutes >= at(y, 3, 2, on) && minutes < at(y, 11, 1, off);
                    Some(if summer { -240 } else { -300 })
                }
                "Asia/Tokyo" => Some(540),
                "UTC" => Some(0),
                _ => None,
            }
        }
    }

    impl TimeZones for World {
        fn offset_at_utc(&self, zone: String, utc_minutes: i64) -> Option<i32> {
            self.offset(&zone, utc_minutes, false)
        }
        fn offset_at_local(&self, zone: String, local_minutes: i64) -> Option<i32> {
            self.offset(&zone, local_minutes, true)
        }
    }
}

/// Time zones: what a file says in UTC or in another zone must land on the right hour here.
#[cfg(test)]
mod zone_tests {
    use super::test_zones::World;
    use super::*;

    const SUNDAY: u32 = 6;

    fn file(head: &str, events: &[&str]) -> String {
        let mut s = format!("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n{head}");
        for (i, e) in events.iter().enumerate() {
            s += &format!("BEGIN:VEVENT\r\nUID:{i}@x\r\nSUMMARY:T{i}\r\n{e}\r\nEND:VEVENT\r\n");
        }
        s + "END:VCALENDAR\r\n"
    }

    fn starts(device: &'static str, head: &str, events: &[&str]) -> Vec<String> {
        ics_import_zoned(&file(head, events), &World(device)).unwrap().into_iter().map(|e| e.start).collect()
    }

    /// Google exports single events in UTC. A summer event imported in winter (and the other way
    /// round) must not be an hour off.
    #[test]
    fn utc_times_use_the_offset_of_their_own_day() {
        let got = starts(
            "Europe/Berlin",
            "",
            &[
                "DTSTART:20260714T080000Z",
                "DTSTART:20261214T080000Z",
                "DTSTART:20260329T003000Z",
                "DTSTART:20260329T010000Z",
            ],
        );
        assert_eq!(got, ["2026-07-14T10:00", "2026-12-14T09:00", "2026-03-29T01:30", "2026-03-29T03:00"]);
        let got = starts("America/New_York", "", &["DTSTART:20260714T020000Z", "DTSTART:20261214T020000Z"]);
        assert_eq!(got, ["2026-07-13T22:00", "2026-12-13T21:00"]);
    }

    #[test]
    fn times_of_another_zone_are_converted() {
        let got = starts(
            "Europe/Berlin",
            "",
            &[
                "DTSTART;TZID=America/New_York:20260714T100000",
                "DTSTART;TZID=America/New_York:20261214T100000",
                // The USA are on summer time already, Europe is not yet.
                "DTSTART;TZID=America/New_York:20260320T100000",
                "DTSTART;TZID=\"America/New_York\":20260714T100000\r\nDTEND;TZID=America/New_York:20260714T113000",
                "DTSTART;TZID=UTC:20260714T080000",
            ],
        );
        assert_eq!(
            got,
            ["2026-07-14T16:00", "2026-12-14T16:00", "2026-03-20T15:00", "2026-07-14T16:00", "2026-07-14T10:00"]
        );
        let v = ics_import_zoned(
            &file(
                "",
                &["DTSTART;TZID=America/New_York:20260714T100000\r\nDTEND;TZID=America/New_York:20260714T113000"],
            ),
            &World("Europe/Berlin"),
        )
        .unwrap();
        assert_eq!(v[0].end, "2026-07-14T17:30");
    }

    #[test]
    fn the_own_zone_and_unknown_zones_stay_as_written() {
        let got = starts(
            "Europe/Berlin",
            "",
            &[
                "DTSTART;TZID=Europe/Berlin:20260714T100000",
                "DTSTART;TZID=/mozilla.org/20050126_1/Europe/Berlin:20261214T100000",
                "DTSTART;TZID=/freeassociation.sourceforge.net/Tzfile/America/New_York:20260714T100000",
                "DTSTART;TZID=Mars/Olympus_Mons:20260714T100000",
                "DTSTART:20260714T100000",
                "DTSTART;VALUE=DATE:20260714",
            ],
        );
        assert_eq!(
            got,
            [
                "2026-07-14T10:00",
                "2026-12-14T10:00",
                "2026-07-14T16:00",
                "2026-07-14T10:00",
                "2026-07-14T10:00",
                "2026-07-14"
            ]
        );
    }

    /// Outlook names its zones the Windows way and always sends their rules along.
    #[test]
    fn zones_only_the_file_knows() {
        let outlook = "BEGIN:VTIMEZONE\r\nTZID:Eastern Standard Time\r\n\
BEGIN:STANDARD\r\nDTSTART:16010101T020000\r\nTZOFFSETFROM:-0400\r\nTZOFFSETTO:-0500\r\n\
RRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=1SU;BYMONTH=11\r\nEND:STANDARD\r\n\
BEGIN:DAYLIGHT\r\nDTSTART:16010101T020000\r\nTZOFFSETFROM:-0500\r\nTZOFFSETTO:-0400\r\n\
RRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=2SU;BYMONTH=3\r\nEND:DAYLIGHT\r\nEND:VTIMEZONE\r\n\
BEGIN:VTIMEZONE\r\nTZID:India\r\nBEGIN:STANDARD\r\nDTSTART:19700101T000000\r\n\
TZOFFSETFROM:+0530\r\nTZOFFSETTO:+0530\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\n";
        let got = starts(
            "Europe/Berlin",
            outlook,
            &[
                "DTSTART;TZID=\"Eastern Standard Time\":20260714T100000",
                "DTSTART;TZID=Eastern Standard Time:20261214T100000",
                "DTSTART;TZID=Eastern Standard Time:20260320T100000",
                // The day the clocks change over there: 01:59 is still winter time, 03:00 summer.
                "DTSTART;TZID=Eastern Standard Time:20260308T015900",
                "DTSTART;TZID=Eastern Standard Time:20260308T030000",
                "DTSTART;TZID=India:20260714T100000",
                "DTSTART;TZID=India:19600101T100000",
            ],
        );
        assert_eq!(
            got,
            [
                "2026-07-14T16:00",
                "2026-12-14T16:00",
                "2026-03-20T15:00",
                "2026-03-08T07:59",
                "2026-03-08T08:00",
                "2026-07-14T06:30",
                "1960-01-01T05:30"
            ]
        );
    }

    /// The file's own rules agree with the database for every day of several years.
    #[test]
    fn file_rules_match_the_database() {
        let rules = "BEGIN:VTIMEZONE\r\nTZID:Berlin der Datei\r\n\
BEGIN:DAYLIGHT\r\nTZOFFSETFROM:+0100\r\nTZOFFSETTO:+0200\r\nDTSTART:19700329T020000\r\n\
RRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU\r\nEND:DAYLIGHT\r\n\
BEGIN:STANDARD\r\nTZOFFSETFROM:+0200\r\nTZOFFSETTO:+0100\r\nDTSTART:19701025T030000\r\n\
RRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\n";
        let mut lines = Vec::new();
        let mut want = Vec::new();
        let mut day = Date::new(2024, 1, 1).unwrap();
        while day.y < 2031 {
            for hour in [0, 4, 12, 23] {
                let dt = DateTime::new(day, hour, 30).unwrap();
                lines.push(format!("DTSTART;TZID=Berlin der Datei:{}", dt.compact()));
                want.push(dt.to_string());
            }
            day = day.add_days(1);
        }
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        // On a device in Berlin the times of "Berlin by the file's rules" must not move at all.
        assert_eq!(starts("Europe/Berlin", rules, &refs), want);
    }

    const NY_SERIES: &str = "DTSTART;TZID=America/New_York:20260714T200000\r\nDTEND;TZID=America/New_York:20260714T210000\r\n\
RRULE:FREQ=WEEKLY;UNTIL=20260902T000000Z\r\nEXDATE;TZID=America/New_York:20260721T200000\r\nBEGIN:VALARM\r\nACTION:DISPLAY\r\n\
TRIGGER;VALUE=DATE-TIME:20260714T233000Z\r\nEND:VALARM";

    /// A series from another time zone stays on the clock of its zone: its days, the days left
    /// out and its last day are the ones of that zone. (What the calendar shows is converted per
    /// occurrence, see `series_from_other_time_zones_are_right_all_year`.)
    #[test]
    fn a_series_keeps_the_time_zone_it_came_from() {
        let v = ics_import_zoned(&file("", &[NY_SERIES]), &World("Europe/Berlin")).unwrap();
        assert_eq!(v[0].zone, "America/New_York");
        assert_eq!((v[0].start.as_str(), v[0].end.as_str()), ("2026-07-14T20:00", "2026-07-14T21:00"));
        assert_eq!(v[0].exceptions, ["2026-07-21"]);
        // 00:00 UTC on 2 September is still the evening of 1 September in New York.
        assert_eq!(v[0].repeat_until, "2026-09-01");
        assert_eq!(v[0].reminders, [30]);
        // Out and in again: the same event.
        let again = ics_import_zoned(&ics_export(&v, "X"), &World("Europe/Berlin")).unwrap();
        assert_eq!(
            (&again[0].zone, &again[0].start, &again[0].end, &again[0].exceptions, &again[0].repeat_until),
            (&v[0].zone, &v[0].start, &v[0].end, &v[0].exceptions, &v[0].repeat_until)
        );
        assert!(ics_export(&v, "X").contains("DTSTART;TZID=America/New_York:20260714T200000"));
    }

    /// No zone of its own is needed (or possible) in these cases; the old conversion applies.
    #[test]
    fn series_without_a_zone_of_their_own() {
        let zone_of = |device: &'static str, event: &str| {
            let v = ics_import_zoned(&file("", &[event]), &World(device)).unwrap();
            (v[0].zone.clone(), v[0].start.clone())
        };
        // The zone of the device itself, also under a longer name.
        let own = "DTSTART;TZID=Europe/Berlin:20260714T200000\r\nRRULE:FREQ=WEEKLY";
        assert_eq!(zone_of("Europe/Berlin", own), (String::new(), "2026-07-14T20:00".into()));
        // A device in New York reading a New York series.
        assert_eq!(zone_of("America/New_York", NY_SERIES).0, "");
        // A zone nobody knows, a single event, an all-day series, a series in UTC.
        let unknown = "DTSTART;TZID=Mars/Olympus_Mons:20260714T200000\r\nRRULE:FREQ=WEEKLY";
        assert_eq!(zone_of("Europe/Berlin", unknown), (String::new(), "2026-07-14T20:00".into()));
        assert_eq!(zone_of("Europe/Berlin", "DTSTART;TZID=America/New_York:20260714T200000").0, "");
        assert_eq!(zone_of("Europe/Berlin", "DTSTART;VALUE=DATE:20260714\r\nRRULE:FREQ=YEARLY").0, "");
        let utc = "DTSTART:20260714T180000Z\r\nRRULE:FREQ=WEEKLY";
        assert_eq!(zone_of("Europe/Berlin", utc), (String::new(), "2026-07-14T20:00".into()));
        // A fixed offset device (as in most other tests) knows no zones at all.
        assert_eq!(ics_import(&file("", &[NY_SERIES]), 60).unwrap()[0].zone, "");
    }

    /// Outlook writes Windows names and sends the rules along. If they agree with the zone the
    /// name stands for, the series gets that zone; made-up rules under a known name do not.
    #[test]
    fn windows_names_of_outlook() {
        let rules = |std: &str, dst: &str| {
            format!(
                "BEGIN:VTIMEZONE\r\nTZID:Eastern Standard Time\r\n\
BEGIN:STANDARD\r\nDTSTART:16010101T020000\r\nTZOFFSETFROM:{dst}\r\nTZOFFSETTO:{std}\r\n\
RRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=1SU;BYMONTH=11\r\nEND:STANDARD\r\n\
BEGIN:DAYLIGHT\r\nDTSTART:16010101T020000\r\nTZOFFSETFROM:{std}\r\nTZOFFSETTO:{dst}\r\n\
RRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=2SU;BYMONTH=3\r\nEND:DAYLIGHT\r\nEND:VTIMEZONE\r\n"
            )
        };
        let series = "DTSTART;TZID=Eastern Standard Time:20260714T100000\r\nDTEND;TZID=Eastern Standard Time:20260714T110000\r\nRRULE:FREQ=WEEKLY";
        let v = ics_import_zoned(&file(&rules("-0500", "-0400"), &[series]), &World("Europe/Berlin")).unwrap();
        assert_eq!((v[0].zone.as_str(), v[0].start.as_str()), ("America/New_York", "2026-07-14T10:00"));
        let v = ics_import_zoned(&file(&rules("+0300", "+0400"), &[series]), &World("Europe/Berlin")).unwrap();
        assert_eq!((v[0].zone.as_str(), v[0].start.as_str()), ("", "2026-07-14T08:00"));
    }

    /// The rules of every time zone of the world as a file carries them (tzurl.org, taken from
    /// ical4j) against what a time zone database says for 2000 to 2023
    /// (tests/data/zone_offsets.tsv, written by tools/make-zone-oracle.py).
    #[test]
    fn world_time_zones_by_the_rules_of_a_file() {
        let data = format!("{}/tests/data", env!("CARGO_MANIFEST_DIR"));
        let oracle = std::fs::read_to_string(format!("{data}/zone_offsets.tsv")).unwrap();
        let first_sample = Date::new(2000, 1, 2).unwrap().days() * 1440 + 680;
        let last_sample = Date::new(2023, 12, 28).unwrap().days() * 1440;
        let (mut zones, mut checked) = (0, 0);
        let mut wrong: Vec<String> = Vec::new();
        for line in oracle.lines().filter(|l| !l.trim().is_empty()) {
            let cols: Vec<&str> = line.split('\t').collect();
            let (name, first): (&str, i64) = (cols[0], cols[1].parse().unwrap());
            let changes: Vec<(i64, i64)> = cols
                .get(2)
                .unwrap_or(&"")
                .split_whitespace()
                .map(|c| c.split_once(':').unwrap())
                .map(|(at, off)| (at.parse().unwrap(), off.parse().unwrap()))
                .collect();
            let rules = std::fs::read_to_string(format!("{data}/zoneinfo/{name}.ics")).unwrap();
            // Some zones are another one under a second name: the file then carries the first name.
            let tzid = rules.lines().find_map(|l| l.strip_prefix("TZID:")).unwrap().trim().to_owned();
            let (mut body, mut want) = (String::new(), Vec::new());
            // Every five days at a drifting time of day, but not on the days the clock changes
            // (an hour that exists twice has two right answers).
            let mut utc = first_sample;
            while utc < last_sample {
                if changes.iter().all(|(at, _)| (at - utc).abs() > 26 * 60) {
                    let offset = changes.iter().rev().find(|(at, _)| *at <= utc).map_or(first, |c| c.1);
                    let local = DateTime::from_minutes(utc + offset);
                    body += &format!(
                        "BEGIN:VEVENT\r\nUID:{utc}\r\nSUMMARY:x\r\nDTSTART;TZID={tzid}:{}\r\nEND:VEVENT\r\n",
                        local.compact()
                    );
                    want.push(DateTime::from_minutes(utc).to_string());
                }
                utc += 5 * 1440 + 7;
            }
            let text = rules.replace("END:VCALENDAR", &format!("{body}END:VCALENDAR"));
            // This device is on UTC and knows no zone by name: only the rules of the file count.
            let got: Vec<String> =
                ics_import_zoned(&text, &FixedOffset(0)).unwrap().into_iter().map(|e| e.start).collect();
            assert_eq!(got.len(), want.len(), "{name}");
            for (g, w) in got.iter().zip(&want) {
                if g != w {
                    wrong.push(format!("{name}: {w} UTC became {g}"));
                }
            }
            zones += 1;
            checked += want.len();
        }
        assert!(zones > 400 && checked > 500_000, "{zones} zones, {checked} times");
        assert!(wrong.is_empty(), "{} of {checked} wrong, e.g. {:#?}", wrong.len(), &wrong[..wrong.len().min(25)]);
    }

    #[test]
    fn nth_weekdays() {
        let d = |y, m, n| nth_weekday(y, m, n, SUNDAY).map(|d| d.to_string());
        assert_eq!(d(2026, 3, -1).as_deref(), Some("2026-03-29"));
        assert_eq!(d(2026, 10, -1).as_deref(), Some("2026-10-25"));
        assert_eq!(d(2026, 3, 2).as_deref(), Some("2026-03-08"));
        assert_eq!(d(2026, 11, 1).as_deref(), Some("2026-11-01"));
        assert_eq!(d(2026, 2, 5), None);
        assert_eq!(d(2026, 2, -5), None);
        assert_eq!(d(2026, 13, 1), None);
        assert_eq!(d(2026, 3, 0), None);
        assert_eq!(parse_utc_offset("+0100"), Some(60));
        assert_eq!(parse_utc_offset("-0530"), Some(-330));
        assert_eq!(parse_utc_offset("+013000"), Some(90));
        assert_eq!(parse_utc_offset("0100"), None);
        assert_eq!(parse_utc_offset("+9999"), None);
    }
}
