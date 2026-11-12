// SPDX-License-Identifier: AGPL-3.0-or-later
// Browser test of the PHP server module: real links created by ghul-core, served by `php -S`.
//
//   D=$(mktemp -d); GHUL_PHP_FIXTURE=$D cargo test -p ghul-core php_browser_fixture &
//   node tools/test-share-php.js $D; wait
//
// Checks the viewer page (CSP header, key removed from the address bar), decryption in the
// browser, the extra password, one-time links, expired links, and that the module answers
// unsigned API calls with an error. Writes `done` so the Rust side can check the counters.
'use strict';
const fs = require('fs');
const path = require('path');
const { chromium } = require('playwright');

const dir = process.argv[2];
const linksFile = path.join(dir, 'links.txt');

(async () => {
  for (let i = 0; i < 600 && !fs.existsSync(linksFile); i++) await new Promise((r) => setTimeout(r, 200));
  const links = Object.fromEntries(fs.readFileSync(linksFile, 'utf8').trim().split('\n').map((l) => l.split(' ')));
  const expected = fs.readFileSync(path.join(dir, 'expected.bin'));
  const moduleUrl = links.plain.split('#')[0];
  const browser = await chromium.launch();
  const errors = [];
  let failed = 0;
  const check = (name, ok) => { console.log((ok ? 'OK   ' : 'FAIL ') + name); if (!ok) failed++; };

  async function open(link) {
    const page = await browser.newPage({ acceptDownloads: true });
    page.on('pageerror', (e) => errors.push(e.message));
    const res = await page.goto(link);
    return [page, res];
  }
  async function download(page) {
    const [dl] = await Promise.all([page.waitForEvent('download', { timeout: 30000 }), page.click('#dl')]);
    return [dl.suggestedFilename(), fs.readFileSync(await dl.path())];
  }

  try {
    let [page, res] = await open(links.plain);
    const csp = res.headers()['content-security-policy'] || '';
    check('viewer page has strict CSP', /default-src 'none'/.test(csp) && /frame-ancestors 'none'/.test(csp));
    check('viewer page: no referrer', (res.headers()['referrer-policy'] || '') === 'no-referrer');
    await page.waitForSelector('#dl:not([hidden])');
    check('key removed from address bar', !(await page.evaluate(() => location.hash)));
    let [name, data] = await download(page);
    check('plain: file name ' + name, name === 'Bericht.pdf');
    check('plain: content identical (' + data.length + ' bytes)', Buffer.compare(data, expected) === 0);

    [page] = await open(links.pw);
    await page.waitForSelector('#pwform:not([hidden])');
    await page.fill('#pw', 'falsch');
    await page.click('#pwbtn');
    await page.waitForSelector('#msg.error');
    check('password: wrong password refused', await page.isHidden('#dl'));
    await page.fill('#pw', 'Grüße 123');
    await page.click('#pwbtn');
    await page.waitForSelector('#dl:not([hidden])');
    [name, data] = await download(page);
    check('password: content identical', Buffer.compare(data, expected) === 0);

    [page] = await open(links.once);
    await page.waitForSelector('#dl:not([hidden])');
    [name, data] = await download(page);
    check('one-time: first download works', data.toString() === 'WLAN: geheim');
    [page] = await open(links.once);
    await page.waitForSelector('#msg.error');
    check('one-time: second call refused', /nicht mehr|no longer/.test(await page.textContent('#msg')));

    [page] = await open(links.expired);
    await page.waitForSelector('#msg.error');
    check('expired: refused', await page.isHidden('#dl'));

    // The API refuses unsigned or forged calls.
    const api = await fetch(moduleUrl + '?a=list', { method: 'POST' });
    check('unsigned API call refused (' + api.status + ')', api.status === 401 || api.status === 403);
    const forged = await fetch(moduleUrl + '?a=list', {
      method: 'POST',
      headers: { 'X-Ghul-Time': String(Math.floor(Date.now() / 1000)), 'X-Ghul-Nonce': '0'.repeat(32), 'X-Ghul-Sig': '0'.repeat(64) },
    });
    check('forged signature refused (' + forged.status + ')', forged.status === 401 || forged.status === 403);
    const missing = await fetch(moduleUrl + '?s=' + '0'.repeat(32));
    check('unknown share: 404', missing.status === 404);
    const bad = await fetch(moduleUrl + '?s=../ghulsuite.php');
    check('path in share id refused (' + bad.status + ')', bad.status === 400 || bad.status === 404);
    check('no script errors on the page', errors.length === 0);
    if (errors.length) console.log(errors.join('\n'));
  } finally {
    await browser.close();
    fs.writeFileSync(path.join(dir, 'done'), '');
  }
  process.exit(failed ? 1 : 0);
})().catch((e) => { console.error(e); fs.writeFileSync(path.join(dir, 'done'), ''); process.exit(1); });
