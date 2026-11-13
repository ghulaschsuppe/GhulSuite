// SPDX-License-Identifier: AGPL-3.0-or-later
//! GhulCal: encrypted calendar vaults (`*.ghlcal`).
//!
//! A calendar vault holds two kinds of entries: `calendar` (name, colour, default reminder) and
//! `event`. Like passwords, entries sync and merge one by one. There is deliberately no CalDAV:
//! events leave the device only encrypted, exchange with other programs works via `.ics` files.

// --- exported types --------------------------------------------------------------------------

#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CalRepeat {
    #[default]
    None,
    Daily,
    /// Monday to Friday.
    Weekdays,
    Weekly,
    Biweekly,
    Monthly,
    Yearly,
}

/// Points to something else in the suite: `drive` (file name), `mail` (`account|folder|uid`),
/// `pass` (`vault|entry`).
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct CalLink {
    pub kind: String,
    pub label: String,
    pub target: String,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct CalEvent {
    /// Hex id; empty for a new event.
    pub id: String,
    /// Hex id of the calendar entry (in the same vault).
    pub calendar_id: String,
    pub title: String,
    pub all_day: bool,
    /// Timed: `YYYY-MM-DDTHH:MM`; all-day: `YYYY-MM-DD` (first day).
    pub start: String,
    /// Timed: `YYYY-MM-DDTHH:MM` (exclusive end); all-day: `YYYY-MM-DD` (last day, inclusive).
    pub end: String,
    pub repeat: CalRepeat,
    /// Last day of the series (inclusive) or empty.
    pub repeat_until: String,
    /// Single occurrences removed from the series (`YYYY-MM-DD` of their start).
    pub exceptions: Vec<String>,
    /// Minutes before the start. All-day events: minutes before 09:00 of the first day
    /// (0 = on the day at 9:00, 1440 = the day before at 9:00).
    pub reminders: Vec<u32>,
    pub location: String,
    pub notes: String,
    pub links: Vec<CalLink>,
    /// Birthdays: year of birth, shown as age (0 = none).
    pub birth_year: u32,
    pub modified: i64,
    /// Time zone of a series that came from another zone (IANA name such as
    /// `America/New_York`); empty: the clock of this device. With a zone, `start`, `end`,
    /// `repeat_until` and `exceptions` are on the clock of that zone, and every occurrence is
    /// converted for display with the offsets of its own day.
    #[uniffi(default = "")]
    pub zone: String,
}

impl Default for CalEvent {
    fn default() -> Self {
        CalEvent {
            id: String::new(),
            calendar_id: String::new(),
            title: String::new(),
            all_day: false,
            start: String::new(),
            end: String::new(),
            repeat: CalRepeat::None,
            repeat_until: String::new(),
            exceptions: Vec::new(),
            reminders: Vec::new(),
            location: String::new(),
            notes: String::new(),
            links: Vec::new(),
            birth_year: 0,
            modified: 0,
            zone: String::new(),
        }
    }
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct CalCalendar {
    /// Hex id; empty for a new calendar.
    pub id: String,
    pub vault_id: String,
    pub name: String,
    /// 0xRRGGBB.
    pub color: u32,
    /// Default reminder for new events in minutes, -1 = none.
    pub default_reminder: i32,
    /// Number of events (series count once).
    pub events: u32,
}

#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalDeleteMode {
    /// Only the occurrence on the given day.
    One,
    /// The occurrence on the given day and all later ones.
    Following,
    /// The whole series.
    All,
}

/// One occurrence of an event (series expanded) or a public holiday.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct CalOccurrence {
    /// Empty for holidays.
    pub event_id: String,
    /// `holidays` for public holidays.
    pub calendar_id: String,
    pub title: String,
    pub all_day: bool,
    /// Same forms as [`CalEvent::start`] / [`CalEvent::end`].
    pub start: String,
    pub end: String,
    pub location: String,
    pub repeating: bool,
    pub has_reminders: bool,
    pub has_links: bool,
    /// Birthdays: the age reached on this day (0 = none).
    pub age: u32,
    pub color: u32,
}

/// A reminder to schedule on the device.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct CalReminder {
    pub event_id: String,
    pub title: String,
    pub location: String,
    pub all_day: bool,
    /// Start of the occurrence (`YYYY-MM-DDTHH:MM`, all-day: midnight).
    pub start: String,
    pub end: String,
    /// When to notify (`YYYY-MM-DDTHH:MM`, floating local time).
    pub at: String,
    pub minutes_before: u32,
}

pub const HOLIDAYS_ID: &str = "holidays";
const HOLIDAY_COLOR: u32 = 0x6f7187;

// --- storage -----------------------------------------------------------------------------------

use std::collections::BTreeMap;

use super::caldate::{Date, DateTime};
use super::error::{GhulError, Result};
use super::suite::{new_id, now_ms, text_field, GhulSuite, Open};
use crate::vault::{Entry, Field, UnlockedContainer};

const T_CAL: &str = "calendar";
const T_EVENT: &str = "event";
const SEP: char = '\u{1f}';

fn repeat_code(r: CalRepeat) -> &'static str {
    match r {
        CalRepeat::None => "",
        CalRepeat::Daily => "d",
        CalRepeat::Weekdays => "wd",
        CalRepeat::Weekly => "w",
        CalRepeat::Biweekly => "w2",
        CalRepeat::Monthly => "m",
        CalRepeat::Yearly => "y",
    }
}

fn repeat_of(s: &str) -> CalRepeat {
    match s {
        "d" => CalRepeat::Daily,
        "wd" => CalRepeat::Weekdays,
        "w" => CalRepeat::Weekly,
        "w2" => CalRepeat::Biweekly,
        "m" => CalRepeat::Monthly,
        "y" => CalRepeat::Yearly,
        _ => CalRepeat::None,
    }
}

fn int(e: &Entry, k: &str, d: i64) -> i64 {
    match e.fields.get(k) {
        Some(Field::Int(x)) => *x,
        _ => d,
    }
}

fn list(e: &Entry, k: &str) -> Vec<String> {
    match e.fields.get(k) {
        Some(Field::List(l)) => l.clone(),
        _ => Vec::new(),
    }
}

fn hex_id(e: &Entry) -> String {
    crate::kdf::hex(&e.id)
}

fn calendar_of(vault_id: &str, e: &Entry, events: u32) -> CalCalendar {
    CalCalendar {
        id: hex_id(e),
        vault_id: vault_id.to_owned(),
        name: text_field(e, "name"),
        color: int(e, "color", 0x2f74e8) as u32,
        default_reminder: int(e, "reminder", 15) as i32,
        events,
    }
}

pub(crate) fn event_of(e: &Entry) -> CalEvent {
    CalEvent {
        id: hex_id(e),
        calendar_id: text_field(e, "calendar"),
        title: text_field(e, "title"),
        all_day: matches!(e.fields.get("all_day"), Some(Field::Bool(true))),
        start: text_field(e, "start"),
        end: text_field(e, "end"),
        repeat: repeat_of(&text_field(e, "repeat")),
        repeat_until: text_field(e, "until"),
        exceptions: list(e, "exceptions"),
        reminders: list(e, "reminders").iter().filter_map(|s| s.parse().ok()).collect(),
        location: text_field(e, "location"),
        notes: text_field(e, "notes"),
        links: list(e, "links")
            .iter()
            .filter_map(|s| {
                let mut p = s.splitn(3, SEP);
                Some(CalLink {
                    kind: p.next()?.to_owned(),
                    label: p.next()?.to_owned(),
                    target: p.next().unwrap_or("").to_owned(),
                })
            })
            .collect(),
        birth_year: int(e, "birth_year", 0) as u32,
        modified: e.modified,
        zone: text_field(e, "zone"),
    }
}

fn put_event_fields(e: &mut Entry, ev: &CalEvent) {
    let f = &mut e.fields;
    f.clear();
    let mut text = |k: &str, v: &str| {
        if !v.is_empty() {
            f.insert(k.into(), Field::Text(v.to_owned()));
        }
    };
    text("calendar", &ev.calendar_id);
    text("title", super::drive::trim_invisible(&ev.title));
    text("start", &ev.start);
    text("end", &ev.end);
    text("repeat", repeat_code(ev.repeat));
    text("until", &ev.repeat_until);
    text("location", ev.location.trim());
    text("notes", ev.notes.trim());
    text("zone", &ev.zone);
    f.insert("all_day".into(), Field::Bool(ev.all_day));
    if !ev.exceptions.is_empty() {
        f.insert("exceptions".into(), Field::List(ev.exceptions.clone()));
    }
    if !ev.reminders.is_empty() {
        let mut r = ev.reminders.clone();
        r.sort_unstable();
        r.dedup();
        f.insert("reminders".into(), Field::List(r.iter().map(u32::to_string).collect()));
    }
    if !ev.links.is_empty() {
        let l = ev.links.iter().map(|l| format!("{}{SEP}{}{SEP}{}", l.kind, l.label, l.target)).collect();
        f.insert("links".into(), Field::List(l));
    }
    if ev.birth_year > 0 {
        f.insert("birth_year".into(), Field::Int(ev.birth_year as i64));
    }
}

impl GhulSuite {
    fn time_zones(&self) -> Option<std::sync::Arc<dyn super::caltext::TimeZones>> {
        self.zones.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
}

/// Checks and normalises an event; returns (start, end) for convenience.
fn validate(ev: &mut CalEvent) -> Result<()> {
    let bad = |m: &str| GhulError::InvalidInput { msg: m.into() };
    if super::drive::trim_invisible(&ev.title).is_empty() {
        return Err(bad("title required"));
    }
    if ev.all_day {
        let s = Date::parse(&ev.start).ok_or_else(|| bad("invalid start"))?;
        let e = Date::parse(&ev.end).unwrap_or(s);
        ev.start = s.to_string();
        ev.end = e.max(s).to_string();
    } else {
        let s = DateTime::parse(&ev.start).ok_or_else(|| bad("invalid start"))?;
        let e = DateTime::parse(&ev.end).filter(|e| *e > s).unwrap_or(s.add_minutes(60));
        ev.start = s.to_string();
        ev.end = e.to_string();
    }
    if !ev.repeat_until.is_empty() {
        ev.repeat_until = Date::parse(&ev.repeat_until).ok_or_else(|| bad("invalid end of series"))?.to_string();
    }
    ev.exceptions.retain(|d| Date::parse(d).is_some());
    ev.exceptions.sort();
    ev.exceptions.dedup();
    ev.reminders.retain(|r| *r <= MAX_REMINDER_MINUTES);
    let plain_name = |z: &str| z.len() <= 64 && z.bytes().all(|b| b.is_ascii_alphanumeric() || b"/_-+".contains(&b));
    if ev.all_day || !plain_name(&ev.zone) {
        ev.zone.clear();
    }
    Ok(())
}

/// A reminder comes at most a year ahead; files of other programs carry odd ones (decades).
pub(crate) const MAX_REMINDER_MINUTES: u32 = 366 * 1440;

fn tombstone(e: &mut Entry) {
    e.fields.clear();
    e.tags.clear();
    e.folder = None;
    e.extra.clear();
    e.deleted = true;
    e.revision += 1;
    e.modified = now_ms();
}

fn live<'a>(c: &'a UnlockedContainer, t: &'a str) -> impl Iterator<Item = &'a Entry> + 'a {
    c.payload.entries.iter().filter(move |e| !e.deleted && e.entry_type == t)
}

fn cal_vault_ids(o: &Open) -> Vec<String> {
    o.vaults
        .iter()
        .filter(|v| v.reference.kind == "cal" && v.container.is_some())
        .map(|v| v.reference.id.clone())
        .collect()
}

/// Vault holding the calendar or event with this hex id.
fn vault_with(o: &mut Open, id: &str, t: &str) -> Result<String> {
    for vid in cal_vault_ids(o) {
        let (_, c) = o.vault_mut(&vid)?;
        if live(c, t).any(|e| hex_id(e) == id) {
            return Ok(vid);
        }
    }
    Err(GhulError::NotFound)
}

fn find_mut<'a>(c: &'a mut UnlockedContainer, id: &str, t: &str) -> Result<&'a mut Entry> {
    c.payload.entries.iter_mut().find(|e| !e.deleted && e.entry_type == t && hex_id(e) == id).ok_or(GhulError::NotFound)
}

// --- series ------------------------------------------------------------------------------------

/// Start of the `k`-th occurrence of a series beginning at `base`.
fn nth(base: DateTime, repeat: CalRepeat, k: i64) -> DateTime {
    let date = match repeat {
        CalRepeat::None => base.date,
        CalRepeat::Daily => base.date.add_days(k),
        CalRepeat::Weekly => base.date.add_days(7 * k),
        CalRepeat::Biweekly => base.date.add_days(14 * k),
        CalRepeat::Monthly => base.date.add_months(k as i32),
        CalRepeat::Yearly => base.date.add_months(12 * k as i32),
        CalRepeat::Weekdays => {
            // Whole weeks plus remaining working days, starting from the base's working day.
            let mut d = base.date;
            while d.weekday() >= 5 {
                d = d.add_days(1);
            }
            let wd = d.weekday() as i64 + k;
            d.add_days(wd.div_euclid(5) * 7 + wd.rem_euclid(5) - d.weekday() as i64)
        }
    };
    DateTime { date, min: base.min }
}

/// Occurrences (start, end) of `ev` that touch the days `[from, to]`. All-day: `end` is the
/// last day at midnight; timed: exclusive end.
pub(crate) fn expand(ev: &CalEvent, from: Date, to: Date) -> Vec<(DateTime, DateTime)> {
    let (start, end) = if ev.all_day {
        match (Date::parse(&ev.start), Date::parse(&ev.end)) {
            (Some(s), Some(e)) => (DateTime { date: s, min: 0 }, DateTime { date: e.max(s), min: 0 }),
            (Some(s), None) => (DateTime { date: s, min: 0 }, DateTime { date: s, min: 0 }),
            _ => return Vec::new(),
        }
    } else {
        match (DateTime::parse(&ev.start), DateTime::parse(&ev.end)) {
            (Some(s), Some(e)) if e > s => (s, e),
            (Some(s), _) => (s, s.add_minutes(60)),
            _ => return Vec::new(),
        }
    };
    let span = end.minutes() - start.minutes();
    let last_day = |s: DateTime| {
        let e = DateTime::from_minutes(s.minutes() + span);
        if ev.all_day || e.min > 0 {
            e.date
        } else {
            e.date.add_days(-1)
        }
    };
    let until = Date::parse(&ev.repeat_until);
    let mut out = Vec::new();
    if ev.repeat == CalRepeat::None {
        if start.date <= to && last_day(start) >= from {
            out.push((start, end));
        }
        return out;
    }
    let span_days = span / 1440 + 2;
    let gap = from.days() - start.date.days() - span_days;
    let k0 = match ev.repeat {
        CalRepeat::Daily => gap,
        CalRepeat::Weekdays => gap * 5 / 7 - 5,
        CalRepeat::Weekly => gap / 7,
        CalRepeat::Biweekly => gap / 14,
        CalRepeat::Monthly => gap / 31,
        CalRepeat::Yearly => gap / 366,
        CalRepeat::None => 0,
    }
    .max(0);
    for k in k0..k0 + 5000 {
        let s = nth(start, ev.repeat, k);
        if s.date > to || until.is_some_and(|u| s.date > u) {
            break;
        }
        if last_day(s) < from || ev.exceptions.iter().any(|x| x == &s.date.to_string()) {
            continue;
        }
        out.push((s, s.add_minutes(span)));
    }
    out
}

/// Occurrences of `ev` on the clock of this device, each with the day it has in the event's own
/// time zone (exceptions and the end of a series are counted in those days). An event without a
/// zone, or with one this device does not know, is taken as written.
fn expand_local(
    ev: &CalEvent,
    from: Date,
    to: Date,
    zones: Option<&dyn super::caltext::TimeZones>,
) -> Vec<(DateTime, DateTime, Date)> {
    let as_written = || expand(ev, from, to).into_iter().map(|(s, e)| (s, e, s.date)).collect();
    let Some(z) = zones.filter(|_| !ev.zone.is_empty() && !ev.all_day) else {
        return as_written();
    };
    let mut out = Vec::new();
    // Two days more on both sides: no zone is further away than that.
    for (s, e) in expand(ev, from.add_days(-2), to.add_days(2)) {
        let (Some(ls), Some(le)) =
            (super::caltext::wall_to_device(z, &ev.zone, s), super::caltext::wall_to_device(z, &ev.zone, e))
        else {
            return as_written();
        };
        let last_day = if le.min > 0 { le.date } else { le.date.add_days(-1) };
        if ls.date <= to && last_day.max(ls.date) >= from {
            out.push((ls, le.max(ls), s.date));
        }
    }
    out
}

fn fmt_occ(all_day: bool, s: DateTime, e: DateTime) -> (String, String) {
    if all_day {
        (s.date.to_string(), e.date.to_string())
    } else {
        (s.to_string(), e.to_string())
    }
}

// --- API ---------------------------------------------------------------------------------------

#[uniffi::export]
impl GhulSuite {
    /// All calendars of all calendar vaults, in the order they were created.
    pub fn cal_calendars(&self) -> Result<Vec<CalCalendar>> {
        self.with_open(|o| {
            let mut out = Vec::new();
            for vid in cal_vault_ids(o) {
                let (_, c) = o.vault_mut(&vid)?;
                let mut count: BTreeMap<String, u32> = BTreeMap::new();
                for e in live(c, T_EVENT) {
                    *count.entry(text_field(e, "calendar")).or_default() += 1;
                }
                let mut cals: Vec<(i64, CalCalendar)> = live(c, T_CAL)
                    .map(|e| {
                        (int(e, "order", e.modified), calendar_of(&vid, e, count.get(&hex_id(e)).copied().unwrap_or(0)))
                    })
                    .collect();
                cals.sort_by_key(|(order, _)| *order);
                out.extend(cals.into_iter().map(|(_, c)| c));
            }
            Ok(out)
        })
    }

    /// Creates (empty id) or updates a calendar in `cal.vault_id`; returns its id.
    pub fn cal_save_calendar(&self, cal: CalCalendar) -> Result<String> {
        if cal.name.trim().is_empty() {
            return Err(GhulError::InvalidInput { msg: "name required".into() });
        }
        self.with_open(|o| {
            let (r, c) = o.vault_mut(&cal.vault_id)?;
            if r.kind != "cal" {
                return Err(GhulError::InvalidInput { msg: "not a calendar vault".into() });
            }
            let now = now_ms();
            let e = if cal.id.is_empty() {
                let order = live(c, T_CAL).map(|e| int(e, "order", 0)).max().map_or(0, |m| m + 1);
                let mut e = Entry::new(new_id(), T_CAL, now);
                e.fields.insert("order".into(), Field::Int(order));
                c.payload.entries.push(e);
                c.payload.entries.last_mut().expect("just pushed")
            } else {
                let e = find_mut(c, &cal.id, T_CAL)?;
                e.revision += 1;
                e.modified = now;
                e
            };
            e.fields.insert("name".into(), Field::Text(cal.name.trim().to_owned()));
            e.fields.insert("color".into(), Field::Int((cal.color & 0xffffff) as i64));
            e.fields.insert("reminder".into(), Field::Int(cal.default_reminder.max(-1) as i64));
            let id = hex_id(e);
            self.save_vault(o, &cal.vault_id)?;
            Ok(id)
        })
    }

    /// Deletes a calendar together with its events.
    pub fn cal_delete_calendar(&self, id: String) -> Result<()> {
        self.with_open(|o| {
            let vid = vault_with(o, &id, T_CAL)?;
            let (_, c) = o.vault_mut(&vid)?;
            for e in c.payload.entries.iter_mut().filter(|e| !e.deleted) {
                if (e.entry_type == T_CAL && hex_id(e) == id)
                    || (e.entry_type == T_EVENT && text_field(e, "calendar") == id)
                {
                    tombstone(e);
                }
            }
            self.save_vault(o, &vid)
        })
    }

    pub fn cal_get_event(&self, id: String) -> Result<CalEvent> {
        self.with_open(|o| {
            let vid = vault_with(o, &id, T_EVENT)?;
            let (_, c) = o.vault_mut(&vid)?;
            Ok(event_of(find_mut(c, &id, T_EVENT)?))
        })
    }

    /// Creates (empty id) or updates an event in the vault of its calendar (moving it there if
    /// the calendar lives in another vault). Returns the event id.
    pub fn cal_save_event(&self, event: CalEvent) -> Result<String> {
        let mut ev = event;
        validate(&mut ev)?;
        self.with_open(|o| {
            let target = vault_with(o, &ev.calendar_id, T_CAL)?;
            let now = now_ms();
            if !ev.id.is_empty() {
                let source = vault_with(o, &ev.id, T_EVENT)?;
                if source == target {
                    let (_, c) = o.vault_mut(&target)?;
                    let e = find_mut(c, &ev.id, T_EVENT)?;
                    put_event_fields(e, &ev);
                    e.revision += 1;
                    e.modified = now;
                    self.save_vault(o, &target)?;
                    return Ok(ev.id.clone());
                }
                let (_, c) = o.vault_mut(&source)?;
                tombstone(find_mut(c, &ev.id, T_EVENT)?);
                self.save_vault(o, &source)?;
            }
            let (_, c) = o.vault_mut(&target)?;
            let mut e = Entry::new(new_id(), T_EVENT, now);
            put_event_fields(&mut e, &ev);
            let id = hex_id(&e);
            c.payload.entries.push(e);
            self.save_vault(o, &target)?;
            Ok(id)
        })
    }

    /// Deletes an event, one occurrence of a series (`date` = its start day) or the series from
    /// that day on.
    pub fn cal_delete_event(&self, id: String, date: String, mode: CalDeleteMode) -> Result<()> {
        let zones = self.time_zones();
        self.with_open(|o| {
            let vid = vault_with(o, &id, T_EVENT)?;
            let (_, c) = o.vault_mut(&vid)?;
            let e = find_mut(c, &id, T_EVENT)?;
            let mut ev = event_of(e);
            // The day comes from the calendar (clock of this device); a series with its own
            // time zone counts its days there.
            let day = Date::parse(&date).map(|d| {
                expand_local(&ev, d, d, zones.as_deref()).into_iter().find(|(s, _, _)| s.date == d).map_or(d, |o| o.2)
            });
            let first = Date::parse(ev.start.get(..10).unwrap_or(""));
            match (mode, day) {
                (CalDeleteMode::One, Some(d)) if ev.repeat != CalRepeat::None => {
                    ev.exceptions.push(d.to_string());
                    validate(&mut ev)?;
                    put_event_fields(e, &ev);
                    e.revision += 1;
                    e.modified = now_ms();
                }
                (CalDeleteMode::Following, Some(d)) if ev.repeat != CalRepeat::None && first.is_some_and(|f| d > f) => {
                    ev.repeat_until = d.add_days(-1).to_string();
                    put_event_fields(e, &ev);
                    e.revision += 1;
                    e.modified = now_ms();
                }
                _ => tombstone(e),
            }
            self.save_vault(o, &vid)
        })
    }

    /// Occurrences touching the days `[from, to]` (inclusive), sorted by start; with public
    /// holidays of `holiday_region` (empty = none) in `lang`.
    pub fn cal_occurrences(
        &self,
        from: String,
        to: String,
        holiday_region: String,
        lang: String,
    ) -> Result<Vec<CalOccurrence>> {
        let (Some(f), Some(t)) = (Date::parse(&from), Date::parse(&to)) else {
            return Err(GhulError::InvalidInput { msg: "invalid range".into() });
        };
        let zones = self.time_zones();
        let mut out = self.with_open(|o| {
            let mut out = Vec::new();
            for vid in cal_vault_ids(o) {
                let (_, c) = o.vault_mut(&vid)?;
                let colors: BTreeMap<String, u32> =
                    live(c, T_CAL).map(|e| (hex_id(e), int(e, "color", 0x2f74e8) as u32)).collect();
                for e in live(c, T_EVENT) {
                    let ev = event_of(e);
                    let Some(&color) = colors.get(&ev.calendar_id) else { continue };
                    for (s, en, _) in expand_local(&ev, f, t, zones.as_deref()) {
                        let (start, end) = fmt_occ(ev.all_day, s, en);
                        out.push(CalOccurrence {
                            event_id: ev.id.clone(),
                            calendar_id: ev.calendar_id.clone(),
                            title: ev.title.clone(),
                            all_day: ev.all_day,
                            start,
                            end,
                            location: ev.location.clone(),
                            repeating: ev.repeat != CalRepeat::None,
                            has_reminders: !ev.reminders.is_empty(),
                            has_links: !ev.links.is_empty(),
                            age: if ev.birth_year > 0 { (s.date.y - ev.birth_year as i32).max(0) as u32 } else { 0 },
                            color,
                        });
                    }
                }
            }
            Ok(out)
        })?;
        if !holiday_region.is_empty() {
            for h in super::caltext::cal_holidays(holiday_region, f.to_string(), t.to_string(), lang) {
                out.push(CalOccurrence {
                    event_id: String::new(),
                    calendar_id: HOLIDAYS_ID.into(),
                    title: h.name,
                    all_day: true,
                    start: h.date.clone(),
                    end: h.date,
                    location: String::new(),
                    repeating: false,
                    has_reminders: false,
                    has_links: false,
                    age: 0,
                    color: HOLIDAY_COLOR,
                });
            }
        }
        // All-day first on their day (they start at midnight), then by start and end.
        out.sort_by(|a, b| {
            let key = |o: &CalOccurrence| {
                (o.start.get(..10).unwrap_or("").to_owned(), !o.all_day, o.start.clone(), o.end.clone())
            };
            key(a).cmp(&key(b))
        });
        Ok(out)
    }

    /// Reminders due from `now` (`YYYY-MM-DDTHH:MM`) over the next `days` days, sorted by time.
    /// The platform schedules them so they arrive while the app is locked.
    pub fn cal_reminders(&self, now: String, days: u32) -> Result<Vec<CalReminder>> {
        let now = DateTime::parse(&now).ok_or_else(|| GhulError::InvalidInput { msg: "invalid time".into() })?;
        let horizon = now.add_minutes(days as i64 * 1440);
        let zones = self.time_zones();
        self.with_open(|o| {
            let mut out = Vec::new();
            for vid in cal_vault_ids(o) {
                let (_, c) = o.vault_mut(&vid)?;
                let cals: Vec<String> = live(c, T_CAL).map(hex_id).collect();
                for e in live(c, T_EVENT) {
                    let ev = event_of(e);
                    if ev.reminders.is_empty() || !cals.contains(&ev.calendar_id) {
                        continue;
                    }
                    let max_before = *ev.reminders.iter().max().unwrap_or(&0) as i64;
                    let to = horizon.add_minutes(max_before + 1440).date;
                    for (s, en, _) in expand_local(&ev, now.date, to, zones.as_deref()) {
                        let anchor = if ev.all_day { s.add_minutes(9 * 60) } else { s };
                        for &m in &ev.reminders {
                            let at = anchor.add_minutes(-(m as i64));
                            if at >= now && at < horizon {
                                let (start, end) = fmt_occ(false, s, en);
                                out.push(CalReminder {
                                    event_id: ev.id.clone(),
                                    title: ev.title.clone(),
                                    location: ev.location.clone(),
                                    all_day: ev.all_day,
                                    start,
                                    end,
                                    at: at.to_string(),
                                    minutes_before: m,
                                });
                            }
                        }
                    }
                }
            }
            out.sort_by(|a, b| a.at.cmp(&b.at).then(a.title.cmp(&b.title)));
            Ok(out)
        })
    }

    /// Hands over the time zone database of this device (the core carries none). Series that
    /// live in another time zone are shown on the clock of this device with it; without it they
    /// are shown as written.
    pub fn set_time_zones(&self, zones: std::sync::Arc<dyn super::caltext::TimeZones>) {
        *self.zones.lock().unwrap_or_else(|p| p.into_inner()) = Some(zones);
    }

    /// The given calendars (all if empty) as one `.ics` file.
    pub fn cal_export_ics(&self, calendar_ids: Vec<String>) -> Result<String> {
        self.with_open(|o| {
            let mut events = Vec::new();
            let mut names = Vec::new();
            for vid in cal_vault_ids(o) {
                let (_, c) = o.vault_mut(&vid)?;
                for e in live(c, T_CAL) {
                    if calendar_ids.is_empty() || calendar_ids.contains(&hex_id(e)) {
                        names.push(text_field(e, "name"));
                    }
                }
                events.extend(
                    live(c, T_EVENT)
                        .map(event_of)
                        .filter(|ev| calendar_ids.is_empty() || calendar_ids.contains(&ev.calendar_id)),
                );
            }
            let name = if names.len() == 1 { names.remove(0) } else { "GhulCal".to_owned() };
            Ok(super::caltext::ics_export(&events, &name))
        })
    }

    /// One event as `.ics` (to share or attach to a mail).
    pub fn cal_event_ics(&self, id: String) -> Result<String> {
        let ev = self.cal_get_event(id)?;
        Ok(super::caltext::ics_export(&[ev], "GhulCal"))
    }

    /// Imports the events of an `.ics` file into a calendar; returns how many were added.
    /// Events already present (same title, start and end) are skipped. Times in UTC or in another
    /// time zone become wall-clock time of this device, with the offsets `zones` gives for that day.
    pub fn cal_import_ics(
        &self,
        calendar_id: String,
        data: Vec<u8>,
        zones: std::sync::Arc<dyn super::caltext::TimeZones>,
    ) -> Result<u32> {
        let text = String::from_utf8_lossy(&data);
        let events =
            super::caltext::ics_import_zoned(&text, zones.as_ref()).map_err(|msg| GhulError::InvalidInput { msg })?;
        self.with_open(|o| {
            let vid = vault_with(o, &calendar_id, T_CAL)?;
            let (_, c) = o.vault_mut(&vid)?;
            let key = |e: &CalEvent| format!("{}{SEP}{}{SEP}{}", e.title.trim().to_lowercase(), e.start, e.end);
            let mut seen: Vec<String> =
                live(c, T_EVENT).map(event_of).filter(|e| e.calendar_id == calendar_id).map(|e| key(&e)).collect();
            let mut added = 0;
            let now = now_ms();
            for mut ev in events {
                ev.calendar_id = calendar_id.clone();
                ev.id.clear();
                if validate(&mut ev).is_err() || seen.contains(&key(&ev)) {
                    continue;
                }
                seen.push(key(&ev));
                let mut e = Entry::new(new_id(), T_EVENT, now);
                put_event_fields(&mut e, &ev);
                c.payload.entries.push(e);
                added += 1;
            }
            if added > 0 {
                self.save_vault(o, &vid)?;
            }
            Ok(added)
        })
    }
}

/// Plain export of a whole calendar vault (for "export unencrypted").
pub(crate) fn export_vault_ics(c: &UnlockedContainer, name: &str) -> zeroize::Zeroizing<String> {
    let events: Vec<CalEvent> = live(c, T_EVENT).map(event_of).collect();
    zeroize::Zeroizing::new(super::caltext::ics_export(&events, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(start: &str, end: &str, all_day: bool, repeat: CalRepeat) -> CalEvent {
        CalEvent { title: "t".into(), start: start.into(), end: end.into(), all_day, repeat, ..Default::default() }
    }

    fn days(v: &[(DateTime, DateTime)]) -> Vec<String> {
        v.iter().map(|(s, _)| s.to_string()).collect()
    }

    #[test]
    fn expands_series() {
        let d = |s: &str| Date::parse(s).unwrap();
        // Weekdays from Monday 9 Nov 2026: two weeks have ten working days.
        let e = ev("2026-11-09T09:30", "2026-11-09T09:45", false, CalRepeat::Weekdays);
        let v = expand(&e, d("2026-11-09"), d("2026-11-22"));
        assert_eq!(v.len(), 10);
        assert!(v.iter().all(|(s, _)| s.date.weekday() < 5 && s.min == 570));
        // Starting on a Saturday moves to Monday.
        let e = ev("2026-11-14T08:00", "2026-11-14T09:00", false, CalRepeat::Weekdays);
        assert_eq!(days(&expand(&e, d("2026-11-14"), d("2026-11-17")))[0], "2026-11-16T08:00");
        // Monthly on the 31st clamps, then returns to the 31st.
        let e = ev("2026-01-31T10:00", "2026-01-31T11:00", false, CalRepeat::Monthly);
        assert_eq!(days(&expand(&e, d("2026-02-01"), d("2026-03-31"))), vec!["2026-02-28T10:00", "2026-03-31T10:00"]);
        // Far in the future without walking every step from the start.
        let e = ev("2020-01-06T18:30", "2020-01-06T19:30", false, CalRepeat::Biweekly);
        let v = expand(&e, d("2026-11-01"), d("2026-11-30"));
        assert_eq!(days(&v), vec!["2026-11-02T18:30", "2026-11-16T18:30", "2026-11-30T18:30"]);
        assert!(v.iter().all(|(s, _)| s.date.weekday() == 0));
        // Exceptions and end of series.
        let mut e = ev("2026-11-02T07:00", "2026-11-02T08:00", false, CalRepeat::Weekly);
        e.exceptions = vec!["2026-11-09".into()];
        e.repeat_until = "2026-11-23".into();
        assert_eq!(
            days(&expand(&e, d("2026-11-01"), d("2026-12-31"))),
            vec!["2026-11-02T07:00", "2026-11-16T07:00", "2026-11-23T07:00"]
        );
        // Multi-day all-day event is found from a day in its middle.
        let e = ev("2026-12-06", "2026-12-13", true, CalRepeat::None);
        assert_eq!(expand(&e, d("2026-12-10"), d("2026-12-10")).len(), 1);
        // Yearly birthday from 1948.
        let e = ev("1948-11-15", "1948-11-15", true, CalRepeat::Yearly);
        assert_eq!(days(&expand(&e, d("2026-11-01"), d("2026-11-30"))), vec!["2026-11-15T00:00"]);
        // A timed event ending at midnight does not touch the next day.
        let e = ev("2026-11-12T22:00", "2026-11-13T00:00", false, CalRepeat::None);
        assert!(expand(&e, d("2026-11-13"), d("2026-11-13")).is_empty());
    }
}
