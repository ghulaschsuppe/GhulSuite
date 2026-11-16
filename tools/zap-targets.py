#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Serves the PHP server module (share links) for an OWASP ZAP scan.

It runs under PHP's built-in web server on 127.0.0.1 only (never on all interfaces), in a
temporary folder laid out as on web hosting: core/ghul-core/server/ghulsuite.php with the real
share page embedded, as the app installs it (net/module.rs, module_file), next to a GhulSuite
folder outside the website with one share in it.

A small router does what the hosting would: requests for anything but the module get a plain
404. ZAP thus judges GhulSuite's answers, not those of the test web server.

Usage: tools/zap-targets.py <port>   (runs until stopped; prints the URL)
"""
import json
import os
import pathlib
import re
import secrets
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
VIEWER = ROOT / "core/ghul-core/share-viewer"

ROUTER = r"""<?php
// Router for `php -S`: plays the hosting's role (see tools/zap-targets.py).
$cfg = json_decode(__CONFIG__, true);
$path = rawurldecode((string) parse_url($_SERVER['REQUEST_URI'] ?? '/', PHP_URL_PATH));
foreach ($cfg['prefixes'] as $prefix => $headers) {
    if (str_starts_with($path, $prefix)) {
        foreach ($headers as $h) {
            header($h);
        }
    }
}
if ($path === '/ghulsuite.php') {
    return false;
}
http_response_code(404);
header('Content-Type: text/plain; charset=utf-8');
echo "Not found\n";
return true;
"""


def b64(data):
    import base64
    return base64.b64encode(data).decode()


def write_router(folder, mode, prefixes):
    cfg = json.dumps({"mode": mode, "prefixes": prefixes})
    (folder / "router.php").write_text(ROUTER.replace("__CONFIG__", "'" + cfg.replace("\\", "\\\\").replace("'", "\\'") + "'"))


def module_root(work):
    web = work / "module-web"
    web.mkdir()
    base = work / "ghulsuite"
    (base / "shares").mkdir(parents=True)
    blocks = base / "drive/blocks/ab"
    blocks.mkdir(parents=True)
    block = "ab" + secrets.token_hex(31)
    (blocks / block).write_bytes(b"GHULBLK2" + os.urandom(4096))
    share = secrets.token_hex(16)
    (base / "shares" / f"{share}.json").write_text(json.dumps({"package": b64(os.urandom(512)), "files": [[block]], "exp": 0, "max": 0}))
    src = (ROOT / "core/ghul-core/server/ghulsuite.php").read_text()
    src = (src.replace("__GHUL_BASE__", "../ghulsuite")
           .replace("__GHUL_VIEWER_HTML__", b64((VIEWER / "index.html").read_bytes()))
           .replace("__GHUL_VIEWER_JS__", b64((VIEWER / "viewer.js").read_bytes()))
           .replace("__GHUL_VIEWER_CSS__", b64((VIEWER / "viewer.css").read_bytes())))
    (web / "ghulsuite.php").write_text(src)
    router_dir = work / "module-router"
    router_dir.mkdir()
    write_router(router_dir, "module", {})
    return web, router_dir / "router.php", share


def wait_port(port):
    for _ in range(100):
        try:
            socket.create_connection(("127.0.0.1", port), timeout=1).close()
            return True
        except OSError:
            time.sleep(0.1)
    return False


def main():
    if len(sys.argv) != 2 or not shutil.which("php"):
        print(__doc__ if len(sys.argv) != 2 else "php not found", file=sys.stderr)
        return 2
    port = int(sys.argv[1])
    work = pathlib.Path(tempfile.mkdtemp(prefix="ghul-zap-"))
    server = None
    try:
        web, router, share = module_root(work)
        # expose_php off: version banners are the hosting's choice, not the module's.
        server = subprocess.Popen(
            ["php", "-d", "expose_php=0", "-S", f"127.0.0.1:{port}", "-t", str(web), str(router)],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        if not wait_port(port):
            print(f"php -S on port {port} did not start", file=sys.stderr)
            return 1
        print(f"module: http://127.0.0.1:{port}/ghulsuite.php (share {share})", flush=True)
        signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
        while server.poll() is None:
            time.sleep(1)
        return 1
    finally:
        if server:
            server.terminate()
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
