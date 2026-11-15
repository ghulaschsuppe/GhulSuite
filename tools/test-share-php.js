// SPDX-License-Identifier: AGPL-3.0-or-later
// Browser test of share links: real links created by ghul-core, handed out by the server module
// under `php -S`, opened in Chromium.
//
//   D=$(mktemp -d); GHUL_PHP_FIXTURE=$D cargo test -p ghul-core php_browser_fixture &
//   node tools/test-share-php.js $D; wait
//
// Checks the share page (CSP header, key removed from the address bar), decryption block by
// block in the browser both straight to disk (save dialog of Chromium, stood in for here) and
// through memory (browsers without it), a folder as single files, the extra password, one-time
// links, expired links, and that the module refuses what it must. Writes `done` so the Rust side
// can check the counters.
'use strict';
const fs = require('fs');
const path = require('path');
const { chromium } = require('playwright');

const dir = process.argv[2];
const linksFile = path.join(dir, 'links.txt');

(async () => {
  for (let i = 0; i < 900 && !fs.existsSync(linksFile); i++) await new Promise((r) => setTimeout(r, 200));
  const links = Object.fromEntries(fs.readFileSync(linksFile, 'utf8').trim().split('\n').map((l) => l.split(' ')));
  const expected = fs.readFileSync(path.join(dir, 'expected.bin'));
  const moduleUrl = links.plain.split('#')[0];
  const browser = await chromium.launch();
  const errors = [];
  let failed = 0;
  const check = (name, ok) => { console.log((ok ? 'OK   ' : 'FAIL ') + name); if (!ok) failed++; };

  // stream: a stand-in for the save dialog that collects what the page writes.
  async function open(link, stream) {
    const page = await browser.newPage({ acceptDownloads: true });
    page.on('pageerror', (e) => errors.push(e.message));
    await page.addInitScript((s) => {
      if (!s) { delete window.showSaveFilePicker; return; }
      window.__written = [];
      window.showSaveFilePicker = async () => ({
        createWritable: async () => ({
          write: async (d) => { window.__written.push(Array.from(d)); },
          close: async () => { window.__closed = true; },
        }),
      });
    }, stream);
    const res = await page.goto(link);
    return [page, res];
  }
  async function viaMemory(page, n) {
    const [dl] = await Promise.all([page.waitForEvent('download', { timeout: 60000 }), page.click('#dl' + n)]);
    return [dl.suggestedFilename(), fs.readFileSync(await dl.path())];
  }
  async function viaDisk(page, n) {
    await page.click('#dl' + n);
    await page.waitForFunction(() => window.__closed === true, null, { timeout: 60000 });
    return Buffer.from((await page.evaluate(() => window.__written)).flat());
  }

  try {
    let [page, res] = await open(links.plain, true);
    const csp = res.headers()['content-security-policy'] || '';
    check('share page has strict CSP', /default-src 'none'/.test(csp) && /frame-ancestors 'none'/.test(csp));
    check('share page: no referrer', (res.headers()['referrer-policy'] || '') === 'no-referrer');
    await page.waitForSelector('#dl0');
    check('key removed from address bar', !(await page.evaluate(() => location.hash)));
    let data = await viaDisk(page, 0);
    check('to disk, block by block: content identical (' + data.length + ' bytes)', Buffer.compare(data, expected) === 0);
    check('to disk: button says done', /Fertig|Done/.test(await page.textContent('#dl0')));

    [page] = await open(links.plain, false);
    await page.waitForSelector('#dl0');
    let name;
    [name, data] = await viaMemory(page, 0);
    check('through memory: file name ' + name, name === 'Bericht.pdf');
    check('through memory: content identical', Buffer.compare(data, expected) === 0);

    [page] = await open(links.folder, false);
    await page.waitForSelector('#dl1');
    check('folder: every file on its own', (await page.textContent('#files')).includes('Fotos/a.txt'));
    check('folder: "download all" offered', await page.isVisible('#all'));
    [name, data] = await viaMemory(page, 1);
    check('folder: second file ' + name, name === 'b.txt' && data.toString() === 'Datei B');

    [page] = await open(links.pw, false);
    await page.waitForSelector('#pwform:not([hidden])');
    await page.fill('#pw', 'falsch');
    await page.click('#pwbtn');
    await page.waitForSelector('#msg.error');
    check('password: wrong password refused', !(await page.$('#dl0')));
    await page.fill('#pw', 'Grüße 123');
    await page.click('#pwbtn');
    await page.waitForSelector('#dl0');
    [name, data] = await viaMemory(page, 0);
    check('password: content identical', Buffer.compare(data, expected) === 0);

    [page] = await open(links.once, false);
    await page.waitForSelector('#dl0');
    [name, data] = await viaMemory(page, 0);
    check('one-time: first download works', data.toString() === 'WLAN: geheim');
    [page] = await open(links.once, false);
    await page.waitForSelector('#msg.error');
    check('one-time: second call refused', /nicht mehr|no longer/.test(await page.textContent('#msg')));

    [page] = await open(links.expired, false);
    await page.waitForSelector('#msg.error');
    check('expired: refused', !(await page.$('#dl0')));

    // The module writes nothing and hands out nothing without a ticket.
    const post = await fetch(moduleUrl + '?s=' + '0'.repeat(32), { method: 'POST' });
    check('POST refused (' + post.status + ')', post.status === 405);
    const id = links.plain.split('#')[1].split('.')[0];
    const noTicket = await fetch(moduleUrl + '?s=' + id + '&t=' + '0'.repeat(32) + '&f=0&b=0');
    check('block without a valid ticket refused (' + noTicket.status + ')', noTicket.status === 403);
    const missing = await fetch(moduleUrl + '?s=' + '0'.repeat(32));
    check('unknown share: 404', missing.status === 404);
    const bad = await fetch(moduleUrl + '?s=../ghulsuite.php');
    check('path in share id refused (' + bad.status + ')', bad.status === 404);
    check('no script errors on the page', errors.length === 0);
    if (errors.length) console.log(errors.join('\n'));
  } finally {
    await browser.close();
    fs.writeFileSync(path.join(dir, 'done'), '');
  }
  process.exit(failed ? 1 : 0);
})().catch((e) => { console.error(e); fs.writeFileSync(path.join(dir, 'done'), ''); process.exit(1); });
