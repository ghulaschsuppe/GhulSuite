// SPDX-License-Identifier: AGPL-3.0-or-later
// Browser test of share links: real links created by ghul-core, handed out by the server module
// under `php -S`, opened in Chromium.
//
//   D=$(mktemp -d); GHUL_PHP_FIXTURE=$D cargo test -p ghul-core php_browser_fixture &
//   node tools/test-share-php.js $D; wait
//
// Checks the share page (CSP header, key removed from the address bar), that the link is only
// opened on a click (a scanner loading the page does not use up a one-time link), decryption
// block by block in the browser both straight to disk (save dialog of Chromium, stood in for
// here) and through memory (browsers without it), a folder as single files, the extra
// password, one-time links, expired links, a server that swaps the blocks of files, broken
// links, and that the module refuses what it must. Writes `done` so the Rust side can check
// the counters.
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
  // What a person does: click "open share".
  async function click(page) {
    await page.waitForSelector('#open:not([hidden])');
    await page.click('#open');
  }
  async function viaMemory(page, n) {
    const [dl] = await Promise.all([page.waitForEvent('download', { timeout: 60000 }), page.click('#dl' + n)]);
    return [dl.suggestedFilename(), fs.readFileSync(await dl.path())];
  }
  async function viaDisk(page, n) {
    await page.click('#dl' + n);
    // Polled with evaluate: waitForFunction may run its predicate through eval, which the page's
    // CSP (rightly) refuses in some Playwright versions.
    for (let i = 0; !(await page.evaluate(() => window.__closed === true)); i++) {
      if (i > 600) throw new Error('the page did not finish writing');
      await page.waitForTimeout(100);
    }
    return Buffer.from((await page.evaluate(() => window.__written)).flat());
  }

  try {
    let [page, res] = await open(links.plain, true);
    const csp = res.headers()['content-security-policy'] || '';
    check('share page has strict CSP', /default-src 'none'/.test(csp) && /frame-ancestors 'none'/.test(csp));
    check('share page: no referrer', (res.headers()['referrer-policy'] || '') === 'no-referrer');
    await click(page);
    await page.waitForSelector('#dl0');
    check('key removed from address bar', !(await page.evaluate(() => location.hash)));
    let data = await viaDisk(page, 0);
    check('to disk, block by block: content identical (' + data.length + ' bytes)', Buffer.compare(data, expected) === 0);
    check('to disk: button says done', /Fertig|Done/.test(await page.textContent('#dl0')));

    [page] = await open(links.plain, false);
    await click(page);
    await page.waitForSelector('#dl0');
    let name;
    [name, data] = await viaMemory(page, 0);
    check('through memory: file name ' + name, name === 'Bericht.pdf');
    check('through memory: content identical', Buffer.compare(data, expected) === 0);

    [page] = await open(links.folder, false);
    await click(page);
    await page.waitForSelector('#dl1');
    check('folder: every file on its own', (await page.textContent('#files')).includes('Fotos/a.txt'));
    check('folder: "download all" offered', await page.isVisible('#all'));
    [name, data] = await viaMemory(page, 1);
    check('folder: second file ' + name, name === 'b.txt' && data.toString() === 'Datei B');

    [page] = await open(links.pw, false);
    await click(page);
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

    // A scanner or preview loads the page (and runs it) but does not click: nothing is used up.
    [page] = await open(links.once, false);
    await page.waitForSelector('#open:not([hidden])');
    await page.waitForTimeout(1500);
    check('one-time: loading the page alone does not open it', !(await page.$('#dl0')));
    await page.close();
    [page] = await open(links.once, false);
    await click(page);
    await page.waitForSelector('#dl0');
    [name, data] = await viaMemory(page, 0);
    check('one-time: first download works', data.toString() === 'WLAN: geheim');
    [page] = await open(links.once, false);
    await click(page);
    await page.waitForSelector('#msg.error');
    check('one-time: second call refused', /nicht mehr|no longer/.test(await page.textContent('#msg')));

    [page] = await open(links.expired, false);
    await click(page);
    await page.waitForSelector('#msg.error');
    check('expired: refused', !(await page.$('#dl0')));

    // A server that swaps the blocks of two files (or drops one): the page notices before
    // anything is downloaded, because the block names are sealed in the package.
    for (const [what, change] of [['swapped', (b) => b.reverse()], ['dropped', (b) => [b[0]]]]) {
      [page] = await open(links.folder, false);
      await page.route(/\?s=[0-9a-f]{32}$/, async (route) => {
        const r = await route.fetch();
        const j = await r.json();
        j.blocks = change(j.blocks);
        await route.fulfill({ response: r, body: JSON.stringify(j) });
      });
      await click(page);
      await page.waitForSelector('#msg.error');
      check('server ' + what + ' blocks: refused', !(await page.$('#dl0')));
    }

    // Broken links: a clear message, no script error.
    for (const frag of ['%E0%A4%A', 'abc', '0'.repeat(32) + '.' + 'A'.repeat(42)]) {
      [page] = await open(moduleUrl + '#' + frag, false);
      await page.waitForSelector('#msg.error');
      check('broken link ' + frag.slice(0, 12) + ': message, no button', !(await page.isVisible('#open')));
    }

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
