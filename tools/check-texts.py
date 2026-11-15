#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Checks the app texts (German and English) for errors and inconsistencies.

Errors (exit code 1): a text missing in one language, placeholders (%1$s ...) that differ
between the languages (crash or wrong text at runtime), broken XML, and long dashes (U+2013,
U+2014) anywhere in the project: texts use full stops, commas, colons or "bis"/"to" instead.
Plural texts (<plurals>) must exist in both languages with the forms "one" and "other" and the
same placeholders in every form. A plain <string> that puts a number before a plural noun
("%1$s entries") is an error too: it reads "1 entries" for one, so it belongs in <plurals>.
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


# Plural nouns that must not follow a number in a plain <string> (it would read "1 entries",
# "1 Einträge"): a <plurals> text is needed. Up to two words may stand between ("%1$s new mails",
# "%1$s not converted items", "%1$s neue Mails").
PLURAL_NOUNS = {
    "en": ["entries", "files", "items", "mails", "messages", "events", "accounts", "vaults", "days", "weeks",
           "hours", "minutes", "seconds", "servers", "downloads", "senders", "links", "keys", "folders", "results",
           "codes", "characters", "photos", "images", "calendars", "attempts", "tries", "devices", "contacts",
           "recipients", "attachments", "shares", "years", "months", "times", "errors", "notes", "logins"],
    "de": ["Einträge", "Einträgen", "Dateien", "Elemente", "Elementen", "Mails", "Nachrichten", "Termine", "Terminen",
           "Konten", "Tresore", "Tresoren", "Tage", "Tagen", "Wochen", "Stunden", "Minuten", "Sekunden", "Abrufe",
           "Abrufen", "Links", "Keys", "Ordner", "Ordnern", "Ergebnisse", "Codes", "Fotos", "Bilder", "Bildern",
           "Kalender", "Kalendern", "Versuche", "Geräte", "Kontakte", "Empfänger", "Anhänge", "Freigaben", "Jahre",
           "Monate", "Fehler", "Notizen", "Anmeldungen"],
}
NUMBER_PLURAL = {
    # Also "image(s)", "Bild(er)": the reader sees a makeshift, a <plurals> text says it properly.
    lang: re.compile(r"%\d+\$[sd] (?:2FA[- ]?)?(?:[\w-]+ ){0,2}?(?:(?:" + "|".join(nouns) + r")\b|\w+\((?:s|e|n|en|er)\))")
    for lang, nouns in PLURAL_NOUNS.items()
}


def load(lang):
    """The <string> texts and the <plurals> texts ({quantity: text}) of one language."""
    path = RES / lang / "strings.xml"
    root = ET.parse(path).getroot()
    strings = {e.get("name"): (e.text or "") for e in root.iter("string")}
    plurals = {e.get("name"): {i.get("quantity"): (i.text or "") for i in e.iter("item")} for e in root.iter("plurals")}
    return strings, plurals


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


def untranslated_core_texts():
    """U5: every error detail the core can show (a literal in the Rust code) has a translation in
    CoreTexts.kt. Test code does not count."""
    core = ROOT / "core" / "ghul-core" / "src"
    table = (CODE / "commonMain/kotlin/de/danerd/ghulsuite/ui/CoreTexts.kt").read_text()
    known = set(re.findall(r'^\s*"((?:[^"\\]|\\.)*)" to Res\.string\.', table, re.M))
    known = {k.replace('\\"', '"') for k in known}
    missing = set()
    for f in core.rglob("*.rs"):
        if "tests" in f.parts or f.name.startswith("tests") or f.name.endswith("_tests.rs"):
            continue
        text = f.read_text(errors="ignore")
        # Only the part before the unit tests of a file.
        text = text.split("#[cfg(test)]\nmod tests")[0]
        for m in re.finditer(r'(?:InvalidInput|Storage|Corrupted|Network|Protocol)\s*\{\s*msg:\s*"((?:[^"\\]|\\.)*)"', text):
            if m.group(1) not in known:
                missing.add(f"{f.name}: {m.group(1)}")
    return sorted(missing)


def placeholders(text):
    return sorted(re.findall(r"%\d+\$[sd]", text))


def main():
    strict = "--strict" in sys.argv
    (de, de_pl), (en, en_pl) = load("values-de"), load("values")
    errors, notes = [], []
    for name in sorted(set(de) ^ set(en)):
        errors.append(f"nur in einer Sprache: {name}")
    for name in sorted(set(de) & set(en)):
        if placeholders(de[name]) != placeholders(en[name]):
            errors.append(f"Platzhalter unterschiedlich: {name}: {placeholders(de[name])} / {placeholders(en[name])}")
    for name in sorted(set(de_pl) ^ set(en_pl)):
        errors.append(f"Mehrzahl-Text nur in einer Sprache: {name}")
    for name in sorted((set(de_pl) | set(en_pl)) & (set(de) | set(en))):
        errors.append(f"Name zugleich als <string> und <plurals>: {name}")
    for name in sorted(set(de_pl) & set(en_pl)):
        forms = {"de": de_pl[name], "en": en_pl[name]}
        for lang, items in forms.items():
            missing = [q for q in ("one", "other") if q not in items]
            if missing:
                errors.append(f"Mehrzahl-Text ohne Form {', '.join(missing)}: {name} ({lang})")
        found = {f"{lang}/{q}": placeholders(text) for lang, items in forms.items() for q, text in sorted(items.items())}
        if len({tuple(p) for p in found.values()}) > 1:
            errors.append(f"Platzhalter unterschiedlich: {name}: " + ", ".join(f"{k} {v}" for k, v in found.items()))
    for lang, texts in (("en", en), ("de", de)):
        for name, text in sorted(texts.items()):
            if NUMBER_PLURAL[lang].search(text):
                errors.append(f"Zahl mit Mehrzahl ohne <plurals>: {name} ({lang}): {text[:90]}")
    for place in long_dashes():
        errors.append(f"langer Strich (stattdessen Punkt, Komma, Doppelpunkt oder „bis“): {place}")
    for place in untranslated_core_texts():
        errors.append(f"Meldung des Kerns ohne Übersetzung in ui/CoreTexts.kt: {place}")
    de_all = dict(de)
    for name, items in de_pl.items():
        for q, text in items.items():
            de_all[f"{name} ({q})"] = text
    for name, text in sorted(de_all.items()):
        for pattern, meant, better in TERMS:
            if re.search(pattern, text):
                notes.append(f"Begriff ({meant} → „{better}“): {name}: {text[:90]}")
    code = "\n".join(p.read_text(errors="ignore") for p in CODE.rglob("*.kt"))
    unused = [n for n in de if f"Res.string.{n}" not in code and not re.search(rf"\b{re.escape(n)}\b", code)]
    unused += [n for n in de_pl if f"Res.plurals.{n}" not in code and not re.search(rf"\b{re.escape(n)}\b", code)]
    for n in sorted(unused):
        notes.append(f"nicht verwendet: {n}")
    for e in errors:
        print("FEHLER:", e)
    for n in notes:
        print("Hinweis:", n)
    print(f"\n{len(de)} Texte und {len(de_pl)} Mehrzahl-Texte geprüft: {len(errors)} Fehler, {len(notes)} Hinweise.")
    return 1 if errors or (strict and notes) else 0


if __name__ == "__main__":
    sys.exit(main())
