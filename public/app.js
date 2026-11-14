'use strict';

// GhulPass PRF-Test: prüft, ob der Security Key über WebAuthn-PRF (FIDO2 hmac-secret)
// einen deterministischen geheimen Wert liefert. Alles läuft lokal im Browser.

const STORE_KEY = 'ghulpass-prf-test-v1';
const $ = (id) => document.getElementById(id);
const lines = [];

const toHex = (buf) => Array.from(new Uint8Array(buf), (b) => b.toString(16).padStart(2, '0')).join('');
const toB64u = (buf) => btoa(String.fromCharCode(...new Uint8Array(buf)))
  .replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
const fromB64u = (s) => {
  const b = atob(s.replace(/-/g, '+').replace(/_/g, '/') + '==='.slice((s.length + 3) % 4));
  return Uint8Array.from(b, (c) => c.charCodeAt(0));
};
const random = (n) => crypto.getRandomValues(new Uint8Array(n));

function loadState() {
  try { return JSON.parse(localStorage.getItem(STORE_KEY)) || {}; } catch { return {}; }
}
function saveState(state) {
  try { localStorage.setItem(STORE_KEY, JSON.stringify(state)); } catch { /* privat/blockiert */ }
}

function log(msg) {
  const t = new Date().toLocaleTimeString('de-DE');
  lines.push(`${t}  ${msg}`);
  $('log').textContent = lines.join('\n');
}

function showResult(kind, text) {
  const el = $('result');
  el.hidden = false;
  el.className = `result ${kind}`;
  el.textContent = text;
}

// Nur ein kurzer Fingerabdruck des PRF-Werts wird angezeigt/gespeichert, nie der Wert selbst.
async function fingerprint(buf) {
  return toHex(await crypto.subtle.digest('SHA-256', buf)).slice(0, 16);
}

async function prfSalt() {
  return new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode('GhulPass PRF-Test v1')));
}

function errText(e) {
  return e && e.name ? `${e.name}: ${e.message}` : String(e);
}

async function renderEnv() {
  const rows = [];
  const add = (k, v, cls = '') => rows.push(`<tr><td>${k}</td><td class="${cls}">${v}</td></tr>`);
  const yes = (b) => (b ? ['ja', 'ok'] : ['nein', 'bad']);

  add('Sicherer Kontext (HTTPS)', ...yes(window.isSecureContext));
  const hasWebAuthn = typeof window.PublicKeyCredential === 'function';
  add('WebAuthn verfügbar', ...yes(hasWebAuthn));

  let prfCap = 'unbekannt (Browser meldet es nicht)';
  let prfCls = 'warn';
  if (hasWebAuthn && typeof PublicKeyCredential.getClientCapabilities === 'function') {
    try {
      const caps = await PublicKeyCredential.getClientCapabilities();
      if ('extension:prf' in caps) {
        prfCap = caps['extension:prf'] ? 'ja' : 'nein';
        prfCls = caps['extension:prf'] ? 'ok' : 'bad';
      }
    } catch (e) { prfCap = `Fehler: ${errText(e)}`; }
  }
  add('PRF laut Browser', prfCap, prfCls);
  add('Online', navigator.onLine ? 'ja' : 'nein (offline)', navigator.onLine ? '' : 'warn');
  const swReady = !!(navigator.serviceWorker && navigator.serviceWorker.controller);
  add('Offline-Cache aktiv', swReady ? 'ja' : 'noch nicht (Seite einmal neu laden)', swReady ? 'ok' : 'warn');
  const state = loadState();
  add('Registrierter Key', state.credId ? `ja (${state.credId.slice(0, 10)}…)` : 'nein');
  add('Browser', navigator.userAgent.replace(/</g, '&lt;'));
  $('env').innerHTML = rows.join('');
  return { prfCap };
}

async function register() {
  $('btn-register').disabled = true;
  try {
    log('Registrierung gestartet, Key einstecken und berühren …');
    const cred = await navigator.credentials.create({
      publicKey: {
        rp: { name: 'GhulPass PRF-Test' },
        user: { id: random(16), name: 'ghulpass-test', displayName: 'GhulPass Test' },
        challenge: random(32),
        pubKeyCredParams: [
          { type: 'public-key', alg: -7 },   // ES256
          { type: 'public-key', alg: -8 },   // EdDSA
          { type: 'public-key', alg: -257 }, // RS256
        ],
        authenticatorSelection: {
          authenticatorAttachment: 'cross-platform',
          residentKey: 'discouraged',
          requireResidentKey: false,
          userVerification: 'preferred',
        },
        attestation: 'none',
        timeout: 120000,
        extensions: { prf: {} },
      },
    });
    const ext = cred.getClientExtensionResults();
    const transports = typeof cred.response.getTransports === 'function' ? cred.response.getTransports() : [];
    const state = { credId: toB64u(cred.rawId), transports, fps: {}, prfEnabled: ext.prf ? ext.prf.enabled : null };
    saveState(state);
    log(`Registriert. Credential-ID ${state.credId.slice(0, 10)}…, Transporte: ${transports.join(', ') || 'keine Angabe'}`);
    if (!ext.prf) {
      log('PRF bei Registrierung: Browser hat keine PRF-Antwort geliefert.');
      showResult('warn', 'Registriert, aber der Browser meldet keine PRF-Unterstützung. Trotzdem Schritt 2 versuchen.');
    } else if (ext.prf.enabled === true) {
      log('PRF bei Registrierung: aktiviert ✓');
      showResult('ok', 'Registriert, PRF ist aktiviert. Jetzt Schritt 2 ausführen.');
    } else {
      log(`PRF bei Registrierung: enabled = ${ext.prf.enabled}`);
      showResult('warn', 'Registriert, PRF-Status unklar. Schritt 2 ausführen.');
    }
  } catch (e) {
    log(`Registrierung fehlgeschlagen: ${errText(e)}`);
    showResult('bad', `Registrierung fehlgeschlagen: ${errText(e)}`);
  } finally {
    $('btn-register').disabled = false;
    renderEnv();
  }
}

async function testPrf() {
  const state = loadState();
  if (!state.credId) {
    showResult('warn', 'Bitte zuerst Schritt 1 ausführen.');
    return;
  }
  $('btn-test').disabled = true;
  try {
    const online = navigator.onLine;
    log(`PRF-Abfrage gestartet (${online ? 'online' : 'OFFLINE'}), Key berühren …`);
    const assertion = await navigator.credentials.get({
      publicKey: {
        challenge: random(32),
        allowCredentials: [{
          type: 'public-key',
          id: fromB64u(state.credId),
          transports: state.transports && state.transports.length ? state.transports : ['usb', 'nfc'],
        }],
        userVerification: 'preferred',
        timeout: 120000,
        extensions: { prf: { eval: { first: await prfSalt() } } },
      },
    });
    const flags = new Uint8Array(assertion.response.authenticatorData)[32];
    const uv = (flags & 0x04) !== 0;
    const ext = assertion.getClientExtensionResults();
    const first = ext.prf && ext.prf.results && ext.prf.results.first;
    if (!first) {
      log('Keine PRF-Ausgabe erhalten ✗');
      showResult('bad', 'Der Key hat sich angemeldet, aber KEIN PRF-Wert geliefert. PRF wird hier nicht unterstützt.');
      return;
    }
    // hmac-secret liefert mit und ohne PIN-Prüfung (UV) unterschiedliche Werte, getrennt vergleichen.
    const slot = uv ? 'mitPIN' : 'ohnePIN';
    const fp = await fingerprint(first);
    const len = first.byteLength;
    state.fps = state.fps || {};
    const prev = state.fps[slot];
    log(`PRF-Wert erhalten ✓ (${len} Byte, PIN geprüft: ${uv ? 'ja' : 'nein'}, Fingerabdruck ${fp})`);
    if (!prev) {
      state.fps[slot] = fp;
      saveState(state);
      showResult('ok', `PRF funktioniert ✓ (${online ? 'online' : 'offline'}). Noch einmal ausführen, um zu prüfen, ob der Wert gleich bleibt.`);
    } else if (prev === fp) {
      log(`Wert identisch mit vorheriger Abfrage ✓ (${online ? 'online' : 'offline'})`);
      showResult('ok', `PRF funktioniert und ist deterministisch ✓ (${online ? 'online' : 'offline'}). Dieser Key ist geeignet.`);
    } else {
      log(`ABWEICHUNG: vorher ${prev}, jetzt ${fp} ✗`);
      showResult('bad', 'PRF-Wert hat sich geändert, nicht geeignet. Bitte Bericht kopieren und schicken.');
    }
  } catch (e) {
    log(`PRF-Abfrage fehlgeschlagen: ${errText(e)}`);
    showResult('bad', `Fehlgeschlagen: ${errText(e)}`);
  } finally {
    $('btn-test').disabled = false;
    renderEnv();
  }
}

async function copyReport() {
  const env = Array.from($('env').querySelectorAll('tr'), (tr) => {
    const [k, v] = tr.querySelectorAll('td');
    return `${k.textContent}: ${v.textContent}`;
  }).join('\n');
  const text = `GhulPass PRF-Test\n\n${env}\n\nProtokoll:\n${lines.join('\n')}`;
  try {
    await navigator.clipboard.writeText(text);
    showResult('ok', 'Bericht in die Zwischenablage kopiert.');
  } catch {
    showResult('warn', 'Kopieren nicht möglich, bitte das Protokoll manuell markieren.');
  }
}

function reset() {
  try { localStorage.removeItem(STORE_KEY); } catch { /* egal */ }
  lines.length = 0;
  $('log').textContent = '';
  $('result').hidden = true;
  renderEnv();
}

$('btn-register').addEventListener('click', register);
$('btn-test').addEventListener('click', testPrf);
$('btn-copy').addEventListener('click', copyReport);
$('btn-reset').addEventListener('click', reset);
window.addEventListener('online', renderEnv);
window.addEventListener('offline', renderEnv);

if ('serviceWorker' in navigator) {
  navigator.serviceWorker.register('sw.js').then(() => navigator.serviceWorker.ready).then(renderEnv)
    .catch((e) => log(`Offline-Cache konnte nicht aktiviert werden: ${errText(e)}`));
  navigator.serviceWorker.addEventListener('controllerchange', renderEnv);
}
renderEnv().then(({ prfCap }) => log(`Seite geladen. PRF laut Browser: ${prfCap}`));
