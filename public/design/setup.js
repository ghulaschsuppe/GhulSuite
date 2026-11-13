// SPDX-License-Identifier: AGPL-3.0-or-later
// Design preview for issue #24: setup wizard that settles the basics, one file per area, area pages with
// "this area" and "this device" groups, areas that are not set up yet. Made-up data, nothing is stored.
'use strict';

// --- areas ---------------------------------------------------------------------------------------
const AREAS = [
  { id: 'pass', name: 'GhulPass', icon: 'key', bg: 'bg-pass', file: '.ghlpss', sub: 'Passwörter, Notizen, Karten, WLAN', def: 'Privat' },
  { id: 'auth', name: 'GhulAuth', icon: 'shieldcheck', bg: 'bg-auth', file: '.ghltotp', sub: '2FA-Codes für deine Konten', def: 'Persönlich' },
  { id: 'drive', name: 'GhulDrive', icon: 'cloud', bg: 'bg-drive', file: '.ghldrv', sub: 'Verschlüsselte Dateien auf deinem Server', def: 'Meine Dateien' },
  { id: 'mail', name: 'GhulMail', icon: 'mail', bg: 'bg-mail', file: '.ghlmail', sub: 'Deine bestehende Mailadresse (IMAP/SMTP)', def: 'Mail' },
  { id: 'cal', name: 'GhulCal', icon: 'cal', bg: 'bg-cal', file: '.ghlcal', sub: 'Kalender mit Erinnerungen', def: 'Kalender' },
];
const areaOf = (id) => AREAS.find((a) => a.id === id);
const aico = (a, cls = '') => `<span class="aico ${a.bg} ${cls}">${icon(a.icon)}</span>`;

Object.assign(S, {
  setup: null,
  /** on = the area's file exists (all devices); hidden = only hidden on this device. */
  areas: Object.fromEntries(AREAS.map((a) => [a.id, { on: true, hidden: false }])),
  syncOn: true,
  driveSetup: true,
  driveMode: 'sync',
  mailSetup: true,
  codeConfirmed: true,
  /** Server forms (sync, drive, …): type, chosen folder, PHP module uploaded, checked. */
  srv: {},
  folderFor: null,
});
const srv = (k) => (S.srv[k] = S.srv[k] || { type: 'sftp', folder: '', uploaded: false, ok: false, keep: true });

/** Folders on the server, as the app lists them (made up). */
const FOLDERS = {
  sftp: [['ghulsuite', 'Neu anlegen (empfohlen)'], ['backup', ''], ['daten', '']],
  ftps: [['public_html', 'Wird im Internet ausgeliefert – nicht für Tresore'], ['ghulsuite', 'Neu anlegen (empfohlen)'], ['private', 'Nicht öffentlich']],
  php: [['public_html/ghulsuite', 'Neu anlegen (empfohlen)'], ['public_html', 'Hauptordner der Website'], ['public_html/cloud', '']],
};

/**
 * Server form used in the wizard, for GhulDrive and in the options. SFTP/FTPS: login and a folder
 * chosen from the server. PHP module: log in to the web hosting once via FTPS, pick a folder, the app
 * uploads the module and suggests and checks the address itself – no typing of URLs.
 */
function serverForm(k) {
  const f = srv(k);
  const types = [['sftp', 'SFTP'], ['ftps', 'FTPS'], ['php', 'PHP-Modul']];
  const seg = `<div class="seg" data-u="30">${types.map(([t, l]) => `<button class="${f.type === t ? 'on' : ''}" data-act="srv-type" data-arg="${k}|${t}">${l}</button>`).join('')}</div>`;
  const host = f.type === 'sftp' ? 'nas.example.org' : 'ftp.max.example.org';
  const login = `<div class="field"><div class="grow"><label>${f.type === 'php' ? 'Webhosting (FTPS)' : 'Server'}</label><input value="${host}" aria-label="Server"></div></div>
    <div class="field"><div class="grow"><label>Benutzer</label><input value="${f.type === 'sftp' ? 'max' : 'w01234ab'}" aria-label="Benutzer"></div></div>
    <div class="field"><div class="grow"><label>Anmeldung</label><div class="value">${f.type === 'sftp' ? 'SSH-Schlüssel dieses Geräts' : 'Passwort ••••••••'}</div></div>${icon('chev')}</div>`;
  const folder = `<button class="row folder-row" data-act="srv-folder" data-arg="${k}"><span class="avatar a9">${icon('folder')}</span>
    <span class="grow"><span class="t">${f.folder ? f.folder + '/' : 'Ordner wählen'}</span><span class="s">${f.folder ? 'Antippen zum Ändern' : 'Die App zeigt die Ordner auf dem Server'}</span></span>${icon('chev', 'chev')}</button>`;
  if (f.ok) {
    const where = f.type === 'php' ? `https://max.example.org/${f.folder.replace(/^public_html\/?/, '')}${f.folder === 'public_html' ? '' : '/'}ghulsuite.php` : `${host}:/${f.folder}`;
    return `${seg}<div class="banner ok-banner">${icon('check')}<span>${f.type === 'php' ? 'Modul läuft unter' : 'Verbunden ·'} <b>${where}</b></span></div>`
      + (f.type === 'php' ? `<div class="card flush sheet-list">${toggle('keepFtps', 'FTPS-Zugang behalten', 'Für Updates des Moduls. Aus: GhulSuite arbeitet nur noch über HTTPS mit dem Schlüssel des Moduls')}</div>` : '');
  }
  if (f.type !== 'php') {
    return `${seg}${login}${folder}${f.folder ? `<button class="btn tonal block" data-act="srv-test" data-arg="${k}">${icon('sync')} Verbindung testen</button>` : ''}`;
  }
  if (!f.uploaded) {
    return `${seg}<p class="hint-text" data-u="36">Für Webhosting ohne SSH: GhulSuite lädt sein Modul einmal per FTPS hoch und arbeitet danach über HTTPS.</p>${login}${folder}
      ${f.folder ? `<button class="btn tonal block" data-act="srv-upload" data-arg="${k}">${icon('upload')} Modul hochladen</button>` : ''}`;
  }
  const url = `https://max.example.org/${f.folder.replace(/^public_html\/?/, '')}${f.folder === 'public_html' ? '' : '/'}ghulsuite.php`;
  return `${seg}<div class="banner ok-banner">${icon('check')}<span>ghulsuite.php und Datenordner liegen in <b>${f.folder}/</b></span></div>
    <div class="field"><div class="grow"><label>Adresse im Internet (Vorschlag)</label><div class="value">${url}</div></div>${icon('edit')}</div>
    <p class="hint-text" data-u="36">Aus Domain und Ordner abgeleitet. Nur ändern, wenn deine Website eine andere Adresse hat.</p>
    <button class="btn tonal block" data-act="srv-test" data-arg="${k}">${icon('sync')} Adresse prüfen</button>`;
}

function folderSheet(k) {
  const f = srv(k);
  const list = FOLDERS[f.type];
  return `<h2>Ordner wählen</h2><p class="hint-text">${f.type === 'php' ? 'Auf deinem Webhosting · der Ordner muss im Internet erreichbar sein.' : 'Auf dem Server · hier liegen später nur verschlüsselte Dateien.'}</p>
    <div class="card flush sheet-list"><div class="row plain"><span class="grow"><span class="s">${f.type === 'sftp' ? '/home/max' : '/'}</span></span></div>
    ${list.map(([d, h]) => `<button class="row ${f.folder === d ? 'selected' : ''}" data-act="srv-pick" data-arg="${k}|${d}"><span class="avatar a1">${icon('folder')}</span><span class="grow"><span class="t">${d}</span>${h ? `<span class="s">${h}</span>` : ''}</span>${f.folder === d ? icon('check') : ''}</button>`).join('')}
    <button class="row" data-act="toast" data-arg="Neuer Ordner"><span class="avatar a5">${icon('plus')}</span><span class="grow"><span class="t">Neuer Ordner …</span></span></button></div>`;
}

function driveServerSheet() {
  return `<h2>Eigener Server für GhulDrive</h2>${serverForm('drive2')}
    <button class="btn primary block" data-u="24" data-act="drive-own-done">${icon('check')} Fertig</button>`;
}
Object.assign(S.toggles, { keepFtps: true });

function newSetup() {
  return {
    step: 1, factor: 'key', storage: 'app', sync: 'later', syncType: 'sftp', syncOk: false,
    areas: { pass: true, auth: true, drive: false, mail: false, cal: true }, driveMode: 'later',
    lock: '1 Min.', unlock: 'bio', pin: '', hours: '24 Std.', confirmed: false,
  };
}

// --- wizard --------------------------------------------------------------------------------------
const WZ_STEPS = ['Master-Passwort', 'Zweiter Faktor', 'Speicherort', 'Synchronisation', 'Bereiche', 'Sperren', 'Entsperren', 'Wiederherstellungscode'];

/** Selectable card (radio). */
const pick = (on, act, arg, avatar, t, s, extra = '') => `<button class="row pick ${on ? 'selected' : ''}" data-act="${act}" data-arg="${arg}">
  ${avatar}<span class="grow"><span class="t">${t}</span>${s ? `<span class="s">${s}</span>` : ''}</span>
  <span class="radio ${on ? 'on' : ''}"></span></button>${extra}`;

function wzStep1() {
  return `<div class="field"><div class="grow"><label>Master-Passwort</label><input type="password" value="Laterne-Kompass-Wiese-Hafen-42" aria-label="Master-Passwort"></div>${icon('eye')}</div>
    <div class="field"><div class="grow"><label>Wiederholen</label><input type="password" value="Laterne-Kompass-Wiese-Hafen-42" aria-label="Master-Passwort wiederholen"></div>${icon('check')}</div>
    <div class="strength"><i class="js-w s-good" data-w="100"></i></div>
    <p class="hint-text" data-u="2">Sehr stark · Passphrase aus 4 Wörtern</p>
    <button class="btn tonal block" data-act="toast" data-arg="Vorschlag: Möwe-Teekanne-Sternbild-Garten-73">${icon('dice')} Passphrase vorschlagen</button>
    <p class="hint-text" data-u="33">Das Passwort verlässt nie dein Gerät. Ohne Passwort und zweiten Faktor hilft nur noch der Wiederherstellungscode (Schritt 8).</p>`;
}

function wzStep2(w) {
  const keyExtra = w.factor === 'key' ? `<div class="pick-more">
      <div class="field"><div class="grow"><label>Name des Keys</label><input value="YubiKey blau" aria-label="Name des Keys"></div></div>
      <div class="keyhint"><span class="pulse">${icon('usbkey')}</span><span>Key einstecken oder ans Handy halten (NFC) und berühren. Die PIN des Keys fragt das System ab.</span></div></div>` : '';
  const fileExtra = w.factor === 'file' ? `<div class="pick-more">
      <button class="btn tonal block" data-act="toast" data-arg="ghulsuite.ghlkey gespeichert">${icon('download')} Schlüsseldatei speichern</button>
      <p class="hint-text" data-u="24">Lege sie auf einen USB-Stick oder an einen anderen sicheren Ort – nicht neben die Suite.</p></div>` : '';
  return `<div class="card flush">
      ${pick(w.factor === 'key', 'wz-set', 'factor:key', `<span class="avatar a0">${icon('usbkey')}</span>`, 'Security Key', 'Empfohlen · YubiKey, Nitrokey … per USB oder NFC', keyExtra)}
      ${pick(w.factor === 'file', 'wz-set', 'factor:file', `<span class="avatar a9">${icon('file')}</span>`, 'Schlüsseldatei', 'Eine Datei, die du getrennt aufbewahrst', fileExtra)}</div>
    <p class="hint-text">Zum Entsperren brauchst du immer Passwort <b>und</b> diesen zweiten Faktor. Weitere Keys oder eine Ersatz-Schlüsseldatei fügst du später unter Optionen hinzu.</p>`;
}

function wzStep3(w) {
  const folder = w.storage === 'folder' ? `<div class="pick-more"><div class="field"><div class="grow"><label>Ordner</label><div class="value">USB-Stick / GhulSuite</div></div>${icon('folder')}</div></div>` : '';
  return `<div class="card flush">
      ${pick(w.storage === 'app', 'wz-set', 'storage:app', `<span class="avatar a5">${icon('phone')}</span>`, 'App-Speicher', 'Empfohlen · nur GhulSuite kommt heran')}
      ${pick(w.storage === 'folder', 'wz-set', 'storage:folder', `<span class="avatar a9">${icon('folder')}</span>`, 'Ordner wählen', 'USB-Stick, SD-Karte, Cloud-Ordner – jede Datei ist einzeln verschlüsselt', folder)}</div>
    <p class="hint-text">Der Ordner ist die Suite: Alle Bereichsdateien liegen dort nebeneinander.</p>`;
}

function wzStep4(w) {
  const server = w.sync === 'server' ? `<div class="pick-more">${serverForm('sync')}</div>` : '';
  return `<div class="card flush">
      ${pick(w.sync === 'later', 'wz-set', 'sync:later', `<span class="avatar a5">${icon('phone')}</span>`, 'Später', 'Erst einmal nur auf diesem Gerät')}
      ${pick(w.sync === 'server', 'wz-set', 'sync:server', `<span class="avatar a1">${icon('server')}</span>`, 'Eigener Server', 'Alle Geräte gleichen sich darüber ab – nur verschlüsselt', server)}</div>
    <p class="hint-text">Optional. Auf weiteren Geräten wählst du dann „Von meinem Server holen“. GhulDrive kann denselben Server mitnutzen (nächster Schritt).</p>`;
}

function wzStep5(w) {
  const syncReady = w.sync === 'server' && srv('sync').ok;
  return AREAS.map((a) => {
    const on = w.areas[a.id];
    let more = '';
    if (on && ['pass', 'auth', 'cal'].includes(a.id)) {
      more = `<div class="pick-more"><div class="field"><div class="grow"><label>Name</label><input value="${a.def}" aria-label="Name"></div></div></div>`;
    }
    if (on && a.id === 'drive') {
      more = `<div class="pick-more"><p class="hint-text" data-u="36">Wo sollen die Dateien liegen?</p><div class="card flush sheet-list">
        ${syncReady
          ? pick(w.driveMode === 'sync', 'wz-set', 'driveMode:sync', `<span class="avatar a2">${icon('sync')}</span>`, 'Wie Synchronisation', 'nas.example.org · eigener Ordner ghulsuite/drive')
          : `<div class="row pick disabled"><span class="avatar a2">${icon('sync')}</span><span class="grow"><span class="t">Wie Synchronisation</span><span class="s">Erst Synchronisation einrichten (Schritt 4)</span></span><span class="radio"></span></div>`}
        ${pick(w.driveMode === 'own', 'wz-set', 'driveMode:own', `<span class="avatar a1">${icon('server')}</span>`, 'Eigener Server', 'Z. B. Webhosting', w.driveMode === 'own' ? `<div class="pick-more">${serverForm('drive')}</div>` : '')}
        ${pick(w.driveMode === 'later', 'wz-set', 'driveMode:later', `<span class="avatar a5">${icon('clock')}</span>`, 'Später', 'GhulDrive zeigt dann „Noch nicht eingerichtet“')}</div></div>`;
    }
    if (on && a.id === 'mail') {
      more = `<div class="pick-more"><p class="hint-text" data-u="36">Dein Mailkonto richtest du direkt nach der Einrichtung in GhulMail ein.</p></div>`;
    }
    return `<div class="card flush area-card ${on ? '' : 'off'}"><div class="row plain">${aico(a)}
      <span class="grow"><span class="t">${a.name}</span><span class="s">${a.sub}</span></span>
      <button class="toggle ${on ? 'on' : ''}" data-act="wz-area" data-arg="${a.id}" aria-label="${a.name}"></button></div>${more}</div>`;
  }).join('') + `<p class="hint-text">Jeder gewählte Bereich bekommt seine eigene Datei mit seinen Einstellungen. Nicht gewählte Bereiche legen keine Datei an und sind ausgeblendet – einschalten jederzeit unter Optionen → Bereiche.</p>`;
}

function wzStep6(w) {
  const times = ['Sofort', '30 s', '1 Min.', '5 Min.', '15 Min.'];
  return `<div class="card"><h3>Automatisch sperren</h3><p>Wenn GhulSuite im Hintergrund ist oder du nichts tust.</p>
      <div class="chips wrap" data-u="24">${times.map((t) => `<button class="chip ${w.lock === t ? 'on' : ''}" data-act="wz-set" data-arg="lock:${t}">${t}</button>`).join('')}</div></div>
    <div class="card flush">${toggle('screenOff', 'Beim Ausschalten des Bildschirms sperren')}${toggle('shake', 'Schütteln = Panik-Sperre', 'Sperrt sofort und löscht den Schnellzugang (Fingerabdruck, PIN)')}</div>
    <p class="hint-text">Alles lässt sich später unter Optionen → Sperren und Panik ändern. Gilt nur für dieses Gerät.</p>`;
}

function wzStep7(w) {
  const hours = ['4 Std.', '24 Std.', '3 Tage', '7 Tage', 'Immer'];
  const pinField = w.unlock === 'pin' ? `<div class="pick-more"><div class="field"><div class="grow"><label>App-PIN (6–12 Ziffern)</label><input type="password" value="482915" aria-label="App-PIN"></div></div></div>` : '';
  return `<p class="hint-text" data-u="36">Schnell entsperren heißt: mit <b>Fingerabdruck</b> oder einer <b>App-PIN</b> statt Passwort und Key – für die Zeit zwischen zwei vollständigen Anmeldungen.</p>
    <div class="card flush">
      ${pick(w.unlock === 'none', 'wz-set', 'unlock:none', `<span class="avatar a0">${icon('usbkey')}</span>`, 'Immer Passwort und Key', 'Am sichersten')}
      ${pick(w.unlock === 'bio', 'wz-set', 'unlock:bio', `<span class="avatar a2">${icon('fingerprint')}</span>`, 'Zusätzlich Fingerabdruck', 'Für die Zeit dazwischen')}
      ${pick(w.unlock === 'pin', 'wz-set', 'unlock:pin', `<span class="avatar a5">${icon('dice')}</span>`, 'Zusätzlich App-PIN', 'Wenn kein Fingerabdruck möglich ist', pinField)}</div>
    ${w.unlock === 'none' ? '' : `<div class="card"><h3>Schnellzugang gilt</h3><p>Danach wieder Passwort und Key.</p>
      <div class="chips wrap" data-u="24">${hours.map((h) => `<button class="chip ${w.hours === h ? 'on' : ''}" data-act="wz-set" data-arg="hours:${h}">${h}</button>`).join('')}</div>
      ${w.hours === 'Immer' ? '<p class="hint-text" data-u="33">Ohne Ablauf – aber nach einem Neustart des Geräts, einer Panik-Sperre oder einem Passwortwechsel trotzdem wieder Passwort und Key.</p>' : ''}</div>`}
    <p class="hint-text">Nach einem Neustart, einer Panik-Sperre oder einem Passwortwechsel brauchst du immer wieder Passwort und Key.</p>`;
}

function wzStep8(w) {
  return `<div class="card"><h3>Dein Notfall-Zugang</h3><p>Der Code öffnet allein alles – auch ohne Passwort und Key. Schreib ihn auf oder drucke das Notfallblatt.</p>
      <div class="codebox">${['K7QD', 'M2XF', '9TRW', 'B4NE', 'H8PZ', 'C6VJ', 'R3LS', 'Y5GA'].map((g) => `<span>${g}</span>`).join('')}</div></div>
    <div class="btn-row"><button class="btn outline" data-act="toast" data-arg="Notfallblatt wird gedruckt">${icon('filetext')} Notfallblatt drucken</button></div>
    <div class="card flush" data-u="24"><div class="row plain"><span class="grow"><span class="t">Ich habe den Code sicher notiert</span><span class="s" data-u="27">Ohne diese Bestätigung erinnert dich GhulSuite nach dem Entsperren daran.</span></span>
      <button class="toggle ${w.confirmed ? 'on' : ''}" data-act="wz-confirm" aria-label="Code notiert"></button></div></div>`;
}

function setupScreen() {
  const w = S.setup;
  const n = w.step;
  const body = [wzStep1, wzStep2, wzStep3, wzStep4, wzStep5, wzStep6, wzStep7, wzStep8][n - 1](w);
  const last = n === 8;
  const next = last
    ? `<button class="btn primary" data-act="wz-finish">${icon('check')} Fertig</button>`
    : `<button class="btn primary" data-act="wz-next">${n === 4 && w.sync === 'later' ? 'Später' : 'Weiter'}</button>`;
  return `<div class="topbar"><button class="icon-btn" data-act="wz-back" aria-label="Zurück">${icon('back')}</button>
      <div class="ttl"><h1 class="small">Neu einrichten</h1><span class="ctx c-set">Schritt ${n} von 8 · ${WZ_STEPS[n - 1]}</span></div></div>
    <div class="screen"><div class="steps">${WZ_STEPS.map((_, i) => `<i class="${i < n ? 'on' : ''}"></i>`).join('')}</div>
      <h2 class="wz-title">${WZ_STEPS[n - 1]}</h2>${body}</div>
    <div class="actionbar">${n > 1 ? `<button class="btn outline" data-act="wz-back">Zurück</button>` : ''}${next}</div>`;
}

// --- areas that are not set up -------------------------------------------------------------------
function driveEmpty() {
  const a = areaOf('drive');
  return `${topbar('GhulDrive', { ctx: { label: 'Noch nicht eingerichtet', c: 'c-drive' } })}<div class="screen">
    <div class="card setup-card">${aico(a, 'lg')}<h3>GhulDrive einrichten</h3>
      <p>Deine Dateien werden auf dem Gerät verschlüsselt und auf einem Server deiner Wahl abgelegt.</p></div>
    <div class="section-title">Wo sollen die Dateien liegen?</div><div class="card flush setup-rows">
      ${S.syncOn ? `<button class="row" data-act="drive-setup" data-arg="sync"><span class="avatar a2">${icon('sync')}</span><span class="grow"><span class="t">Wie Synchronisation</span><span class="s">Heimserver · eigener Ordner ghulsuite/drive</span></span>${icon('chev', 'chev')}</button>` : ''}
      <button class="row" data-act="drive-setup" data-arg="own"><span class="avatar a1">${icon('server')}</span><span class="grow"><span class="t">Eigener Server</span><span class="s">SFTP, FTPS (Webhosting) oder GhulSuite-PHP-Modul</span></span>${icon('chev', 'chev')}</button></div>
    ${S.syncOn ? '' : '<p class="hint-text">Tipp: Richtest du unter Optionen → Synchronisation einen Server ein, kann GhulDrive ihn mitnutzen.</p>'}
  </div>`;
}

function mailEmpty() {
  const a = areaOf('mail');
  return `${topbar('GhulMail', { ctx: { label: 'Noch kein Konto', c: 'c-mail' } })}<div class="screen">
    <div class="card setup-card">${aico(a, 'lg')}<h3>GhulMail einrichten</h3>
      <p>Nutze deine bestehende Mailadresse. Die Anmeldung liegt verschlüsselt in der Mail-Datei deiner Suite.</p></div>
    <div class="field"><div class="grow"><label>Mailadresse</label><input value="max@posteo.de" aria-label="Mailadresse"></div>${icon('mail')}</div>
    <div class="field"><div class="grow"><label>Passwort</label><input type="password" value="geheim123" aria-label="Passwort"></div>${icon('eye')}</div>
    <button class="btn primary block" data-act="mail-setup">${icon('check')} Konto prüfen und hinzufügen</button>
    <p class="hint-text" data-u="33">Server werden aus der Adresse erkannt (z. B. Posteo, mailbox.org, eigene Domain). Nur verschlüsselte Verbindungen.</p>
  </div>`;
}

const baseDriveScreen = driveScreen;
driveScreen = function () { // eslint-disable-line no-global-assign
  if (!S.driveSetup) return driveEmpty();
  const html = baseDriveScreen();
  return S.driveMode === 'own' ? html.replace('Heimserver · Meine Dateien', 'Webhosting · Meine Dateien') : html;
};
const baseMailScreen = mailScreen;
mailScreen = function () { // eslint-disable-line no-global-assign
  return S.mailSetup ? baseMailScreen() : mailEmpty();
};

/** Reminder after unlocking when the recovery code was never confirmed. */
const basePassList = passList;
passList = function () { // eslint-disable-line no-global-assign
  const html = basePassList();
  if (S.codeConfirmed) return html;
  return html.replace('<div class="screen">', `<div class="screen"><div class="banner warn-banner">${icon('alert')}<span>Dein Wiederherstellungscode wurde nie bestätigt.</span><button data-act="code-sheet">Jetzt erledigen</button></div>`);
};

// --- navigation follows the areas ---------------------------------------------------------------
const visibleAreas = () => AREAS.filter((a) => S.areas[a.id].on && !S.areas[a.id].hidden).map((a) => a.id);
nav = function () { // eslint-disable-line no-global-assign
  const labels = { pass: ['GhulPass', 'key', 'c-pass'], auth: ['GhulAuth', 'shieldcheck', 'c-auth'], drive: ['GhulDrive', 'cloud', 'c-drive'], mail: ['GhulMail', 'mail', 'c-mail'], cal: ['GhulCal', 'cal', 'c-cal'] };
  const tabs = [...visibleAreas().map((id) => [id, ...labels[id]]), ['settings', 'Optionen', 'sliders', 'c-set']];
  const unread = S.mailSetup ? MAILS.filter((m) => m.unread && !S.read[m.id]).length : 0;
  return `<nav class="nav n${tabs.length}">${tabs.map(([id, label, ic, c]) => `<button class="${c} ${S.tab === id ? 'on' : ''}" data-act="tab" data-arg="${id}">
    <span class="ind">${icon(ic)}${id === 'mail' && unread ? `<span class="badge">${unread}</span>` : ''}</span>${label}</button>`).join('')}</nav>`;
};
const baseScreenHtml = screenHtml;
screenHtml = function () { // eslint-disable-line no-global-assign
  if (S.phase === 'setup') return setupScreen();
  if (S.phase === 'app' && S.tab !== 'settings' && !visibleAreas().includes(S.tab)) S.tab = visibleAreas()[0] || 'settings';
  return baseScreenHtml();
};

// --- settings: area pages with two groups --------------------------------------------------------
const group = (title, body) => `<div class="section-title">${title}</div><div class="card flush">${body}</div>`;
const deviceTitle = `Nur auf diesem Gerät <span class="tag">${icon('phone')} Pixel 8</span>`;
const areaFooter = (id) => `<div class="section-title">Bereich</div><div class="card flush">
  <button class="row plain" data-act="area-hide" data-arg="${id}"><span class="grow"><span class="t">Auf diesem Gerät ausblenden</span><span class="s">Die Datei bleibt, andere Geräte zeigen den Bereich weiter</span></span>${icon('eyeoff')}</button>
  <button class="row plain" data-act="area-remove" data-arg="${id}"><span class="grow"><span class="t" data-u="34">Bereich entfernen</span><span class="s">Löscht ${areaOf(id).file}-Datei auf allen Geräten</span></span>${icon('trash')}</button></div>`;
const fileNote = (id) => `<p class="hint-text">Oben: liegt in der ${areaOf(id).file}-Datei und gilt auf allen Geräten. Unten: nur auf diesem Gerät.</p>`;

const AREA_PAGES = {
  'm-pass': () => group('Für GhulPass · alle Geräte', `${choice('Generator-Standard', 'Passphrase, 4 Wörter', 'Zeichen oder Passphrase, Länge')}`)
    + group(deviceTitle, `${toggle('autofill', 'Autofill', 'Anmeldungen in Apps und Browsern ausfüllen')}${toggle('subtitles', 'Benutzernamen in der Liste zeigen')}${toggle('pwned', 'Datenlecks prüfen', 'Fragt „Have I Been Pwned“ mit k-Anonymität – standardmäßig aus')}`)
    + fileNote('pass') + areaFooter('pass'),
  'm-auth': () => group('Für GhulAuth · alle Geräte', `<div class="row plain"><span class="grow"><span class="s" data-u="27">Konten und Favoriten liegen im Tresor – hier gibt es nichts weiter einzustellen.</span></span></div>`)
    + group(deviceTitle, `${toggle('hideCodes', 'Codes erst nach Antippen zeigen')}${toggle('nextCode', 'Nächsten Code vorab zeigen')}${choice('Sortierung', 'Name', 'Name, Neueste, Favoriten zuerst')}`)
    + fileNote('auth') + areaFooter('auth'),
  'm-drive': () => group('Für GhulDrive · alle Geräte', `
      <button class="row" data-act="drive-storage"><span class="avatar ${S.driveMode === 'own' ? 'a1' : 'a2'}">${icon(S.driveMode === 'own' ? 'server' : 'sync')}</span><span class="grow"><span class="t">Speicher</span>
        <span class="s">${S.driveMode === 'own' ? 'Eigener Server · Webhosting (FTPS)' : 'Wie Synchronisation · Heimserver'}</span></span>${icon('chev', 'chev')}</button>
      ${choice('Papierkorb leeren', 'nach 30 Tagen', '7, 30, 90 Tage, nie')}
      ${link('Teilen per Link', S.shareReady ? 'Eingerichtet · https://max.example.org/teilen' : 'Teilen-Ordner auf diesem Speicher einrichten', 'share-setup')}`)
    + group(deviceTitle, `${toggle('photoBackup', 'Foto-Sicherung', 'Neue Fotos automatisch verschlüsselt hochladen')}${toggle('wifiOnly', 'Nur im WLAN übertragen')}${choice('Ansicht', 'Liste', 'Liste, Raster')}`)
    + fileNote('drive') + areaFooter('drive'),
  'm-mail': () => `<div class="section-title">Konten · alle Geräte</div><div class="card flush">${ACCOUNTS.map((a) => `<button class="row" data-act="toast" data-arg="Konto bearbeiten"><span class="avatar ${a.cls}">${letter(a.name)}</span><span class="grow"><span class="t">${a.name}</span><span class="s">${a.email} · Signatur, PGP-Schlüssel</span></span>${icon('chev', 'chev')}</button>`).join('')}
        <button class="row" data-act="toast" data-arg="Konto hinzufügen"><span class="avatar a5">${icon('plus')}</span><span class="grow"><span class="t">Konto hinzufügen</span></span></button></div>`
    + group('Für GhulMail · alle Geräte', `${link('Bilder immer laden', '2 Absender')}${link('Bekannte Schlüssel (PGP)', '5 Kontakte')}`)
    + group(deviceTitle, `${toggle('notifyMail', 'Benachrichtigungen', 'Auch wenn GhulSuite gesperrt ist')}${choice('Inhalt der Benachrichtigung', 'Nur „Neue E-Mail“', 'Nur Hinweis, mit Absender, mit Betreff')}${toggle('offlineMail', 'Offline lesen', 'Mails verschlüsselt auf diesem Gerät zwischenspeichern')}${choice('Wischen nach links', 'Archivieren', 'Archivieren, Löschen, Gelesen')}`)
    + fileNote('mail') + areaFooter('mail'),
  'm-cal': () => group('Für GhulCal · alle Geräte', `${toggle('calHoliday', 'Feiertage anzeigen', 'Eingebaut und offline')}${choice('Bundesland', 'Nordrhein-Westfalen', 'alle 16 Bundesländer, Österreich, Schweiz')}
      ${choice('Standard für Termine', '15 Min. vorher', 'keine, 5 – 60 Min., 1 Tag')}${choice('Standard für ganztägige', 'Am Vortag um 9:00', 'keine, Am Tag, Am Vortag')}${choice('Neue Termine in', 'Privat', 'Kalender wählen')}${choice('Woche beginnt am', 'Montag', 'Montag, Sonntag')}
      ${link('Kalender verwalten', '5 Kalender · Farben', 'cal-cals')}`)
    + group(deviceTitle, `${choice('Startansicht', 'Monat', 'Monat, Woche, Tag, Liste')}${toggle('calKw', 'Kalenderwochen anzeigen')}${link('Ausgeblendete Kalender', 'keine', 'cal-cals')}
      ${choice('Inhalt auf dem Sperrbildschirm', 'Mit Titel', 'Nur Hinweis, Mit Titel, Alles')}${toggle('calSound', 'Ton und Vibration')}`)
    + fileNote('cal') + areaFooter('cal'),
};

function areasPage() {
  return `<div class="card flush">${AREAS.map((a) => {
    const st = S.areas[a.id];
    const tag = !st.on ? '<span class="tag">nicht eingerichtet</span>' : st.hidden ? '<span class="tag warn">hier ausgeblendet</span>' : `<span class="tag ok">${icon('check')} aktiv</span>`;
    return `<button class="row" data-act="area-sheet" data-arg="${a.id}">${aico(a)}<span class="grow"><span class="t">${a.name}</span><span class="s">${st.on ? `${a.def}${a.file}` : a.sub}</span></span>${tag}</button>`;
  }).join('')}</div>
  <p class="hint-text">Jeder Bereich ist eine eigene verschlüsselte Datei mit seinen Einstellungen. „Ausblenden“ gilt nur für dieses Gerät, „Entfernen“ löscht die Datei überall.</p>`;
}

function areaSheet(id) {
  const a = areaOf(id);
  const st = S.areas[id];
  if (!st.on) {
    const nameField = ['pass', 'auth', 'cal'].includes(id) ? `<div class="field" data-u="31"><div class="grow"><label>Name</label><input value="${a.def}" aria-label="Name"></div></div>` : '';
    const driveOpts = id === 'drive' ? `<div class="card flush sheet-list">
        ${S.syncOn ? pick(true, 'toast', 'Wie Synchronisation', `<span class="avatar a2">${icon('sync')}</span>`, 'Wie Synchronisation', 'Heimserver') : ''}
        ${pick(!S.syncOn, 'toast', 'Eigener Server', `<span class="avatar a1">${icon('server')}</span>`, 'Eigener Server', 'SFTP, FTPS oder PHP-Modul')}</div>` : '';
    const mailNote = id === 'mail' ? '<p class="hint-text">Danach fragt GhulMail nach deinem Konto.</p>' : '';
    return `<h2>${a.name} einrichten</h2><div class="sheet-head">${aico(a, 'lg')}<p class="hint-text">${a.sub}. Es entsteht die Datei <b>${a.def}${a.file}</b> – sie wird wie alles andere synchronisiert.</p></div>
      ${nameField}${driveOpts}${mailNote}
      <button class="btn primary block" data-u="24" data-act="area-enable" data-arg="${id}">${icon('check')} Einrichten</button>`;
  }
  return `<h2>${a.name}</h2><div class="card flush sheet-list">
    <button class="row plain" data-act="set-page" data-arg="m-${id}"><span class="grow"><span class="t">Einstellungen</span></span>${icon('chev', 'chev')}</button>
    <button class="row plain" data-act="area-hide" data-arg="${id}"><span class="grow"><span class="t">${st.hidden ? 'Auf diesem Gerät einblenden' : 'Auf diesem Gerät ausblenden'}</span><span class="s">Nur Anzeige – die Datei bleibt</span></span>${icon(st.hidden ? 'eye' : 'eyeoff')}</button>
    <button class="row plain" data-act="area-remove" data-arg="${id}"><span class="grow"><span class="t" data-u="34">Bereich entfernen</span><span class="s">Auf allen Geräten, mit Rückfrage</span></span>${icon('trash')}</button></div>`;
}

function removeSheet(id) {
  const a = areaOf(id);
  return `<h2>${a.name} entfernen?</h2>
    <div class="banner warn-banner">${icon('alert')}<span>Die Datei <b>${a.def}${a.file}</b> wird auf allen Geräten gelöscht – samt Inhalt und Einstellungen.</span></div>
    <p class="hint-text">Tipp: Exportiere vorher, was du behalten willst. Willst du den Bereich nur hier nicht sehen, wähle „Auf diesem Gerät ausblenden“.</p>
    <div class="btn-row" data-u="24"><button class="btn outline" data-act="close-sheet">Abbrechen</button><button class="btn danger-fill" data-act="area-remove-ok" data-arg="${id}">${icon('trash')} Entfernen</button></div>`;
}

function codeSheet() {
  return `<h2>Neuen Wiederherstellungscode erzeugen</h2><p class="hint-text">Der bisherige Code wird ungültig. Schreib den neuen auf oder drucke das Notfallblatt.</p>
    <div class="codebox">${['P9WD', 'X4KM', 'T7RA', 'N2HE', 'Q5ZB', 'G8VL', 'D3SC', 'F6YJ'].map((g) => `<span>${g}</span>`).join('')}</div>
    <div class="btn-row" data-u="24"><button class="btn outline" data-act="toast" data-arg="Notfallblatt wird gedruckt">${icon('filetext')} Drucken</button><button class="btn primary" data-act="code-done">${icon('check')} Notiert</button></div>`;
}

function driveStorageSheet() {
  return `<h2>Speicher von GhulDrive</h2><div class="card flush sheet-list">
    ${pick(S.driveMode === 'sync', 'drive-mode', 'sync', `<span class="avatar a2">${icon('sync')}</span>`, 'Wie Synchronisation', 'Heimserver · nur ein Verweis, keine Kopie der Zugangsdaten')}
    ${pick(S.driveMode === 'own', 'drive-mode', 'own', `<span class="avatar a1">${icon('server')}</span>`, 'Eigener Server', 'Webhosting (FTPS) · Zugang liegt in der .ghldrv-Datei')}</div>
    <p class="hint-text">Beim Wechsel zieht GhulSuite die verschlüsselten Dateien um. Wird der Sync-Server entfernt, fragt GhulSuite, was mit dem Drive geschehen soll.</p>`;
}

// Replace the settings structure: sync only on the server page, areas page, two-group area pages.
{
  const serverGroup = SETTINGS.find(([g]) => g === 'Server und Synchronisation');
  serverGroup[0] = 'Synchronisation';
  serverGroup[1] = [{
    id: 'sync', icon: 'sync', cls: 'i-teal', title: 'Synchronisation', sub: 'Heimserver · automatisch · vor 2 Min.',
    page: () => (S.syncOn
      ? `<div class="card flush"><div class="row"><span class="avatar a1">${icon('server')}</span><span class="grow"><span class="t">Heimserver</span><span class="s">SFTP · max@nas.example.org · SSH-Schlüssel</span></span>${icon('more')}</div>
          ${toggle('autosync', 'Automatisch', 'Nach dem Entsperren und bei jeder Änderung – nur auf diesem Gerät')}</div>
        <div class="card"><h3>Zuletzt synchronisiert: vor 2 Min.</h3><p>${AREAS.filter((a) => S.areas[a.id].on).length} Bereichsdateien + Suite · keine Konflikte</p></div>
        <button class="btn primary block" data-act="toast" data-arg="Synchronisiert">${icon('sync')} Jetzt synchronisieren</button>
        <p class="hint-text" data-u="33">GhulDrive nutzt diesen Server mit („wie Synchronisation“). Eigene Server für GhulDrive stellst du unter GhulDrive ein.</p>`
      : `<div class="card">${serverForm('set-sync')}</div>${srv('set-sync').ok ? `<button class="btn primary block" data-act="sync-on">${icon('check')} Synchronisation einschalten</button>` : ''}`),
  }];
  const areaGroup = SETTINGS.find(([g]) => g === 'Bereiche');
  areaGroup[1].unshift({ id: 'areas', icon: 'grid', cls: 'i-gray', title: 'Bereiche verwalten', sub: 'Einrichten, ausblenden, entfernen', page: areasPage });
  for (const it of areaGroup[1]) if (AREA_PAGES[it.id]) it.page = AREA_PAGES[it.id];
  const lookItem = SETTINGS.flatMap(([, items]) => items).find((x) => x.id === 'look');
  const baseLook = lookItem.page;
  lookItem.page = () => baseLook().replace(/<button class="row plain" data-act="toast" data-arg="Bereiche in der Leiste[^"]*">[\s\S]*?<\/button>/, link('Bereiche in der Leiste', 'Optionen → Bereiche verwalten', 'set-page', 'areas'));
}
const baseSettingsScreen = settingsScreen;
settingsScreen = function () { // eslint-disable-line no-global-assign
  // Only areas that exist have a settings page in the overview.
  const html = baseSettingsScreen();
  return AREAS.reduce((h, a) => (S.areas[a.id].on ? h : h.replace(new RegExp(`<button class="row set-row" data-act="set-page" data-arg="m-${a.id}">[\\s\\S]*?</button>`), '')), html);
};

/** Re-renders the app, or the drive server sheet when it is open. */
function rerender() {
  if ($('device').classList.contains('sheet-open') && $('sheet').innerHTML.includes('Eigener Server für GhulDrive')) sheet(driveServerSheet());
  else render(true);
}

// --- actions -------------------------------------------------------------------------------------
Object.assign(ACT, {
  'start-new': () => { S.phase = 'setup'; S.setup = newSetup(); S.srv = {}; closeSheet(); render(); },
  'wz-back': () => { if (S.setup.step > 1) S.setup.step--; else S.phase = 'start'; render(); },
  'wz-next': () => {
    if (S.setup.step === 4 && S.setup.sync === 'server' && !srv('sync').ok) { toast('Bitte erst den Server fertig einrichten'); return; }
    if (S.setup.step === 5 && S.setup.areas.drive && S.setup.driveMode === 'own' && !srv('drive').ok) { toast('Bitte erst den Server für GhulDrive einrichten'); return; }
    if (S.setup.step === 5 && !Object.values(S.setup.areas).some(Boolean)) { toast('Mindestens einen Bereich wählen'); return; }
    S.setup.step++; render();
  },
  'wz-set': (a) => {
    const [k, v] = a.split(':');
    S.setup[k] = v;
    render(true);
  },
  'srv-type': (a) => { const [k, t] = a.split('|'); Object.assign(srv(k), { type: t, folder: '', uploaded: false, ok: false }); rerender(); },
  'srv-folder': (a) => { S.folderFor = a; sheet(folderSheet(a)); },
  'srv-pick': (a) => { const [k, d] = a.split('|'); Object.assign(srv(k), { folder: d, uploaded: false, ok: false }); closeSheet(); rerender(); },
  'srv-upload': (a) => { srv(a).uploaded = true; rerender(); toast('Modul hochgeladen · eigener Schlüssel erzeugt'); },
  'srv-test': (a) => { srv(a).ok = true; rerender(); toast(srv(a).type === 'php' ? 'Modul antwortet mit dem richtigen Schlüssel · nur https' : 'Verbunden · Server-Fingerabdruck bestätigt'); },
  'drive-own-done': () => { if (!srv('drive2').ok) { toast('Bitte erst den Server fertig einrichten'); return; } S.driveSetup = true; S.driveMode = 'own'; closeSheet(); render(); toast('GhulDrive eingerichtet'); },
  'wz-area': (a) => {
    const w = S.setup;
    w.areas[a] = !w.areas[a];
    if (a === 'drive' && w.areas.drive) w.driveMode = w.sync === 'server' && srv('sync').ok ? 'sync' : 'later';
    render(true);
  },
  'wz-confirm': () => { S.setup.confirmed = !S.setup.confirmed; render(true); },
  'wz-finish': () => {
    const w = S.setup;
    for (const a of AREAS) S.areas[a.id] = { on: !!w.areas[a.id], hidden: false };
    S.syncOn = w.sync === 'server';
    S.driveSetup = w.areas.drive && w.driveMode !== 'later';
    S.driveMode = w.driveMode === 'own' ? 'own' : 'sync';
    S.mailSetup = false;
    S.codeConfirmed = w.confirmed;
    S.phase = 'app'; S.stack = []; S.tab = visibleAreas()[0] || 'settings';
    S.setup = null;
    render();
    toast(`Suite eingerichtet · ${AREAS.filter((a) => S.areas[a.id].on).length} Bereiche`);
  },
  'drive-setup': (a) => {
    if (a === 'own') { sheet(driveServerSheet()); return; }
    S.driveSetup = true; S.driveMode = a; render(); toast('GhulDrive nutzt den Sync-Server');
  },
  'mail-setup': () => { S.mailSetup = true; render(); toast('Konto geprüft und hinzugefügt'); },
  'code-sheet': () => sheet(codeSheet()),
  'code-done': () => { S.codeConfirmed = true; closeSheet(); render(true); toast('Wiederherstellungscode bestätigt'); },
  'area-sheet': (a) => sheet(areaSheet(a)),
  'area-enable': (a) => {
    S.areas[a] = { on: true, hidden: false };
    if (a === 'drive') { S.driveSetup = true; S.driveMode = S.syncOn ? 'sync' : 'own'; }
    if (a === 'mail') S.mailSetup = false;
    closeSheet(); render(true); toast(`${areaOf(a).name} eingerichtet`);
  },
  'area-hide': (a) => { S.areas[a].hidden = !S.areas[a].hidden; closeSheet(); if (current()?.id === `m-${a}`) pop(); else render(true); toast(S.areas[a].hidden ? `${areaOf(a).name} auf diesem Gerät ausgeblendet` : `${areaOf(a).name} wieder eingeblendet`); },
  'area-remove': (a) => sheet(removeSheet(a)),
  'area-remove-ok': (a) => { S.areas[a] = { on: false, hidden: false }; closeSheet(); if (current()?.id === `m-${a}`) pop(); else render(true); toast(`${areaOf(a).name} entfernt`); },
  'drive-storage': () => sheet(driveStorageSheet()),
  'sync-on': () => { S.syncOn = true; render(true); toast('Synchronisation eingeschaltet'); },
  'drive-mode': (a) => { S.driveMode = a; closeSheet(); render(true); toast(a === 'own' ? 'Dateien werden auf den eigenen Server umgezogen' : 'GhulDrive nutzt jetzt den Sync-Server'); },
  'demo': (a) => {
    closeSheet();
    S.phase = 'app'; S.stack = [];
    if (a === 'drive') { S.areas.drive = { on: true, hidden: false }; S.driveSetup = false; S.tab = 'drive'; }
    if (a === 'mail') { S.areas.mail = { on: true, hidden: false }; S.mailSetup = false; S.tab = 'mail'; }
    if (a === 'code') { S.areas.pass = { on: true, hidden: false }; S.codeConfirmed = false; S.tab = 'pass'; }
    if (a === 'areas') { S.tab = 'settings'; S.stack = [{ screen: 'set-page', id: 'areas' }]; }
    if (a === 'reset') {
      for (const x of AREAS) S.areas[x.id] = { on: true, hidden: false };
      Object.assign(S, { syncOn: true, driveSetup: true, driveMode: 'sync', mailSetup: true, codeConfirmed: true, tab: 'pass' });
    }
    render();
  },
});

// --- side panel ----------------------------------------------------------------------------------
{
  const jump = $('jump');
  const add = (label, act, arg) => {
    const b = document.createElement('button');
    b.className = 'chip'; b.dataset.act = act; if (arg) b.dataset.arg = arg; b.textContent = label;
    jump.appendChild(b);
  };
  add('Neu einrichten (Assistent)', 'start-new');
  add('Bereiche verwalten', 'demo', 'areas');
  add('Drive nicht eingerichtet', 'demo', 'drive');
  add('Mail ohne Konto', 'demo', 'mail');
  add('Code nicht bestätigt', 'demo', 'code');
  add('Alles zurücksetzen', 'demo', 'reset');
  const li = document.createElement('li');
  li.innerHTML = '<b>Ein Bereich, eine Datei (#24)</b><span>Assistent klärt Passwort, Faktor, Speicherort, Synchronisation, Bereiche, Sperren, Entsperren und Notfallcode. Bereichsseiten trennen „alle Geräte“ und „nur dieses Gerät“.</span>';
  $('proposals').prepend(li);
}
render();
