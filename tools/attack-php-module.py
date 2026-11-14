#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Attacks the GhulSuite PHP server module (core/ghul-core/server/ghulsuite.php).

Starts the module with PHP's built-in web server in a temporary folder and tries what an
attacker on the internet (without the key) and a thief of one signed request can do: forged,
stale and replayed requests, changed bodies and parameters, paths that leave the data folder or
plant executable files, wrong methods, share ids with tricks, upload parts out of order.
Every case names the expected answer; any other answer fails. Needs `php` (8.1+).

Usage: tools/attack-php-module.py
"""
import base64
import hashlib
import hmac
import http.client
import os
import pathlib
import secrets
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import urllib.parse

ROOT = pathlib.Path(__file__).resolve().parent.parent
KEY = bytes(range(32))
SIGNED = ["a", "p", "id", "exp", "max", "n", "u", "size"]
failed = []
count = 0


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


def raw(port, method, query, body=b"", headers=None):
    c = http.client.HTTPConnection("127.0.0.1", port, timeout=20)
    c.request(method, "/ghulsuite.php" + ("?" + query if query else ""), body=body, headers=headers or {})
    r = c.getresponse()
    data = r.read()
    out = (r.status, dict((k.lower(), v) for k, v in r.getheaders()), data)
    c.close()
    return out


def sign(method, params, body, key=KEY, when=None, nonce=None):
    canonical = "&".join(f"{k}={urllib.parse.quote(str(params[k]), safe='-_.~')}" for k in SIGNED if k in params)
    t = str(int(when if when is not None else time.time()))
    n = nonce or secrets.token_hex(16)
    msg = f"ghul/v1\n{method}\n{canonical}\n{t}\n{n}\n{hashlib.sha256(body).hexdigest()}"
    sig = hmac.new(key, msg.encode(), hashlib.sha256).hexdigest()
    return {"X-Ghul-Time": t, "X-Ghul-Nonce": n, "X-Ghul-Sig": sig}


def call(port, params, body=b"", **kw):
    q = urllib.parse.urlencode(params, quote_via=urllib.parse.quote)
    return raw(port, "POST", q, body, sign("POST", params, body, **kw))


def check(name, got, want):
    """`want`: the expected value, or a set of allowed values."""
    global count
    count += 1
    ok = got in want if isinstance(want, set) else got == want
    if not ok:
        failed.append(f"{name}: got {got!r}, expected {want!r}")


def main():
    if not shutil.which("php"):
        print("php not found, skipped")
        return 0
    work = pathlib.Path(tempfile.mkdtemp(prefix="ghul-php-attack-"))
    web = work / "web"
    web.mkdir()
    src = (ROOT / "core/ghul-core/server/ghulsuite.php").read_text()
    b64 = lambda s: base64.b64encode(s.encode()).decode()
    src = (src.replace("__GHUL_KEY__", KEY.hex()).replace("__GHUL_DATA__", "ghul-data-test")
           .replace("__GHUL_VIEWER_HTML__", b64('<link href="viewer.css"><script src="viewer.js"></script>'))
           .replace("__GHUL_VIEWER_JS__", b64("//js")).replace("__GHUL_VIEWER_CSS__", b64("/*css*/")))
    (web / "ghulsuite.php").write_text(src)
    (work / "outside.txt").write_text("OUTSIDE")
    port = free_port()
    server = subprocess.Popen(["php", "-S", f"127.0.0.1:{port}", "-t", str(web)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        for _ in range(50):
            try:
                raw(port, "GET", "")
                break
            except OSError:
                time.sleep(0.1)
        data = web / "ghul-data-test"

        # --- the owner: everything works -----------------------------------------------------
        check("info", call(port, {"a": "info"})[0], 200)
        check("write", call(port, {"a": "write", "p": "suite/a.ghlpss"}, b"DATA1")[0], 200)
        check("read", call(port, {"a": "read", "p": "suite/a.ghlpss"})[2], b"DATA1")

        # --- paths: nothing outside the data folder, nothing executable (signed with the key!) --
        outside = [
            "../outside.txt", "../../outside.txt", "suite/../../outside.txt", "/etc/passwd", "..", ".", "a/./b", "a//b", "/abs",
            "a\\..\\b", "..\\outside.txt", "a\x00b", "a\nb", "%2e%2e/outside.txt", "..%2foutside.txt", "....//outside.txt",
            "a/../b", "~/x", "a:b", "a|b", "a*b", "a?b", "a<b", "a\"b", "x" * 1100,
        ]
        for p in outside:
            for a in ["read", "write", "delete", "mkdir", "list", "write_part", "write_done"]:
                params = {"a": a, "p": p, "n": 0, "u": "0" * 32, "size": 3}
                check(f"path {a} {p!r}", call(port, params, b"xxx")[0], {400, 404})
        planted = [
            "x.php", "x.PHP", "x.pHp", "x.php5", "x.php7", "x.phtml", "x.phar", "x.pht", "x.phps", "x.cgi", "x.pl", "x.py", "x.sh",
            "x.shtml", ".htaccess", ".htpasswd", ".user.ini", "dir/.htaccess", "dir/x.php", "x.php.", "x.php ", " x.php", "x.php..",
            "x.", "x ", ".", "x.php/y", "x.php.jpg", "x.phtml.png", "a.b.PHP7.txt", "x.cgi.gif",
        ]
        for p in planted:
            for a in ["write", "write_part", "mkdir"]:
                r = call(port, {"a": a, "p": p, "n": 0, "u": "0" * 32}, b"<?php echo 1;")
                check(f"plant {a} {p!r}", r[0], 400)
        for f in web.rglob("*"):
            n = f.name.lower()
            if f.is_file() and f != web / "ghulsuite.php" and (".php" in n or n in (".htpasswd", ".user.ini") or n.endswith((".phtml", ".phar", ".cgi", ".sh"))):
                failed.append(f"planted file exists: {f}")
        check("outside untouched", (work / "outside.txt").read_text(), "OUTSIDE")
        check("no file outside data", sorted(p.name for p in web.iterdir()), ["ghul-data-test", "ghulsuite.php"])
        check("data folder denies web access", (data / ".htaccess").read_text().count("denied") >= 1, True)

        # --- without the key ------------------------------------------------------------------
        # (few cases here: after 20 failures the module locks this address for ten minutes)
        check("no headers", raw(port, "POST", "a=info")[0], 401)
        check("wrong key", call(port, {"a": "info"}, key=bytes(32))[0], 401)
        check("stale request", call(port, {"a": "info"}, when=time.time() - 400)[0], 401)
        check("request from the future", call(port, {"a": "info"}, when=time.time() + 400)[0], 401)
        check("nonce not hex", call(port, {"a": "info"}, nonce="../../../../etc/passwd")[0], 401)
        # A stolen, correctly signed request ...
        params, body = {"a": "write", "p": "suite/b.ghlpss"}, b"ORIGINAL"
        h = sign("POST", params, body)
        q = urllib.parse.urlencode(params, quote_via=urllib.parse.quote)
        check("signed write", raw(port, "POST", q, body, h)[0], 200)
        check("replayed", raw(port, "POST", q, body, h)[0], 401)
        h = sign("POST", params, body)
        check("other body", raw(port, "POST", q, b"CHANGED", h)[0], 401)
        h = sign("POST", params, body)
        check("other path", raw(port, "POST", q.replace("b.ghlpss", "a.ghlpss"), body, h)[0], 401)
        h = sign("POST", params, body)
        check("other action", raw(port, "POST", q.replace("write", "delete"), body, h)[0], 401)
        h = sign("POST", {"a": "read", "p": "suite/b.ghlpss"}, b"")
        check("extra parameter overrides", raw(port, "POST", "a=read&p=suite/b.ghlpss&p=suite/a.ghlpss", b"", h)[0], 401)
        check("a.ghlpss intact", call(port, {"a": "read", "p": "suite/a.ghlpss"})[2], b"DATA1")
        check("b.ghlpss intact", call(port, {"a": "read", "p": "suite/b.ghlpss"})[2], b"ORIGINAL")

        # --- methods ---------------------------------------------------------------------------
        for m in ["GET", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"]:
            r = raw(port, m, "a=read&p=suite/a.ghlpss", b"", sign(m, {"a": "read", "p": "suite/a.ghlpss"}, b""))
            check(f"method {m}", r[0], 405)
            check(f"method {m} leaks nothing", b"DATA1" in r[2], False)

        # --- uploads in parts ------------------------------------------------------------------
        u = secrets.token_hex(16)
        p = "suite/big.ghldrv"
        check("part 1 without part 0", call(port, {"a": "write_part", "p": p, "n": 1, "u": u}, b"B")[0], 409)
        check("part 0", call(port, {"a": "write_part", "p": p, "n": 0, "u": u}, b"AAAA")[0], 200)
        check("part 1", call(port, {"a": "write_part", "p": p, "n": 1, "u": u}, b"BBBB")[0], 200)
        check("done with wrong size", call(port, {"a": "write_done", "p": p, "u": u, "size": 7})[0], 409)
        check("nothing written", call(port, {"a": "read", "p": p})[0], 404)
        for bad in ["../x", "zz", "", "G" * 32, "0" * 15, "0" * 65, "0" * 31 + "/"]:
            check(f"upload id {bad!r}", call(port, {"a": "write_part", "p": p, "n": 0, "u": bad}, b"A")[0], 400)

        # --- shares ----------------------------------------------------------------------------
        sid = secrets.token_hex(16)
        check("share part", call(port, {"a": "share_part", "id": sid, "n": 0}, b"SHARED")[0], 200)
        check("share done (one download)", call(port, {"a": "share_done", "id": sid, "exp": 0, "max": 1})[0], 200)
        r = raw(port, "GET", "s=" + sid)
        check("share download", (r[0], r[2]), (200, b"SHARED"))
        check("share is a download, never a page", (r[1].get("content-type"), r[1].get("x-content-type-options")), ("application/octet-stream", "nosniff"))
        check("one-time share is gone", raw(port, "GET", "s=" + sid)[0], {404, 410})
        sid2 = secrets.token_hex(16)
        call(port, {"a": "share_part", "id": sid2, "n": 0}, b"OLD")
        call(port, {"a": "share_done", "id": sid2, "exp": 1, "max": 0})
        check("expired share", raw(port, "GET", "s=" + sid2)[0], {404, 410})
        for bad in ["../files/suite/a.ghlpss", "..%2ffiles%2fsuite%2fa.ghlpss", sid.upper(), "0" * 31, "0" * 33, "x" * 32, "", sid + "/..", sid + "%00", "*"]:
            r = raw(port, "GET", "s=" + bad)
            check(f"share id {bad!r}", r[0] in (200, 404) and b"DATA1" not in r[2] and b"ORIGINAL" not in r[2], True)
        for bad in ["../x", "zz", sid + "x"]:
            check(f"share upload id {bad!r}", call(port, {"a": "share_part", "id": bad, "n": 0}, b"X")[0], 404)

        # --- the viewer page -------------------------------------------------------------------
        for q in ["", "v=js", "v=css", "v=<script>alert(1)</script>", "x=<script>alert(1)</script>"]:
            r = raw(port, "GET", urllib.parse.quote(q, safe="=&"))
            check(f"viewer {q!r}", r[0], 200)
            check(f"viewer {q!r} headers", ("default-src 'none'" in r[1].get("content-security-policy", ""), r[1].get("x-content-type-options")), (True, "nosniff"))
            check(f"viewer {q!r} reflects nothing", b"alert(1)" in r[2], False)

        # --- guessing the key is slowed down (last: locks this address) ------------------------
        codes = [call(port, {"a": "info"}, key=secrets.token_bytes(32))[0] for _ in range(30)]
        check("too many failures are locked out", 429 in codes, True)
        # B15: the owner (with the key) is never locked out, also behind the same address.
        check("not locked out with the right key", call(port, {"a": "info"})[0], 200)
    finally:
        server.terminate()
        shutil.rmtree(work, ignore_errors=True)
    for f in failed:
        print("FEHLER:", f)
    print(f"{count} Angriffe auf das PHP-Modul geprüft, {len(failed)} Fehler.")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
