// SPDX-License-Identifier: AGPL-3.0-or-later
//! GhulCal text formats: iCalendar (.ics) import/export, quick add, public holidays.
//!
//! Everything here works on plain text and the calendar types of [`super::cal`]; nothing touches
//! a vault. Times are floating local wall-clock times (see [`super::caldate`]).

use super::cal::{CalEvent, CalRepeat};
use super::caldate::{Date, DateTime};

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
        l.push(format!("DTSTART:{}", start.compact()));
        l.push(format!("DTEND:{}", end.compact()));
        if let Some(rule) = rrule_text(ev.repeat, &ev.repeat_until, Some(start)) {
            l.push(format!("RRULE:{rule}"));
        }
        let ex: Vec<String> =
            exception_dates(&ev.exceptions).iter().map(|d| DateTime { date: *d, min: start.min }.compact()).collect();
        if ev.repeat != CalRepeat::None && !ex.is_empty() {
            l.push(format!("EXDATE:{}", ex.join(",")));
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

/// Parses one date or date-time value. `Z` (UTC) is converted to local time with the offset,
/// `TZID` is ignored (treated as floating local time).
fn parse_ics_time(value: &str, value_param: Option<&str>, offset: i32) -> Option<IcsTime> {
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
        dt = dt.add_minutes(offset as i64);
    }
    Some(IcsTime { dt, is_date: false })
}

fn prop_time(p: &Prop, offset: i32) -> Option<IcsTime> {
    parse_ics_time(&p.value, p.param("VALUE"), offset)
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

/// Reads an `.ics` file. `utc_offset_minutes` converts UTC times (`…Z`) to local wall-clock time.
/// Ids and calendar ids of the result are empty: the caller assigns them.
pub fn ics_import(text: &str, utc_offset_minutes: i32) -> Result<Vec<CalEvent>, String> {
    let offset = utc_offset_minutes.clamp(-24 * 60, 24 * 60);
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
        let rid = raw.recurrence_id.as_ref().and_then(|p| prop_time(p, offset)).map(|t| t.dt.date);
        let is_override = raw.recurrence_id.is_some();
        let cancelled = raw.cancelled;
        let uid = raw.uid.clone();
        let ev = finish_event(raw, offset);
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

fn finish_event(raw: RawEvent, offset: i32) -> Option<CalEvent> {
    let st = prop_time(raw.dtstart.as_ref()?, offset)?;
    let end = raw.dtend.as_ref().and_then(|p| prop_time(p, offset));
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
        let (rep, until) = map_rrule(rule, start_date, offset);
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
                    .filter_map(|v| parse_ics_time(v, p.param("VALUE"), offset))
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
                prop_time(t, offset)?.dt.minutes() - start_min
            } else {
                parse_duration(&t.value)?
            };
            let before = if ev.all_day { 540 - rel } else { -rel };
            Some(before.clamp(0, u32::MAX as i64) as u32)
        })
        .collect();
    rem.sort_unstable_by(|a, b| b.cmp(a));
    rem.dedup();
    ev.reminders = rem;
    Some(ev)
}

/// Maps the supported `RRULE` subset onto [`CalRepeat`]; returns the last day if known.
fn map_rrule(rule: &str, start: Date, offset: i32) -> (CalRepeat, Option<Date>) {
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
            "UNTIL" => until = parse_ics_time(v, None, offset).map(|t| t.dt.date),
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
    matches!(c, '-' | '–' | '—')
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
        // Split "14-15", "18:30–19:30", "2-3pm" into time, dash, time; keep "Team-Call" whole.
        let chars: Vec<char> = word.chars().collect();
        let mut piece = String::new();
        let mut first = true;
        for (i, &c) in chars.iter().enumerate() {
            let split = is_dash(c)
                && i > 0
                && i + 1 < chars.len()
                && chars[i - 1].is_ascii_digit()
                && chars[i + 1].is_ascii_digit();
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
        assert_eq!(q("Workshop 14:30–16:00 übermorgen"), timed("Workshop", "2026-11-14", "14:30", "16:00"));
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
