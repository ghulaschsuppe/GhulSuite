#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Checks the texts the Maestro flows (app/maestro) look for against the texts of the app.

The flows only run on the emulator in the CI. A text that was renamed or removed in the app
makes a flow fail there, after the push. This check finds it before: every text a flow taps,
waits for, scrolls to or asserts must be a text of the app (German or English), a text the flow
typed in itself, or listed in tools/maestro-texts.txt (texts of the system or of test data).

It cannot see a changed order of steps (a dialog that no longer comes): for that, go through the
flows whose steps a change touches (PROJEKT.md, "Vor dem Push").

Exit code 1 with a list of the texts not found.
"""
from __future__ import annotations

import re
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parent.parent
FLOWS = ROOT / "app" / "maestro"
# The texts of the shared screens and of Android (notification, system texts of the app).
RES = [
    ROOT / "app" / "composeApp" / "src" / "commonMain" / "composeResources",
    ROOT / "app" / "composeApp" / "src" / "androidMain" / "res",
]
KNOWN = Path(__file__).resolve().parent / "maestro-texts.txt"

# Commands whose text is a selector of something on the screen. Checks that something is NOT
# there (assertNotVisible, notVisible: mostly error texts) are not checked.
SELECTING = {"tapOn", "longPressOn", "doubleTapOn", "assertVisible"}
META = set("\\^$.|?*+()[]{}")


def unescape(s: str) -> str:
    """A text of strings.xml as the app shows it."""
    s = re.sub(r"\\u([0-9a-fA-F]{4})", lambda m: chr(int(m.group(1), 16)), s)
    return s.replace("\\'", "'").replace('\\"', '"').replace("\\n", "\n").replace("\\@", "@")


def app_texts() -> list[str]:
    texts: list[str] = []
    for f in sorted(p for r in RES for p in r.glob("values*/strings.xml")):
        for el in ET.parse(f).getroot().iter():
            if el.tag in ("string", "item") and el.text:
                texts.append(unescape("".join(el.itertext())))
    return texts


def template_regex(text: str) -> re.Pattern[str]:
    """An app text with its placeholders (%1$s, %d) standing for anything."""
    parts = re.split(r"%(?:\d+\$)?[sd]", text)
    return re.compile(".*".join(re.escape(p) for p in parts), re.S)


def selectors(node, out: list[str]) -> None:
    """Every text selector in a flow (also inside runFlow commands, repeat, retry, ...)."""
    if isinstance(node, list):
        for n in node:
            selectors(n, out)
    elif isinstance(node, dict):
        for key, value in node.items():
            if key in SELECTING:
                if isinstance(value, str):
                    out.append(value)
                elif isinstance(value, dict) and isinstance(value.get("text"), str):
                    out.append(value["text"])
            elif key in ("extendedWaitUntil",) and isinstance(value, dict):
                for k in ("visible",):
                    v = value.get(k)
                    if isinstance(v, str):
                        out.append(v)
                    elif isinstance(v, dict) and isinstance(v.get("text"), str):
                        out.append(v["text"])
            elif key == "scrollUntilVisible" and isinstance(value, dict):
                v = value.get("element")
                if isinstance(v, str):
                    out.append(v)
                elif isinstance(v, dict) and isinstance(v.get("text"), str):
                    out.append(v["text"])
            else:
                selectors(value, out)


def typed(node, out: set[str]) -> None:
    """Texts the flows type in themselves (names of entries, events, ...)."""
    if isinstance(node, list):
        for n in node:
            typed(n, out)
    elif isinstance(node, dict):
        for key, value in node.items():
            if key == "inputText" and isinstance(value, str):
                out.add(value)
            else:
                typed(value, out)


def alternatives(pattern: str) -> list[str]:
    """The top-level alternatives of a regex ("A|B" -> A, B; groups stay whole)."""
    parts, depth, cur, esc = [], 0, "", False
    for ch in pattern:
        if esc:
            cur += ch
            esc = False
            continue
        if ch == "\\":
            cur += ch
            esc = True
        elif ch in "([":
            depth += 1
            cur += ch
        elif ch in ")]":
            depth -= 1
            cur += ch
        elif ch == "|" and depth == 0:
            parts.append(cur)
            cur = ""
        else:
            cur += ch
    parts.append(cur)
    return parts


def literal_pieces(pattern: str) -> list[str]:
    """The literal pieces of a regex (escaped characters taken as they are; a class [..] or a
    count {..} is no letter of the text)."""
    pieces, cur, i = [], "", 0
    while i < len(pattern):
        ch = pattern[i]
        if ch in "[{":
            close = pattern.find("]" if ch == "[" else "}", i + 1)
            if close > i:
                pieces.append(cur)
                cur = ""
                i = close + 1
                continue
        if ch == "\\" and i + 1 < len(pattern):
            nxt = pattern[i + 1]
            if nxt.isalnum():  # \d, \s, ...: a class, not a letter
                pieces.append(cur)
                cur = ""
            else:
                cur += nxt
            i += 2
            continue
        if ch in META:
            pieces.append(cur)
            cur = ""
        else:
            cur += ch
        i += 1
    pieces.append(cur)
    return [p for p in pieces if p.strip()]


def found(pattern: str, texts: list[str], templates: list[re.Pattern[str]], data: set[str]) -> bool:
    for alt in alternatives(pattern):
        if not alt:
            continue
        # As Maestro: the whole text, or the regex over the whole text.
        try:
            rx = re.compile(alt, re.S)
        except re.error:
            rx = None
        if alt in data or any(alt == t or alt == t.upper() for t in texts):
            return True
        if rx and (any(rx.fullmatch(t) for t in texts) or any(rx.fullmatch(t) for t in data)):
            return True
        # A literal text: the whole text (Maestro compares the whole text), also one that an app
        # text with placeholders shows ("Step 4 of 8 · Areas") or a heading in capitals ("2 ENTRIES").
        if not (set(alt) & META):
            if any(re.fullmatch(tp.pattern, alt, re.S | re.I) for tp in templates):
                return True
            continue
        # A regex over a text with placeholders: its literal pieces in one app text, a piece
        # also in a text the flows typed ("Delete “Steuer 2025”?") or a number ("Step 4 of 8").
        pieces = [q for p in literal_pieces(alt) for q in re.split(r"\d+", p) if q.strip()]
        in_data = [p for p in pieces if any(p in d for d in data)]
        rest = [p for p in pieces if p not in in_data]
        if pieces and not rest:
            return True
        if rest and any(all(p in t for p in rest) for t in texts):
            return True
        # A regex over an app text with placeholders ("Suite file: GhulSuite.*").
        sample = "".join(literal_pieces(alt))
        if sample and any(tp.fullmatch(sample) for tp in templates):
            return True
    return False


def main() -> int:
    texts = app_texts()
    # Only texts with words of their own: "%1$s" or "%1$s: %2$s" would match any text.
    templates = [
        template_regex(t) for t in texts
        if "%" in t and len(re.sub(r"[^A-Za-zÄÖÜäöüß]", "", re.sub(r"%(?:\d+\$)?[sd]", "", t))) >= 4
    ]
    known = set()
    if KNOWN.exists():
        for line in KNOWN.read_text(encoding="utf-8").splitlines():
            line = line.rstrip("\n")
            if line.strip() and not line.lstrip().startswith("#"):
                known.add(line)
    flows = sorted(FLOWS.rglob("*.yaml"))
    data: set[str] = set(known)
    parsed = {}
    for f in flows:
        docs = list(yaml.safe_load_all(f.read_text(encoding="utf-8")))
        parsed[f] = docs
        typed(docs, data)
    missing: list[str] = []
    checked = 0
    for f, docs in parsed.items():
        sel: list[str] = []
        for d in docs[1:] if len(docs) > 1 else docs:
            selectors(d, sel)
        for s in sel:
            checked += 1
            if "${" in s:  # a value of the flow itself (output, env)
                continue
            if not found(s, texts, templates, data):
                missing.append(f"{f.relative_to(ROOT)}: {s!r}")
    for m in missing:
        print("Nicht in der App gefunden:", m)
    print(f"{checked} Texte in {len(flows)} Maestro-Abläufen geprüft: {len(missing)} nicht gefunden.")
    if missing:
        print("Text in der App umbenannt? Dann den Ablauf anpassen. Ein Text des Systems oder der")
        print("Testdaten? Dann in tools/maestro-texts.txt eintragen.")
    return 1 if missing else 0


if __name__ == "__main__":
    sys.exit(main())
