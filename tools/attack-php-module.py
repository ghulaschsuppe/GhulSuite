#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Attacks the GhulSuite PHP server module (core/ghul-core/server/ghulsuite.php).

Lays out a temporary web hosting (the module in the website, the GhulSuite folder next to it
with GhulDrive blocks and share descriptions), starts PHP's built-in web server for the website
and tries what an attacker on the internet can do: other methods, share ids, tickets, file and
block numbers with tricks, descriptions that name paths instead of blocks, broken descriptions,
expired and used-up shares (also many openings at the same moment), check files, oversized
requests. Every case names the expected answer; any other answer fails. Error answers must not
leak paths or PHP messages. Needs `php` (8.1+).

Usage: tools/attack-php-module.py
"""
import base64
import concurrent.futures
import http.client
import json
import pathlib
import secrets
import shutil
import socket
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
VIEWER = ROOT / "core/ghul-core/share-viewer"
failed = []
count = 0


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


def raw(port, method, query, body=b""):
    c = http.client.HTTPConnection("127.0.0.1", port, timeout=20)
    c.request(method, "/ghulsuite.php" + ("?" + query if query else ""), body=body)
    r = c.getresponse()
    data = r.read()
    out = (r.status, dict((k.lower(), v) for k, v in r.getheaders()), data)
    c.close()
    return out


def expect(name, got, want):
    global count
    count += 1
    status, _, body = got
    ok = status == want if isinstance(want, int) else status in want
    # An error answer says nothing about the server: no paths, no PHP messages.
    if ok and status >= 400 and (b"/" in body or b"Warning" in body or b"Fatal" in body or b"Notice" in body):
        ok = False
        name += " (error answer leaks details: %r)" % body[:80]
    print(("OK   " if ok else "FAIL ") + f"{name}: {status}")
    if not ok:
        failed.append(name)


def b64(data):
    return base64.b64encode(data).decode()


def main():
    if not shutil.which("php"):
        print("php not found", file=sys.stderr)
        return 2
    work = pathlib.Path(tempfile.mkdtemp(prefix="ghul-attack-"))
    web = work / "public_html"
    base = work / "ghulsuite"
    web.mkdir()
    (base / "shares").mkdir(parents=True)
    src = (ROOT / "core/ghul-core/server/ghulsuite.php").read_text()
    src = (src.replace("__GHUL_BASE__", "../ghulsuite")
           .replace("__GHUL_VIEWER_HTML__", b64((VIEWER / "index.html").read_bytes()))
           .replace("__GHUL_VIEWER_JS__", b64((VIEWER / "viewer.js").read_bytes()))
           .replace("__GHUL_VIEWER_CSS__", b64((VIEWER / "viewer.css").read_bytes())))
    (web / "ghulsuite.php").write_text(src)
    (work / "secret.txt").write_text("not for the web")
    (base / "suite").mkdir()
    (base / "suite/suite.ghlsuite").write_bytes(b"vault")

    def block(data):
        name = secrets.token_hex(32)
        d = base / "drive/blocks" / name[:2]
        d.mkdir(parents=True, exist_ok=True)
        (d / name).write_bytes(data)
        return name

    def share(files, exp=0, mx=0, raw_json=None):
        sid = secrets.token_hex(16)
        body = raw_json if raw_json is not None else json.dumps({"package": b64(b"PACKAGE"), "files": files, "exp": exp, "max": mx})
        (base / "shares" / f"{sid}.json").write_text(body)
        return sid

    a, b = block(b"block A"), block(b"block B")
    unshared = block(b"not shared")
    good = share([[a], [b]])
    port = free_port()
    proc = subprocess.Popen(["php", "-S", f"127.0.0.1:{port}", "-t", str(web)],
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        for _ in range(100):
            try:
                socket.create_connection(("127.0.0.1", port), timeout=1).close()
                break
            except OSError:
                time.sleep(0.1)
        get = lambda q: raw(port, "GET", q)

        # Methods: nothing can be written over the web.
        for m in ("POST", "PUT", "DELETE", "PATCH", "OPTIONS"):
            expect(f"{m} refused", raw(port, m, f"s={good}", b"x"), 405)
        expect("version", get("v"), 200)
        expect("page", get(""), 200)
        expect("page with odd asset", get("asset=../../secret.txt"), 200)

        # Share ids.
        for sid in ("../../secret.txt", good.upper(), good[:-1], good + "0", "%00" + good[1:], good[:-1] + "%00",
                    "", "0" * 31 + "g", "s[]=1"):
            expect(f"share id {sid!r}", get(f"s={sid}"), 404)
        expect("share id as array", get(f"s[]={good}"), 404)
        expect("unknown share", get(f"s={secrets.token_hex(16)}"), 404)

        # Opening and tickets.
        st, _, body = get(f"s={good}")
        expect("open share", (st, {}, b""), 200)
        ticket = json.loads(body)["ticket"]
        expect("block 0/0 with ticket", get(f"s={good}&t={ticket}&f=0&b=0"), 200)
        expect("block without ticket", get(f"s={good}&f=0&b=0"), 404)
        expect("block with forged ticket", get(f"s={good}&t={'0' * 32}&f=0&b=0"), 403)
        expect("ticket of another share", get(f"s={share([[a]])}&t={ticket}&f=0&b=0"), 403)
        expect("ticket as array", get(f"s={good}&t[]={ticket}&f=0&b=0"), 404)
        expect("ticket in upper case", get(f"s={good}&t={ticket.upper()}&f=0&b=0"), 404)
        for f, n in (("2", "0"), ("0", "1"), ("-1", "0"), ("00", "0"), ("0", "01"), ("0x0", "0"), ("0.0", "0"),
                     ("%200", "0"), ("0", "1e0"), ("99999999999", "0"), ("", "0"), ("f[]=0&x", "0"), (unshared, "0")):
            expect(f"file {f!r} block {n!r}", get(f"s={good}&t={ticket}&f={f}&b={n}"), 404)

        # Descriptions that name paths or wrong names instead of blocks reach nothing.
        for name in ("../../../secret.txt", "../../suite/suite.ghlsuite", a.upper(), a[:-1], a + "0", "/etc/passwd"):
            sid = share([[name]])
            t = json.loads(get(f"s={sid}")[2])["ticket"]
            expect(f"description naming {name!r}", get(f"s={sid}&t={t}&f=0&b=0"), 404)
        for broken in ("", "not json", "[]", '{"files":[]}', '{"package":1,"files":[]}', '{"package":"x","files":"x"}'):
            expect(f"broken description {broken!r}", get(f"s={share(None, raw_json=broken)}"), 404)

        # Expiry and download limits, also many openings at the same moment.
        expired = share([[a]], exp=1000)
        expect("expired share", get(f"s={expired}"), 410)
        once = share([[a]], mx=1)
        with concurrent.futures.ThreadPoolExecutor(8) as pool:
            answers = list(pool.map(lambda _: get(f"s={once}")[0], range(16)))
        global count
        count += 1
        ok = answers.count(200) == 1 and answers.count(410) == 15
        print(("OK   " if ok else "FAIL ") + f"one-time link opened 16 times at once: {sorted(answers)}")
        if not ok:
            failed.append("one-time link race")

        # Check files: only the app's test files of the shares folder, by exact name.
        (base / "shares/check-00112233445566778899aabbccddeeff").write_text("token")
        expect("check file", get("check=00112233445566778899aabbccddeeff"), 200)
        for name in ("../../secret.txt", "00112233445566778899AABBCCDDEEFF", "0011", f"{good}.json"):
            expect(f"check {name!r}", get(f"check={name}"), 404)

        # Oversized request: refused or ignored, never answered with an error message.
        st = get("s=" + "a" * 60000)[0]
        expect("oversized query", (st, {}, b""), (404, 414, 431, 400))

        # The state the module writes holds no ticket in clear.
        state = (base / "shares" / f"{good}.state").read_text()
        count += 1
        ok = ticket not in state
        print(("OK   " if ok else "FAIL ") + "tickets are stored hashed")
        if not ok:
            failed.append("tickets in clear")
    finally:
        proc.terminate()
        proc.wait()
        shutil.rmtree(work, ignore_errors=True)
    print(f"\n{count - len(failed)} of {count} cases as expected")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
