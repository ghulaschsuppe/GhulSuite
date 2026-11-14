#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Checks the Android manifest against a fixed list of what may be reachable from outside.

Every component other apps can start, every permission and every intent filter is security
relevant. A change there must be a decision, not an accident: this check fails when the
manifest differs from the lists below; whoever changes the manifest changes the lists too (and
the reviewer sees it).

Usage: tools/check-android-manifest.py
"""
import pathlib
import sys
import xml.etree.ElementTree as ET

ROOT = pathlib.Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "app/composeApp/src/androidMain/AndroidManifest.xml"
A = "{http://schemas.android.com/apk/res/android}"

# Components other apps may reach, and what protects them.
EXPORTED = {
    ".MainActivity": None,  # launcher
    ".IntentEntryActivity": None,  # mailto, share, .ics, add to calendar (takes only foreign content)
    ".LockTileService": "android.permission.BIND_QUICK_SETTINGS_TILE",
    ".autofill.GhulAutofillService": "android.permission.BIND_AUTOFILL_SERVICE",
}
PERMISSIONS = {
    "android.permission.CAMERA",
    "android.permission.READ_MEDIA_IMAGES",
    "android.permission.READ_EXTERNAL_STORAGE",
    "android.permission.INTERNET",
    "android.permission.ACCESS_NETWORK_STATE",
    "android.permission.USE_BIOMETRIC",
    "android.permission.VIBRATE",
    "android.permission.FOREGROUND_SERVICE",
    "android.permission.FOREGROUND_SERVICE_SPECIAL_USE",
    "android.permission.POST_NOTIFICATIONS",
    "android.permission.FOREGROUND_SERVICE_REMOTE_MESSAGING",
    "android.permission.RECEIVE_BOOT_COMPLETED",
    "android.permission.USE_EXACT_ALARM",
    "android.permission.SCHEDULE_EXACT_ALARM",
    "android.permission.REQUEST_IGNORE_BATTERY_OPTIMIZATIONS",
    "android.permission.REQUEST_INSTALL_PACKAGES",  # app updates from signed releases (AndroidUpdater)
}
# Never, whatever the reason.
FORBIDDEN = {
    "android.permission.READ_SMS", "android.permission.RECEIVE_SMS", "android.permission.READ_CONTACTS",
    "android.permission.ACCESS_FINE_LOCATION", "android.permission.ACCESS_COARSE_LOCATION", "android.permission.RECORD_AUDIO",
    "android.permission.SYSTEM_ALERT_WINDOW", "android.permission.BIND_ACCESSIBILITY_SERVICE", "android.permission.READ_PHONE_STATE",
    "android.permission.QUERY_ALL_PACKAGES", "android.permission.MANAGE_EXTERNAL_STORAGE", "android.permission.WRITE_EXTERNAL_STORAGE",
}


def main():
    root = ET.parse(MANIFEST).getroot()
    app = root.find("application")
    errors = []
    if app.get(A + "allowBackup") != "false":
        errors.append("allowBackup must be false (no copies of the suite in device backups)")
    if app.get(A + "debuggable") == "true":
        errors.append("debuggable must not be set in the manifest")
    if app.get(A + "usesCleartextTraffic") == "true":
        errors.append("usesCleartextTraffic must not be true")
    perms = {p.get(A + "name") for p in root.findall("uses-permission")}
    for p in sorted(perms - PERMISSIONS):
        errors.append(f"new permission {p}: add it to PERMISSIONS in this script if it is intended")
    for p in sorted(perms & FORBIDDEN):
        errors.append(f"forbidden permission {p}")
    exported = {}
    for tag in ("activity", "activity-alias", "service", "receiver", "provider"):
        for c in app.findall(tag):
            name = c.get(A + "name")
            is_exported = c.get(A + "exported")
            if is_exported is None and c.find("intent-filter") is not None:
                errors.append(f"{name}: has an intent filter but no android:exported")
            if is_exported == "true":
                exported[name] = c.get(A + "permission")
            if tag == "provider" and is_exported == "true":
                errors.append(f"{name}: a content provider must not be exported")
            if tag == "provider" and c.get(A + "grantUriPermissions") == "true" and is_exported != "false":
                errors.append(f"{name}: grantUriPermissions needs exported=false")
    for name, perm in sorted(exported.items()):
        if name not in EXPORTED:
            errors.append(f"{name} is exported: add it to EXPORTED in this script if it is intended")
        elif EXPORTED[name] != perm:
            errors.append(f"{name}: exported with permission {perm!r}, expected {EXPORTED[name]!r}")
    for name in sorted(set(EXPORTED) - set(exported)):
        errors.append(f"{name} is no longer exported: remove it from EXPORTED in this script")
    for e in errors:
        print("FEHLER:", e)
    print(f"Manifest geprüft: {len(exported)} von außen erreichbare Komponenten, {len(perms)} Berechtigungen, {len(errors)} Fehler.")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
