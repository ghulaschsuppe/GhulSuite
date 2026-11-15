// SPDX-License-Identifier: AGPL-3.0-or-later
// GhulSuite share page: opens a share link in the browser. The server module hands out an
// encrypted package (names, sizes and keys of the files) and then the files' encrypted blocks;
// everything is decrypted here, block by block. Nothing secret ever leaves this page.
// Formats: ghul-core/src/share.rs (package) and ghul-core/src/app/drive.rs (blocks).
'use strict';
(function () {
  const de = (navigator.language || 'de').toLowerCase().startsWith('de');
  const T = de ? {
    loading: 'Wird geladen …', gone: 'Diese Freigabe gibt es nicht mehr. Sie ist abgelaufen, wurde beendet oder schon abgerufen.',
    bad: 'Der Link ist unvollständig. Bitte den ganzen Link kopieren.', wrong: 'Falsches Passwort oder der Link ist beschädigt.',
    pw: 'Diese Freigabe ist zusätzlich mit einem Passwort geschützt.', ready: 'Bereit zum Herunterladen.',
    dl: 'Herunterladen', all: 'Alle herunterladen', stop: 'Abbrechen', again: 'Erneut versuchen', done: 'Fertig',
    stopped: 'Abgebrochen', failed: 'Unterbrochen. Erneut versuchen setzt fort.',
    old: 'Dieser Browser ist zu alt für verschlüsselte Freigaben.', net: 'Die Freigabe konnte nicht geladen werden.',
    files: (n) => n === 1 ? '1 Datei' : n + ' Dateien',
    big: 'Große Dateien schreibt dieser Browser erst in den Arbeitsspeicher. Bei sehr großen Dateien (mehrere GB) besser Chrome, Edge oder Firefox am Computer nutzen.',
  } : {
    loading: 'Loading …', gone: 'This share no longer exists. It expired, was ended or was already downloaded.',
    bad: 'The link is incomplete. Please copy the whole link.', wrong: 'Wrong password, or the link is damaged.',
    pw: 'This share is additionally protected with a password.', ready: 'Ready to download.',
    dl: 'Download', all: 'Download all', stop: 'Cancel', again: 'Try again', done: 'Done',
    stopped: 'Cancelled', failed: 'Interrupted. Try again continues.',
    old: 'This browser is too old for encrypted shares.', net: 'The share could not be loaded.',
    files: (n) => n === 1 ? '1 file' : n + ' files',
    big: 'This browser keeps large files in memory first. For very large files (several GB), better use Chrome, Edge or Firefox on a computer.',
  };
  const $ = (id) => document.getElementById(id);
  const msg = (text, error) => { $('msg').textContent = text; $('msg').className = error ? 'error' : 'muted'; };
  if (!de) {
    document.documentElement.lang = 'en';
    $('title').textContent = 'Shared files';
    $('pwlabel').textContent = 'Password';
    $('pwbtn').textContent = 'Open';
    $('note').textContent = 'End-to-end encrypted. The key is only in the link and is never sent to the server.';
  }
  $('all').textContent = T.all;
  msg(T.loading);
  if (!window.crypto || !crypto.subtle) { msg(T.old, true); return; }

  const frag = decodeURIComponent(location.hash.slice(1));
  const m = /^([0-9a-f]{32})\.([A-Za-z0-9_-]{43})$/.exec(frag);
  if (!m) { msg(T.bad, true); return; }
  const id = m[1];
  const linkKey = b64url(m[2]);
  // Remove the key from the address bar (and so from history and screenshots of the page).
  try { history.replaceState(null, '', location.pathname + location.search); } catch (e) { /* ignore */ }

  const opts = { cache: 'no-store', credentials: 'omit', referrerPolicy: 'no-referrer' };
  let data, blocks, ticket;
  fetch('?s=' + id, opts)
    .then((r) => {
      if (r.status === 404 || r.status === 410) throw new Error('gone');
      if (!r.ok) throw new Error('net');
      return r.json();
    })
    .then((j) => {
      data = b64(j.package);
      blocks = j.blocks;
      ticket = j.ticket;
      return start();
    })
    .catch((e) => msg(e.message === 'gone' ? T.gone : T.net, true));

  function b64url(s) {
    const bin = atob(s.replace(/-/g, '+').replace(/_/g, '/') + '==='.slice((s.length + 3) % 4));
    return Uint8Array.from(bin, (c) => c.charCodeAt(0));
  }
  function b64(s) { return Uint8Array.from(atob(s), (c) => c.charCodeAt(0)); }
  const u32 = (a, o) => ((a[o] << 24) >>> 0) + (a[o + 1] << 16) + (a[o + 2] << 8) + a[o + 3];
  const enc = new TextEncoder();

  // --- The package (share.rs): header, then AES-256-GCM chunks ---------------------------------
  const MAX_ITERATIONS = 10000000;
  let hdr, key;
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
      const pwKey = await crypto.subtle.importKey('raw', enc.encode(password.normalize('NFC')), 'PBKDF2', false, ['deriveBits']);
      const bits = new Uint8Array(await crypto.subtle.deriveBits({ name: 'PBKDF2', hash: 'SHA-256', salt, iterations }, pwKey, 256));
      ikm = new Uint8Array(64);
      ikm.set(linkKey, 0);
      ikm.set(bits, 32);
    }
    key = await hkdfKey(ikm, 'ghul/v1/share');
    const size = u32(hdr, 12) + 16;
    const metaLen = u32(hdr, 44);
    const body = data.length - 48 - metaLen;
    const meta = JSON.parse(new TextDecoder().decode(await chunk(0, data.subarray(48, 48 + metaLen), body === 0)));
    const parts = [];
    let pos = 48 + metaLen;
    for (let i = 1; pos < data.length; i++) {
      const end = Math.min(pos + size, data.length);
      parts.push(new Uint8Array(await chunk(i, data.subarray(pos, end), end === data.length)));
      pos = end;
    }
    if (meta.type !== 'application/x-ghulsuite-share') throw new Error('bad');
    const list = JSON.parse(new TextDecoder().decode(concat(parts))).files;
    $('pwform').hidden = true;
    show(meta, list);
  }
  async function hkdfKey(ikm, info) {
    const base = await crypto.subtle.importKey('raw', ikm, 'HKDF', false, ['deriveKey']);
    return crypto.subtle.deriveKey({ name: 'HKDF', hash: 'SHA-256', salt: new Uint8Array(0), info: enc.encode(info) },
      base, { name: 'AES-GCM', length: 256 }, false, ['decrypt']);
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
  function concat(parts) {
    const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
    let o = 0;
    for (const p of parts) { out.set(p, o); o += p.length; }
    return out;
  }

  // --- The files: blocks of GhulDrive (drive.rs), fetched one by one ---------------------------
  const canStream = typeof window.showSaveFilePicker === 'function';
  function show(meta, list) {
    $('title').textContent = meta.name;
    $('file').hidden = false;
    $('name').textContent = T.files(list.length);
    $('size').textContent = size(list.reduce((n, f) => n + f.size, 0));
    msg(T.ready);
    const ul = $('files');
    ul.hidden = false;
    list.forEach((f, i) => {
      const li = document.createElement('li');
      const name = document.createElement('div');
      name.className = 'fname';
      name.textContent = f.name;
      const row = document.createElement('div');
      row.className = 'row';
      const info = document.createElement('p');
      info.className = 'muted';
      info.textContent = size(f.size);
      const btn = document.createElement('button');
      btn.textContent = T.dl;
      btn.id = 'dl' + i;
      const bar = document.createElement('progress');
      bar.max = Math.max(1, blocks[i].length);
      bar.value = 0;
      bar.hidden = true;
      row.append(info, btn);
      li.append(name, row, bar);
      ul.append(li);
      f.ui = { btn, bar, info };
      f.index = i;
      f.next = 0;
      btn.onclick = () => (f.running ? (f.stop = true) : download(f));
    });
    if (list.length > 1) {
      $('all').hidden = false;
      $('all').onclick = async () => { for (const f of list) if (!f.done) await download(f); };
    }
    if (!canStream && list.some((f) => f.size > 1024 * 1024 * 1024)) {
      $('hint').hidden = false;
      $('hint').textContent = T.big;
    }
  }
  async function fileKey(f) {
    if (!f.bk) {
      const raw = b64url(f.key);
      f.bk = await hkdfKey(raw, 'ghul/v2/drive/block');
    }
    return f.bk;
  }
  async function block(f, n) {
    const ids = blocks[f.index];
    for (let attempt = 0; ; attempt++) {
      try {
        const r = await fetch('?s=' + id + '&t=' + ticket + '&f=' + f.index + '&b=' + n, opts);
        if (r.status === 404 || r.status === 410 || r.status === 403) throw new Error('gone');
        if (!r.ok) throw new Error('net');
        const enc8 = new Uint8Array(await r.arrayBuffer());
        if (enc8.length < 36 || new TextDecoder().decode(enc8.subarray(0, 8)) !== 'GHULBLK2') throw new Error('bad');
        const aad = concat([enc8.subarray(0, 8), new TextEncoder().encode(ids[n])]);
        const plain = await crypto.subtle.decrypt({ name: 'AES-GCM', iv: enc8.subarray(8, 20), additionalData: aad, tagLength: 128 },
          await fileKey(f), enc8.subarray(20));
        return new Uint8Array(plain);
      } catch (e) {
        if (e.message === 'gone' || e.message === 'bad' || e.name === 'OperationError' || attempt >= 2) throw e;
        await new Promise((res) => setTimeout(res, 1000 * (attempt + 1)));
      }
    }
  }
  async function download(f) {
    const ui = f.ui;
    f.running = true;
    f.stop = false;
    ui.btn.textContent = T.stop;
    ui.btn.className = 'stop';
    ui.bar.hidden = false;
    let sink = f.sink;
    try {
      if (!sink) {
        if (canStream) {
          const handle = await window.showSaveFilePicker({ suggestedName: f.name.split('/').pop() });
          sink = { writable: await handle.createWritable(), parts: null };
        } else {
          sink = { writable: null, parts: [] };
        }
        f.sink = sink;
      }
      const ids = blocks[f.index];
      for (; f.next < ids.length; f.next++) {
        if (f.stop) throw new Error('stopped');
        const plain = await block(f, f.next);
        if (sink.writable) await sink.writable.write(plain); else sink.parts.push(plain);
        ui.bar.value = f.next + 1;
        ui.info.textContent = size(Math.min(f.size, (f.next + 1) * 4194304)) + ' / ' + size(f.size);
      }
      if (sink.writable) {
        await sink.writable.close();
      } else {
        const blob = new Blob(sink.parts, { type: 'application/octet-stream' });
        const a = document.createElement('a');
        a.href = URL.createObjectURL(blob);
        a.download = f.name.split('/').pop();
        document.body.appendChild(a);
        a.click();
        setTimeout(() => { URL.revokeObjectURL(a.href); a.remove(); }, 30000);
      }
      f.done = true;
      ui.btn.textContent = T.done;
      ui.btn.disabled = true;
      ui.btn.className = '';
      ui.info.textContent = size(f.size);
    } catch (e) {
      if (e && e.name === 'AbortError') { // the save dialog was closed
        ui.btn.textContent = T.dl;
        ui.btn.className = '';
        ui.bar.hidden = true;
      } else if (e && e.message === 'stopped') {
        ui.btn.textContent = T.again;
        ui.btn.className = '';
        ui.info.textContent = T.stopped;
      } else if (e && e.message === 'gone') {
        ui.btn.disabled = true;
        msg(T.gone, true);
      } else if (e && (e.message === 'bad' || e.name === 'OperationError')) {
        ui.btn.disabled = true;
        msg(T.wrong, true);
      } else {
        ui.btn.textContent = T.again;
        ui.btn.className = '';
        ui.info.textContent = T.failed;
      }
    }
    f.running = false;
  }
  function size(n) {
    if (n < 1024) return n + ' B';
    if (n < 1048576) return (n / 1024).toFixed(1) + ' KB';
    if (n < 1073741824) return (n / 1048576).toFixed(1) + ' MB';
    return (n / 1073741824).toFixed(1) + ' GB';
  }
})();
