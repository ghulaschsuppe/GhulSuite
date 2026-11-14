#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Tests website/static/report.php (error reports from the app) against `php -S` on this computer.

Valid reports arrive (as files in an outbox instead of mail); everything else is refused with the
expected answer: wrong method or type, broken JSON, no report of the app, missing or wrong proof
of work, old time, replays, too large, header injection in the reply address, too many reports.

Usage: tools/test-report-php.py   (needs php 8.1+)
"""
import hashlib
import json
import pathlib
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
REPORT = "GhulSuite 0.5.0-dev.1\nghul-core 0.5.0\nAndroid 15, Pixel 8\n\n-"
BITS = 8
failed = []


def check(name, ok, detail=""):
    print(("ok   " if ok else "FAIL ") + name + (f"  ({detail})" if detail and not ok else ""))
    if not ok:
        failed.append(name)


def content_hash(report, note, contact):
    return hashlib.sha256(f"{report}\0{note}\0{contact}".encode()).hexdigest()


def zero_bits(h):
    n = 0
    for b in h:
        if b == 0:
            n += 8
            continue
        return n + 8 - b.bit_length()
    return n


def solve(report, note, contact, t, bits=BITS):
    c = content_hash(report, note, contact)
    i = 0
    while True:
        if zero_bits(hashlib.sha256(f"ghulsuite-report|{t}|{c}|{i}".encode()).digest()) >= bits:
            return str(i)
        i += 1


def post(url, body, ctype="application/json", method="POST"):
    data = body if isinstance(body, bytes) else json.dumps(body).encode()
    req = urllib.request.Request(url, data=data if method == "POST" else None, method=method,
                                 headers={"Content-Type": ctype})
    try:
        with urllib.request.urlopen(req, timeout=10) as r:
            return r.status, json.loads(r.read() or b"{}").get("status")
    except urllib.error.HTTPError as e:
        try:
            return e.code, json.loads(e.read() or b"{}").get("status")
        except ValueError:
            return e.code, None


def report(note="", contact="", t=None, nonce=None, text=REPORT):
    t = int(time.time()) if t is None else t
    return {"report": text, "note": note, "contact": contact, "time": t,
            "nonce": nonce if nonce is not None else solve(text, note, contact, t)}


def main():
    if not shutil.which("php"):
        sys.exit("php fehlt")
    tmp = pathlib.Path(tempfile.mkdtemp(prefix="ghul-report-"))
    web, outbox = tmp / "web", tmp / "outbox"
    web.mkdir()
    outbox.mkdir()
    shutil.copy(ROOT / "website/static/report.php", web / "report.php")
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    proc = subprocess.Popen(["php", "-S", f"127.0.0.1:{port}", "-t", str(web)],
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    url = f"http://127.0.0.1:{port}/report.php"
    try:
        for _ in range(50):
            try:
                socket.create_connection(("127.0.0.1", port), timeout=0.2).close()
                break
            except OSError:
                time.sleep(0.1)
        check("ohne Konfiguration: 503", post(url, report()) == (503, "not_configured"))
        (web / "report-config.php").write_text(
            "<?php return ['to' => 'dev@example.org', 'bits' => %d, 'data' => '%s', 'outbox' => '%s'];\n"
            % (BITS, tmp / "data", outbox))

        good = report(note="Beim Öffnen von Mails stürzt es ab.", contact="ich@example.org")
        check("gültiger Bericht kommt an", post(url, good) == (200, "ok"))
        mails = list(outbox.glob("*.txt"))
        body = mails[0].read_text() if mails else ""
        check("Mail enthält Notiz, Kontakt und Bericht",
              len(mails) == 1 and "Beim Öffnen" in body and "ich@example.org" in body and "Pixel 8" in body)
        check("derselbe Beweis ein zweites Mal: 409", post(url, good) == (409, "already_sent"))
        check("GET: 405", post(url, b"", method="GET")[0] == 405)
        check("falscher Typ: 415", post(url, b"report=x", ctype="application/x-www-form-urlencoded") == (415, "json_only"))
        check("kaputtes JSON: 400", post(url, b"{not json") == (400, "bad_request"))
        check("fehlende Felder: 400", post(url, {"report": REPORT}) == (400, "bad_request"))
        check("kein Bericht der App: 400", post(url, report(text="Kauf billige Uhren!\nhttp://spam")) == (400, "not_a_report"))
        check("falscher Beweis: 400", post(url, report(nonce="x")) == (400, "proof"))
        r = report(note="a")
        r["note"] = "b"
        check("Beweis zu anderem Inhalt: 400", post(url, r) == (400, "proof"))
        check("alte Zeit: 400", post(url, report(t=int(time.time()) - 3600)) == (400, "clock"))
        check("Notiz zu lang: 413", post(url, report(note="x" * 4001)) == (413, "too_large"))
        check("riesige Anfrage: 413", post(url, b"{" + b" " * 70000 + b"}") == (413, "too_large"))
        check("Kopfzeilen einschleusen über die Adresse: 400",
              post(url, report(contact="a@b.de\r\nBcc: viele@example.org")) == (400, "bad_contact"))
        check("ungültiges UTF-8: 400", post(url, b'{"report":"GhulSuite 1.0.0\\n\xff","note":"","contact":"","time":1,"nonce":"1"}')[0] == 400)
        # Four more from this address are fine, the sixth within the hour is refused.
        codes = [post(url, report(note=f"n{i}"))[0] for i in range(4)]
        check("fünf Berichte pro Stunde", codes == [200] * 4, str(codes))
        check("der sechste: 429", post(url, report(note="n6")) == (429, "busy"))
        check("nichts liegt offen im Datenordner",
              all(p.name == ".counters.json" for p in (tmp / "data").iterdir()))
        counters = (tmp / "data" / ".counters.json").read_text()
        check("Adressen nur gehasht", "127.0.0.1" not in counters)
    finally:
        proc.terminate()
        shutil.rmtree(tmp, ignore_errors=True)
    print(f"\n{'Alle Prüfungen bestanden' if not failed else str(len(failed)) + ' fehlgeschlagen'}.")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
