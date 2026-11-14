#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Checks the app texts (German and English) for errors and inconsistencies.

Errors (exit code 1): a text missing in one language, placeholders (%1$s ...) that differ
between the languages (crash or wrong text at runtime), broken XML, and long dashes (U+2013,
U+2014) anywhere in the project: texts use full stops, commas, colons or "bis"/"to" instead.
Notes (only listed, see docs/TESTPROTOKOLL.md): terms used inconsistently, English words in
the German texts, texts that are no longer used.

Usage: tools/check-texts.py [--strict]   (--strict: notes count as errors too)
"""
import pathlib
import re
import subprocess
import sys
import xml.etree.ElementTree as ET

ROOT = pathlib.Path(__file__).resolve().parent.parent
RES = ROOT / "app/composeApp/src/commonMain/composeResources"
CODE = ROOT / "app/composeApp/src"

# Inconsistent terms in the German texts: (pattern, what is meant, preferred wording).
TERMS = [
    (r"\bPasswort \+ Key\b|\bPasswort und Key\b", "zweiter Faktor ohne Schlüsseldatei", "Master-Passwort + Key oder Schlüsseldatei"),
    (r"\bSecurity[- ]Keys?\b|\bFIDO-Security-Key\b", "Sicherheitsschlüssel", "Key (Sicherheitsschlüssel)"),
    (r"\bErsatzkey\b", "Sicherheitsschlüssel", "Zweiter Key"),
    (r"\bEinstellungen\b", "der Reiter heißt „Optionen“", "Optionen"),
    (r"\bSchnellzugang\b|\bSchnell entsperren\b", "Schnell-Entsperren", "Schnell-Entsperren"),
    (r"\bSync\b", "Synchronisation", "Synchronisation"),
    (r"\bWebspace\b", "Webhosting", "Webhosting"),
    (r"\bLogin\b", "englisches Wort", "Anmeldung"),
    (r"\bScreenshot\b", "englisches Wort", "Bildschirmfoto"),
    (r"\bLabels?\b", "englisches Wort", "Schlagwort"),
    (r"\bans Handy halten\b|\bNFC\b", "NFC gibt es nicht", "Key einstecken"),
]


def load(lang):
    path = RES / lang / "strings.xml"
    tree = ET.parse(path)
    return {e.get("name"): (e.text or "") for e in tree.getroot().iter("string")}


def long_dashes():
    """Files of the project (not the test data of others) that contain a long dash."""
    try:
        names = subprocess.run(["git", "ls-files"], cwd=ROOT, capture_output=True, text=True, check=True).stdout.split("\n")
    except (OSError, subprocess.CalledProcessError):
        names = [str(p.relative_to(ROOT)) for p in ROOT.rglob("*") if p.is_file() and ".git/" not in str(p)]
    found = []
    for name in names:
        if not name or "tests/data/" in name or "/build/" in name or "/target/" in name:
            continue
        try:
            text = (ROOT / name).read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        for number, line in enumerate(text.split("\n"), 1):
            if "\u2013" in line or "\u2014" in line:
                found.append(f"{name}:{number}: {line.strip()[:80]}")
    return found


def placeholders(text):
    return sorted(re.findall(r"%\d+\$[sd]", text))


def main():
    strict = "--strict" in sys.argv
    de, en = load("values-de"), load("values")
    errors, notes = [], []
    for name in sorted(set(de) ^ set(en)):
        errors.append(f"nur in einer Sprache: {name}")
    for name in sorted(set(de) & set(en)):
        if placeholders(de[name]) != placeholders(en[name]):
            errors.append(f"Platzhalter unterschiedlich: {name}: {placeholders(de[name])} / {placeholders(en[name])}")
    for place in long_dashes():
        errors.append(f"langer Strich (stattdessen Punkt, Komma, Doppelpunkt oder „bis“): {place}")
    for name, text in sorted(de.items()):
        for pattern, meant, better in TERMS:
            if re.search(pattern, text):
                notes.append(f"Begriff ({meant} → „{better}“): {name}: {text[:90]}")
    code = "\n".join(p.read_text(errors="ignore") for p in CODE.rglob("*.kt"))
    unused = [n for n in de if f"Res.string.{n}" not in code and not re.search(rf"\b{re.escape(n)}\b", code)]
    for n in sorted(unused):
        notes.append(f"nicht verwendet: {n}")
    for e in errors:
        print("FEHLER:", e)
    for n in notes:
        print("Hinweis:", n)
    print(f"\n{len(de)} Texte geprüft: {len(errors)} Fehler, {len(notes)} Hinweise.")
    return 1 if errors or (strict and notes) else 0


if __name__ == "__main__":
    sys.exit(main())
