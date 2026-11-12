// SPDX-License-Identifier: AGPL-3.0-or-later
// Browser test of the share viewer: packages sealed by ghul-core must decrypt in Chromium.
//
//   GHUL_SHARE_FIXTURE=/tmp/x cargo test -p ghul-core viewer_fixture   (writes packages + links)
//   cp core/ghul-core/share-viewer/{index.html,viewer.js,viewer.css} /tmp/x/
//   node tools/test-share-viewer.js /tmp/x
//
// Serves the folder with the same strict CSP as the share folder, opens each link, enters the
// password where needed, and compares the downloaded file with expected.bin. Also checks that
// a wrong password and a missing package show an error instead of a download.
'use strict';
const http = require('http');
const fs = require('fs');
const path = require('path');
const { chromium } = require('playwright');

const dir = process.argv[2];
const CSP = "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";
const types = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript', '.css': 'text/css' };

const server = http.createServer((req, res) => {
  const p = path.join(dir, decodeURIComponent(req.url.split('?')[0]).replace(/\.\./g, ''));
  const file = p.endsWith('/') ? path.join(p, 'index.html') : p;
  if (!fs.existsSync(file) || fs.statSync(file).isDirectory()) { res.writeHead(404); res.end(); return; }
  res.writeHead(200, { 'Content-Security-Policy': CSP, 'Content-Type': types[path.extname(file)] || 'application/octet-stream' });
  fs.createReadStream(file).pipe(res);
});

(async () => {
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  const base = `http://127.0.0.1:${server.address().port}/`;
  const links = Object.fromEntries(fs.readFileSync(path.join(dir, 'links.txt'), 'utf8').trim().split('\n').map((l) => l.split(' ')));
  const expected = fs.readFileSync(path.join(dir, 'expected.bin'));
  const browser = await chromium.launch();
  const errors = [];
  let failed = 0;
  const check = (name, ok) => { console.log((ok ? 'OK   ' : 'FAIL ') + name); if (!ok) failed++; };

  async function open(link) {
    const page = await browser.newPage({ acceptDownloads: true });
    page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()); });
    page.on('pageerror', (e) => errors.push(e.message));
    await page.goto(base + '#' + link);
    return page;
  }
  async function download(page) {
    const [dl] = await Promise.all([page.waitForEvent('download', { timeout: 30000 }), page.click('#dl')]);
    const file = await dl.path();
    return [dl.suggestedFilename(), fs.readFileSync(file)];
  }

  let page = await open(links.plain);
  await page.waitForSelector('#dl:not([hidden])');
  check('key removed from address bar', !(await page.evaluate(() => location.hash)));
  let [name, data] = await download(page);
  check('plain: file name', name === 'plain.bin');
  check('plain: content identical (' + data.length + ' bytes)', Buffer.compare(data, expected) === 0);

  page = await open(links.pw);
  await page.waitForSelector('#pwform:not([hidden])');
  await page.fill('#pw', 'falsch');
  await page.click('#pwbtn');
  await page.waitForSelector('#msg.error');
  check('password: wrong password shows error, no download button', await page.isHidden('#dl'));
  await page.fill('#pw', 'Grüße 123');
  await page.click('#pwbtn');
  await page.waitForSelector('#dl:not([hidden])');
  [name, data] = await download(page);
  check('password: content identical', Buffer.compare(data, expected) === 0);

  page = await open(links.plain.replace(/^./, (c) => (c === 'a' ? 'b' : 'a')));
  await page.waitForSelector('#msg.error');
  check('missing package: error message', /nicht mehr|no longer/.test(await page.textContent('#msg')));

  const [id] = links.plain.split('.');
  page = await open(id + '.' + 'A'.repeat(43));
  await page.waitForSelector('#msg.error');
  check('wrong key: error, no download', await page.isHidden('#dl'));

  page = await open(links.expired);
  await page.waitForSelector('#msg.error');
  check('expired package (static folder): refused by the page', (await page.isHidden('#dl')) && /nicht mehr|no longer/.test(await page.textContent('#msg')));

  page = await open('kaputt');
  await page.waitForSelector('#msg.error');
  check('incomplete link: error', true);

  check('no CSP violations or script errors (' + errors.join(' | ') + ')', errors.filter((e) => !/404/.test(e)).length === 0);
  await browser.close();
  server.close();
  process.exit(failed ? 1 : 0);
})();
