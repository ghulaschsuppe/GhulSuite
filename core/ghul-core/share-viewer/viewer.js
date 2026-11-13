// SPDX-License-Identifier: AGPL-3.0-or-later
// GhulSuite share viewer: downloads an encrypted package and decrypts it in the browser.
// Format: see ghul-core/src/share.rs. Nothing secret ever leaves this page.
'use strict';
(function () {
  const de = (navigator.language || 'de').toLowerCase().startsWith('de');
  const T = de ? {
    loading: 'Wird geladen …', gone: 'Diese Freigabe gibt es nicht mehr. Sie ist abgelaufen oder wurde beendet.',
    bad: 'Der Link ist unvollständig. Bitte den ganzen Link kopieren.', wrong: 'Falsches Passwort oder der Link ist beschädigt.',
    pw: 'Diese Datei ist zusätzlich mit einem Passwort geschützt.', ready: 'Bereit zum Herunterladen.',
    dl: 'Herunterladen', working: 'Wird entschlüsselt …', done: 'Fertig.', old: 'Dieser Browser ist zu alt für verschlüsselte Freigaben.',
    net: 'Die Datei konnte nicht geladen werden.',
  } : {
    loading: 'Loading …', gone: 'This share no longer exists. It expired or was ended.',
    bad: 'The link is incomplete. Please copy the whole link.', wrong: 'Wrong password, or the link is damaged.',
    pw: 'This file is additionally protected with a password.', ready: 'Ready to download.',
    dl: 'Download', working: 'Decrypting …', done: 'Done.', old: 'This browser is too old for encrypted shares.',
    net: 'The file could not be loaded.',
  };
  const $ = (id) => document.getElementById(id);
  const msg = (text, error) => { $('msg').textContent = text; $('msg').className = error ? 'error' : 'muted'; };
  if (!de) {
    document.documentElement.lang = 'en';
    $('title').textContent = 'Shared file';
    $('pwlabel').textContent = 'Password';
    $('pwbtn').textContent = 'Open';
    $('note').textContent = 'End-to-end encrypted. The key is only in the link and is never sent to the server.';
  }
  $('dl').textContent = T.dl;
  msg(T.loading);
  if (!window.crypto || !crypto.subtle) { msg(T.old, true); return; }

  const frag = decodeURIComponent(location.hash.slice(1));
  const m = /^([0-9a-f]{32})\.([A-Za-z0-9_-]{43})$/.exec(frag);
  if (!m) { msg(T.bad, true); return; }
  const id = m[1];
  const linkKey = b64(m[2]);
  // Remove the key from the address bar (and so from history and screenshots of the page).
  try { history.replaceState(null, '', location.pathname + location.search); } catch (e) { /* ignore */ }

  const url = /\.php$/.test(location.pathname) ? '?s=' + id : 's/' + id;
  let data;
  fetch(url, { cache: 'no-store', credentials: 'omit', referrerPolicy: 'no-referrer' })
    .then((r) => {
      if (r.status === 404 || r.status === 410) throw new Error('gone');
      if (!r.ok) throw new Error('net');
      return r.arrayBuffer();
    })
    .then((buf) => { data = new Uint8Array(buf); return start(); })
    .catch((e) => msg(e.message === 'gone' ? T.gone : T.net, true));

  function b64(s) {
    const bin = atob(s.replace(/-/g, '+').replace(/_/g, '/') + '==='.slice((s.length + 3) % 4));
    return Uint8Array.from(bin, (c) => c.charCodeAt(0));
  }
  const u32 = (a, o) => ((a[o] << 24) >>> 0) + (a[o + 1] << 16) + (a[o + 2] << 8) + a[o + 3];

  const MAX_ITERATIONS = 10000000;
  let hdr, key, meta;
  async function start() {
    const magic = new TextDecoder().decode(data.subarray(0, 8));
    if (data.length < 48 || magic !== 'GHSHARE1' || data[8] !== 1) { msg(T.bad, true); return; }
    hdr = data.subarray(0, 48);
    if (data[9] & 1) {
      msg(T.pw);
      $('pwform').hidden = false;
      $('pwform').addEventListener('submit', async (ev) => {
        ev.preventDefault();
        $('pwbtn').disabled = true;
        try { await unlock($('pw').value); } catch (e) { msg(T.wrong, true); }
        $('pwbtn').disabled = false;
      });
    } else {
      try { await unlock(null); } catch (e) { msg(T.wrong, true); }
    }
  }
  async function unlock(password) {
    let ikm = linkKey;
    if (password !== null) {
      const iterations = u32(hdr, 16);
      // A manipulated package must not freeze the browser.
      if (iterations < 1 || iterations > MAX_ITERATIONS) throw new Error('bad');
      const salt = hdr.subarray(20, 36);
      const pwKey = await crypto.subtle.importKey('raw', new TextEncoder().encode(password.normalize('NFC')), 'PBKDF2', false, ['deriveBits']);
      const bits = new Uint8Array(await crypto.subtle.deriveBits({ name: 'PBKDF2', hash: 'SHA-256', salt, iterations }, pwKey, 256));
      ikm = new Uint8Array(64);
      ikm.set(linkKey, 0);
      ikm.set(bits, 32);
    }
    const base = await crypto.subtle.importKey('raw', ikm, 'HKDF', false, ['deriveKey']);
    key = await crypto.subtle.deriveKey({ name: 'HKDF', hash: 'SHA-256', salt: new Uint8Array(0), info: new TextEncoder().encode('ghul/v1/share') },
      base, { name: 'AES-GCM', length: 256 }, false, ['decrypt']);
    const metaLen = u32(hdr, 44);
    const body = data.length - 48 - metaLen;
    const plain = await chunk(0, data.subarray(48, 48 + metaLen), body === 0);
    meta = JSON.parse(new TextDecoder().decode(plain));
    $('pwform').hidden = true;
    // The expiry is inside the encrypted package: also enforced where the server cannot do it.
    if (meta.expires > 0 && Date.now() > meta.expires) {
      data = null;
      msg(T.gone, true);
      return;
    }
    $('file').hidden = false;
    $('name').textContent = meta.name;
    $('size').textContent = size(meta.size);
    msg(T.ready);
    $('dl').hidden = false;
    $('dl').onclick = download;
  }
  function chunk(i, ct, last) {
    const iv = new Uint8Array(12);
    iv.set(hdr.subarray(36, 44), 0);
    new DataView(iv.buffer).setUint32(8, i);
    const aad = new Uint8Array(49);
    aad.set(hdr, 0);
    aad[48] = last ? 1 : 0;
    return crypto.subtle.decrypt({ name: 'AES-GCM', iv, additionalData: aad, tagLength: 128 }, key, ct);
  }
  async function download() {
    $('dl').disabled = true;
    msg(T.working);
    try {
      const size = u32(hdr, 12) + 16;
      const metaLen = u32(hdr, 44);
      const parts = [];
      let pos = 48 + metaLen;
      for (let i = 1; pos < data.length; i++) {
        const end = Math.min(pos + size, data.length);
        parts.push(await chunk(i, data.subarray(pos, end), end === data.length));
        pos = end;
      }
      const blob = new Blob(parts, { type: 'application/octet-stream' });
      const a = document.createElement('a');
      a.href = URL.createObjectURL(blob);
      a.download = meta.name;
      document.body.appendChild(a);
      a.click();
      setTimeout(() => { URL.revokeObjectURL(a.href); a.remove(); }, 30000);
      msg(T.done);
    } catch (e) {
      msg(T.wrong, true);
    }
    $('dl').disabled = false;
  }
  function size(n) {
    if (n < 1024) return n + ' B';
    if (n < 1048576) return (n / 1024).toFixed(1) + ' KB';
    if (n < 1073741824) return (n / 1048576).toFixed(1) + ' MB';
    return (n / 1073741824).toFixed(1) + ' GB';
  }
})();
