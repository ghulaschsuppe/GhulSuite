// SPDX-License-Identifier: AGPL-3.0-or-later
// GhulSuite design preview: a clickable prototype with made-up data. Nothing is stored or sent.
'use strict';

// --- icons (24px, stroke) ------------------------------------------------------------------------
const P = {
  lock: '<rect x="4" y="11" width="16" height="10" rx="2"/><path d="M8 11V7a4 4 0 0 1 8 0v4"/>',
  key: '<circle cx="7.5" cy="15.5" r="4.5"/><path d="m10.7 12.3 9.3-9.3M17 6l3 3M14 9l2 2"/>',
  search: '<circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/>',
  star: '<path d="m12 3 2.8 5.7 6.2.9-4.5 4.4 1.1 6.2L12 17.3l-5.6 2.9 1.1-6.2L3 9.6l6.2-.9z"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  copy: '<rect x="9" y="9" width="12" height="12" rx="2"/><path d="M5 15V5a2 2 0 0 1 2-2h10"/>',
  eye: '<path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12z"/><circle cx="12" cy="12" r="3"/>',
  eyeoff: '<path d="M3 3l18 18M10.6 5.1A10.9 10.9 0 0 1 12 5c6.5 0 10 7 10 7a17 17 0 0 1-3.2 4.1M6.6 6.6C3.7 8.4 2 12 2 12s3.5 7 10 7c1.9 0 3.6-.6 5-1.4M9.9 9.9a3 3 0 0 0 4.2 4.2"/>',
  chev: '<path d="m9 6 6 6-6 6"/>',
  down: '<path d="m6 9 6 6 6-6"/>',
  back: '<path d="M19 12H5M11 18l-6-6 6-6"/>',
  cloud: '<path d="M7 18a5 5 0 0 1-.9-9.9A6 6 0 0 1 17.6 9 4.5 4.5 0 0 1 17.5 18z"/>',
  mail: '<rect x="3" y="5" width="18" height="14" rx="2"/><path d="m3 7 9 6 9-6"/>',
  sliders: '<path d="M4 6h10M18 6h2M4 12h4M12 12h8M4 18h12"/><circle cx="16" cy="6" r="2"/><circle cx="10" cy="12" r="2"/><circle cx="18" cy="18" r="2"/>',
  shield: '<path d="M12 3 4 6v6c0 5 3.5 8 8 9 4.5-1 8-4 8-9V6z"/>',
  shieldcheck: '<path d="M12 3 4 6v6c0 5 3.5 8 8 9 4.5-1 8-4 8-9V6z"/><path d="m9 12 2 2 4-4"/>',
  folder: '<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>',
  file: '<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5"/>',
  filetext: '<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5M9 13h6M9 17h6"/>',
  image: '<rect x="3" y="4" width="18" height="16" rx="2"/><circle cx="9" cy="10" r="2"/><path d="m21 16-5-5L6 20"/>',
  film: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M7 4v16M17 4v16M3 9h4M3 15h4M17 9h4M17 15h4"/>',
  music: '<path d="M9 18V5l11-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="17" cy="16" r="3"/>',
  upload: '<path d="M12 16V4M7 9l5-5 5 5M4 20h16"/>',
  download: '<path d="M12 4v12M7 11l5 5 5-5M4 20h16"/>',
  trash: '<path d="M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3"/>',
  edit: '<path d="M4 20h4L19 9l-4-4L4 16z"/>',
  more: '<circle cx="12" cy="5" r="1.6" fill="currentColor"/><circle cx="12" cy="12" r="1.6" fill="currentColor"/><circle cx="12" cy="19" r="1.6" fill="currentColor"/>',
  check: '<path d="m5 12 5 5 9-10"/>',
  x: '<path d="M6 6l12 12M18 6 6 18"/>',
  refresh: '<path d="M20 11a8 8 0 1 0-2.3 5.7M20 4v7h-7"/>',
  user: '<circle cx="12" cy="8" r="4"/><path d="M4 21a8 8 0 0 1 16 0"/>',
  globe: '<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18"/>',
  card: '<rect x="2" y="5" width="20" height="14" rx="2"/><path d="M2 10h20M6 15h4"/>',
  note: '<path d="M5 3h10l4 4v14H5z"/><path d="M9 9h6M9 13h6M9 17h4"/>',
  wifi: '<path d="M2 9a15 15 0 0 1 20 0M5 12.5a10 10 0 0 1 14 0M8.5 16a5 5 0 0 1 7 0"/><path d="M12 19.5h0"/>',
  terminal: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="m7 9 3 3-3 3M13 15h4"/>',
  idcard: '<rect x="3" y="5" width="18" height="14" rx="2"/><circle cx="9" cy="11" r="2"/><path d="M6 16a3 3 0 0 1 6 0M14 10h4M14 14h3"/>',
  fingerprint: '<path d="M7 10a5 5 0 0 1 10 0v2a14 14 0 0 1-1.5 6.5M12 10v3a10 10 0 0 1-2.5 7M4.5 7.5A9 9 0 0 1 21 11M3 15a14 14 0 0 0 .5-5M8 21a14 14 0 0 0 1.5-8"/>',
  inbox: '<path d="M3 13h5l2 3h4l2-3h5M5 5h14l2 8v6H3v-6z"/>',
  send: '<path d="M21 3 10 14M21 3l-7 18-4-7-7-4z"/>',
  archive: '<rect x="3" y="4" width="18" height="5" rx="1"/><path d="M5 9v10h14V9M10 13h4"/>',
  clip: '<path d="m20 11-8.5 8.5a5 5 0 0 1-7-7L13 4a3.5 3.5 0 0 1 5 5l-8.5 8.5a2 2 0 0 1-3-3L14 7"/>',
  reply: '<path d="M9 14 4 9l5-5M4 9h11a5 5 0 0 1 5 5v5"/>',
  forward: '<path d="m15 14 5-5-5-5M20 9H9a5 5 0 0 0-5 5v5"/>',
  menu: '<path d="M4 6h16M4 12h16M4 18h16"/>',
  clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 3"/>',
  alert: '<path d="M12 3 2 21h20z"/><path d="M12 10v5M12 18h0"/>',
  server: '<rect x="3" y="4" width="18" height="7" rx="2"/><rect x="3" y="13" width="18" height="7" rx="2"/><path d="M7 7.5h0M7 16.5h0"/>',
  sync: '<path d="M4 12a8 8 0 0 1 13.7-5.7L20 8M20 4v4h-4M20 12a8 8 0 0 1-13.7 5.7L4 16M4 20v-4h4"/>',
  palette: '<path d="M12 3a9 9 0 1 0 0 18c1.5 0 2-1 2-2s-1-2 0-3 2-1 3-1a4 4 0 0 0 4-4c0-4.5-4-8-9-8z"/><path d="M7.5 11h0M10 7h0M15 7.5h0"/>',
  info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 7.5h0"/>',
  bell: '<path d="M6 16V11a6 6 0 0 1 12 0v5l2 2H4zM10 21h4"/>',
  phone: '<rect x="6" y="2" width="12" height="20" rx="3"/><path d="M11 18h2"/>',
  grid: '<rect x="3" y="3" width="8" height="8" rx="2"/><rect x="13" y="3" width="8" height="8" rx="2"/><rect x="3" y="13" width="8" height="8" rx="2"/><rect x="13" y="13" width="8" height="8" rx="2"/>',
  list: '<path d="M8 6h13M8 12h13M8 18h13M3 6h0M3 12h0M3 18h0"/>',
  qr: '<rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><path d="M14 14h3v3M17 21h4v-4M14 21h0M21 14h0"/>',
  history: '<path d="M3 12a9 9 0 1 0 3-6.7L3 8M3 3v5h5M12 7v5l3 2"/>',
  tag: '<path d="M3 12V3h9l9 9-9 9z"/><path d="M7.5 7.5h0"/>',
  link: '<path d="M10 14a5 5 0 0 0 7 0l3-3a5 5 0 0 0-7-7l-1 1M14 10a5 5 0 0 0-7 0l-3 3a5 5 0 0 0 7 7l1-1"/>',
  usbkey: '<rect x="8" y="2" width="8" height="8" rx="1"/><path d="M6 10h12v8a4 4 0 0 1-4 4h-4a4 4 0 0 1-4-4zM10 5h0M14 5h0"/>',
  camera: '<path d="M4 8h3l2-3h6l2 3h3v11H4z"/><circle cx="12" cy="13" r="4"/>',
  box: '<path d="M12 3 3 8v8l9 5 9-5V8z"/><path d="m3 8 9 5 9-5M12 13v8"/>',
  layers: '<path d="m12 3 9 5-9 5-9-5z"/><path d="m3 13 9 5 9-5"/>',
  wand: '<path d="M5 19 19 5M15 4v3M20 8h-3M4 10h3M7 14v3"/>',
  help: '<circle cx="12" cy="12" r="9"/><path d="M9.5 9a2.5 2.5 0 1 1 3.5 2.3c-.6.3-1 .9-1 1.7M12 17h0"/>',
  power: '<path d="M12 3v9M6.3 6.3a8 8 0 1 0 11.4 0"/>',
  filter: '<path d="M3 5h18l-7 8v6l-4-2v-4z"/>',
  pin: '<path d="M12 17v5M8 3h8l-1 6 3 3H6l3-3z"/>',
  bold: '<path d="M7 5h6a3.5 3.5 0 0 1 0 7H7zM7 12h7a3.5 3.5 0 0 1 0 7H7z"/>',
  italic: '<path d="M10 5h8M6 19h8M14 5l-4 14"/>',
  ul: '<path d="M9 6h11M9 12h11M9 18h11M4.5 6h0M4.5 12h0M4.5 18h0"/>',
  smile: '<circle cx="12" cy="12" r="9"/><path d="M8.5 14a4 4 0 0 0 7 0M9 9.5h0M15 9.5h0"/>',
  pgp: '<path d="M12 3 4 6v6c0 5 3.5 8 8 9 4.5-1 8-4 8-9V6z"/><rect x="9" y="11" width="6" height="5" rx="1"/><path d="M10 11V9.5a2 2 0 0 1 4 0V11"/>',
  dice: '<rect x="4" y="4" width="16" height="16" rx="3"/><path d="M9 9h0M15 9h0M12 12h0M9 15h0M15 15h0"/>',
};

function icon(name, cls = '') {
  return `<svg class="${cls}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${P[name] || ''}</svg>`;
}
const esc = (s) => String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
function hash(s) { let h = 0; for (const c of String(s).toLowerCase()) h = (h * 31 + c.charCodeAt(0)) | 0; return Math.abs(h); }
const av = (name) => 'a' + (hash(name) % 10);
const letter = (s) => esc((String(s).trim().match(/[\p{L}\p{N}]/u) || ['•'])[0].toUpperCase());
const NEW = '<span class="new-tag">neu</span>';

function pwHtml(pw) {
  return [...pw].map((c) => {
    const e = esc(c);
    if (/\p{N}/u.test(c)) return `<span class="d">${e}</span>`;
    if (!/\p{L}/u.test(c)) return `<span class="y">${e}</span>`;
    return e;
  }).join('');
}
function strength(pw) {
  let s = 0;
  if (pw.length >= 12) s++; if (pw.length >= 16) s++;
  if (/\d/.test(pw) && /[a-z]/.test(pw) && /[A-Z]/.test(pw)) s++;
  if (/[^A-Za-z0-9]/.test(pw)) s++;
  return Math.min(4, s);
}
const STRENGTH = ['Sehr schwach', 'Schwach', 'Mittel', 'Stark', 'Sehr stark'];

// --- sample data ---------------------------------------------------------------------------------
const VAULTS = [{ id: 'p', name: 'Privat' }, { id: 'w', name: 'Arbeit' }];
const ENTRIES = [
  { id: 1, type: 'login', title: 'Codeberg', user: 'ghul', url: 'codeberg.org', pw: 'T7#vq!Lm2pR9zX$e', vault: 'p', fav: true, totp: 'codeberg', used: 1 },
  { id: 2, type: 'login', title: 'Posteo', user: 'max@posteo.de', url: 'posteo.de', pw: 'Wiese-Kompass-71-Laterne', vault: 'p', fav: true, used: 2 },
  { id: 3, type: 'login', title: 'Jira', user: 'm.mustermann@firma.de', url: 'firma.atlassian.net', pw: 'Sommer2024!', vault: 'w', weak: true, reused: true, used: 3 },
  { id: 4, type: 'login', title: 'Nextcloud Firma', user: 'mmustermann', url: 'cloud.firma.de', pw: 'Sommer2024!', vault: 'w', reused: true, no2fa: true },
  { id: 5, type: 'login', title: 'Stadtwerke', user: 'kunde-48211', url: 'stadtwerke.example', pw: 'kL9$mQ2#vB7!', vault: 'p', no2fa: true },
  { id: 6, type: 'card', title: 'Visa', user: '•••• 4821', vault: 'p', number: '4111 1111 1111 4821', exp: '08/29', cvc: '713', holder: 'Max Mustermann' },
  { id: 7, type: 'wifi', title: 'WLAN Zuhause', user: 'FritzBox 7590', vault: 'p', pw: 'Kaffee&Kuchen-42-Garten', ssid: 'FRITZ!Box 7590 XY' },
  { id: 8, type: 'note', title: 'Router-Notizen', user: 'Portfreigaben, DynDNS', vault: 'p', text: 'SSH auf 2222 weitergeleitet.\nDynDNS: max.example.org\nWireGuard-Port 51820/udp.' },
  { id: 9, type: 'identity', title: 'Personalausweis', user: 'Max Mustermann', vault: 'p' },
  { id: 10, type: 'ssh', title: 'Heimserver', user: 'ed25519 · max@nas', vault: 'p' },
  { id: 11, type: 'login', title: 'GitHub', user: 'ghulaschsuppe', url: 'github.com', pw: 'q9!Rv#2LmT$8xZpW', vault: 'p', totp: 'github' },
];
const TYPES = {
  login: { label: 'Anmeldung', icon: 'globe' }, card: { label: 'Kreditkarte', icon: 'card' }, note: { label: 'Sichere Notiz', icon: 'note' },
  wifi: { label: 'WLAN', icon: 'wifi' }, identity: { label: 'Identität', icon: 'idcard' }, ssh: { label: 'SSH-Schlüssel', icon: 'terminal' },
};
const TOTPS = [
  { id: 'codeberg', issuer: 'Codeberg', account: 'ghul', fav: true },
  { id: 'github', issuer: 'GitHub', account: 'ghulaschsuppe', fav: true },
  { id: 'posteo', issuer: 'Posteo', account: 'max@posteo.de' },
  { id: 'proton', issuer: 'Hetzner', account: 'K1234567' },
  { id: 'bank', issuer: 'Sparkasse', account: 'Online-Banking' },
  { id: 'aws', issuer: 'Cloudflare', account: 'max@example.org' },
];
const DRIVE = {
  root: [
    { id: 'd-fotos', name: 'Fotos', dir: true, n: 248 },
    { id: 'd-vertr', name: 'Verträge', dir: true, n: 12 },
    { id: 'd-steuer', name: 'Steuer', dir: true, n: 31 },
    { id: 'f1', name: 'Steuer 2025.pdf', size: '2,4 MB', kind: 'pdf', date: 'Gestern', fav: true },
    { id: 'f2', name: 'Urlaub Ostsee.jpg', size: '4,1 MB', kind: 'img', date: 'Mo.' },
    { id: 'f3', name: 'Notizen.txt', size: '1,2 KB', kind: 'txt', date: '28. Okt.', offline: true },
    { id: 'f4', name: 'Hochzeit Schnitt.mp4', size: '812 MB', kind: 'vid', date: '12. Okt.' },
    { id: 'f5', name: 'Podcast Folge 3.mp3', size: '48 MB', kind: 'aud', date: '3. Okt.' },
  ],
  'd-vertr': [
    { id: 'f6', name: 'Mietvertrag.pdf', size: '1,1 MB', kind: 'pdf', date: '2. Jan.' },
    { id: 'f7', name: 'Handyvertrag.pdf', size: '640 KB', kind: 'pdf', date: '14. Mai' },
    { id: 'f8', name: 'Versicherung KFZ.pdf', size: '2,0 MB', kind: 'pdf', date: '9. Feb.' },
  ],
};
const KIND = { pdf: ['filetext', 'a7'], img: ['image', 'a1'], txt: ['note', 'a9'], vid: ['film', 'a4'], aud: ['music', 'a5'] };
const ACCOUNTS = [
  { id: 'priv', name: 'Privat', email: 'max@posteo.de', cls: 'a3' },
  { id: 'work', name: 'Arbeit', email: 'm.mustermann@firma.de', cls: 'a1' },
];
const MAILS = [
  { id: 1, acct: 'priv', from: 'Erika Musterfrau', addr: 'erika@example.org', subj: 'Treffen am Freitag', snip: 'Hi Max, passt dir Freitag um 19 Uhr beim Italiener? Ich habe einen Tisch für vier reserviert …', time: '10:42', day: 'Heute', unread: true, thread: 3 },
  { id: 2, acct: 'work', from: 'Jana Schulz', addr: 'j.schulz@firma.de', subj: 'Review: Quartalsbericht Q3', snip: 'Anbei die überarbeitete Fassung mit deinen Anmerkungen. Bitte bis Mittwoch freigeben.', time: '09:15', day: 'Heute', unread: true, attach: 'Q3-Bericht.pdf', label: 'Wichtig' },
  { id: 3, acct: 'priv', from: 'Stadtwerke', addr: 'service@stadtwerke.example', subj: 'Ihre Jahresabrechnung 2025', snip: 'Guten Tag, anbei erhalten Sie Ihre Jahresabrechnung. Das Guthaben wird Ihnen erstattet.', time: 'Gestern', day: 'Gestern', fav: true, attach: 'Abrechnung.pdf' },
  { id: 4, acct: 'priv', from: 'Codeberg', addr: 'noreply@codeberg.org', subj: 'Neue Anmeldung bei deinem Konto', snip: 'Es gab eine neue Anmeldung von Firefox unter Linux. Warst du das?', time: 'Gestern', day: 'Gestern' },
  { id: 5, acct: 'work', from: 'IT-Service', addr: 'it@firma.de', subj: 'Wartungsfenster am Samstag', snip: 'Am Samstag zwischen 6 und 9 Uhr sind VPN und Mail nicht erreichbar.', time: 'Mo.', day: 'Diese Woche' },
  { id: 6, acct: 'priv', from: 'Max Mustermann', addr: 'max@posteo.de', subj: 'Notiz an mich', snip: 'Reifenwechsel Termin 18.11., Werkstatt Müller.', time: 'Mo.', day: 'Diese Woche' },
];

// --- state ---------------------------------------------------------------------------------------
const S = {
  phase: 'unlock', // start | unlock | app
  tab: 'pass',
  stack: [],
  passFilter: 'all',
  vault: null,
  query: '',
  revealed: {},
  authGrid: false,
  driveGrid: false,
  driveFolder: null,
  select: false,
  sel: {},
  linkDays: 7,
  shareReady: false,
  shareStep: 1,
  shareDir: 'public_html/teilen',
  linkPw: false,
  mailAcct: 'all',
  mailFolder: 'inbox',
  read: {},
  setGen: { len: 20, mode: 'chars', upper: true, lower: true, digits: true, symbols: true },
  toggles: { bio: true, pin: false, shake: true, screenOff: true, clip: true, shots: false, autosync: true, autofill: true, subtitles: true, hideCodes: false, nextCode: true, photoBackup: true, wifiOnly: true, offlineMail: true, remoteImages: false, notifyMail: true, notifyPreview: false, pwned: false },
  theme: 'light',
};
const current = () => S.stack[S.stack.length - 1];
const push = (screen, params = {}) => { S.stack.push({ screen, ...params }); render(); };
const pop = () => { S.stack.pop(); render(); };

// --- shell ---------------------------------------------------------------------------------------
const $ = (id) => document.getElementById(id);
function toast(text) {
  const t = $('toast');
  t.textContent = text;
  t.classList.add('show');
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => t.classList.remove('show'), 1800);
}
function sheet(html) {
  $('sheet').innerHTML = html;
  $('device').classList.add('sheet-open');
  afterRender($('sheet'));
}
function closeSheet() { $('device').classList.remove('sheet-open', 'drawer-open'); }

function topbar(title, opts = {}) {
  const left = opts.back ? `<button class="icon-btn" data-act="back" aria-label="Zurück">${icon('back')}</button>` : '';
  const ctx = opts.ctx ? (opts.ctx.act
    ? `<button class="ctx ${opts.ctx.c || ''}" data-act="${opts.ctx.act}">${opts.ctx.label}${icon('down')}</button>`
    : `<span class="ctx ${opts.ctx.c || ''}">${opts.ctx.label}</span>`) : '';
  return `<div class="topbar">${left}<div class="ttl"><h1 class="${opts.back ? 'small' : ''}">${title}</h1>${ctx}</div>${opts.right || ''}
    <button class="icon-btn" data-act="lock" aria-label="Sperren">${icon('lock')}</button></div>`;
}
function nav() {
  const tabs = [['pass', 'GhulPass', 'key', 'c-pass'], ['auth', 'GhulAuth', 'shieldcheck', 'c-auth'], ['drive', 'GhulDrive', 'cloud', 'c-drive'], ['mail', 'GhulMail', 'mail', 'c-mail'], ['cal', 'GhulCal', 'cal', 'c-cal'], ['settings', 'Optionen', 'sliders', 'c-set']];
  const unread = MAILS.filter((m) => m.unread && !S.read[m.id]).length;
  return `<nav class="nav">${tabs.map(([id, label, ic, c]) => `<button class="${c} ${S.tab === id ? 'on' : ''}" data-act="tab" data-arg="${id}">
    <span class="ind">${icon(ic)}${id === 'mail' && unread ? `<span class="badge">${unread}</span>` : ''}</span>${label}</button>`).join('')}</nav>`;
}

// --- start and unlock ----------------------------------------------------------------------------
function brand(sub) {
  return `<div class="hero"><div class="logo">${icon('shield')}</div><h1>GhulSuite</h1>
    <div class="bars"><i class="bg-pass"></i><i class="bg-auth"></i><i class="bg-drive"></i><i class="bg-mail"></i><i class="bg-cal"></i></div>${sub ? `<p>${sub}</p>` : ''}</div>`;
}
function startScreen() {
  const opt = (ic, cls, t, s, act, primary) => `<button class="card row plain" data-act="${act}">
    <span class="avatar ${cls}">${icon(ic)}</span><span class="grow"><span class="t">${t}</span><span class="s">${s}</span></span>${icon('chev', 'chev')}</button>`;
  return `<div class="screen">${brand('Passwörter, 2FA, Dateien, Mail und Kalender – verschlüsselt auf deinen Geräten und deinem eigenen Server.')}
    ${opt('plus', 'a0', 'Neu einrichten', 'Master-Passwort + Security Key oder Schlüsseldatei', 'start-new')}
    ${opt('cloud', 'a1', 'Von meinem Server holen', 'Suite liegt schon auf deinem Server', 'start-new')}
    ${opt('folder', 'a2', 'Vorhandene Suite öffnen', 'Ordner auf dem Gerät, USB-Stick, Cloud-Ordner', 'start-new')}
    ${opt('download', 'a3', 'Aus anderer App umziehen', 'Proton Pass, Bitwarden, KeePass, Aegis …', 'start-new')}
    <p class="hint-text">Ohne Konto, ohne Cloud-Zwang. Wiederherstellungscode und Notfallblatt gehören immer dazu.</p></div>`;
}
function unlockScreen() {
  return `<div class="screen">${brand()}
    <div class="field"><div class="grow"><label>Master-Passwort</label><input type="password" value="correct-horse-battery" aria-label="Master-Passwort"></div>${icon('eye')}</div>
    <div class="keyhint"><span class="pulse">${icon('usbkey')}</span><span>Security Key einstecken und berühren …</span></div>
    <button class="btn primary block" data-act="unlock">${icon('lock')} Entsperren</button>
    <div class="btn-row" data-u="0"></div>
    <button class="btn tonal block" data-act="unlock">${icon('fingerprint')} Mit Fingerabdruck</button>
    <p class="hint-text" data-u="1"></p>
    <div class="card flush">
      <button class="row plain" data-act="phase" data-arg="start"><span class="grow">Andere Suite oder Tresor öffnen</span>${icon('chev', 'chev')}</button>
      <button class="row plain" data-act="toast" data-arg="Notfall: Wiederherstellungscode eingeben"><span class="grow">Kein Zugang mehr? Wiederherstellungscode</span>${icon('chev', 'chev')}</button>
    </div></div>`;
}

// --- GhulPass ------------------------------------------------------------------------------------
function passList() {
  const q = S.query.toLowerCase();
  const list = ENTRIES.filter((e) => (S.passFilter === 'all' || (S.passFilter === 'fav' ? e.fav : e.type === S.passFilter))
    && (!S.vault || e.vault === S.vault) && (!q || (e.title + e.user).toLowerCase().includes(q)));
  const chips = [['all', 'Alle'], ['fav', 'Favoriten', 'star'], ['login', 'Anmeldungen', 'globe'], ['card', 'Karten', 'card'], ['note', 'Notizen', 'note'], ['wifi', 'WLAN', 'wifi'], ['identity', 'Identitäten', 'idcard']];
  const recent = ENTRIES.filter((e) => e.used).sort((a, b) => a.used - b.used);
  const issues = ENTRIES.filter((e) => e.weak || e.reused || e.no2fa).length;
  const vaultName = S.vault ? VAULTS.find((v) => v.id === S.vault).name : 'Alle Tresore';
  return `${topbar('GhulPass', { ctx: { label: esc(vaultName), act: 'sheet-vaults', c: 'c-pass' } })}
  <div class="screen">
    <label class="search">${icon('search')}<input id="q" placeholder="Einträge suchen" value="${esc(S.query)}"></label>
    <div class="chips">${chips.map(([id, l, ic]) => `<button class="chip ${S.passFilter === id ? 'on' : ''}" data-act="pass-filter" data-arg="${id}">${ic ? icon(ic) : ''}${l}</button>`).join('')}</div>
    ${!q && S.passFilter === 'all' ? `
    <button class="card row plain" data-act="push" data-arg="security"><span class="avatar a3">${icon('shieldcheck')}</span>
      <span class="grow"><span class="t">Sicherheitscheck${NEW}</span><span class="s">${issues} Einträge brauchen Aufmerksamkeit</span></span>${icon('chev', 'chev')}</button>
    <div class="section-title">Zuletzt verwendet ${NEW}</div>
    <div class="chips">${recent.map((e) => `<button class="chip" data-act="entry" data-arg="${e.id}"><span class="avatar sm ${av(e.title)}">${letter(e.title)}</span>${esc(e.title)}</button>`).join('')}</div>` : ''}
    <div class="section-title">${list.length} Einträge</div>
    <div class="card flush">${list.map((e) => `<button class="row" data-act="entry" data-arg="${e.id}">
      ${e.type === 'login' ? `<span class="avatar ${av(e.title)}">${letter(e.title)}</span>` : `<span class="avatar ${av(e.type)}">${icon(TYPES[e.type].icon)}</span>`}
      <span class="grow"><span class="t">${esc(e.title)}</span><span class="s">${esc(e.user)}${S.vault ? '' : ' · ' + VAULTS.find((v) => v.id === e.vault).name}</span></span>
      <span class="end">${e.totp ? icon('clock') : ''}${e.fav ? `<span class="c-mail">${icon('star')}</span>` : ''}</span></button>`).join('') || '<div class="empty">Nichts gefunden.</div>'}</div>
  </div><button class="fab bg-pass" data-act="sheet-newitem" aria-label="Neuer Eintrag">${icon('plus')}</button>`;
}

function fieldRow(label, value, opts = {}) {
  const id = opts.id;
  const shown = !opts.secret || S.revealed[id];
  const val = opts.secret ? (shown ? `<span class="pw">${pwHtml(value)}</span>` : '••••••••••••') : esc(value);
  return `<div class="field"><div class="grow"><label>${label}</label><div class="value">${val}</div>${opts.extra || ''}</div>
    ${opts.secret ? `<button class="icon-btn" data-act="reveal" data-arg="${id}" aria-label="Anzeigen">${icon(shown ? 'eyeoff' : 'eye')}</button>` : ''}
    ${opts.open ? `<button class="icon-btn" data-act="toast" data-arg="Öffnet ${esc(value)}" aria-label="Öffnen">${icon('link')}</button>` : ''}
    <button class="icon-btn" data-act="copy" data-arg="${esc(label)}" aria-label="Kopieren">${icon('copy')}</button></div>`;
}
function totpCode(id, offset = 0) {
  const step = Math.floor(Date.now() / 30000) + offset;
  return String(hash(id + ':' + step) % 1000000).padStart(6, '0');
}
const remaining = () => 30 - (Math.floor(Date.now() / 1000) % 30);
function ringSvg(rem, period = 30) {
  const c = 2 * Math.PI * 15;
  return `<span class="ring"><svg viewBox="0 0 38 38"><circle class="trk" cx="19" cy="19" r="15" fill="none" stroke-width="4"/>
    <circle class="val ${rem <= 5 ? 'soon' : ''}" cx="19" cy="19" r="15" fill="none" stroke-width="4" stroke-linecap="round"
    stroke-dasharray="${c.toFixed(1)}" stroke-dashoffset="${(c * (1 - rem / period)).toFixed(1)}"/></svg><span>${rem}</span></span>`;
}
function entryScreen(id) {
  const e = ENTRIES.find((x) => x.id === Number(id));
  const t = TYPES[e.type];
  let fields = '';
  if (e.type === 'login') {
    const st = strength(e.pw);
    fields = fieldRow('Benutzername', e.user)
      + fieldRow('Passwort', e.pw, { secret: true, id: 'pw' + e.id, extra: `<div class="strength"><i class="js-w ${st < 2 ? 's-weak' : st < 3 ? 's-ok' : 's-good'}" data-w="${(st + 1) * 20}"></i></div><div class="hint-text" data-u="2">${STRENGTH[st]}${e.reused ? ' · <b>mehrfach verwendet</b>' : ''}</div>` })
      + fieldRow('Website', 'https://' + e.url, { open: true })
      + (e.totp ? `<div class="field"><div class="grow"><label>2FA-Code ${NEW}</label><div class="pw" data-u="3">${totpCode(e.totp).replace(/(\d{3})/, '$1 ')}</div></div>${ringSvg(remaining())}<button class="icon-btn" data-act="copy" data-arg="2FA-Code">${icon('copy')}</button></div>` : '')
      + (e.no2fa ? `<div class="banner">${icon('alert')}<span>Diese Website unterstützt 2FA – noch nicht eingerichtet.</span><button data-act="tab" data-arg="auth">Einrichten</button></div>` : '');
  } else if (e.type === 'card') {
    fields = fieldRow('Karteninhaber', e.holder) + fieldRow('Kartennummer', e.number, { secret: true, id: 'cn' + e.id }) + `<div class="btn-row">${fieldRow('Gültig bis', e.exp)}</div>` + fieldRow('Prüfnummer', e.cvc, { secret: true, id: 'cvc' + e.id });
  } else if (e.type === 'wifi') {
    fields = fieldRow('Netzwerk (SSID)', e.ssid) + fieldRow('Passwort', e.pw, { secret: true, id: 'pw' + e.id })
      + `<button class="btn tonal block" data-act="toast" data-arg="QR-Code zum Teilen angezeigt">${icon('qr')} Als QR-Code teilen ${NEW}</button>`;
  } else if (e.type === 'note') {
    fields = `<div class="field"><div class="grow"><label>Notiz</label><div class="value" data-u="4">${esc(e.text)}</div></div></div>`;
  } else {
    fields = fieldRow('Name', e.user);
  }
  return `${topbar(t.label, { back: true, right: `<button class="icon-btn" data-act="toast" data-arg="${e.fav ? 'Aus Favoriten entfernt' : 'Zu Favoriten'}">${icon('star')}</button><button class="icon-btn" data-act="sheet-entrymenu">${icon('more')}</button>` })}
  <div class="screen">
    <div class="profile" data-u="5">${e.type === 'login' ? `<span class="avatar lg ${av(e.title)}">${letter(e.title)}</span>` : `<span class="avatar lg ${av(e.type)}">${icon(t.icon)}</span>`}
      <div class="grow"><h2 data-u="6">${esc(e.title)}</h2><span class="tag">${icon('layers')} ${VAULTS.find((v) => v.id === e.vault).name}</span> ${e.weak ? '<span class="tag danger">Schwach</span>' : ''}</div></div>
    ${fields}
    <div class="section-title">Mehr</div>
    <div class="card flush">
      <button class="row plain" data-act="toast" data-arg="Passwortverlauf: 3 frühere Passwörter"><span class="avatar sm a5">${icon('history')}</span><span class="grow">Passwortverlauf ${NEW}</span><span class="end">3 ${icon('chev', 'chev')}</span></button>
      <button class="row plain" data-act="toast" data-arg="Anhang hinzufügen"><span class="avatar sm a1">${icon('clip')}</span><span class="grow">Anhänge ${NEW}</span><span class="end">0 ${icon('chev', 'chev')}</span></button>
      <button class="row plain" data-act="toast" data-arg="Eigenes Feld hinzufügen"><span class="avatar sm a2">${icon('plus')}</span><span class="grow">Eigenes Feld hinzufügen</span>${icon('chev', 'chev')}</button>
    </div>
    <p class="hint-text">Geändert am 9. Nov. 2026 · angelegt am 3. März 2025</p>
    <button class="btn primary block" data-act="toast" data-arg="Bearbeiten">${icon('edit')} Bearbeiten</button>
  </div>`;
}
function securityScreen() {
  const weak = ENTRIES.filter((e) => e.weak), reused = ENTRIES.filter((e) => e.reused), no2fa = ENTRIES.filter((e) => e.no2fa);
  const group = (title, ic, cls, list, why) => `<div class="section-title">${title}</div><div class="card flush">${list.map((e) => `<button class="row" data-act="entry" data-arg="${e.id}">
    <span class="avatar ${av(e.title)}">${letter(e.title)}</span><span class="grow"><span class="t">${esc(e.title)}</span><span class="s">${why}</span></span><span class="tag ${cls}">${icon(ic)}</span></button>`).join('')}</div>`;
  return `${topbar('Sicherheitscheck', { back: true })}<div class="screen">
    <div class="card score"><span class="num">78</span><div><h3>Gut, mit Luft nach oben</h3><p>Alles wird nur auf diesem Gerät geprüft.</p></div></div>
    ${group('Mehrfach verwendet', 'copy', 'warn', reused, 'Gleiches Passwort wie ein anderer Eintrag')}
    ${group('Schwach', 'alert', 'danger', weak, 'Leicht zu erraten')}
    ${group('2FA möglich, aber nicht eingerichtet', 'shieldcheck', 'warn', no2fa, 'Website unterstützt Einmal-Codes')}
    <div class="card"><div class="row plain" data-u="7"><span class="grow"><b>Datenlecks prüfen</b><span class="s">Über „Have I Been Pwned“ mit k-Anonymität: nur 5 Zeichen eines Hashes verlassen das Gerät.</span></span>
      <button class="toggle ${S.toggles.pwned ? 'on' : ''}" data-act="toggle" data-arg="pwned" aria-label="Datenlecks prüfen"></button></div></div>
  </div>`;
}

// --- GhulAuth ------------------------------------------------------------------------------------
function authScreen() {
  const rem = remaining();
  const q = S.query.toLowerCase();
  const list = TOTPS.filter((t) => !q || (t.issuer + t.account).toLowerCase().includes(q));
  const favs = list.filter((t) => t.fav), rest = list.filter((t) => !t.fav);
  const card = (t) => {
    const code = S.toggles.hideCodes && !S.revealed['t' + t.id] ? '••• •••' : totpCode(t.id).replace(/(\d{3})/, '$1 ');
    return `<button class="otp" data-act="otp" data-arg="${t.id}"><span class="avatar ${av(t.issuer)}">${letter(t.issuer)}</span>
      <span class="grow" data-u="8"><span class="t" data-u="9">${esc(t.issuer)}</span> <span class="s" data-u="10">${esc(t.account)}</span>
      <span class="code ${rem <= 5 ? 'soon' : ''}" data-u="11">${code}</span>
      ${S.toggles.nextCode && rem <= 8 ? `<span class="next">danach ${totpCode(t.id, 1)}</span>` : ''}</span>${ringSvg(rem)}</button>`;
  };
  return `${topbar('GhulAuth', { ctx: { label: `Persönlich · ${TOTPS.length} Konten`, act: 'sheet-authvaults', c: 'c-auth' }, right: `<button class="icon-btn" data-act="auth-grid" aria-label="Ansicht">${icon(S.authGrid ? 'list' : 'grid')}</button>` })}
  <div class="screen">
    <label class="search">${icon('search')}<input id="q" placeholder="Konten suchen" value="${esc(S.query)}"></label>
    ${favs.length ? `<div class="section-title">Favoriten ${NEW}</div><div class="${S.authGrid ? 'otp-grid' : ''}">${favs.map(card).join('')}</div>` : ''}
    <div class="section-title">Alle Konten <button data-act="toast" data-arg="Ziehen zum Sortieren">Sortieren</button></div>
    <div class="${S.authGrid ? 'otp-grid' : ''}">${rest.map(card).join('')}</div>
    <p class="hint-text">Antippen kopiert den Code. Lange drücken: umbenennen, Favorit, löschen, als QR exportieren ${NEW}.</p>
    <div class="banner">${icon('clock')}<span>Geräteuhr geht genau – Codes stimmen. ${NEW}</span></div>
  </div><button class="fab bg-auth" data-act="sheet-addtotp" aria-label="Konto hinzufügen">${icon('qr')}</button>`;
}

// --- GhulDrive -----------------------------------------------------------------------------------
function driveItem(f) {
  const [ic, cls] = f.dir ? ['folder', 'a1'] : KIND[f.kind];
  if (S.select) {
    const on = !!S.sel[f.id];
    return `<button class="row ${on ? 'selected' : ''}" data-act="drive-sel" data-arg="${f.id}"><span class="avatar ${on ? 'checked' : cls}">${icon(on ? 'check' : ic)}</span>
      <span class="grow"><span class="t">${esc(f.name)}</span><span class="s">${f.dir ? f.n + ' Elemente' : f.size + ' · ' + f.date}</span></span></button>`;
  }
  if (S.driveGrid) {
    return `<button class="tile" data-act="${f.dir ? 'drive-open' : 'drive-file'}" data-arg="${f.id}"><span class="thumb ${cls} avatar" data-u="12">${icon(ic)}</span>
      <div class="t">${esc(f.name)}</div><div class="s">${f.dir ? f.n + ' Elemente' : f.size}</div></button>`;
  }
  return `<button class="row" data-act="${f.dir ? 'drive-open' : 'drive-file'}" data-arg="${f.id}"><span class="avatar ${cls}">${icon(ic)}</span>
    <span class="grow"><span class="t">${esc(f.name)}</span><span class="s">${f.dir ? f.n + ' Elemente' : f.size + ' · ' + f.date}</span></span>
    <span class="end">${f.offline ? icon('download') : ''}${f.fav ? `<span class="c-mail">${icon('star')}</span>` : ''}</span></button>`;
}
function driveScreen() {
  const folder = S.driveFolder;
  const items = DRIVE[folder || 'root'] || [];
  const title = folder ? DRIVE.root.find((f) => f.id === folder).name : 'GhulDrive';
  return `${topbar(folder ? title : 'GhulDrive', { back: !!folder, ctx: folder ? null : { label: 'Heimserver · Meine Dateien', act: 'sheet-servers', c: 'c-drive' }, right: `<button class="icon-btn" data-act="drive-grid" aria-label="Ansicht">${icon(S.driveGrid ? 'list' : 'grid')}</button>` })}
  <div class="screen">
    <label class="search">${icon('search')}<input id="q" placeholder="In GhulDrive suchen" value="${esc(S.query)}"></label>
    ${!folder ? `
    <div class="card storage"><div class="profile"><span class="avatar a1">${icon('server')}</span><div class="grow"><b>Heimserver</b><span class="s" data-u="10">/home/max/ghulsuite/drive · verschlüsselt</span></div><span class="tag ok">${icon('check')} Aktuell</span></div>
      <div class="meter"><i class="bg-drive js-w" data-w="38"></i><i class="bg-pass js-w" data-w="9"></i><i class="bg-mail js-w" data-w="6"></i></div>
      <div class="s" data-u="10">1,1 TB frei von 2 TB · Fotos 760 GB · Dateien 180 GB · Videos 110 GB ${NEW}</div></div>
    <div class="quick">
      <button data-act="toast" data-arg="Zuletzt geöffnet">${icon('clock')}Zuletzt</button>
      <button data-act="toast" data-arg="Favoriten">${icon('star')}Favoriten</button>
      <button data-act="toast" data-arg="Offline verfügbar">${icon('download')}Offline</button>
      <button data-act="toast" data-arg="Papierkorb: 30 Tage">${icon('trash')}Papierkorb</button>
    </div>
    <button class="card row plain" data-act="sheet-shares"><span class="avatar a5">${icon('link')}</span><span class="grow"><span class="t">Freigaben ${NEW}</span><span class="s">2 aktive Links · 1 läuft morgen ab</span></span>${icon('chev', 'chev')}</button>
    <div class="card transfer"><span class="avatar a4">${icon('upload')}</span><div class="grow"><b>Lädt hoch: Hochzeit Schnitt.mp4</b><div class="bar"><i class="js-w" data-w="64"></i></div>
      <span class="s" data-u="13">520 von 812 MB · noch 2 Min. · setzt nach Abbruch fort ${NEW}</span></div><button class="icon-btn" data-act="toast" data-arg="Pausiert">${icon('x')}</button></div>
    <div class="card"><div class="row plain" data-u="7"><span class="avatar a2">${icon('camera')}</span><span class="grow"><b>Foto-Sicherung ${NEW}</b><span class="s">Neue Fotos automatisch verschlüsselt sichern · nur im WLAN</span></span>
      <button class="toggle ${S.toggles.photoBackup ? 'on' : ''}" data-act="toggle" data-arg="photoBackup" aria-label="Foto-Sicherung"></button></div></div>` : `<p class="hint-text">Meine Dateien › ${esc(title)}</p>`}
    <div class="section-title">${folder ? items.length + ' Dateien' : 'Meine Dateien'} <span><button data-act="drive-select">${S.select ? 'Fertig' : 'Auswählen'}</button> · <button data-act="toast" data-arg="Sortieren: Name, Datum, Größe">Name ${icon('down')}</button></span></div>
    ${S.driveGrid ? `<div class="grid">${items.map(driveItem).join('')}</div>` : `<div class="card flush">${items.map(driveItem).join('')}</div>`}
  </div><button class="fab bg-drive" data-act="sheet-upload" aria-label="Hochladen">${icon('plus')}</button>`;
}

// --- GhulMail ------------------------------------------------------------------------------------
function mailScreen() {
  const acct = S.mailAcct === 'all' ? null : ACCOUNTS.find((a) => a.id === S.mailAcct);
  const q = S.query.toLowerCase();
  const list = MAILS.filter((m) => (!acct || m.acct === acct.id) && (!q || (m.from + m.subj + m.snip).toLowerCase().includes(q)));
  let lastDay = '';
  const rows = list.map((m) => {
    const unread = m.unread && !S.read[m.id];
    const a = ACCOUNTS.find((x) => x.id === m.acct);
    const head = m.day !== lastDay ? `</div><div class="section-title">${m.day}</div><div class="card flush">` : '';
    lastDay = m.day;
    return `${head}<button class="mail-row ${unread ? 'unread' : ''}" data-act="mail-open" data-arg="${m.id}">
      <span class="avatar ${av(m.from)}">${letter(m.from)}</span>
      <span class="grow"><span class="top"><span class="from">${esc(m.from)}${m.thread ? ` <span class="s" data-u="14">(${m.thread})</span>` : ''}</span><span class="time">${m.time}</span></span>
        <span class="top"><span class="subj">${esc(m.subj)}</span>${m.fav ? `<span class="c-mail">${icon('star')}</span>` : ''}</span>
        <span class="snip">${esc(m.snip)}</span>
        <span class="meta">${!acct ? `<span class="tag"><i class="dot" data-u="15"></i>${a.name}</span>` : ''}${m.attach ? `<span class="tag">${icon('clip')}${esc(m.attach)}</span>` : ''}${m.label ? `<span class="tag warn">${icon('tag')}${m.label}</span>` : ''}</span></span>
      ${unread ? '<span class="dot"></span>' : ''}</button>`;
  }).join('');
  return `${topbar('GhulMail', { ctx: { label: acct ? `${acct.name} · Posteingang` : 'Alle Posteingänge', act: 'sheet-mailbox', c: 'c-mail' }, right: `<button class="icon-btn" data-act="toast" data-arg="Synchronisiert">${icon('refresh')}</button>` })}
  <div class="screen">
    <label class="search">${icon('search')}<input id="q" placeholder="Mails durchsuchen" value="${esc(S.query)}"></label>
    <div class="chips"><button class="chip on">Alle</button><button class="chip" data-act="toast" data-arg="Filter: Ungelesen">Ungelesen</button><button class="chip" data-act="toast" data-arg="Filter: Markiert">${icon('star')}Markiert</button><button class="chip" data-act="toast" data-arg="Filter: Mit Anhang">${icon('clip')}Anhänge</button></div>
    <div>${rows.slice(6)}</div></div>
    <p class="hint-text" data-u="16"></p>
  </div><button class="fab bg-mail" data-act="compose" aria-label="Schreiben">${icon('edit')} Schreiben</button>`;
}
function mailDrawer() {
  const unread = (id) => MAILS.filter((m) => (id === 'all' || m.acct === id) && m.unread && !S.read[m.id]).length;
  const folder = (ic, name, n) => `<button class="row" data-act="toast" data-arg="${name}">${icon(ic)}<span class="grow">${name}</span>${n ? `<span class="end">${n}</span>` : ''}</button>`;
  return `<h2>Postfach</h2><div class="mailbox">
    <button class="row ${S.mailAcct === 'all' ? 'on' : ''}" data-act="mail-acct" data-arg="all">${icon('inbox')}<span class="grow">Alle Posteingänge ${NEW}</span><span class="end">${unread('all') || ''}</span></button>
    ${ACCOUNTS.map((a) => `<button class="row ${S.mailAcct === a.id ? 'on' : ''}" data-act="mail-acct" data-arg="${a.id}"><span class="avatar sm ${a.cls}">${letter(a.name)}</span><span class="grow">${a.name}<br><span class="s">${a.email}</span></span><span class="end">${unread(a.id) || ''}</span></button>`).join('')}
    <h3>Ordner</h3>${folder('star', 'Markiert', 1)}${folder('clock', 'Später senden', '')}${folder('send', 'Gesendet', '')}${folder('file', 'Entwürfe', 2)}${folder('archive', 'Archiv', '')}${folder('alert', 'Spam', 4)}${folder('trash', 'Papierkorb', '')}
    <h3>Labels ${NEW}</h3>${folder('tag', 'Wichtig', 1)}${folder('tag', 'Rechnungen', '')}
    <h3></h3><button class="row" data-act="drawer-settings">${icon('sliders')}<span class="grow">Mail-Einstellungen</span></button></div>`;
}
function mailThread(id) {
  const m = MAILS.find((x) => x.id === Number(id));
  const a = ACCOUNTS.find((x) => x.id === m.acct);
  const earlier = m.thread ? `<div class="msg collapsed"><div class="head"><span class="avatar sm ${av('Max Mustermann')}">M</span><div class="grow"><b>Du</b> <span class="s" data-u="18">· Mi.</span><span class="s" data-u="19">Klingt gut! Welcher Tag passt euch denn …</span></div></div></div>
    <div class="msg collapsed"><div class="head"><span class="avatar sm ${av(m.from)}">${letter(m.from)}</span><div class="grow"><b>${esc(m.from)}</b> <span class="s" data-u="18">· Mi.</span><span class="s" data-u="19">Wollen wir uns mal wieder treffen? …</span></div></div></div>` : '';
  return `${topbar('', { back: true, right: `<button class="icon-btn" data-act="toast" data-arg="Archiviert">${icon('archive')}</button><button class="icon-btn" data-act="toast" data-arg="In den Papierkorb">${icon('trash')}</button><button class="icon-btn" data-act="sheet-mailmenu">${icon('more')}</button>` })}
  <div class="screen" data-u="20">
    <h2 data-u="21">${esc(m.subj)}</h2>
    <div data-u="22"><span class="tag">${a.name}</span>${m.label ? `<span class="tag warn">${m.label}</span>` : ''}<span class="tag ok">${icon('lock')} TLS</span></div>
    ${earlier}
    <div class="msg"><div class="head"><span class="avatar ${av(m.from)}">${letter(m.from)}</span><div class="grow"><b>${esc(m.from)}</b><span class="s" data-u="10">${esc(m.addr)} · an ${esc(a.email)}</span></div><span class="s" data-u="23">${m.time}</span></div>
      <div class="banner" data-u="24">${icon('image')}<span>Bilder aus dem Internet blockiert (Schutz vor Tracking).</span><button data-act="toast" data-arg="Bilder geladen">Laden</button></div>
      <div class="body">${esc(m.snip)}\n\nViele Grüße\n${esc(m.from.split(' ')[0])}</div>
      ${m.attach ? `<div class="attach"><span class="avatar sm a7">${icon('filetext')}</span><div class="grow"><b>${esc(m.attach)}</b><span class="s" data-u="13">PDF · 118 KB</span></div><button class="icon-btn" data-act="toast" data-arg="In GhulDrive gespeichert">${icon('cloud')}</button><button class="icon-btn" data-act="toast" data-arg="Gespeichert">${icon('download')}</button></div>` : ''}
    </div>
  </div>
  <div class="actionbar"><button class="btn tonal" data-act="compose" data-arg="re">${icon('reply')} Antworten</button><button class="btn outline" data-act="compose" data-arg="all">Allen</button><button class="btn outline" data-act="compose" data-arg="fw">${icon('forward')}</button></div>`;
}
function composeScreen(kind) {
  const m = kind && kind !== 'new' ? MAILS[0] : null;
  const subj = m ? (kind === 'fw' ? 'WG: ' : 'AW: ') + m.subj : '';
  return `<div class="topbar"><button class="icon-btn" data-act="back" aria-label="Schließen">${icon('x')}</button><h1 class="small">${m ? (kind === 'fw' ? 'Weiterleiten' : 'Antworten') : 'Neue Mail'}</h1>
    <button class="icon-btn" data-act="toast" data-arg="Anhang: Gerät, GhulDrive oder Foto">${icon('clip')}</button>
    <button class="btn primary" data-u="25" data-act="send">${icon('send')} Senden</button></div>
  <div class="screen compose" data-u="20">
    <div class="line"><label>Von</label><span class="rcpt"><span class="avatar a3">P</span>max@posteo.de</span>${icon('down')}</div>
    <div class="line"><label>An</label>${m && kind !== 'fw' ? `<span class="rcpt"><span class="avatar ${av(m.from)}">${letter(m.from)}</span>${esc(m.from)}</span>` : '<input placeholder="Name oder Adresse">'}</div>
    <div class="line"><label>Betreff</label><input value="${esc(subj)}"></div>
    <textarea placeholder="Nachricht">${m ? '\n\n-- \nMax\n\n> ' + esc(m.snip) : '\n\n-- \nMax'}</textarea>
    <div class="toolbar"><button class="icon-btn">${icon('bold')}</button><button class="icon-btn">${icon('italic')}</button><button class="icon-btn">${icon('ul')}</button><button class="icon-btn">${icon('link')}</button>
      <span data-u="26"></span><button class="icon-btn" data-act="toast" data-arg="Später senden: morgen 8:00">${icon('clock')}</button><button class="icon-btn" data-act="toast" data-arg="Verschlüsselt mit OpenPGP (später)">${icon('pgp')}</button></div>
    <p class="hint-text">Entwurf wird automatisch verschlüsselt gesichert ${NEW}. Adressen werden aus deinen Mails vorgeschlagen ${NEW}.</p>
  </div>`;
}

// --- settings ------------------------------------------------------------------------------------
const toggle = (key, label, sub) => `<div class="row plain"><span class="grow"><span class="t">${label}</span>${sub ? `<span class="s" data-u="27">${sub}</span>` : ''}</span>
  <button class="toggle ${S.toggles[key] ? 'on' : ''}" data-act="toggle" data-arg="${key}" aria-label="${esc(label)}"></button></div>`;
const choice = (label, value, opts) => `<button class="row plain" data-act="toast" data-arg="${esc(label)}: ${esc(opts)}"><span class="grow">${label}</span><span class="end">${value} ${icon('chev', 'chev')}</span></button>`;
const link = (label, sub, act = 'toast', arg) => `<button class="row plain" data-act="${act}" data-arg="${esc(arg || label)}"><span class="grow"><span class="t">${label}</span>${sub ? `<span class="s">${sub}</span>` : ''}</span>${icon('chev', 'chev')}</button>`;

const SETTINGS = [
  ['Sicherheit', [
    { id: 'unlock', icon: 'fingerprint', cls: 'i-violet', title: 'Entsperren', sub: 'Fingerabdruck an · Key oder Schlüsseldatei',
      page: () => `<div class="section-title">Schnell entsperren</div><div class="card flush">${toggle('bio', 'Fingerabdruck', 'Nur nach vollständigem Entsperren, gilt höchstens 24 Std.')}${toggle('pin', 'App-PIN')}${choice('Gültig für', '24 Stunden', '4 / 12 / 24 Std. / 7 Tage')}</div>
        <p class="hint-text">Nach einem Neustart, einer Panik-Sperre oder einem Passwortwechsel brauchst du wieder Passwort und Key.</p>` },
    { id: 'lock', icon: 'lock', cls: 'i-red', title: 'Sperren und Panik', sub: 'Nach 1 Min. · Schütteln sperrt sofort',
      page: () => `<div class="card flush">${choice('Automatisch sperren', 'nach 1 Minute', 'sofort, 30 s, 1, 5, 15 Min.')}${toggle('screenOff', 'Beim Ausschalten des Bildschirms')}${toggle('shake', 'Schütteln = Panik-Sperre', 'Sperrt sofort und löscht Fingerabdruck-/PIN-Zugang und Mail-Abrufdaten')}${choice('Empfindlichkeit', 'Mittel', 'Niedrig, Mittel, Hoch')}</div>
        <div class="card flush">${link('Schnellkachel einrichten', 'In den Android-Schnelleinstellungen')}</div>` },
    { id: 'privacy', icon: 'eyeoff', cls: 'i-gray', title: 'Privatsphäre', sub: 'Zwischenablage, Bildschirmfotos',
      page: () => `<div class="card flush">${choice('Zwischenablage leeren', 'nach 45 s', 'nie, 15, 30, 45, 90 s')}${toggle('shots', 'Bildschirmfotos erlauben', 'Aus: keine Screenshots, keine Vorschau im App-Wechsler')}</div>` },
  ]],
  ['Schutz der Suite', [
    { id: 'factors', icon: 'usbkey', cls: 'i-violet', title: 'Keys und Schlüsseldateien', sub: '2 Security Keys · 1 Schlüsseldatei',
      page: () => `<div class="card flush">
        <div class="row"><span class="avatar a0">${icon('usbkey')}</span><span class="grow"><span class="t">YubiKey blau</span><span class="s">Security Key · seit 10.11.2026</span></span>${icon('more')}</div>
        <div class="row"><span class="avatar a0">${icon('usbkey')}</span><span class="grow"><span class="t">Ersatz-Key (Schublade)</span><span class="s">Security Key · seit 10.11.2026</span></span>${icon('more')}</div>
        <div class="row"><span class="avatar a9">${icon('file')}</span><span class="grow"><span class="t">Schlüsseldatei USB-Stick</span><span class="s">ghulsuite.ghlkey</span></span>${icon('more')}</div></div>
        <div class="btn-row"><button class="btn tonal" data-act="toast" data-arg="Key einstecken …">${icon('plus')} Key</button><button class="btn outline" data-act="toast" data-arg="Schlüsseldatei erzeugen">${icon('plus')} Schlüsseldatei</button></div>
        <p class="hint-text">Hinzufügen oder Entfernen macht nichts anderes ungültig. Der letzte Key bzw. die letzte Schlüsseldatei lässt sich nicht entfernen.</p>` },
    { id: 'password', icon: 'key', cls: 'i-blue', title: 'Master-Passwort', sub: 'Zuletzt geändert vor 3 Tagen',
      page: () => `<div class="card flush">${link('Master-Passwort ändern', 'Gilt sofort für alle Tresore, Synchronisation übernimmt es')}</div>` },
    { id: 'recovery', icon: 'help', cls: 'i-orange', title: 'Wiederherstellungscode', sub: 'Notfallblatt gedruckt',
      page: () => `<div class="card"><h3>Dein Notfall-Zugang</h3><p>Der Code öffnet allein alles – auch ohne Passwort und Key. Bewahre das Notfallblatt sicher auf.</p></div>
        <div class="card flush">${link('Notfallblatt erneut drucken')}${link('Neuen Code erzeugen', 'Der alte Code wird ungültig')}</div>` },
  ]],
  ['Daten', [
    { id: 'vaults', icon: 'layers', cls: 'i-teal', title: 'Tresore', sub: '2 Passwort-Tresore · 1 Authenticator',
      page: () => `<div class="card flush">${VAULTS.map((v) => `<div class="row"><span class="avatar ${av(v.name)}">${letter(v.name)}</span><span class="grow"><span class="t">${v.name}</span><span class="s">GhulPass · ${ENTRIES.filter((e) => e.vault === v.id).length} Einträge</span></span>${icon('more')}</div>`).join('')}
        <div class="row"><span class="avatar a2">${icon('shieldcheck')}</span><span class="grow"><span class="t">Persönlich</span><span class="s">GhulAuth · ${TOTPS.length} Konten</span></span>${icon('more')}</div></div>
        <div class="btn-row"><button class="btn tonal" data-act="toast" data-arg="Neuer Tresor">${icon('plus')} Neuer Tresor</button></div>
        <div class="card flush" data-u="24">${link('Speicherort', 'Ordner „GhulSuite“ auf dem Gerät')}${link('Ordner nach Tresoren durchsuchen')}</div>` },
    { id: 'import', icon: 'download', cls: 'i-cyan', title: 'Import, Export und Sicherung', sub: 'Umzug aus anderen Apps, Backups, alle Bereiche',
      page: () => `<div class="section-title">Komplette Suite</div><div class="card flush">
          ${link('Verschlüsselte Sicherung erstellen ' + NEW, 'Eine Datei mit allem: Tresore, Einstellungen, Mail-Konten, Drive-Verzeichnis')}
          ${link('Automatische Sicherung ' + NEW, 'Wöchentlich in einen Ordner oder auf einen USB-Stick')}
          ${link('Sicherung wiederherstellen ' + NEW, 'Mit Passwort + Key oder Wiederherstellungscode')}</div>
        <div class="section-title">GhulPass</div><div class="card flush">
          ${link('Importieren', 'Proton Pass, Bitwarden, KeePass(XC)-CSV, andere CSV')}
          ${link('Mehr Formate ' + NEW, 'KeePass .kdbx, 1Password .1pux, LastPass, Dashlane, Enpass, NordPass, Chrome, Firefox, Edge, Safari')}
          ${link('Exportieren', 'Tresor verschlüsselt · CSV (nur mit Master-Passwort)')}
          ${link('Mehr Exportformate ' + NEW, 'KeePass .kdbx, Bitwarden-JSON, nur ausgewählte Einträge')}</div>
        <div class="section-title">GhulAuth</div><div class="card flush">
          ${link('Importieren', 'Proton Authenticator, Aegis, 2FAS, Google Authenticator (QR)')}
          ${link('Mehr Formate ' + NEW, 'Ente Auth, Raivo, andOTP, FreeOTP+, QR-Code aus Bild')}
          ${link('Exportieren', 'otpauth-Links (nur mit Master-Passwort)')}
          ${link('Mehr Exportformate ' + NEW, 'QR-Codes einzeln oder für Google Authenticator, Aegis-Datei, Notfall-PDF zum Ausdrucken')}</div>
        <div class="section-title">GhulDrive</div><div class="card flush">
          ${link('Ordner vom Gerät hochladen ' + NEW, 'Ganze Ordner samt Unterordnern')}
          ${link('„In GhulDrive speichern“ ' + NEW, 'Aus jeder App über das Teilen-Menü')}
          ${link('Umzug aus anderer Cloud ' + NEW, 'Google Takeout, Proton-Drive-Export (ZIP), Nextcloud/WebDAV')}
          ${link('Ordner exportieren ' + NEW, 'Als ZIP, auf USB-Stick, oder mit einem Geräteordner synchron halten')}</div>
        <div class="section-title">GhulMail</div><div class="card flush">
          ${link('Mails importieren ' + NEW, 'EML- und MBOX-Dateien, z. B. Proton-Mail-Export')}
          ${link('Mails exportieren ' + NEW, 'Einzeln als EML oder PDF, ganze Ordner als MBOX')}
          ${link('Adressen exportieren ' + NEW, 'Bekannte Kontakte als vCard')}</div>
        <p class="hint-text">Unverschlüsselte Exporte verlangen immer das Master-Passwort und zeigen eine Warnung.</p>` },
  ]],
  ['Server und Synchronisation', [
    { id: 'servers', icon: 'server', cls: 'i-blue', title: 'Eigene Server', sub: 'Heimserver · /home/max/ghulsuite',
      page: () => `<div class="card flush"><div class="row"><span class="avatar a1">${icon('server')}</span><span class="grow"><span class="t">Heimserver</span><span class="s">max@nas.example.org:22 · SSH-Schlüssel</span></span>${icon('more')}</div></div>
        <div class="btn-row"><button class="btn tonal" data-act="toast" data-arg="Server hinzufügen">${icon('plus')} Server hinzufügen</button></div>
        <div class="section-title">Teilen ${NEW}</div>
        ${S.shareReady ? `<div class="card flush"><div class="row"><span class="avatar a2">${icon('link')}</span><span class="grow"><span class="t">Teilen ist eingerichtet</span><span class="s">~/${S.shareDir} · https://max.example.org/teilen</span></span><span class="tag ok">${icon('check')} geprüft</span></div>
          <button class="row plain" data-act="share-setup"><span class="grow">Erneut prüfen oder ändern</span>${icon('chev', 'chev')}</button></div>`
        : `<button class="card row plain" data-act="share-setup"><span class="avatar a5">${icon('link')}</span><span class="grow"><span class="t">Teilen einrichten</span><span class="s">Ordner wählen, den dein Webserver ausliefert – dauert eine Minute</span></span>${icon('chev', 'chev')}</button>`}
        <p class="hint-text">Teilen per Link geht mit SFTP (eigener Server mit Webserver), FTPS (Webhosting) und später dem GhulSuite-Server-Modul.</p>` },
    { id: 'sync', icon: 'sync', cls: 'i-teal', title: 'Synchronisation', sub: 'Automatisch · vor 2 Min.',
      page: () => `<div class="card flush">${choice('Server', 'Heimserver', 'Aus, Heimserver')}${toggle('autosync', 'Automatisch', 'Nach dem Entsperren und bei jeder Änderung')}</div>
        <div class="card"><h3>Zuletzt synchronisiert: vor 2 Min.</h3><p>3 gesendet · 1 empfangen · keine Konflikte</p></div>
        <button class="btn primary block" data-act="toast" data-arg="Synchronisiert">${icon('sync')} Jetzt synchronisieren</button>` },
  ]],
  ['Bereiche', [
    { id: 'm-pass', icon: 'key', cls: 'i-violet', title: 'GhulPass', sub: 'Autofill an',
      page: () => `<div class="card flush">${toggle('autofill', 'Autofill', 'Anmeldungen in Apps und Browsern ausfüllen')}${toggle('subtitles', 'Benutzernamen in der Liste zeigen')}${choice('Generator-Standard', '20 Zeichen', 'Zeichen oder Passphrase')}${toggle('pwned', 'Datenlecks prüfen ' + NEW, 'k-Anonymität, standardmäßig aus')}</div>` },
    { id: 'm-auth', icon: 'shieldcheck', cls: 'i-teal', title: 'GhulAuth', sub: 'Nächsten Code zeigen',
      page: () => `<div class="card flush">${toggle('hideCodes', 'Codes erst nach Antippen zeigen')}${toggle('nextCode', 'Nächsten Code vorab zeigen')}</div>` },
    { id: 'm-drive', icon: 'cloud', cls: 'i-blue', title: 'GhulDrive', sub: 'Foto-Sicherung an · nur WLAN',
      page: () => `<div class="card flush">${choice('Speicher', 'Heimserver', 'Server wählen')}${toggle('photoBackup', 'Foto-Sicherung ' + NEW)}${toggle('wifiOnly', 'Nur im WLAN übertragen')}${choice('Papierkorb leeren', 'nach 30 Tagen', '7, 30, 90 Tage, nie')}</div>` },
    { id: 'm-mail', icon: 'mail', cls: 'i-orange', title: 'GhulMail', sub: '2 Konten · Benachrichtigungen an',
      page: () => `<div class="section-title">Konten</div><div class="card flush">${ACCOUNTS.map((a) => `<button class="row" data-act="toast" data-arg="Konto bearbeiten"><span class="avatar ${a.cls}">${letter(a.name)}</span><span class="grow"><span class="t">${a.name}</span><span class="s">${a.email}</span></span>${icon('chev', 'chev')}</button>`).join('')}</div>
        <div class="btn-row"><button class="btn tonal" data-act="toast" data-arg="Konto hinzufügen">${icon('plus')} Konto hinzufügen</button></div>
        <div class="section-title">Verhalten</div><div class="card flush">${toggle('notifyMail', 'Benachrichtigungen', 'Auch wenn GhulSuite gesperrt ist')}${toggle('notifyPreview', 'Absender in Benachrichtigung', 'Standard: nur „Neue E-Mail“')}${toggle('remoteImages', 'Bilder aus dem Internet laden', 'Aus: Schutz vor Tracking-Pixeln')}${toggle('offlineMail', 'Offline lesen ' + NEW, 'Mails verschlüsselt auf dem Gerät zwischenspeichern')}${choice('Wischen nach links', 'Archivieren', 'Archivieren, Löschen, Gelesen')}${choice('Signatur', 'Max', 'je Konto')}</div>` },
  ]],
  ['Allgemein', [
    { id: 'look', icon: 'palette', cls: 'i-pink', title: 'Darstellung', sub: 'System · Deutsch',
      page: () => `<div class="card"><h3 data-u="28">Design</h3><div class="seg"><button class="${S.theme === 'light' ? 'on' : ''}" data-act="set-theme" data-arg="light">Hell</button><button class="${S.theme === 'dark' ? 'on' : ''}" data-act="set-theme" data-arg="dark">Dunkel</button><button data-act="toast" data-arg="Folgt dem System">System</button></div></div>
        <div class="card flush">${choice('Sprache', 'Deutsch', 'Deutsch, English')}${choice('Beim Start öffnen', 'GhulPass', 'zuletzt genutzt, GhulPass, GhulAuth …')}${choice('Bereiche in der Leiste', 'alle 4', 'ein- und ausblenden')}</div>` },
    { id: 'about', icon: 'info', cls: 'i-gray', title: 'Über und Hilfe', sub: 'Version 0.5.0 · Fehler melden',
      page: () => `<div class="card flush">${link('Fehlerbericht senden', 'Mit Gerät und Version, ohne Tresor-Inhalte')}${link('Testplan')}${link('Quelltext und Lizenz (AGPL-3.0)')}${link('Verwendete Bibliotheken')}</div><p class="hint-text">GhulSuite 0.5.0-dev · ghul-core 0.5.0</p>` },
  ]],
];
function settingsScreen() {
  return `${topbar('Optionen', { ctx: { label: 'Meine Suite · Version 0.5', c: 'c-set' } })}<div class="screen">
    <div class="card profile"><span class="avatar lg a0">${icon('shield')}</span><div class="grow"><h3>Meine Suite</h3><p>3 Tresore · synchronisiert vor 2 Min.</p>
      <span class="tag ok">${icon('shieldcheck')} Geschützt mit Passwort + Key</span></div></div>
    <div class="card"><div class="row plain" data-u="7"><span class="grow"><b>Jetzt sperren</b><span class="s">Oder Handy schütteln</span></span><button class="btn tonal" data-act="lock">${icon('lock')} Sperren</button></div></div>
    ${SETTINGS.map(([group, items]) => `<div class="section-title">${group}</div><div class="card flush">${items.map((it) => `<button class="row set-row" data-act="set-page" data-arg="${it.id}">
      <span class="ico ${it.cls}">${icon(it.icon)}</span><span class="grow"><span class="t">${it.title}</span><span class="s">${it.sub}</span></span>${icon('chev', 'chev')}</button>`).join('')}</div>`).join('')}
  </div>`;
}
function settingsPage(id) {
  const it = SETTINGS.flatMap(([, items]) => items).find((x) => x.id === id);
  return `${topbar(it.title, { back: true })}<div class="screen">${it.page()}</div>`;
}

// --- sheets --------------------------------------------------------------------------------------
function genPassword() {
  const g = S.setGen;
  if (g.mode === 'words') {
    const w = ['Laterne', 'Kompass', 'Wiese', 'Kaffee', 'Garten', 'Möwe', 'Sternbild', 'Fahrrad', 'Gewitter', 'Teekanne', 'Hafen', 'Leuchtturm'];
    return Array.from({ length: 4 }, () => w[Math.floor(Math.random() * w.length)]).join('-') + '-' + Math.floor(Math.random() * 90 + 10);
  }
  let set = '';
  if (g.lower) set += 'abcdefghijkmnopqrstuvwxyz'; if (g.upper) set += 'ABCDEFGHJKLMNPQRSTUVWXYZ';
  if (g.digits) set += '23456789'; if (g.symbols) set += '!#$%&*+-=?@_~';
  const a = new Uint32Array(g.len); crypto.getRandomValues(a);
  return [...a].map((n) => set[n % set.length]).join('');
}
function generatorSheet() {
  const g = S.setGen, pw = genPassword(), st = strength(pw);
  const opt = (k, l) => `<div class="row plain" data-u="29"><span class="grow">${l}</span><button class="toggle ${g[k] ? 'on' : ''}" data-act="gen-opt" data-arg="${k}"></button></div>`;
  return `<h2>Passwort-Generator</h2>
    <div class="seg" data-u="30"><button class="${g.mode === 'chars' ? 'on' : ''}" data-act="gen-mode" data-arg="chars">Zeichen</button><button class="${g.mode === 'words' ? 'on' : ''}" data-act="gen-mode" data-arg="words">Passphrase ${NEW}</button></div>
    <div class="field" data-u="31"><div class="grow"><div class="pw" data-u="32">${pwHtml(pw)}</div>
      <div class="strength"><i class="js-w ${st < 2 ? 's-weak' : st < 3 ? 's-ok' : 's-good'}" data-w="${(st + 1) * 20}"></i></div><div class="hint-text" data-u="2">${STRENGTH[st]} · Ziffern blau, Sonderzeichen rot</div></div>
      <button class="icon-btn" data-act="sheet-gen" aria-label="Neu">${icon('dice')}</button></div>
    ${g.mode === 'chars' ? `<div class="slider"><span>Länge</span><input type="range" min="8" max="64" value="${g.len}" id="gen-len"><b>${g.len}</b></div>
    ${opt('upper', 'Großbuchstaben (A–Z)')}${opt('lower', 'Kleinbuchstaben (a–z)')}${opt('digits', 'Ziffern (0–9)')}${opt('symbols', 'Sonderzeichen (!#$…)')}` : '<p class="hint-text">Vier zufällige Wörter plus Zahl – leicht zu tippen, schwer zu erraten.</p>'}
    <div class="btn-row" data-u="33"><button class="btn outline" data-act="copy" data-arg="Passwort">${icon('copy')} Kopieren</button><button class="btn primary" data-act="close-sheet">Übernehmen</button></div>`;
}
const SHEETS = {
  newitem: () => `<h2>Neuer Eintrag</h2><div class="card flush" data-u="31">${Object.entries(TYPES).map(([k, t]) => `<button class="row" data-act="toast" data-arg="Neu: ${t.label}"><span class="avatar ${av(k)}">${icon(t.icon)}</span><span class="grow"><span class="t">${t.label}</span></span>${icon('chev', 'chev')}</button>`).join('')}</div>
    <button class="btn tonal block" data-u="24" data-act="sheet-gen">${icon('wand')} Nur Passwort erzeugen</button>`,
  vaults: () => `<h2>Tresor wählen</h2><div class="card flush" data-u="31"><button class="row" data-act="vault" data-arg=""><span class="avatar a0">${icon('layers')}</span><span class="grow"><span class="t">Alle Tresore</span></span>${!S.vault ? icon('check') : ''}</button>
    ${VAULTS.map((v) => `<button class="row" data-act="vault" data-arg="${v.id}"><span class="avatar ${av(v.name)}">${letter(v.name)}</span><span class="grow"><span class="t">${v.name}</span><span class="s">${ENTRIES.filter((e) => e.vault === v.id).length} Einträge</span></span>${S.vault === v.id ? icon('check') : ''}</button>`).join('')}</div>`,
  authvaults: () => `<h2>GhulAuth-Tresor</h2><div class="card flush sheet-list"><button class="row" data-act="close-sheet"><span class="avatar a2">${icon('shieldcheck')}</span><span class="grow"><span class="t">Persönlich</span><span class="s">${TOTPS.length} Konten</span></span>${icon('check')}</button>
    <button class="row" data-act="toast" data-arg="Neuer Tresor"><span class="avatar a5">${icon('plus')}</span><span class="grow"><span class="t">Neuer Tresor</span></span></button></div>`,
  servers: () => `<h2>Speicher</h2><div class="card flush sheet-list"><button class="row" data-act="close-sheet"><span class="avatar a1">${icon('server')}</span><span class="grow"><span class="t">Heimserver</span><span class="s">/home/max/ghulsuite/drive</span></span>${icon('check')}</button>
    <button class="row" data-act="toast" data-arg="Server hinzufügen"><span class="avatar a5">${icon('plus')}</span><span class="grow"><span class="t">Server hinzufügen</span><span class="s">SFTP · bald auch WebDAV</span></span></button></div>`,
  addtotp: () => `<h2>2FA-Konto hinzufügen</h2><div class="card flush" data-u="31">
    <button class="row" data-act="toast" data-arg="Kamera öffnet sich"><span class="avatar a2">${icon('qr')}</span><span class="grow"><span class="t">QR-Code scannen</span><span class="s">Auch Google-Authenticator-Export</span></span></button>
    <button class="row" data-act="toast" data-arg="QR-Code im Bild suchen"><span class="avatar a1">${icon('image')}</span><span class="grow"><span class="t">QR-Code aus Bild ${NEW}</span><span class="s">Screenshot oder Foto</span></span></button>
    <button class="row" data-act="toast" data-arg="Manuell eingeben"><span class="avatar a5">${icon('edit')}</span><span class="grow"><span class="t">Schlüssel eingeben</span></span></button>
    <button class="row" data-act="toast" data-arg="Import"><span class="avatar a3">${icon('download')}</span><span class="grow"><span class="t">Importieren</span><span class="s">Aegis, 2FAS, Proton Authenticator</span></span></button></div>`,
  upload: () => `<h2>Zu GhulDrive hinzufügen</h2><div class="card flush" data-u="31">
    <button class="row" data-act="toast" data-arg="Dateiauswahl"><span class="avatar a1">${icon('upload')}</span><span class="grow"><span class="t">Dateien hochladen</span></span></button>
    <button class="row" data-act="toast" data-arg="Fotos auswählen"><span class="avatar a4">${icon('image')}</span><span class="grow"><span class="t">Fotos und Videos</span></span></button>
    <button class="row" data-act="toast" data-arg="Dokument scannen"><span class="avatar a2">${icon('camera')}</span><span class="grow"><span class="t">Dokument scannen ${NEW}</span><span class="s">Als PDF, direkt verschlüsselt</span></span></button>
    <button class="row" data-act="toast" data-arg="Neuer Ordner"><span class="avatar a9">${icon('folder')}</span><span class="grow"><span class="t">Neuer Ordner</span></span></button></div>`,
  entrymenu: () => `<h2>Eintrag</h2><div class="card flush" data-u="31">
    <button class="row plain" data-act="sheet-entryshare"><span class="grow">Sicher teilen … ${NEW}</span></button>
    <button class="row plain" data-act="toast" data-arg="Exportiert als CSV / KeePass / verschlüsselt"><span class="grow">Exportieren ${NEW}</span></button>
    <button class="row plain" data-act="toast" data-arg="In anderen Tresor verschieben"><span class="grow">In anderen Tresor verschieben</span></button>
    <button class="row plain" data-act="toast" data-arg="Kopie angelegt"><span class="grow">Duplizieren</span></button>
    <button class="row plain" data-act="toast" data-arg="An Startbildschirm angeheftet"><span class="grow">Ans Autofill-Menü anheften ${NEW}</span></button>
    <button class="row plain" data-act="toast" data-arg="Gelöscht (rückgängig möglich)"><span class="grow" data-u="34">Löschen</span></button></div>`,
};
function entryShareSheet() {
  return `<h2>Eintrag sicher teilen ${NEW}</h2><div class="card flush sheet-list">
    <button class="row" data-act="sheet-link"><span class="avatar a5">${icon('link')}</span><span class="grow"><span class="t">Einmal-Link</span><span class="s">Verschlüsselt, löscht sich nach dem ersten Öffnen (Server-Modul) oder nach Ablauf</span></span>${icon('chev', 'chev')}</button>
    <button class="row" data-act="toast" data-arg="QR-Code angezeigt"><span class="avatar a2">${icon('qr')}</span><span class="grow"><span class="t">QR-Code (WLAN, Zugangsdaten)</span><span class="s">Für Gäste-WLAN oder ein Gerät daneben – nichts geht übers Netz</span></span></button>
    <button class="row" data-act="toast" data-arg="In geteilten Tresor kopiert"><span class="avatar a1">${icon('layers')}</span><span class="grow"><span class="t">In einen geteilten Tresor</span><span class="s">Später: Tresor mit Familie teilen (eigener Schlüssel je Person)</span></span></button></div>`;
}
function mailMenuSheet() {
  return `<h2>Nachricht</h2><div class="card flush sheet-list">
    <button class="row plain" data-act="toast" data-arg="Als ungelesen markiert"><span class="grow">Als ungelesen markieren</span></button>
    <button class="row plain" data-act="toast" data-arg="Verschieben"><span class="grow">Verschieben</span></button>
    <button class="row plain" data-act="toast" data-arg="Gespeichert als .eml"><span class="grow">Als Datei speichern (.eml) ${NEW}</span></button>
    <button class="row plain" data-act="toast" data-arg="Als PDF gespeichert"><span class="grow">Als PDF speichern ${NEW}</span></button>
    <button class="row plain" data-act="toast" data-arg="Mail und Anhänge in GhulDrive gespeichert"><span class="grow">In GhulDrive ablegen ${NEW}</span></button>
    <button class="row plain" data-act="toast" data-arg="Original angezeigt"><span class="grow">Original anzeigen (Kopfzeilen)</span></button></div>`;
}
function fileSheet(id) {
  const f = [...DRIVE.root, ...DRIVE['d-vertr']].find((x) => x.id === id);
  const [ic, cls] = KIND[f.kind];
  return `<div class="preview avatar ${cls}" data-u="35">${icon(ic)}</div><h2>${esc(f.name)}</h2>
    <p class="hint-text" data-u="36">${f.size} · geändert ${f.date} · verschlüsselt in 4-MiB-Blöcken</p>
    <div class="btn-row"><button class="btn primary" data-act="toast" data-arg="Geöffnet">${icon('eye')} Öffnen</button><button class="btn outline" data-act="toast" data-arg="Gespeichert">${icon('download')} Speichern</button></div>
    <div class="card flush" data-u="37">
      <button class="row plain" data-act="toast" data-arg="Offline verfügbar"><span class="grow">Offline verfügbar machen ${NEW}</span>${icon('download')}</button>
      <button class="row plain" data-act="sheet-share"><span class="grow">Teilen … ${NEW}</span>${icon('send')}</button>
      <button class="row plain" data-act="toast" data-arg="Versionen: 3"><span class="grow">Frühere Versionen ${NEW}</span>${icon('history')}</button>
      <button class="row plain" data-act="toast" data-arg="Umbenennen"><span class="grow">Umbenennen</span>${icon('edit')}</button>
      <button class="row plain" data-act="toast" data-arg="Verschieben"><span class="grow">Verschieben</span>${icon('folder')}</button>
      <button class="row plain" data-act="toast" data-arg="In den Papierkorb"><span class="grow" data-u="34">In den Papierkorb</span>${icon('trash')}</button></div>`;
}

function selBar() {
  const n = Object.values(S.sel).filter(Boolean).length;
  const b = (ic, l, act, arg) => `<button data-act="${act}" data-arg="${arg || ''}" ${n ? '' : 'disabled'}><span class="ind">${icon(ic)}</span>${l}</button>`;
  return `<nav class="nav selbar"><div class="selcount">${n} ausgewählt ${NEW}</div>${b('send', 'Teilen', 'sheet-share')}${b('download', 'Als ZIP', 'toast', n + ' Dateien als ZIP gespeichert')}${b('folder', 'Verschieben', 'toast', 'Ziel wählen')}${b('box', 'Offline', 'toast', 'Offline verfügbar')}${b('trash', 'Löschen', 'toast', 'In den Papierkorb')}</nav>`;
}

// --- sharing ---------------------------------------------------------------------------------------
function selNames() {
  const all = [...DRIVE.root, ...DRIVE['d-vertr']];
  const picked = all.filter((f) => S.sel[f.id]);
  return picked.length ? picked : [all.find((f) => f.id === S.shareId) || all[3]];
}
function shareSheet() {
  const files = selNames();
  const what = files.length === 1 ? esc(files[0].name) : `${files.length} Dateien`;
  const opt = (ic, cls, t, sub, act, tag) => `<button class="row" data-act="${act}"><span class="avatar ${cls}">${icon(ic)}</span><span class="grow"><span class="t">${t}</span><span class="s">${sub}</span></span>${tag || icon('chev', 'chev')}</button>`;
  return `<h2>Teilen ${NEW}</h2><p class="hint-text">${what}</p><div class="card flush sheet-list">
    ${opt('phone', 'a2', 'Mit anderer App teilen', 'Entschlüsselt an Messenger, Mail, Galerie … – geht mit jeder Verbindung', 'toast', '<span class="tag ok">immer</span>')}
    ${opt('link', 'a5', 'Freigabelink erstellen', 'Ende-zu-Ende verschlüsselt, Empfänger braucht keine App', 'sheet-link')}
    ${opt('archive', 'a9', 'Als verschlüsseltes ZIP', 'AES-256 mit Passwort – für E-Mail oder USB-Stick', 'toast')}
    ${opt('mail', 'a3', 'Mit GhulMail senden', files.length > 1 ? 'Als Anhänge oder als Link' : 'Als Anhang oder als Link', 'compose')}
    ${opt('qr', 'a1', 'Als QR-Code zeigen', 'Link zum Abscannen am anderen Gerät', 'toast')}
  </div>`;
}
function linkSheet() {
  const days = [[1, '1 Tag'], [7, '7 Tage'], [30, '30 Tage'], [0, 'Unbegrenzt']];
  return `<h2>Freigabelink ${NEW}</h2>
    ${S.shareReady ? `<div class="banner">${icon('server')}<span><b>Heimserver</b> · max.example.org/teilen</span><span class="tag ok">bereit</span></div>`
      : `<div class="banner">${icon('alert')}<span>Für Links braucht GhulSuite einmal einen Teilen-Ordner auf deinem Server.</span><button data-act="share-setup">Einrichten</button></div>`}
    <div class="section-title">Gültig</div>
    <div class="chips">${days.map(([d, l]) => `<button class="chip ${S.linkDays === d ? 'on' : ''}" data-act="link-days" data-arg="${d}">${l}</button>`).join('')}</div>
    <div class="card flush sheet-list">
      <div class="row plain"><span class="grow"><span class="t">Zusätzlich mit Passwort</span><span class="s">Link allein reicht dann nicht – Passwort getrennt mitteilen</span></span><button class="toggle ${S.linkPw ? 'on' : ''}" data-act="link-pw"></button></div>
      <div class="row plain disabled"><span class="grow"><span class="t">Höchstens 3 Downloads</span><span class="s">Nur mit dem GhulSuite-Server-Modul (PHP) – ein reiner Datei-Server kann nicht mitzählen</span></span><button class="toggle" disabled></button></div>
    </div>
    <div class="field"><div class="grow"><label>Link</label><div class="value pw">https://max.example.org/teilen/#k3F9xQ.T7v2…Lm9</div></div><button class="icon-btn" data-act="copy" data-arg="Link">${icon('copy')}</button></div>
    <p class="hint-text">Der Teil nach „#“ ist der Schlüssel. Browser schicken ihn nie an den Server – dort liegt nur Chiffretext. Die Empfängerseite entschlüsselt im Browser.</p>
    <div class="btn-row"><button class="btn outline" data-act="toast" data-arg="QR-Code angezeigt">${icon('qr')} QR-Code</button><button class="btn primary" data-act="copy" data-arg="Link">${icon('link')} Link kopieren</button></div>`;
}
function shareSetupSheet() {
  const st = S.shareStep;
  const steps = `<div class="steps">${[1, 2, 3].map((n) => `<i class="${n <= st ? 'on' : ''}"></i>`).join('')}</div>`;
  if (st === 1) {
    const dirs = [['public_html', 'Typischer Web-Ordner'], ['public_html/teilen', 'Neu anlegen (empfohlen)'], ['www', ''], ['ghulsuite', 'Deine Tresore – nicht zum Teilen geeignet']];
    return `<h2>Teilen einrichten</h2>${steps}<p class="hint-text">Schritt 1 von 3: Welchen Ordner auf <b>Heimserver</b> liefert dein Webserver aus? GhulSuite legt dort nur verschlüsselte Kopien ab.</p>
      <div class="card flush sheet-list"><div class="row plain"><span class="grow"><span class="s">/home/max</span></span></div>
      ${dirs.map(([d, h]) => `<button class="row ${S.shareDir === d ? 'selected' : ''} ${d === 'ghulsuite' ? 'disabled' : ''}" data-act="${d === 'ghulsuite' ? 'toast' : 'share-dir'}" data-arg="${d === 'ghulsuite' ? 'Dieser Ordner ist für deine Tresore reserviert' : d}"><span class="avatar a1">${icon('folder')}</span><span class="grow"><span class="t">${d}</span><span class="s">${h}</span></span>${S.shareDir === d ? icon('check') : ''}</button>`).join('')}</div>
      <button class="btn primary block" data-act="share-step" data-arg="2">Weiter</button>`;
  }
  if (st === 2) {
    return `<h2>Teilen einrichten</h2>${steps}<p class="hint-text">Schritt 2 von 3: Unter welcher Adresse ist dieser Ordner im Internet erreichbar?</p>
      <div class="field"><div class="grow"><label>Öffentliche Adresse (nur https)</label><div class="value">https://max.example.org/teilen</div></div>${icon('edit')}</div>
      <p class="hint-text">Vorschlag aus dem Servernamen. Nur ändern, wenn deine Website eine andere Adresse hat.</p>
      <div class="btn-row"><button class="btn outline" data-act="share-step" data-arg="1">Zurück</button><button class="btn primary" data-act="share-step" data-arg="3">Prüfen</button></div>`;
  }
  const check = (t, s) => `<div class="row plain"><span class="avatar sm a2">${icon('check')}</span><span class="grow"><span class="t">${t}</span><span class="s">${s}</span></span></div>`;
  return `<h2>Teilen einrichten</h2>${steps}<p class="hint-text">Schritt 3 von 3: GhulSuite hat alles selbst geprüft.</p>
    <div class="card flush sheet-list">
      ${check('Ordner beschreibbar', 'Testdatei per SFTP angelegt und wieder gelöscht')}
      ${check('Im Internet erreichbar', 'Dieselbe Testdatei über https abgerufen – Inhalt stimmt')}
      ${check('Sichere Verbindung', 'Gültiges Zertifikat, http wird nicht verwendet')}
      ${check('Kein Verzeichnis-Listing', 'Niemand kann sehen, welche Freigaben es gibt')}
    </div>
    <button class="btn primary block" data-act="share-done">Fertig – Teilen ist bereit</button>`;
}
function sharesSheet() {
  return `<h2>Aktive Freigaben ${NEW}</h2><div class="card flush sheet-list">
    <div class="row"><span class="avatar a7">${icon('filetext')}</span><span class="grow"><span class="t">Steuer 2025.pdf</span><span class="s">läuft morgen ab · mit Passwort</span></span><button class="icon-btn" data-act="toast" data-arg="Freigabe beendet – Datei vom Server gelöscht">${icon('x')}</button></div>
    <div class="row"><span class="avatar a1">${icon('folder')}</span><span class="grow"><span class="t">Urlaub Ostsee (12 Fotos)</span><span class="s">noch 6 Tage</span></span><button class="icon-btn" data-act="toast" data-arg="Freigabe beendet – Datei vom Server gelöscht">${icon('x')}</button></div></div>
    <p class="hint-text">Beenden löscht die geteilte Kopie sofort vom Server. Abgelaufene Freigaben räumt die App beim nächsten Verbinden auf.</p>`;
}

// --- render --------------------------------------------------------------------------------------
function screenHtml() {
  if (S.phase === 'start') return startScreen();
  if (S.phase === 'unlock') return unlockScreen();
  const t = current();
  if (t) {
    switch (t.screen) {
      case 'entry': return entryScreen(t.id);
      case 'security': return securityScreen();
      case 'thread': return mailThread(t.id);
      case 'compose': return composeScreen(t.kind);
      case 'set-page': return settingsPage(t.id);
      case 'cal-event': return calEvent(t.id, t.day);
      case 'cal-edit': return calEdit();
      default: break;
    }
  }
  const tabs = { pass: passList, auth: authScreen, drive: driveScreen, mail: mailScreen, cal: () => calScreen(), settings: settingsScreen };
  if (S.tab === 'drive' && S.select) return tabs[S.tab]().replace(/<button class="fab[^]*?<\/button>/, '') + selBar();
  return tabs[S.tab]() + nav();
}
function afterRender(root) {
  root.querySelectorAll('.js-w').forEach((el) => { el.style.width = el.dataset.w + '%'; });
}
function render(keepScroll) {
  const app = $('app');
  const scroller = app.querySelector('.screen');
  const y = keepScroll && scroller ? scroller.scrollTop : 0;
  const focused = document.activeElement && document.activeElement.id === 'q';
  app.innerHTML = screenHtml();
  afterRender(app);
  const s = app.querySelector('.screen');
  if (s) s.scrollTop = y;
  const q = $('q');
  if (q && focused) { q.focus(); q.setSelectionRange(q.value.length, q.value.length); }
  const d = $('device');
  d.classList.toggle('in-app', S.phase === 'app');
}

// --- events --------------------------------------------------------------------------------------
const ACT = {
  tab: (a) => { S.tab = a; S.stack = []; S.query = ''; S.select = false; render(); },
  back: () => { if (S.tab === 'drive' && !current() && S.driveFolder) { S.driveFolder = null; render(); } else pop(); },
  push: (a) => push(a),
  lock: () => { S.phase = 'unlock'; S.stack = []; render(); toast('Gesperrt'); },
  unlock: () => { S.phase = 'app'; render(); toast('Entsperrt'); },
  phase: (a) => { S.phase = a; render(); },
  'start-new': () => { S.phase = 'unlock'; render(); },
  toast: (a) => toast(a),
  copy: (a) => toast(`${a} kopiert · wird in 45 s gelöscht`),
  reveal: (a) => { S.revealed[a] = !S.revealed[a]; render(true); },
  entry: (a) => { closeSheet(); push('entry', { id: a }); },
  'pass-filter': (a) => { S.passFilter = a; render(true); },
  vault: (a) => { S.vault = a || null; closeSheet(); render(); },
  otp: (a) => {
    if (S.toggles.hideCodes && !S.revealed['t' + a]) { S.revealed['t' + a] = true; render(true); return; }
    toast('Code kopiert');
  },
  'auth-grid': () => { S.authGrid = !S.authGrid; render(true); },
  'drive-grid': () => { S.driveGrid = !S.driveGrid; render(true); },
  'drive-open': (a) => { if (DRIVE[a]) { S.driveFolder = a; render(); } else toast('Ordner öffnen'); },
  'drive-file': (a) => { S.shareId = a; sheet(fileSheet(a)); },
  'drive-select': () => { S.select = !S.select; S.sel = {}; render(true); },
  'drive-sel': (a) => { S.sel[a] = !S.sel[a]; render(true); },
  'sheet-share': () => sheet(shareSheet()),
  'sheet-link': () => sheet(linkSheet()),
  'sheet-shares': () => sheet(sharesSheet()),
  'link-days': (a) => { S.linkDays = Number(a); sheet(linkSheet()); },
  'share-setup': () => { S.shareStep = 1; sheet(shareSetupSheet()); },
  'share-dir': (a) => { S.shareDir = a; sheet(shareSetupSheet()); },
  'share-step': (a) => { S.shareStep = Number(a); sheet(shareSetupSheet()); },
  'share-done': () => { S.shareReady = true; closeSheet(); render(true); toast('Teilen ist eingerichtet'); },
  'link-pw': () => { S.linkPw = !S.linkPw; sheet(linkSheet()); },
  'sheet-entryshare': () => sheet(entryShareSheet()),
  'sheet-mailmenu': () => sheet(mailMenuSheet()),
  'mail-open': (a) => { S.read[a] = true; push('thread', { id: a }); },
  'mail-acct': (a) => { S.mailAcct = a; closeSheet(); render(); },
  compose: (a) => push('compose', { kind: a || 'new' }),
  send: () => { pop(); toast('Gesendet · Rückgängig (5 s)'); },
  drawer: () => sheet(mailDrawer()),
  'sheet-mailbox': () => sheet(mailDrawer()),
  'drawer-settings': () => { closeSheet(); S.tab = 'settings'; S.stack = [{ screen: 'set-page', id: 'm-mail' }]; render(); },
  toggle: (a) => { S.toggles[a] = !S.toggles[a]; render(true); if ($('device').classList.contains('sheet-open')) return; },
  'set-page': (a) => push('set-page', { id: a }),
  'set-theme': (a) => setTheme(a),
  theme: () => setTheme(S.theme === 'dark' ? 'light' : 'dark'),
  'close-sheet': () => closeSheet(),
  'sheet-gen': () => sheet(generatorSheet()),
  'gen-mode': (a) => { S.setGen.mode = a; sheet(generatorSheet()); },
  'gen-opt': (a) => { S.setGen[a] = !S.setGen[a]; sheet(generatorSheet()); },
  jump: (a) => {
    closeSheet();
    const [phase, tab, screen, arg] = a.split(':');
    S.phase = phase; if (tab) S.tab = tab; S.stack = []; S.query = '';
    if (screen) S.stack.push({ screen, id: arg, kind: arg });
    if (screen === 'gen') { S.stack = []; render(); sheet(generatorSheet()); return; }
    render();
  },
};
for (const k of ['newitem', 'vaults', 'authvaults', 'servers', 'addtotp', 'upload', 'entrymenu']) ACT['sheet-' + k] = () => sheet(SHEETS[k]());

function setTheme(t) {
  S.theme = t;
  document.documentElement.dataset.theme = t;
  try { localStorage.setItem('ghul-design-theme', t); } catch (e) { /* private mode */ }
  render(true);
}

document.addEventListener('click', (ev) => {
  const el = ev.target.closest('[data-act]');
  if (!el) return;
  const fn = ACT[el.dataset.act];
  if (fn) { ev.preventDefault(); fn(el.dataset.arg || ''); }
});
document.addEventListener('input', (ev) => {
  if (ev.target.id === 'q') { S.query = ev.target.value; render(true); }
  if (ev.target.id === 'gen-len') { S.setGen.len = Number(ev.target.value); sheet(generatorSheet()); }
});
document.addEventListener('keydown', (ev) => { if (ev.key === 'Escape') closeSheet(); });
$('mark-new').addEventListener('change', (ev) => document.body.classList.toggle('hide-new', !ev.target.checked));

// --- side panel ------------------------------------------------------------------------------------
const JUMPS = [
  ['Start', 'start'], ['Entsperren', 'unlock'], ['GhulPass', 'app:pass'], ['Eintrag', 'app:pass:entry:1'], ['Sicherheitscheck', 'app:pass:security'],
  ['Generator', 'app:pass:gen'], ['GhulAuth', 'app:auth'], ['GhulDrive', 'app:drive'], ['GhulMail', 'app:mail'], ['Mail lesen', 'app:mail:thread:1'],
  ['Mail schreiben', 'app:mail:compose:new'], ['GhulCal', 'app:cal'], ['Termin', 'app:cal:cal-event:2'], ['Neuer Termin', 'app:cal:cal-edit:new'],
  ['Optionen', 'app:settings'], ['Mail-Optionen', 'app:settings:set-page:m-mail'], ['Kalender-Optionen', 'app:settings:set-page:m-cal'],
];
$('jump').innerHTML = JUMPS.map(([l, a]) => `<button class="chip" data-act="jump" data-arg="${a}">${l}</button>`).join('');
const PROPOSALS = [
  ['GhulCal bauen', 'Kalender als eigener verschlüsselter Tresor (.ghlcal): Monat, Woche, Tag, Liste, Serien, Erinnerungen, Feiertage offline, Schnell-Eintragen, .ics-Austausch – bewusst ohne CalDAV.'],
  ['Dieses Design in die App übernehmen', 'Einheitliche Bausteine, neue Startseiten je Bereich, neu gegliederte Optionen. Danach Screenshots zum Abgleich.'],
  ['GhulPass ausbauen', 'Favoriten, Kategorien, zuletzt verwendet, Sicherheitscheck, Passwortverlauf, Passphrasen, Anhänge, 2FA-Code direkt im Eintrag.'],
  ['GhulMail zu einem echten Mailprogramm machen', 'Gemeinsamer Posteingang, Unterhaltungen, Vorschautext, verschlüsselter Offline-Speicher, Entwürfe, Adressvorschläge, Wischgesten.'],
  ['GhulDrive komfortabel machen', 'Raster/Liste, Vorschau, Zuletzt/Favoriten/Offline/Papierkorb, Warteschlange mit Fortsetzen, Foto-Sicherung, Teilen-Menü und Android-Dateiauswahl.'],
  ['GhulAuth verfeinern', 'Kachelansicht, Favoriten und Sortierung, QR aus Bild, QR-Export, Uhrabweichungs-Warnung.'],
  ['Weitere Speicher', 'WebDAV (Nextcloud) und FTPS; danach das PHP-Server-Modul für Webhosting ohne SSH.'],
  ['Synchronisation abrunden', 'Löschungen von Servern/Konten übertragen, Konfliktansicht, Sync-Verlauf.'],
  ['Windows-Version', 'Gleicher Kern, gleiche Oberfläche; portabel vom USB-Stick, Auto-Type.'],
  ['Weg zur ersten Version', 'Eigener Signierschlüssel, unabhängige Sicherheitsprüfung, Datenschutzerklärung, Veröffentlichung (z. B. F-Droid).'],
];
$('proposals').innerHTML = PROPOSALS.map(([t, d]) => `<li><b>${t}</b><span>${d}</span></li>`).join('');

// --- start -----------------------------------------------------------------------------------------
try {
  const saved = localStorage.getItem('ghul-design-theme');
  if (saved) S.theme = saved; else if (matchMedia('(prefers-color-scheme: dark)').matches) S.theme = 'dark';
} catch (e) { /* ignore */ }
document.documentElement.dataset.theme = S.theme;
function tick() {
  const d = new Date();
  $('clock').textContent = d.getHours() + ':' + String(d.getMinutes()).padStart(2, '0');
  const t = current();
  const live = S.phase === 'app' && ((S.tab === 'auth' && !t) || (t && t.screen === 'entry'));
  if (live && !$('device').classList.contains('sheet-open') && document.activeElement.id !== 'q') render(true);
}
setInterval(tick, 1000);
render();
tick();
