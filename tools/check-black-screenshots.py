#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""CI only, after the emulator flows: every step leaves a picture to be looked at. A black picture
means the app forbade screenshots (FLAG_SECURE, see TestDevice.kt) and nothing can be seen.
Fails if a picture of the flows is (almost) entirely black. Dark design is not black: its text and
icons are light.

Usage: tools/check-black-screenshots.py <output-dir>
"""
import pathlib
import sys

from PIL import Image

black = []
pictures = [p for p in pathlib.Path(sys.argv[1]).rglob("*.png") if p.name != "last-screen.png"]
for p in pictures:
    lo, hi = Image.open(p).convert("L").getextrema()
    if hi < 16:
        black.append(str(p))
if black:
    print(f"::error::{len(black)} von {len(pictures)} Bildschirmfotos sind schwarz (die App hat Bildschirmfotos verboten): "
          + " ".join(black[:5]))
    sys.exit(1)
print(f"Bildschirmfotos: {len(pictures)}, keines schwarz")
