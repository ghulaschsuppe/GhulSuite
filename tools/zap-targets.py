#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Serves the PHP server module and the static share viewer for an OWASP ZAP scan.

Both run under PHP's built-in web server on 127.0.0.1 only (never on all interfaces), each in
its own temporary folder:

- module: core/ghul-core/server/ghulsuite.php with a random key and the real share viewer
  embedded, as the app creates it (net/phpfs.rs, server_file),
- viewer: the static share folder as the app uploads it to SFTP/FTPS hosting (app/shares.rs),
  with the headers of its .htaccess files applied, as Apache with mod_headers would.

`php -S` knows no .htaccess, so a small router does what the hosting would: requests outside
the module or the viewer files get a plain 404 (the module's data folder is denied there by its
own .htaccess). ZAP thus judges GhulSuite's answers, not those of the test web server.

Usage: tools/zap-targets.py <module-port> <viewer-port>   (runs until stopped; prints the URLs)
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
if ($cfg['mode'] === 'module') {
    if ($path === '/ghulsuite.php') {
        return false;
    }
} else {
    $rel = $path === '/' ? '/index.html' : $path;
    $types = ['html' => 'text/html; charset=utf-8', 'js' => 'text/javascript; charset=utf-8', 'css' => 'text/css; charset=utf-8'];
    $ext = strtolower(pathinfo($rel, PATHINFO_EXTENSION));
    $file = realpath($_SERVER['DOCUMENT_ROOT'] . $rel);
    $root = realpath($_SERVER['DOCUMENT_ROOT']);
    if ($file !== false && str_starts_with($file, $root . '/') && is_file($file) && basename($file)[0] !== '.') {
        header('Content-Type: ' . (str_starts_with($rel, '/s/') ? 'application/octet-stream' : ($types[$ext] ?? 'application/octet-stream')));
        readfile($file);
        return true;
    }
}
http_response_code(404);
header('Content-Type: text/plain; charset=utf-8');
echo "Not found\n";
return true;
"""


def htaccess_headers(text):
    """`Header always set Name "value"` lines of an .htaccess file, as `Name: value`."""
    out = []
    for m in re.finditer(r'^\s*Header\s+always\s+set\s+(\S+)\s+"(.*)"\s*$', text, re.M):
        out.append(f"{m.group(1)}: {m.group(2)}")
    for m in re.finditer(r"^\s*ForceType\s+(\S+)\s*$", text, re.M):
        out.append(f"Content-Type: {m.group(1)}")
    return out


def b64(data):
    import base64
    return base64.b64encode(data).decode()


def write_router(folder, mode, prefixes):
    cfg = json.dumps({"mode": mode, "prefixes": prefixes})
    (folder / "router.php").write_text(ROUTER.replace("__CONFIG__", "'" + cfg.replace("\\", "\\\\").replace("'", "\\'") + "'"))


def module_root(work):
    web = work / "module-web"
    web.mkdir()
    src = (ROOT / "core/ghul-core/server/ghulsuite.php").read_text()
    src = (src.replace("__GHUL_KEY__", secrets.token_hex(32)).replace("__GHUL_DATA__", "ghul-data-" + secrets.token_hex(8))
           .replace("__GHUL_VIEWER_HTML__", b64((VIEWER / "index.html").read_bytes()))
           .replace("__GHUL_VIEWER_JS__", b64((VIEWER / "viewer.js").read_bytes()))
           .replace("__GHUL_VIEWER_CSS__", b64((VIEWER / "viewer.css").read_bytes())))
    (web / "ghulsuite.php").write_text(src)
    router_dir = work / "module-router"
    router_dir.mkdir()
    write_router(router_dir, "module", {})
    return web, router_dir / "router.php"


def viewer_root(work):
    web = work / "viewer-web"
    (web / "s").mkdir(parents=True)
    for name in ("index.html", "viewer.js", "viewer.css"):
        shutil.copy(VIEWER / name, web / name)
    (web / "s/index.html").write_text("")
    # An encrypted package as the app uploads it (random bytes are as good for the headers).
    (web / "s" / secrets.token_urlsafe(16)).write_bytes(os.urandom(4096))
    router_dir = work / "viewer-router"
    router_dir.mkdir()
    prefixes = {"/": htaccess_headers((VIEWER / "htaccess.txt").read_text()),
                "/s/": htaccess_headers((VIEWER / "s-htaccess.txt").read_text())}
    write_router(router_dir, "viewer", prefixes)
    return web, router_dir / "router.php"


def wait_port(port):
    for _ in range(100):
        try:
            socket.create_connection(("127.0.0.1", port), timeout=1).close()
            return True
        except OSError:
            time.sleep(0.1)
    return False


def main():
    if len(sys.argv) != 3 or not shutil.which("php"):
        print(__doc__ if len(sys.argv) != 3 else "php not found", file=sys.stderr)
        return 2
    ports = int(sys.argv[1]), int(sys.argv[2])
    work = pathlib.Path(tempfile.mkdtemp(prefix="ghul-zap-"))
    servers = []
    try:
        for (web, router), port in zip((module_root(work), viewer_root(work)), ports):
            # expose_php off: version banners are the hosting's choice, not the module's.
            servers.append(subprocess.Popen(
                ["php", "-d", "expose_php=0", "-S", f"127.0.0.1:{port}", "-t", str(web), str(router)],
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
            if not wait_port(port):
                print(f"php -S on port {port} did not start", file=sys.stderr)
                return 1
        print(f"module: http://127.0.0.1:{ports[0]}/ghulsuite.php", flush=True)
        print(f"viewer: http://127.0.0.1:{ports[1]}/", flush=True)
        signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
        while all(s.poll() is None for s in servers):
            time.sleep(1)
        return 1
    finally:
        for s in servers:
            s.terminate()
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
