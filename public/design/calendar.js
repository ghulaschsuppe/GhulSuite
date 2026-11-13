// SPDX-License-Identifier: AGPL-3.0-or-later
// GhulCal design preview: month, week, day and list views, series, reminders, holidays, quick add,
// .ics exchange and notifications. Extends design.js; all data is made up and stays in the browser tab.
'use strict';

Object.assign(P, {
  cal: '<rect x="3" y="5" width="18" height="16" rx="2"/><path d="M3 10h18M8 3v4M16 3v4"/>',
  repeat: '<path d="m17 2 4 4-4 4"/><path d="M3 12v-2a4 4 0 0 1 4-4h14M7 22l-4-4 4-4"/><path d="M21 12v2a4 4 0 0 1-4 4H3"/>',
  pinmap: '<path d="M12 21s-7-6.2-7-12a7 7 0 0 1 14 0c0 5.8-7 12-7 12z"/><circle cx="12" cy="9" r="2.5"/>',
  prev: '<path d="m15 6-6 6 6 6"/>',
  zap: '<path d="M13 2 4 14h7l-1 8 9-12h-7z"/>',
  cake: '<path d="M4 21h16v-8a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2zM4 16c2 0 2-1.5 4-1.5s2 1.5 4 1.5 2-1.5 4-1.5 2 1.5 4 1.5M12 11V7M12 4.5h0"/>',
});

// --- dates ---------------------------------------------------------------------------------------
const DAYMS = 864e5;
const pad = (n) => String(n).padStart(2, '0');
const sod = (d) => new Date(d.getFullYear(), d.getMonth(), d.getDate());
const addD = (d, n) => new Date(d.getFullYear(), d.getMonth(), d.getDate() + n, d.getHours(), d.getMinutes());
const at = (d, h, m = 0) => new Date(d.getFullYear(), d.getMonth(), d.getDate(), h, m);
const dkey = (d) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
const fromKey = (k) => { const [y, m, d] = k.split('-').map(Number); return new Date(y, m - 1, d); };
const sameDay = (a, b) => dkey(a) === dkey(b);
const today = () => sod(new Date());
const monday = (d) => addD(sod(d), -((d.getDay() + 6) % 7));
const hm = (d) => `${pad(d.getHours())}:${pad(d.getMinutes())}`;
const WD = ['So', 'Mo', 'Di', 'Mi', 'Do', 'Fr', 'Sa'];
const WDL = ['Sonntag', 'Montag', 'Dienstag', 'Mittwoch', 'Donnerstag', 'Freitag', 'Samstag'];
const MON = ['Januar', 'Februar', 'März', 'April', 'Mai', 'Juni', 'Juli', 'August', 'September', 'Oktober', 'November', 'Dezember'];
const MONS = ['Jan.', 'Feb.', 'März', 'Apr.', 'Mai', 'Juni', 'Juli', 'Aug.', 'Sept.', 'Okt.', 'Nov.', 'Dez.'];
const shortDate = (d) => `${WD[d.getDay()]}., ${d.getDate()}. ${MONS[d.getMonth()]}`;
const longDate = (d) => `${WDL[d.getDay()]}, ${d.getDate()}. ${MON[d.getMonth()]} ${d.getFullYear()}`;
function isoWeek(d) {
  const t = new Date(Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()));
  t.setUTCDate(t.getUTCDate() + 3 - ((t.getUTCDay() + 6) % 7));
  return 1 + Math.round((t - Date.UTC(t.getUTCFullYear(), 0, 4)) / DAYMS / 7 - 3 / 7 + ((new Date(Date.UTC(t.getUTCFullYear(), 0, 4)).getUTCDay() + 6) % 7) / 7);
}
function dayLabel(d) {
  const diff = Math.round((sod(d) - today()) / DAYMS);
  const rel = { 0: 'Heute', 1: 'Morgen', '-1': 'Gestern' }[diff];
  return rel ? `${rel} · ${shortDate(d)}` : `${WDL[d.getDay()]}, ${d.getDate()}. ${MON[d.getMonth()]}`;
}
function fmtDur(ms) {
  const m = Math.round(ms / 6e4);
  if (m < 60) return `${m} Min.`;
  const h = m / 60;
  return `${String(Math.round(h * 10) / 10).replace('.', ',')} Std.`;
}

// --- data ----------------------------------------------------------------------------------------
const CAL_COLORS = ['#d6336c', '#2f74e8', '#0e9f80', '#e0700f', '#8e5bd8', '#1e9bb8', '#4e8a2e', '#b58600', '#c0533a', '#6f7187'];
const CALS = [
  { id: 'priv', name: 'Privat', color: '#d6336c' },
  { id: 'work', name: 'Arbeit', color: '#2f74e8' },
  { id: 'fam', name: 'Familie', color: '#0e9f80' },
  { id: 'sport', name: 'Sport', color: '#e0700f' },
  { id: 'bday', name: 'Geburtstage', color: '#8e5bd8' },
  { id: 'hol', name: 'Feiertage', color: '#6f7187', ro: true, sub: 'Nordrhein-Westfalen · eingebaut, offline' },
];
const calOf = (id) => CALS.find((c) => c.id === id) || CALS[0];
const REP = { '': 'Nie', d: 'Täglich', wd: 'Werktags (Mo bis Fr)', w: 'Wöchentlich', w2: 'Alle 2 Wochen', m: 'Monatlich', y: 'Jährlich' };

const T0 = today();
const M0 = monday(T0);
const EV = (o) => ({ rem: [], links: [], loc: '', notes: '', rep: '', all: false, ...o });
const allDay = (d, days = 1) => ({ all: true, s: sod(d), e: addD(sod(d), days) });
let evSeq = 100;
const EVENTS = [
  EV({ id: 1, cal: 'work', title: 'Team-Standup', s: at(M0, 9, 30), e: at(M0, 9, 45), rep: 'wd', rem: [5], loc: 'Video-Raum' }),
  EV({ id: 2, cal: 'priv', title: 'Zahnarzt Dr. Weber', s: at(addD(T0, 1), 10), e: at(addD(T0, 1), 10, 45), rem: [1440, 60], loc: 'Praxis am Markt, Hauptstraße 12', notes: 'Überweisung und Versichertenkarte mitnehmen.', links: [{ k: 'drive', label: 'Überweisung.pdf' }] }),
  EV({ id: 3, cal: 'work', title: 'Quartalsbericht freigeben', s: at(addD(M0, 2), 14), e: at(addD(M0, 2), 15), rem: [30], links: [{ k: 'mail', label: 'Review: Quartalsbericht Q3', arg: 2 }] }),
  EV({ id: 4, cal: 'work', title: 'Wartungsfenster VPN und Mail', s: at(addD(M0, 5), 6), e: at(addD(M0, 5), 9), links: [{ k: 'mail', label: 'Wartungsfenster am Samstag', arg: 5 }] }),
  EV({ id: 5, cal: 'sport', title: 'Laufen im Stadtpark', s: at(addD(M0, 1), 18, 30), e: at(addD(M0, 1), 19, 30), rep: 'w', rem: [30] }),
  EV({ id: 6, cal: 'sport', title: 'Laufen im Stadtpark', s: at(addD(M0, 3), 18, 30), e: at(addD(M0, 3), 19, 30), rep: 'w', rem: [30] }),
  EV({ id: 7, cal: 'sport', title: 'Yoga', s: at(addD(M0, 6), 10), e: at(addD(M0, 6), 11, 15), rep: 'w', loc: 'Studio Lotus' }),
  EV({ id: 8, cal: 'bday', title: 'Oma Hilde', born: 1948, ...allDay(new Date(1948, addD(T0, 3).getMonth(), addD(T0, 3).getDate())), rep: 'y', rem: [1440], notes: 'Lieblingskuchen: Bienenstich' }),
  EV({ id: 9, cal: 'priv', title: 'Abendessen mit Jana', s: at(T0, 19), e: at(T0, 21), rem: [60], loc: 'Trattoria Da Luca' }),
  EV({ id: 10, cal: 'fam', title: 'Elternabend Klasse 3b', s: at(addD(T0, 8), 19, 30), e: at(addD(T0, 8), 21), loc: 'Grundschule am Park, Raum 12', rem: [120] }),
  EV({ id: 11, cal: 'fam', title: 'Urlaub an der Ostsee', ...allDay(addD(T0, 24), 8), loc: 'Zingst', rem: [10080], links: [{ k: 'drive', label: 'Buchungsbestätigung.pdf' }] }),
  EV({ id: 12, cal: 'priv', title: 'Steuererklärung abgeben', ...allDay(addD(T0, 18)), rem: [10080, 1440] }),
  EV({ id: 13, cal: 'fam', title: 'Gelbe Tonne rausstellen', ...allDay(addD(M0, 4)), rep: 'w2', rem: [1440] }),
  EV({ id: 14, cal: 'work', title: 'Kundentermin Müller GmbH', s: at(T0, 10), e: at(T0, 11, 30), loc: 'Besprechungsraum 2', rem: [15] }),
  EV({ id: 15, cal: 'work', title: 'Review Designentwurf', s: at(T0, 11), e: at(T0, 12), rem: [10] }),
  EV({ id: 16, cal: 'work', title: 'Mittag mit dem Team', s: at(T0, 12, 15), e: at(T0, 13, 15), loc: 'Kantine' }),
  EV({ id: 17, cal: 'priv', title: 'Bürgeramt: Ausweis abholen', s: at(addD(T0, 5), 9, 20), e: at(addD(T0, 5), 9, 40), rem: [60], loc: 'Bürgerbüro Mitte', links: [{ k: 'pass', label: 'Personalausweis', arg: 9 }] }),
  EV({ id: 18, cal: 'work', title: '1:1 mit Jana', s: at(addD(M0, 3), 15), e: at(addD(M0, 3), 15, 30), rep: 'w2' }),
];
const evById = (id) => EVENTS.find((e) => String(e.id) === String(id));
const evTitle = (o) => (o.ev.born ? `${o.ev.title} (${o.s.getFullYear() - o.ev.born})` : o.ev.title);

// --- series and holidays -------------------------------------------------------------------------
function stepRep(d, rep) {
  switch (rep) {
    case 'd': return addD(d, 1);
    case 'wd': { let n = addD(d, 1); while (n.getDay() === 0 || n.getDay() === 6) n = addD(n, 1); return n; }
    case 'w': return addD(d, 7);
    case 'w2': return addD(d, 14);
    case 'm': return new Date(d.getFullYear(), d.getMonth() + 1, d.getDate(), d.getHours(), d.getMinutes());
    default: return new Date(d.getFullYear() + 1, d.getMonth(), d.getDate(), d.getHours(), d.getMinutes());
  }
}
function occEnd(ev, s) {
  if (ev.all) return addD(s, Math.round((sod(ev.e) - sod(ev.s)) / DAYMS));
  return new Date(+s + (ev.e - ev.s));
}
function easter(y) {
  const a = y % 19, b = Math.floor(y / 100), c = y % 100, d = Math.floor(b / 4), e = b % 4, f = Math.floor((b + 8) / 25);
  const g = Math.floor((b - f + 1) / 3), h = (19 * a + b - d - g + 15) % 30, i = Math.floor(c / 4), k = c % 4;
  const l = (32 + 2 * e + 2 * i - h - k) % 7, m = Math.floor((a + 11 * h + 22 * l) / 451);
  return new Date(y, Math.floor((h + l - 7 * m + 114) / 31) - 1, ((h + l - 7 * m + 114) % 31) + 1);
}
function holidays(from, to) {
  const out = [];
  for (let y = from.getFullYear(); y <= to.getFullYear(); y++) {
    const E = easter(y);
    const list = [[new Date(y, 0, 1), 'Neujahr'], [addD(E, -2), 'Karfreitag'], [E, 'Ostersonntag'], [addD(E, 1), 'Ostermontag'],
      [new Date(y, 4, 1), 'Tag der Arbeit'], [addD(E, 39), 'Christi Himmelfahrt'], [addD(E, 50), 'Pfingstmontag'], [addD(E, 60), 'Fronleichnam'],
      [new Date(y, 9, 3), 'Tag der Deutschen Einheit'], [new Date(y, 10, 1), 'Allerheiligen'], [new Date(y, 11, 25), '1. Weihnachtstag'], [new Date(y, 11, 26), '2. Weihnachtstag']];
    for (const [d, t] of list) if (d >= from && d < to) out.push({ ev: EV({ id: 'h' + dkey(d), cal: 'hol', title: t, all: true, ro: true, s: d, e: addD(d, 1) }), s: d, e: addD(d, 1) });
  }
  return out;
}
function occurrences(from, to) {
  const out = [];
  for (const ev of EVENTS) {
    if (S.calHidden[ev.cal]) continue;
    let s = ev.s;
    if (ev.rep === 'y') s = new Date(Math.max(ev.s.getFullYear(), from.getFullYear() - 1), ev.s.getMonth(), ev.s.getDate(), ev.s.getHours(), ev.s.getMinutes());
    for (let n = 0; s < to && n < 900; n++) {
      if (ev.until && s >= ev.until) break;
      const e = occEnd(ev, s);
      if (e > from && !(ev.ex && ev.ex.has(dkey(s)))) out.push({ ev, s, e });
      if (!ev.rep) break;
      s = stepRep(s, ev.rep);
    }
  }
  if (S.toggles.calHoliday && !S.calHidden.hol) out.push(...holidays(from, to));
  return out.sort((a, b) => a.s - b.s || b.ev.all - a.ev.all || a.e - b.e);
}
const dayOcc = (occ, d) => occ.filter((o) => o.s < addD(sod(d), 1) && o.e > sod(d));
function occAt(ev, k) {
  const day = k ? fromKey(k) : null;
  if (day && ev.rep) { const s = ev.all ? day : at(day, ev.s.getHours(), ev.s.getMinutes()); return { ev, s, e: occEnd(ev, s) }; }
  if (ev.rep) {
    const next = occurrences(new Date(), addD(today(), 400)).find((o) => o.ev === ev);
    if (next) return next;
  }
  return { ev, s: ev.s, e: ev.e };
}

// --- state ---------------------------------------------------------------------------------------
Object.assign(S, { calView: 'month', calSel: today(), calHidden: {}, calNotify: 'title', calDraft: null, calDefRem: 15, calPending: null });
Object.assign(S.toggles, { calKw: true, calHoliday: true, calSound: true });

// --- shared pieces -------------------------------------------------------------------------------
const cdot = (color) => `<i class="cdot js-c" data-c="${color}"></i>`;
function remLabel(m, all) {
  if (all) return m === 0 ? 'Am Tag um 9:00' : m === 1440 ? 'Am Vortag um 9:00' : m === 10080 ? '1 Woche vorher' : `${m / 1440} Tage vorher`;
  if (m === 0) return 'Zum Beginn';
  if (m < 60) return `${m} Min. vorher`;
  if (m < 1440) return `${m / 60} Std. vorher`;
  if (m === 10080) return '1 Woche vorher';
  return m === 1440 ? '1 Tag vorher' : `${m / 1440} Tage vorher`;
}
function calHead(label) {
  return `<div class="cal-head"><button class="icon-btn" data-act="cal-nav" data-arg="-1" aria-label="Zurück">${icon('prev')}</button>
    <button class="cal-title" data-act="cal-pick">${label}${icon('down')}</button>
    <button class="icon-btn" data-act="cal-nav" data-arg="1" aria-label="Weiter">${icon('chev')}</button></div>`;
}
function evRow(o, day) {
  const c = calOf(o.ev.cal);
  const days = Math.round((o.e - o.s) / DAYMS);
  const tm = o.ev.all
    ? `<span class="tm">ganzt.${days > 1 ? `<span>Tag ${Math.round((sod(day || o.s) - sod(o.s)) / DAYMS) + 1}/${days}</span>` : ''}</span>`
    : `<span class="tm">${hm(o.s)}<span>${hm(o.e)}</span></span>`;
  const sub = o.ev.loc ? `${icon('pinmap')}${esc(o.ev.loc)}` : o.ev.born ? `${icon('cake')}Geburtstag` : esc(c.name);
  const icons = `${o.ev.rep ? icon('repeat') : ''}${o.ev.rem.length ? icon('bell') : ''}${o.ev.links.length ? icon('link') : ''}`;
  return `<button class="ev-row js-c ${o.e < new Date() ? 'past' : ''}" data-c="${c.color}" data-act="cal-open" data-arg="${o.ev.id}|${dkey(o.s)}">${tm}<span class="bar"></span>
    <span class="grow"><span class="t">${esc(evTitle(o))}</span><span class="s">${sub}</span></span><span class="icons">${icons}</span></button>`;
}
function dayAgenda(d, occ) {
  const list = dayOcc(occ, d);
  return `<div class="section-title">${dayLabel(d)}<button data-act="cal-new" data-arg="${dkey(d)}">${icon('plus')} Termin</button></div>
    <div class="card flush">${list.length ? list.map((o) => evRow(o, d)).join('') : `<div class="cal-free">${icon('smile')} Keine Termine. Der Tag gehört dir.</div>`}</div>`;
}
function strip(days, occ, pick) {
  return `<div class="cal-strip ${pick ? 'pick' : ''}">${pick ? '' : '<span></span>'}${days.map((d) => {
    const n = dayOcc(occ, d);
    return `<button class="${sameDay(d, today()) ? 'today' : ''} ${sameDay(d, S.calSel) ? 'sel' : ''}" data-act="${pick ? 'cal-sel' : 'cal-day'}" data-arg="${dkey(d)}">
      <span class="wd">${WD[d.getDay()]}</span><span class="n">${d.getDate()}</span><span class="dots">${n.slice(0, 3).map((o) => `<i class="js-c" data-c="${calOf(o.ev.cal).color}"></i>`).join('')}</span></button>`;
  }).join('')}</div>`;
}
function allDayRow(days, occ) {
  const cells = days.map((d) => `<div>${dayOcc(occ, d).filter((o) => o.ev.all).map((o) => `<button class="pill all js-c" data-c="${calOf(o.ev.cal).color}" data-act="cal-open" data-arg="${o.ev.id}|${dkey(o.s)}">${esc(evTitle(o))}</button>`).join('')}</div>`);
  if (cells.every((c) => c === '<div></div>')) return '';
  return `<div class="cal-allday ${days.length === 1 ? 'one' : ''}"><span>ganzt.</span>${cells.join('')}</div>`;
}

// --- time grid (week and day) --------------------------------------------------------------------
const HH = 44, H0 = 6, H1 = 24;
function lanes(list) {
  const res = [];
  let cluster = [], ends = [], cEnd = 0;
  const flush = () => { cluster.forEach((x) => { x.n = ends.length; }); res.push(...cluster); cluster = []; ends = []; };
  for (const o of list) {
    if (cluster.length && o.s >= cEnd) flush();
    let l = ends.findIndex((end) => end <= o.s);
    if (l < 0) { l = ends.length; ends.push(o.e); } else ends[l] = o.e;
    cluster.push({ o, l });
    cEnd = cluster.length === 1 ? +o.e : Math.max(cEnd, +o.e);
  }
  flush();
  return res;
}
function timeGrid(days, occ) {
  const one = days.length === 1;
  const hours = `<div class="tg-hours">${Array.from({ length: H1 - H0 }, (_, i) => `<span>${i ? pad(H0 + i) + ':00' : ''}</span>`).join('')}</div>`;
  const cols = days.map((d) => {
    const ds = at(d, H0), de = addD(sod(d), 1);
    const list = dayOcc(occ, d).filter((o) => !o.ev.all && o.e > ds);
    const slots = Array.from({ length: H1 - H0 }, (_, i) => `<button class="slot js-pos" data-top="${i * HH}px" data-act="cal-new" data-arg="${dkey(d)}|${H0 + i}" aria-label="Neuer Termin ${H0 + i} Uhr"></button>`).join('');
    const blocks = lanes(list).map(({ o, l, n }) => {
      const s = Math.max(o.s, ds), e = Math.min(o.e, de);
      const top = ((s - ds) / 36e5) * HH, h = Math.max(((e - s) / 36e5) * HH - 2, 18);
      const meta = h > 34 ? `<span>${hm(o.s)}-${hm(o.e)}${one && o.ev.loc ? ` · ${esc(o.ev.loc)}` : ''}</span>` : '';
      return `<button class="blk js-c js-pos ${o.e < new Date() ? 'past' : ''}" data-c="${calOf(o.ev.cal).color}" data-top="${top}px" data-height="${h}px"
        data-left="calc(${(l / n) * 100}% + 1px)" data-width="calc(${100 / n}% - 2px)" data-act="cal-open" data-arg="${o.ev.id}|${dkey(o.s)}"><b>${esc(evTitle(o))}</b>${meta}</button>`;
    }).join('');
    const now = new Date();
    const line = sameDay(d, now) && now >= ds ? `<i class="now js-pos" data-top="${((now - ds) / 36e5) * HH}px"></i>` : '';
    return `<div class="tg-col ${sameDay(d, now) ? 'today' : ''}">${slots}${blocks}${line}</div>`;
  }).join('');
  return `<div class="tg ${one ? 'one' : ''}">${hours}${cols}</div>`;
}

// --- views ---------------------------------------------------------------------------------------
function calMonth() {
  const sel = S.calSel, first = new Date(sel.getFullYear(), sel.getMonth(), 1);
  const start = monday(first), occ = occurrences(start, addD(start, 42));
  const kw = S.toggles.calKw;
  let weeks = '';
  for (let w = 0; w < 6; w++) {
    const ws = addD(start, w * 7);
    if (w > 3 && ws.getMonth() !== sel.getMonth()) break;
    let cells = kw ? `<span class="cal-kw">${isoWeek(ws)}</span>` : '';
    for (let i = 0; i < 7; i++) {
      const d = addD(ws, i), list = dayOcc(occ, d);
      const cls = [d.getMonth() !== sel.getMonth() && 'out', sameDay(d, today()) && 'today', sameDay(d, sel) && 'sel', i >= 5 && 'we', list.some((o) => o.ev.cal === 'hol') && 'hol'].filter(Boolean).join(' ');
      const pills = list.slice(0, 3).map((o) => `<span class="pill js-c ${o.ev.all ? 'all' : ''}" data-c="${calOf(o.ev.cal).color}">${esc(evTitle(o))}</span>`).join('');
      cells += `<button class="cal-day ${cls}" data-act="cal-sel" data-arg="${dkey(d)}" aria-label="${longDate(d)}, ${list.length} Termine"><span class="n">${d.getDate()}</span>${pills}${list.length > 3 ? `<span class="cal-more">+${list.length - 3}</span>` : ''}</button>`;
    }
    weeks += `<div class="cal-week">${cells}</div>`;
  }
  return `${calHead(`${MON[sel.getMonth()]} ${sel.getFullYear()}`)}
    <div class="cal-month ${kw ? 'kw' : ''}"><div class="cal-wd">${kw ? '<span>KW</span>' : ''}${['Mo', 'Di', 'Mi', 'Do', 'Fr', 'Sa', 'So'].map((x) => `<span>${x}</span>`).join('')}</div>${weeks}</div>
    ${dayAgenda(sel, occ)}`;
}
function calWeek() {
  const d0 = monday(S.calSel), days = Array.from({ length: 7 }, (_, i) => addD(d0, i)), d6 = days[6];
  const occ = occurrences(d0, addD(d0, 7));
  const label = `KW ${isoWeek(d0)} · ${d0.getDate()}.${d0.getMonth() !== d6.getMonth() ? ' ' + MONS[d0.getMonth()] : ''} bis ${d6.getDate()}. ${MONS[d6.getMonth()]}`;
  return `${calHead(label)}${strip(days, occ, false)}${allDayRow(days, occ)}${timeGrid(days, occ)}`;
}
function calDay() {
  const d = S.calSel, d0 = monday(d), days = Array.from({ length: 7 }, (_, i) => addD(d0, i));
  const occ = occurrences(d0, addD(d0, 7));
  const n = dayOcc(occ, d).length;
  return `${calHead(shortDate(d) + (d.getFullYear() !== today().getFullYear() ? ' ' + d.getFullYear() : ''))}${strip(days, occ, true)}
    <p class="cal-sum">${n ? `${n} ${n === 1 ? 'Termin' : 'Termine'}` : 'Keine Termine'}${sameDay(d, today()) ? ' heute' : ''}</p>
    ${allDayRow([d], occ)}${timeGrid([d], occ)}`;
}
function calList() {
  const q = S.query.trim().toLowerCase();
  const from = q ? addD(today(), -365) : today(), to = addD(today(), q ? 366 : 60);
  let occ = occurrences(from, to);
  if (q) occ = occ.filter((o) => [o.ev.title, o.ev.loc, o.ev.notes, calOf(o.ev.cal).name].join(' ').toLowerCase().includes(q));
  const groups = new Map();
  for (const o of occ) {
    const k = dkey(o.s < from ? from : o.s);
    if (!groups.has(k)) groups.set(k, []);
    groups.get(k).push(o);
  }
  const body = [...groups.keys()].sort().map((k) => `<div class="section-title">${dayLabel(fromKey(k))}</div><div class="card flush">${groups.get(k).map((o) => evRow(o, fromKey(k))).join('')}</div>`).join('');
  return `<label class="search">${icon('search')}<input id="q" placeholder="Termine, Orte, Notizen durchsuchen" value="${esc(S.query)}" autocomplete="off"></label>
    ${body || `<div class="empty">${q ? `Nichts gefunden zu „${esc(S.query)}“.` : 'Keine Termine in den nächsten 60 Tagen.'}</div>`}
    <p class="hint-text">${q ? 'Durchsucht ein Jahr zurück und voraus, nur auf dem Gerät, im entsperrten Kalender.' : 'Zeigt die nächsten 60 Tage.'}</p>`;
}
function calScreen() {
  const v = S.calView, visible = CALS.filter((c) => !S.calHidden[c.id]).length;
  const views = [['month', 'Monat'], ['week', 'Woche'], ['day', 'Tag'], ['list', 'Liste']];
  const body = { month: calMonth, week: calWeek, day: calDay, list: calList }[v]();
  return `${topbar('GhulCal', {
    ctx: { label: visible === CALS.length ? 'Alle Kalender' : `${visible} von ${CALS.length} Kalendern`, act: 'cal-cals', c: 'c-cal' },
    right: `<button class="icon-btn" data-act="cal-today" aria-label="Heute"><span class="cal-todayic">${today().getDate()}</span></button><button class="icon-btn" data-act="cal-search" aria-label="Suchen">${icon('search')}</button>`,
  })}<div class="screen"><div class="seg cal-seg">${views.map(([k, l]) => `<button class="${v === k ? 'on' : ''}" data-act="cal-view" data-arg="${k}">${l}</button>`).join('')}</div>${body}</div>
    <button class="fab bg-cal" data-act="cal-quick" aria-label="Neuer Termin">${icon('plus')} Termin</button>`;
}

// --- event detail --------------------------------------------------------------------------------
function whenLines(o) {
  const days = Math.round((o.e - o.s) / DAYMS);
  if (o.ev.all) {
    return days > 1 ? [`${shortDate(o.s)} bis ${shortDate(addD(o.e, -1))} ${o.e.getFullYear()}`, `Ganztägig · ${days} Tage`] : [longDate(o.s), 'Ganztägig'];
  }
  return [longDate(o.s), `${hm(o.s)} - ${hm(o.e)} · ${fmtDur(o.e - o.s)}`];
}
function calEvent(id, k) {
  const ev = evById(id);
  if (!ev) return `${topbar('Termin', { back: true })}<div class="screen"><div class="empty">Dieser Termin wurde gelöscht.</div></div>`;
  const o = occAt(ev, k), c = calOf(ev.cal), [l1, l2] = whenLines(o);
  const linkIcon = { drive: ['cloud', 'a1', 'GhulDrive'], mail: ['mail', 'a3', 'GhulMail'], pass: ['key', 'a0', 'GhulPass'] };
  const next = ev.rep ? occurrences(addD(o.s, 1), addD(o.s, 400)).filter((x) => x.ev === ev).slice(0, 3) : [];
  return `${topbar('', { back: true, right: `<button class="icon-btn" data-act="cal-edit" data-arg="${ev.id}" aria-label="Bearbeiten">${icon('edit')}</button><button class="icon-btn" data-act="cal-evmenu" data-arg="${ev.id}|${dkey(o.s)}" aria-label="Mehr">${icon('more')}</button>` })}
  <div class="screen">
    <div class="ev-hero js-c" data-c="${c.color}"><span class="tag ev">${cdot(c.color)}${esc(c.name)}</span>
      <h2>${esc(evTitle(o))}</h2><p class="when">${l1}</p><p class="s">${l2}</p>
      ${ev.rep ? `<p class="s">${icon('repeat')}${REP[ev.rep]}${ev.until ? ` · bis ${shortDate(addD(ev.until, -1))}` : ''}</p>` : ''}</div>
    <div class="card flush">
      ${ev.loc ? `<button class="row" data-act="toast" data-arg="Öffnet die Karten-App mit „${esc(ev.loc)}“"><span class="avatar a7">${icon('pinmap')}</span><span class="grow"><span class="t">${esc(ev.loc)}</span><span class="s">In Karten-App öffnen</span></span>${icon('chev', 'chev')}</button>` : ''}
      <button class="row" data-act="cal-edit" data-arg="${ev.id}"><span class="avatar a4">${icon('bell')}</span><span class="grow"><span class="t">${ev.rem.length ? ev.rem.map((m) => remLabel(m, ev.all)).join(', ') : 'Keine Erinnerung'}</span><span class="s">${ev.rem.length ? 'Benachrichtigung, auch bei gesperrter App' : 'Tippen zum Hinzufügen'}</span></span>${icon('chev', 'chev')}</button>
      ${ev.notes ? `<div class="row"><span class="avatar a9">${icon('note')}</span><span class="grow"><span class="s wrap">${esc(ev.notes)}</span></span></div>` : ''}
    </div>
    ${ev.links.length ? `<div class="section-title">Verknüpft</div><div class="card flush">${ev.links.map((l) => `<button class="row" data-act="cal-link" data-arg="${l.k}|${l.arg || ''}|${esc(l.label)}"><span class="avatar ${linkIcon[l.k][1]}">${icon(linkIcon[l.k][0])}</span><span class="grow"><span class="t">${esc(l.label)}</span><span class="s">${linkIcon[l.k][2]}</span></span>${icon('chev', 'chev')}</button>`).join('')}</div>` : ''}
    ${next.length ? `<div class="section-title">Nächste Termine der Serie</div><div class="card flush">${next.map((x) => evRow(x)).join('')}</div>` : ''}
    <div class="btn-row"><button class="btn tonal" data-act="toast" data-arg="${esc(ev.title)}.ics, an andere App teilen">${icon('upload')} Als .ics teilen</button><button class="btn outline" data-act="cal-mail" data-arg="${ev.id}">${icon('send')} Per GhulMail</button></div>
    <p class="hint-text" data-u="33">Einladungen gehen als .ics-Anhang per Mail, ohne Kalenderserver, ohne Teilnehmerverwaltung.</p>
  </div>`;
}
function evMenuSheet(a) {
  const [id, k] = a.split('|'), ev = evById(id);
  return `<h2>${esc(ev.title)}</h2><div class="card flush sheet-list">
    <button class="row plain" data-act="cal-dup" data-arg="${id}"><span class="grow">Duplizieren</span></button>
    <button class="row plain" data-act="cal-movesheet" data-arg="${id}"><span class="grow">In anderen Kalender verschieben</span></button>
    <button class="row plain" data-act="toast" data-arg="${esc(ev.title)}.ics gespeichert"><span class="grow">Als .ics-Datei speichern</span></button>
    <button class="row plain" data-act="cal-del" data-arg="${id}|${k}"><span class="grow" data-u="34">Löschen</span></button></div>`;
}
function delSheet(a) {
  const [id, k] = a.split('|');
  return `<h2>Serientermin löschen</h2><div class="card flush sheet-list">
    <button class="row plain" data-act="cal-delx" data-arg="one|${id}|${k}"><span class="grow"><span class="t">Nur diesen Termin</span><span class="s">${shortDate(fromKey(k))}</span></span></button>
    <button class="row plain" data-act="cal-delx" data-arg="next|${id}|${k}"><span class="grow"><span class="t">Diesen und alle folgenden</span></span></button>
    <button class="row plain" data-act="cal-delx" data-arg="all|${id}|${k}"><span class="grow"><span class="t" data-u="34">Alle Termine der Serie</span></span></button></div>`;
}

// --- edit ----------------------------------------------------------------------------------------
function newDraft(day, hour) {
  const now = new Date();
  const h = hour != null ? hour : sameDay(day, now) ? Math.min(now.getHours() + 1, 22) : 10;
  return { id: null, title: '', cal: 'priv', all: false, date: dkey(day), from: `${pad(h)}:00`, toDate: dkey(day), to: `${pad(h + 1)}:00`, rep: '', rem: [S.calDefRem], loc: '', notes: '', links: [] };
}
function draftOf(ev) {
  return { id: ev.id, title: ev.title, cal: ev.cal, all: ev.all, date: dkey(ev.s), from: hm(ev.s), toDate: dkey(ev.all ? addD(ev.e, -1) : ev.e), to: hm(ev.e), rep: ev.rep, rem: [...ev.rem], loc: ev.loc, notes: ev.notes, links: [...ev.links], born: ev.born };
}
function calEdit() {
  if (!S.calDraft) S.calDraft = newDraft(S.calSel);
  const d = S.calDraft;
  return `${topbar(d.id ? 'Termin bearbeiten' : 'Neuer Termin', { back: true, right: `<button class="btn primary cal-save" data-act="cal-save">Speichern</button>` })}
  <div class="screen">
    <div class="field big"><div class="grow"><input id="cd-title" data-draft="title" placeholder="Titel" value="${esc(d.title)}" autocomplete="off"></div></div>
    <div class="chips">${CALS.filter((c) => !c.ro).map((c) => `<button class="chip cal-c js-c ${d.cal === c.id ? 'on' : ''}" data-c="${c.color}" data-act="cal-d-cal" data-arg="${c.id}">${cdot(c.color)}${esc(c.name)}</button>`).join('')}</div>
    <div class="card flush">
      <div class="row plain"><span class="grow"><span class="t">Ganztägig</span></span><button class="toggle ${d.all ? 'on' : ''}" data-act="cal-d-all" aria-label="Ganztägig"></button></div>
      <div class="row plain"><span class="grow">Beginn</span><span class="dt"><input type="date" data-draft="date" value="${d.date}" aria-label="Datum Beginn">${d.all ? '' : `<input type="time" data-draft="from" value="${d.from}" step="300" aria-label="Uhrzeit Beginn">`}</span></div>
      <div class="row plain"><span class="grow">Ende</span><span class="dt"><input type="date" data-draft="toDate" value="${d.toDate}" aria-label="Datum Ende">${d.all ? '' : `<input type="time" data-draft="to" value="${d.to}" step="300" aria-label="Uhrzeit Ende">`}</span></div>
      ${d.all ? '' : choice('Zeitzone', 'Europe/Berlin', 'Zeitzone je Termin, z. B. für Reisen')}
    </div>
    <div class="card flush">
      <button class="row plain" data-act="cal-repsheet"><span class="grow"><span class="t">Wiederholen</span>${d.id && d.rep ? '<span class="s">Änderungen gelten für die ganze Serie</span>' : ''}</span><span class="end">${REP[d.rep]} ${icon('chev', 'chev')}</span></button>
      <div class="row plain top"><span class="grow"><span class="t">Erinnerungen</span><span class="rems">${d.rem.map((m, i) => `<span class="chip sm">${icon('bell')}${remLabel(m, d.all)}<button data-act="cal-d-remdel" data-arg="${i}" aria-label="Entfernen">${icon('x')}</button></span>`).join('')}<button class="chip sm add" data-act="cal-remsheet">${icon('plus')} Erinnerung</button></span></span></div>
    </div>
    <div class="card flush">
      <label class="row plain">${icon('pinmap')}<input class="grow inline" data-draft="loc" placeholder="Ort hinzufügen" value="${esc(d.loc)}" autocomplete="off"></label>
      <label class="row plain top">${icon('note')}<textarea class="grow inline" data-draft="notes" rows="3" placeholder="Notiz">${esc(d.notes)}</textarea></label>
      <button class="row plain" data-act="cal-linksheet">${icon('link')}<span class="grow">Verknüpfen: Datei, Mail oder Eintrag</span>${icon('chev', 'chev')}</button>
      ${d.links.length ? `<div class="rems pad">${d.links.map((l, i) => `<span class="chip sm">${icon({ drive: 'cloud', mail: 'mail', pass: 'key' }[l.k])}${esc(l.label)}<button data-act="cal-d-linkdel" data-arg="${i}" aria-label="Entfernen">${icon('x')}</button></span>`).join('')}</div>` : ''}
    </div>
    <p class="hint-text">Gespeichert wird nur verschlüsselt im Kalender-Tresor (.ghlcal). Erinnerungen plant das Gerät selbst, ohne Server und ohne Dauerbenachrichtigung.</p>
  </div>`;
}
function repSheet() {
  const d = S.calDraft;
  return `<h2>Wiederholen</h2><div class="card flush sheet-list">${Object.entries(REP).map(([k, l]) => `<button class="row plain" data-act="cal-d-rep" data-arg="${k}"><span class="grow">${l}</span>${d.rep === k ? icon('check') : ''}</button>`).join('')}
    <button class="row plain" data-act="toast" data-arg="Benutzerdefiniert: z. B. jeden 2. Dienstag im Monat, endet nach 10 Terminen"><span class="grow">Benutzerdefiniert …</span>${icon('chev', 'chev')}</button></div>`;
}
function remSheet() {
  const d = S.calDraft;
  const opts = d.all ? [0, 1440, 2880, 10080] : [0, 5, 10, 15, 30, 60, 120, 1440, 2880, 10080];
  return `<h2>Erinnerung hinzufügen</h2><div class="card flush sheet-list">${opts.map((m) => `<button class="row plain" data-act="cal-d-rem" data-arg="${m}"><span class="grow">${remLabel(m, d.all)}</span>${d.rem.includes(m) ? icon('check') : ''}</button>`).join('')}</div>`;
}
function linkSheet2() {
  const opt = (k, ic, cls, t, s, arg) => `<button class="row" data-act="cal-d-link" data-arg="${k}|${arg || ''}|${t}"><span class="avatar ${cls}">${icon(ic)}</span><span class="grow"><span class="t">${t}</span><span class="s">${s}</span></span></button>`;
  return `<h2>Verknüpfen</h2><div class="card flush sheet-list">
    ${opt('drive', 'cloud', 'a1', 'Q3-Bericht.pdf', 'Datei aus GhulDrive')}
    ${opt('mail', 'mail', 'a3', 'Review: Quartalsbericht Q3', 'Mail aus GhulMail', 2)}
    ${opt('pass', 'key', 'a0', 'Stadtwerke', 'Eintrag aus GhulPass', 5)}</div>
    <p class="hint-text">Verknüpfungen bleiben in der Suite: Der Termin merkt sich nur, worauf er zeigt.</p>`;
}
function saveDraft() {
  const d = S.calDraft;
  if (!d.title.trim()) { toast('Bitte einen Titel eingeben'); $('cd-title')?.focus(); return; }
  const day = fromKey(d.date), endDay = fromKey(d.toDate || d.date);
  const tm = (v) => v.split(':').map(Number);
  const s = d.all ? day : at(day, ...tm(d.from));
  let e = d.all ? addD(endDay, 1) : at(endDay, ...tm(d.to));
  if (e <= s) e = d.all ? addD(s, 1) : new Date(+s + 36e5);
  const data = { cal: d.cal, title: d.title.trim(), s, e, all: d.all, rep: d.rep, rem: [...d.rem].sort((a, b) => b - a), loc: d.loc.trim(), notes: d.notes.trim(), links: d.links };
  if (d.id) Object.assign(evById(d.id), data); else EVENTS.push(EV({ id: ++evSeq, ...data }));
  S.calSel = sod(s);
  S.calDraft = null;
  pop();
  toast(data.rem.length ? `Gespeichert · Erinnerung ${remLabel(data.rem[data.rem.length - 1], data.all)}` : 'Gespeichert');
}

// --- quick add (natural language) ----------------------------------------------------------------
const WDRE = 'montag|dienstag|mittwoch|donnerstag|freitag|samstag|sonntag|mo|di|mi|do|fr|sa|so';
const wdIndex = (w) => ['so', 'mo', 'di', 'mi', 'do', 'fr', 'sa'].indexOf(w.toLowerCase().slice(0, 2));
function parseQuick(text) {
  let t = ` ${text} `;
  const r = { day: null, from: null, to: null, cal: null, rep: '', found: [] };
  const take = (re, fn) => { t = t.replace(re, (...m) => { fn(...m); return ' '; }); };
  take(/#(\p{L}+)/u, (_, n) => { const c = CALS.find((x) => !x.ro && x.name.toLowerCase().startsWith(n.toLowerCase())); if (c) r.cal = c.id; });
  take(new RegExp(`(?<=\\s)jede[nrs]?\\s+(tag|werktag|woche|monat|jahr|${WDRE})\\.?(?=[\\s,])`, 'i'), (_, w) => {
    const map = { tag: 'd', werktag: 'wd', woche: 'w', monat: 'm', jahr: 'y' };
    const k = w.toLowerCase();
    if (map[k]) r.rep = map[k]; else { r.rep = 'w'; r.day = addD(today(), (wdIndex(k) - today().getDay() + 7) % 7); }
  });
  take(/(?<=\s)(täglich|wöchentlich|monatlich|jährlich)(?=\s)/i, (_, w) => { r.rep = { täglich: 'd', wöchentlich: 'w', monatlich: 'm', jährlich: 'y' }[w.toLowerCase()]; });
  take(/(?<=\s)(?:am\s+)?(\d{1,2})\.(\d{1,2})\.(\d{2,4})?(?=[\s,])/, (_, dd, mm, yy) => {
    let y = yy ? Number(yy.length === 2 ? '20' + yy : yy) : today().getFullYear();
    let d = new Date(y, Number(mm) - 1, Number(dd));
    if (!yy && d < today()) d = new Date(y + 1, Number(mm) - 1, Number(dd));
    r.day = d;
  });
  take(/(?<=\s)(übermorgen|morgen|heute)(?=[\s,])/i, (_, w) => { r.day = addD(today(), { heute: 0, morgen: 1, übermorgen: 2 }[w.toLowerCase()]); });
  take(new RegExp(`(?<=\\s)(?:am\\s+|nächsten\\s+|kommenden\\s+)?(${WDRE})\\.?(?=[\\s,])`, 'i'), (_, w) => {
    const diff = (wdIndex(w) - today().getDay() + 7) % 7;
    r.day = addD(today(), diff);
  });
  const hmOf = (h, m) => (Number(h) < 24 && Number(m || 0) < 60 ? `${pad(h)}:${pad(m || 0)}` : null);
  take(/(?<=\s)(?:von\s+|um\s+|ab\s+)?(\d{1,2})(?:[:.](\d{2}))?\s*(?:uhr\s*)?(?:-|\u2013|bis)\s*(\d{1,2})(?:[:.](\d{2}))?\s*(?:uhr)?(?=[\s,])/i, (_, h1, m1, h2, m2) => { r.from = hmOf(h1, m1); r.to = hmOf(h2, m2); });
  if (!r.from) take(/(?<=\s)(?:um\s+|ab\s+)?(\d{1,2})(?:[:.](\d{2}))?\s*uhr(?=[\s,])/i, (_, h, m) => { r.from = hmOf(h, m); });
  if (!r.from) take(/(?<=\s)(?:um\s+|ab\s+)?(\d{1,2}):(\d{2})(?=[\s,])/, (_, h, m) => { r.from = hmOf(h, m); });
  let title = t.replace(/\s(am|um|ab|von|bis|,)(?=\s)/gi, ' ').replace(/\s+/g, ' ').trim().replace(/^[,\s]+|[,\s]+$/g, '');
  title = title ? title[0].toUpperCase() + title.slice(1) : '';
  const day = r.day || (r.from && hmOf(...r.from.split(':')) && at(today(), ...r.from.split(':').map(Number)) < new Date() ? addD(today(), 1) : today());
  let to = r.to;
  if (r.from && !to) { const [h, m] = r.from.split(':').map(Number); to = `${pad(Math.min(h + 1, 23))}:${pad(m)}`; }
  return { title, day, from: r.from, to, cal: r.cal || 'priv', rep: r.rep, all: !r.from };
}
function quickPreview() {
  const box = $('cal-qprev');
  if (!box) return;
  const v = $('cal-quick').value.trim();
  if (!v) { box.innerHTML = '<p class="hint-text">Schreib einfach los: Tag, Uhrzeit, Wiederholung und #Kalender werden erkannt.</p>'; return; }
  const p = parseQuick(v), c = calOf(p.cal);
  box.innerHTML = `<div class="qa js-c" data-c="${c.color}"><span class="bar"></span><span class="grow"><b>${esc(p.title || '(ohne Titel)')}</b>
    <span class="s">${longDate(p.day)}</span><span class="s">${p.all ? 'Ganztägig' : `${p.from} - ${p.to}`}${p.rep ? ` · ${REP[p.rep]}` : ''}</span></span>
    <span class="tag ev">${cdot(c.color)}${esc(c.name)}</span></div>`;
  afterRender(box);
}
function quickSheet() {
  const ex = ['Zahnarzt Fr 10 Uhr', 'Laufen morgen 18:30-19:30 #sport', 'Team-Call jeden Montag 9 Uhr #arbeit', 'Omas Geburtstag 24.12. #familie'];
  return `<h2>Neuer Termin</h2>
    <div class="field" data-u="31"><div class="grow"><label for="cal-quick">Schnell eintragen</label><input id="cal-quick" placeholder="z. B. Zahnarzt Fr 10 Uhr #privat" autocomplete="off"></div>${icon('zap')}</div>
    <div id="cal-qprev"></div>
    <div class="chips">${ex.map((x) => `<button class="chip" data-act="cal-qex" data-arg="${esc(x)}">${esc(x)}</button>`).join('')}</div>
    <div class="btn-row"><button class="btn outline" data-act="cal-qmore">Mehr Optionen</button><button class="btn primary" data-act="cal-qsave">${icon('check')} Eintragen</button></div>`;
}

// --- calendars -----------------------------------------------------------------------------------
function calsSheet() {
  const now = today(), occ = occurrences(now, addD(now, 365));
  return `<h2>Kalender</h2><div class="card flush sheet-list">${CALS.map((c) => {
    const n = occ.filter((o) => o.ev.cal === c.id).length;
    return `<div class="row cal-row js-c" data-c="${c.color}"><button class="cal-vis ${S.calHidden[c.id] ? '' : 'on'}" data-act="cal-vis" data-arg="${c.id}" aria-label="${esc(c.name)} anzeigen">${icon('check')}</button>
      <span class="grow"><span class="t">${esc(c.name)}</span><span class="s">${c.sub || `${n} Termine im nächsten Jahr · verschlüsselt`}</span></span>
      ${c.ro ? '' : `<button class="icon-btn" data-act="cal-calmenu" data-arg="${c.id}" aria-label="${esc(c.name)} bearbeiten">${icon('more')}</button>`}</div>`;
  }).join('')}
    <button class="row" data-act="cal-newcal"><span class="avatar a5">${icon('plus')}</span><span class="grow"><span class="t">Neuer Kalender</span></span></button></div>
    <div class="btn-row" data-u="24"><button class="btn outline" data-act="toast" data-arg=".ics-Datei auswählen, z. B. aus Google, Outlook oder Apple">${icon('download')} Import (.ics)</button><button class="btn outline" data-act="toast" data-arg="Alle sichtbaren Kalender als .ics exportiert">${icon('upload')} Export (.ics)</button></div>
    <p class="hint-text" data-u="24">Jeder Kalender liegt verschlüsselt in deiner Suite (.ghlcal) und synchronisiert wie deine Passwörter über deinen eigenen Speicher. Bewusst ohne CalDAV.</p>`;
}
function calMenuSheet(id) {
  const c = calOf(id);
  return `<h2>Kalender bearbeiten</h2>
    <div class="field" data-u="31"><div class="grow"><label for="cal-name">Name</label><input id="cal-name" value="${esc(c.name)}" data-cal="${c.id}" autocomplete="off"></div></div>
    <div class="swatches">${CAL_COLORS.map((col) => `<button class="sw js-c ${c.color === col ? 'on' : ''}" data-c="${col}" data-act="cal-color" data-arg="${c.id}|${col}" aria-label="Farbe ${col}"></button>`).join('')}</div>
    <div class="card flush sheet-list">
      ${choice('Standard-Erinnerung', '15 Min. vorher', 'keine, Zum Beginn, 5 bis 60 Min., 1 Tag')}
      <button class="row plain" data-act="toast" data-arg="Kommt mit geteilten Tresoren: Familie lädt den Kalender mit eigenem Schlüssel"><span class="grow"><span class="t">Mit anderen teilen ${NEW}</span><span class="s">Als geteilter Tresor, Ende-zu-Ende verschlüsselt</span></span>${icon('chev', 'chev')}</button>
      <button class="row plain" data-act="toast" data-arg="${esc(c.name)}.ics exportiert"><span class="grow">Als .ics exportieren</span></button>
      <button class="row plain" data-act="cal-delcal" data-arg="${c.id}"><span class="grow" data-u="34">Kalender löschen</span></button></div>
    <button class="btn primary block" data-u="24" data-act="cal-cals">Fertig</button>`;
}

// --- notification --------------------------------------------------------------------------------
function showNotification() {
  const now = new Date();
  const o = occurrences(now, addD(today(), 8)).find((x) => !x.ev.all && x.s > now) || occAt(evById(2));
  const lvl = S.calNotify;
  const title = lvl === 'min' ? 'Termin in 15 Minuten' : `${evTitle(o)}`;
  const text = lvl === 'min' ? 'Details siehst du nach dem Entsperren.' : lvl === 'title' ? `in 15 Min. · ${shortDate(o.s)}, ${hm(o.s)}` : `${shortDate(o.s)}, ${hm(o.s)} - ${hm(o.e)}${o.ev.loc ? ` · ${esc(o.ev.loc)}` : ''}`;
  const n = $('notif');
  n.innerHTML = `<div class="hd"><span class="ic">${icon('cal')}</span>GhulCal · jetzt<span class="grow"></span>${S.toggles.calSound ? icon('bell') : ''}</div>
    <b>${esc(title)}</b><p>${text}</p>
    <div class="acts"><button data-act="cal-n-open" data-arg="${o.ev.id}|${dkey(o.s)}">Öffnen</button><button data-act="cal-n-snooze">In 5 Min. erneut</button><button data-act="cal-n-close">Erledigt</button></div>`;
  n.classList.add('show');
  clearTimeout(showNotification.timer);
  showNotification.timer = setTimeout(() => n.classList.remove('show'), 9000);
}
const hideNotification = () => $('notif').classList.remove('show');
function openOcc(a) {
  const [id, k] = a.split('|');
  if (String(id).startsWith('h')) { toast(`Feiertag in ${calOf('hol').sub.split(' ·')[0]}`); return; }
  closeSheet();
  push('cal-event', { id, day: k });
}

// --- settings page -------------------------------------------------------------------------------
const CAL_SETTINGS = {
  id: 'm-cal', icon: 'cal', cls: 'i-rose', title: 'GhulCal', sub: 'Erinnerungen an · Feiertage NRW',
  page: () => {
    const lv = [['min', 'Nur Hinweis'], ['title', 'Mit Titel'], ['full', 'Alles']];
    return `<div class="section-title">Ansicht</div><div class="card flush">
      ${choice('Startansicht', { month: 'Monat', week: 'Woche', day: 'Tag', list: 'Liste' }[S.calView], 'Monat, Woche, Tag, Liste')}
      ${choice('Woche beginnt am', 'Montag', 'Montag, Samstag, Sonntag')}
      ${toggle('calKw', 'Kalenderwochen anzeigen')}
      ${toggle('calHoliday', 'Feiertage anzeigen', 'Eingebaut und offline, keine Abfrage im Netz')}
      ${choice('Bundesland', 'Nordrhein-Westfalen', 'alle 16 Bundesländer, Österreich, Schweiz')}</div>
    <div class="section-title">Erinnerungen</div><div class="card flush">
      ${choice('Standard für Termine', remLabel(S.calDefRem), 'keine, Zum Beginn, 5, 10, 15, 30, 60 Min., 1 Tag')}
      ${choice('Standard für ganztägige', 'Am Vortag um 9:00', 'keine, Am Tag um 9:00, Am Vortag um 9:00')}
      ${toggle('calSound', 'Ton und Vibration')}
      <div class="row plain"><span class="grow"><span class="t">Inhalt auf dem Sperrbildschirm</span><span class="s">${{ min: '„Termin in 15 Minuten“, ohne Titel und Ort', title: 'Titel und Uhrzeit, ohne Ort', full: 'Titel, Uhrzeit und Ort' }[S.calNotify]}</span></span></div>
      <div class="seg cal-nseg">${lv.map(([k, l]) => `<button class="${S.calNotify === k ? 'on' : ''}" data-act="cal-nlevel" data-arg="${k}">${l}</button>`).join('')}</div>
      ${link('Test-Erinnerung zeigen', 'So sieht eine Benachrichtigung aus', 'cal-notify')}</div>
    <p class="hint-text">Beim Entsperren plant das Gerät die Erinnerungen der nächsten 30 Tage selbst ein (Android: exakte Wecker, Windows: Infobereich). Sie kommen pünktlich, auch wenn die App gesperrt oder geschlossen ist, ohne Server und ohne Dauerbenachrichtigung.</p>
    <div class="section-title">Daten</div><div class="card flush">
      ${link('Kalender verwalten', `${CALS.length} Kalender · Farben, Sichtbarkeit`, 'cal-cals')}
      ${link('Importieren (.ics)', 'Export aus Google, Outlook, Apple oder Thunderbird')}
      ${link('Exportieren (.ics)', 'Einzelne Kalender oder alle, als Datei')}</div>
    <div class="card"><h3>Bewusst geschlossen</h3><p>GhulCal spricht kein CalDAV und abonniert keine fremden Kalender. Termine verlassen das Gerät nur verschlüsselt und nur zu deinem eigenen Speicher. Austausch mit anderen Programmen über .ics-Dateien oder als Mail-Anhang.</p></div>`;
  },
};
{
  const group = SETTINGS.find(([, items]) => items.some((x) => x.id === 'm-mail'));
  const items = group ? group[1] : SETTINGS[SETTINGS.length - 1][1];
  items.splice(items.findIndex((x) => x.id === 'm-mail') + 1, 0, CAL_SETTINGS);
}

// --- mail integration ----------------------------------------------------------------------------
const baseMailThread = mailThread;
mailThread = function (id) { // eslint-disable-line no-global-assign
  const html = baseMailThread(id);
  const tip = {
    6: `<div class="banner cal-banner">${icon('cal')}<span>Termin erkannt: Reifenwechsel am 18.11.</span><button data-act="cal-frommail" data-arg="6">In GhulCal ${NEW}</button></div>`,
    5: `<div class="banner cal-banner">${icon('cal')}<span>Steht schon in GhulCal: ${shortDate(addD(M0, 5))}, 06:00 - 09:00</span><button data-act="cal-open" data-arg="4|${dkey(addD(M0, 5))}">Ansehen</button></div>`,
  }[id];
  return tip ? html.replace('<div class="screen">', `<div class="screen">${tip}`) : html;
};

// --- actions -------------------------------------------------------------------------------------
const calNav = (n) => {
  const s = S.calSel;
  if (S.calView === 'month') {
    const d = new Date(s.getFullYear(), s.getMonth() + n, 1);
    S.calSel = d.getMonth() === today().getMonth() && d.getFullYear() === today().getFullYear() ? today() : d;
  } else S.calSel = addD(s, S.calView === 'week' ? 7 * n : n);
  render();
};
function pickSheet() {
  const y = S.calPickYear;
  return `<h2>Springen zu</h2><div class="cal-head"><button class="icon-btn" data-act="cal-pyear" data-arg="-1" aria-label="Vorjahr">${icon('prev')}</button><b class="cal-title">${y}</b><button class="icon-btn" data-act="cal-pyear" data-arg="1" aria-label="Folgejahr">${icon('chev')}</button></div>
    <div class="mgrid">${MON.map((m, i) => `<button class="${i === S.calSel.getMonth() && y === S.calSel.getFullYear() ? 'on' : ''} ${i === today().getMonth() && y === today().getFullYear() ? 'now' : ''}" data-act="cal-pmonth" data-arg="${i}">${m.slice(0, 3)}</button>`).join('')}</div>
    <button class="btn tonal block" data-u="24" data-act="cal-today">${icon('cal')} Heute</button>`;
}
Object.assign(ACT, {
  'cal-view': (a) => { S.calView = a; render(); },
  'cal-search': () => { S.calView = 'list'; render(); $('q')?.focus(); },
  'cal-today': () => { closeSheet(); S.calSel = today(); render(); },
  'cal-nav': (a) => calNav(Number(a)),
  'cal-sel': (a) => { S.calSel = fromKey(a); render(true); },
  'cal-day': (a) => { S.calSel = fromKey(a); S.calView = 'day'; render(); },
  'cal-pick': () => { S.calPickYear = S.calSel.getFullYear(); sheet(pickSheet()); },
  'cal-pyear': (a) => { S.calPickYear += Number(a); sheet(pickSheet()); },
  'cal-pmonth': (a) => { S.calSel = new Date(S.calPickYear, Number(a), 1); if (S.calView === 'list') S.calView = 'month'; closeSheet(); render(); },
  'cal-open': (a) => openOcc(a),
  'cal-new': (a) => { const [k, h] = (a || dkey(S.calSel)).split('|'); S.calDraft = newDraft(fromKey(k), h ? Number(h) : null); closeSheet(); push('cal-edit'); },
  'cal-edit': (a) => { S.calDraft = draftOf(evById(a)); push('cal-edit'); },
  'cal-save': () => saveDraft(),
  'cal-evmenu': (a) => sheet(evMenuSheet(a)),
  'cal-dup': (a) => { const ev = evById(a); EVENTS.push({ ...ev, id: ++evSeq, title: `${ev.title} (Kopie)`, rem: [...ev.rem], links: [...ev.links], ex: null }); closeSheet(); pop(); toast('Kopie angelegt'); },
  'cal-movesheet': (a) => sheet(`<h2>Verschieben nach</h2><div class="card flush sheet-list">${CALS.filter((c) => !c.ro).map((c) => `<button class="row plain js-c" data-c="${c.color}" data-act="cal-move" data-arg="${a}|${c.id}">${cdot(c.color)}<span class="grow">${esc(c.name)}</span>${evById(a).cal === c.id ? icon('check') : ''}</button>`).join('')}</div>`),
  'cal-move': (a) => { const [id, c] = a.split('|'); evById(id).cal = c; closeSheet(); render(true); toast(`Verschoben nach ${calOf(c).name}`); },
  'cal-del': (a) => {
    const [id] = a.split('|');
    if (evById(id).rep) { sheet(delSheet(a)); return; }
    EVENTS.splice(EVENTS.indexOf(evById(id)), 1); closeSheet(); pop(); toast('Termin gelöscht · Rückgängig');
  },
  'cal-delx': (a) => {
    const [mode, id, k] = a.split('|'), ev = evById(id);
    if (mode === 'all') EVENTS.splice(EVENTS.indexOf(ev), 1);
    else if (mode === 'next') ev.until = fromKey(k);
    else (ev.ex = ev.ex || new Set()).add(k);
    closeSheet(); pop(); toast(mode === 'one' ? 'Dieser Termin wurde gelöscht' : 'Serie gelöscht · Rückgängig');
  },
  'cal-link': (a) => {
    const [k, arg, label] = a.split('|');
    if (k === 'mail' && arg) { S.tab = 'mail'; S.read[arg] = true; S.stack = [{ screen: 'thread', id: Number(arg) }]; render(); return; }
    if (k === 'pass' && arg) { S.tab = 'pass'; S.stack = [{ screen: 'entry', id: Number(arg) }]; render(); return; }
    S.tab = 'drive'; S.stack = []; render(); toast(`${label} öffnet sich in GhulDrive`);
  },
  'cal-mail': (a) => { const ev = evById(a); S.tab = 'mail'; S.stack = [{ screen: 'compose', kind: 'new' }]; render(); toast(`${ev.title}.ics ist angehängt`); },
  'cal-d-cal': (a) => { S.calDraft.cal = a; render(true); },
  'cal-d-all': () => { S.calDraft.all = !S.calDraft.all; S.calDraft.rem = [S.calDraft.all ? 1440 : S.calDefRem]; render(true); },
  'cal-repsheet': () => sheet(repSheet()),
  'cal-d-rep': (a) => { S.calDraft.rep = a; closeSheet(); render(true); },
  'cal-remsheet': () => sheet(remSheet()),
  'cal-d-rem': (a) => { const r = S.calDraft.rem, m = Number(a); if (!r.includes(m)) r.push(m); r.sort((x, y) => y - x); closeSheet(); render(true); },
  'cal-d-remdel': (a) => { S.calDraft.rem.splice(Number(a), 1); render(true); },
  'cal-linksheet': () => sheet(linkSheet2()),
  'cal-d-link': (a) => { const [k, arg, label] = a.split('|'); S.calDraft.links.push({ k, arg: arg ? Number(arg) : undefined, label }); closeSheet(); render(true); },
  'cal-d-linkdel': (a) => { S.calDraft.links.splice(Number(a), 1); render(true); },
  'cal-quick': () => { sheet(quickSheet()); quickPreview(); setTimeout(() => $('cal-quick')?.focus(), 260); },
  'cal-qex': (a) => { $('cal-quick').value = a; quickPreview(); },
  'cal-qsave': () => {
    const p = parseQuick($('cal-quick').value.trim());
    if (!p.title) { toast('Bitte einen Titel eingeben'); return; }
    const s = p.all ? p.day : at(p.day, ...p.from.split(':').map(Number));
    const e = p.all ? addD(p.day, 1) : at(p.day, ...p.to.split(':').map(Number));
    EVENTS.push(EV({ id: ++evSeq, cal: p.cal, title: p.title, s, e: e > s ? e : new Date(+s + 36e5), all: p.all, rep: p.rep, rem: [p.all ? 1440 : S.calDefRem] }));
    S.calSel = sod(s); closeSheet(); render();
    toast(`Eingetragen: ${shortDate(s)}${p.all ? '' : `, ${p.from}`}`);
  },
  'cal-qmore': () => {
    const v = $('cal-quick').value.trim(), p = parseQuick(v || ' ');
    S.calDraft = { ...newDraft(p.day), title: p.title, cal: p.cal, rep: p.rep, all: v ? p.all : false };
    if (v && !p.all) Object.assign(S.calDraft, { from: p.from, to: p.to });
    if (S.calDraft.all) S.calDraft.rem = [1440];
    closeSheet(); push('cal-edit');
  },
  'cal-cals': () => sheet(calsSheet()),
  'cal-vis': (a) => { S.calHidden[a] = !S.calHidden[a]; render(true); sheet(calsSheet()); },
  'cal-calmenu': (a) => sheet(calMenuSheet(a)),
  'cal-color': (a) => { const [id, col] = a.split('|'); calOf(id).color = col; render(true); sheet(calMenuSheet(id)); },
  'cal-newcal': () => {
    const id = 'c' + (++evSeq);
    CALS.splice(CALS.length - 1, 0, { id, name: 'Neuer Kalender', color: CAL_COLORS[CALS.length % CAL_COLORS.length] });
    render(true); sheet(calMenuSheet(id)); setTimeout(() => { const i = $('cal-name'); i?.focus(); i?.select(); }, 260);
  },
  'cal-delcal': (a) => {
    for (let i = EVENTS.length - 1; i >= 0; i--) if (EVENTS[i].cal === a) EVENTS.splice(i, 1);
    const c = calOf(a); CALS.splice(CALS.indexOf(c), 1); render(true); sheet(calsSheet()); toast(`„${c.name}“ gelöscht · Rückgängig`);
  },
  'cal-nlevel': (a) => { S.calNotify = a; render(true); },
  'cal-notify': () => showNotification(),
  'cal-n-close': () => hideNotification(),
  'cal-n-snooze': () => { hideNotification(); toast('Erinnert in 5 Minuten erneut'); },
  'cal-n-open': (a) => {
    hideNotification();
    if (S.phase !== 'app') { S.calPending = a; toast('Zum Öffnen entsperren'); return; }
    S.tab = 'cal'; S.stack = []; openOcc(a);
  },
  'cal-frommail': () => {
    const y = today() > new Date(today().getFullYear(), 10, 18) ? today().getFullYear() + 1 : today().getFullYear();
    S.calDraft = { ...newDraft(new Date(y, 10, 18), 8), title: 'Reifenwechsel Werkstatt Müller', loc: 'Werkstatt Müller', links: [{ k: 'mail', label: 'Notiz an mich', arg: 6 }] };
    push('cal-edit');
  },
});
const baseJump = ACT.jump;
ACT.jump = (a) => { S.calDraft = null; baseJump(a); };
const baseUnlock = ACT.unlock;
ACT.unlock = () => {
  baseUnlock();
  if (S.calPending) { const a = S.calPending; S.calPending = null; S.tab = 'cal'; S.stack = []; openOcc(a); }
};

// --- rendering hooks -----------------------------------------------------------------------------
const baseAfterRender = afterRender;
afterRender = function (root) { // eslint-disable-line no-global-assign
  baseAfterRender(root);
  root.querySelectorAll('.js-c').forEach((el) => el.style.setProperty('--ev', el.dataset.c));
  root.querySelectorAll('.js-pos').forEach((el) => {
    for (const k of ['top', 'height', 'left', 'width']) if (el.dataset[k]) el.style[k] = el.dataset[k];
  });
};
document.addEventListener('input', (ev) => {
  const k = ev.target.dataset && ev.target.dataset.draft;
  if (k && S.calDraft) {
    const d = S.calDraft;
    d[k] = ev.target.value;
    const sync = (key, value) => { d[key] = value; const el = document.querySelector(`[data-draft="${key}"]`); if (el) el.value = value; };
    if (k === 'date' && d.toDate < d.date) sync('toDate', d.date);
    if (k === 'from' && d.toDate === d.date && d.to <= d.from) { const [h, m] = d.from.split(':').map(Number); sync('to', `${pad(Math.min(h + 1, 23))}:${pad(m)}`); }
  }
  if (ev.target.id === 'cal-quick') quickPreview();
  if (ev.target.id === 'cal-name') { const c = calOf(ev.target.dataset.cal); c.name = ev.target.value || 'Ohne Namen'; render(true); }
});
document.addEventListener('keydown', (ev) => {
  if (ev.target.id === 'cal-quick' && ev.key === 'Enter') ACT['cal-qsave']();
  const typing = /INPUT|TEXTAREA/.test(document.activeElement.tagName);
  if (S.phase === 'app' && S.tab === 'cal' && !current() && !typing && !$('device').classList.contains('sheet-open')) {
    if (ev.key === 'ArrowLeft') calNav(-1);
    if (ev.key === 'ArrowRight') calNav(1);
    if (ev.key === 't') ACT['cal-today']();
  }
});
let touchX = null;
$('app').addEventListener('touchstart', (ev) => { touchX = ev.target.closest('.cal-month, .tg, .cal-strip') ? ev.touches[0].clientX : null; }, { passive: true });
$('app').addEventListener('touchend', (ev) => {
  if (touchX == null || S.tab !== 'cal' || current()) return;
  const dx = ev.changedTouches[0].clientX - touchX;
  if (Math.abs(dx) > 60) calNav(dx < 0 ? 1 : -1);
  touchX = null;
});

// --- side panel and start ------------------------------------------------------------------------
{
  const n = document.createElement('div');
  n.className = 'notif'; n.id = 'notif'; n.setAttribute('role', 'alert');
  $('device').appendChild(n);
  const b = document.createElement('button');
  b.className = 'chip'; b.dataset.act = 'cal-notify'; b.textContent = 'Erinnerung (Benachrichtigung)';
  $('jump').appendChild(b);
}
render();
