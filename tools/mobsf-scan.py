#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Static scan of the Android app with MobSF (Mobile Security Framework) through its REST API.

Uploads the APK to a running MobSF (nightly: a Docker container on 127.0.0.1), starts the scan,
saves the JSON report and the scorecard, and prints the findings of high severity as GitHub
`::warning::` lines. MobSF reports many findings that are fine for this app (or false
positives), so nothing here fails the run; only an unusable MobSF ends with exit code 1.

Usage: tools/mobsf-scan.py [--url http://127.0.0.1:8000] [--wait 600] <apk> <output-dir>
The API key comes from the environment variable MOBSF_API_KEY (the value MobSF was started with).
"""
import argparse
import json
import os
import pathlib
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid


class MobSF:
    def __init__(self, url, key):
        self.url = url.rstrip("/")
        self.key = key

    def post(self, path, fields=None, files=None, timeout=1800):
        headers = {"Authorization": self.key, "X-Mobsf-Api-Key": self.key}
        if files:
            boundary = uuid.uuid4().hex
            body = bytearray()
            for name, value in (fields or {}).items():
                body += f'--{boundary}\r\nContent-Disposition: form-data; name="{name}"\r\n\r\n{value}\r\n'.encode()
            for name, (filename, data, ctype) in files.items():
                body += (f'--{boundary}\r\nContent-Disposition: form-data; name="{name}"; filename="{filename}"\r\n'
                         f"Content-Type: {ctype}\r\n\r\n").encode()
                body += data + b"\r\n"
            body += f"--{boundary}--\r\n".encode()
            headers["Content-Type"] = f"multipart/form-data; boundary={boundary}"
            data = bytes(body)
        else:
            data = urllib.parse.urlencode(fields or {}).encode()
            headers["Content-Type"] = "application/x-www-form-urlencoded"
        req = urllib.request.Request(self.url + path, data=data, headers=headers, method="POST")
        with urllib.request.urlopen(req, timeout=timeout) as r:
            raw = r.read()
        try:
            return json.loads(raw)
        except ValueError:
            raise RuntimeError(f"{path}: no JSON answer: {raw[:300]!r}")

    def wait_ready(self, seconds):
        """MobSF needs a while after the container starts (database migrations)."""
        deadline = time.time() + seconds
        last = None
        while time.time() < deadline:
            req = urllib.request.Request(self.url + "/api/v1/scans", headers={"Authorization": self.key, "X-Mobsf-Api-Key": self.key})
            try:
                with urllib.request.urlopen(req, timeout=10) as r:
                    r.read()
                return
            except urllib.error.HTTPError as e:
                if e.code in (401, 403):
                    raise RuntimeError("MobSF refuses the API key (MOBSF_API_KEY)")
                last = e
            except OSError as e:
                last = e
            time.sleep(5)
        raise RuntimeError(f"MobSF not ready after {seconds} s: {last}")


def severity_items(report, scorecard):
    """(section, title, description) of the high findings, from the scorecard if it has them."""
    items = []
    if isinstance(scorecard, dict) and isinstance(scorecard.get("high"), list):
        for f in scorecard["high"]:
            items.append((f.get("section", ""), f.get("title", ""), f.get("description", "")))
        return items
    # Fallback: the sections of the full report.
    for f in (report.get("manifest_analysis") or {}).get("manifest_findings", []) or []:
        if str(f.get("severity", "")).lower() == "high":
            items.append(("manifest", f.get("title") or f.get("rule", ""), f.get("description", "")))
    for rule, f in ((report.get("code_analysis") or {}).get("findings") or {}).items():
        meta = f.get("metadata", {}) if isinstance(f, dict) else {}
        if str(meta.get("severity", "")).lower() == "high":
            items.append(("code", rule, meta.get("description", "")))
    for f in (report.get("network_security") or {}).get("network_findings", []) or []:
        if str(f.get("severity", "")).lower() == "high":
            items.append(("network", f.get("scope", ""), f.get("description", "")))
    for f in (report.get("certificate_analysis") or {}).get("certificate_findings", []) or []:
        if isinstance(f, list) and f and str(f[0]).lower() == "high":
            items.append(("certificate", f[-1], f[1] if len(f) > 1 else ""))
    return items


def one_line(s, n=300):
    s = " ".join(str(s).split())
    return s if len(s) <= n else s[: n - 3] + "..."


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("apk")
    ap.add_argument("out")
    ap.add_argument("--url", default="http://127.0.0.1:8000")
    ap.add_argument("--wait", type=int, default=600, help="seconds to wait for MobSF to come up")
    args = ap.parse_args()
    key = os.environ.get("MOBSF_API_KEY", "")
    if not key:
        print("MOBSF_API_KEY is not set", file=sys.stderr)
        return 1
    out = pathlib.Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    apk = pathlib.Path(args.apk)
    m = MobSF(args.url, key)
    try:
        m.wait_ready(args.wait)
        print(f"MobSF ready, uploading {apk.name} ({apk.stat().st_size // 1024} KiB)", flush=True)
        up = m.post("/api/v1/upload", files={"file": (apk.name, apk.read_bytes(), "application/octet-stream")})
        h = up.get("hash")
        if not h:
            raise RuntimeError(f"upload failed: {up}")
        print(f"scanning (hash {h})", flush=True)
        started = time.time()
        m.post("/api/v1/scan", {"hash": h, "scan_type": up.get("scan_type", "apk"), "file_name": up.get("file_name", apk.name)})
        print(f"scan done in {int(time.time() - started)} s", flush=True)
        report = m.post("/api/v1/report_json", {"hash": h})
        (out / "mobsf-report.json").write_text(json.dumps(report, indent=1, ensure_ascii=False))
        try:
            scorecard = m.post("/api/v1/scorecard", {"hash": h})
            (out / "mobsf-scorecard.json").write_text(json.dumps(scorecard, indent=1, ensure_ascii=False))
        except (urllib.error.HTTPError, RuntimeError) as e:
            print(f"no scorecard ({e}), using the report", flush=True)
            scorecard = None
    except (OSError, RuntimeError, ValueError) as e:
        detail = e.read()[:500] if isinstance(e, urllib.error.HTTPError) else ""
        print(f"MobSF failed: {e} {detail!r}", file=sys.stderr)
        return 1

    high = severity_items(report, scorecard)
    lines = []
    score = (scorecard or {}).get("security_score", report.get("security_score", "?"))
    lines.append(f"MobSF {report.get('version', '')}: {report.get('app_name', '')} {report.get('version_name', '')}, "
                 f"security score {score}, {len(high)} high finding(s)")
    if isinstance(scorecard, dict):
        for level in ("high", "warning", "info", "secure", "hotspot"):
            if isinstance(scorecard.get(level), list):
                lines.append(f"  {level}: {len(scorecard[level])}")
    for section, title, desc in high:
        lines.append(f"HIGH [{section}] {one_line(title, 120)}: {one_line(desc)}")
    (out / "summary.txt").write_text("\n".join(lines) + "\n")
    print("\n".join(lines))
    for section, title, desc in high:
        print(f"::warning title=MobSF {section}::{one_line(title, 120)}: {one_line(desc, 200)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
