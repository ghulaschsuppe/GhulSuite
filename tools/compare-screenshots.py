#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Compares two sets of screenshots (before: the last run of dev, after: this pull request) and
writes a page that shows only what changed: before, after, and the changed pixels in red.

Nothing fails here: a change can be wanted. The page is for looking at every change on purpose,
also the ones nobody meant to make (a change on one screen that moved something on another).

Usage: tools/compare-screenshots.py <before-dir> <after-dir> <out-dir>
Writes <out-dir>/index.html with the images next to it, and prints a short summary.
"""
import html
import pathlib
import shutil
import sys

from PIL import Image, ImageChops

# Pixels whose colour channels differ by less than this count as equal (antialiasing).
TOLERANCE = 24


def compare(a_path, b_path, diff_path):
    a = Image.open(a_path).convert("RGB")
    b = Image.open(b_path).convert("RGB")
    if a.size != b.size:
        return 100.0, "andere Größe"
    d = ImageChops.difference(a, b).convert("L").point(lambda v: 255 if v > TOLERANCE else 0)
    changed = d.histogram()[255]
    share = 100.0 * changed / (a.size[0] * a.size[1])
    if changed:
        # The new picture, faded, with the changed pixels in red.
        faded = Image.blend(b, Image.new("RGB", b.size, (255, 255, 255)), 0.6)
        red = Image.new("RGB", b.size, (220, 20, 60))
        Image.composite(red, faded, d).save(diff_path)
    return share, ""


def main():
    before, after, out = (pathlib.Path(p) for p in sys.argv[1:4])
    out.mkdir(parents=True, exist_ok=True)
    names_a = {p.name for p in before.glob("*.png")}
    names_b = {p.name for p in after.glob("*.png")}
    rows, changed, new, gone = [], 0, sorted(names_b - names_a), sorted(names_a - names_b)
    for name in sorted(names_a & names_b):
        stem = name[:-4]
        share, note = compare(before / name, after / name, out / f"{stem}-diff.png")
        if share == 0:
            continue
        changed += 1
        shutil.copy(before / name, out / f"{stem}-vorher.png")
        shutil.copy(after / name, out / f"{stem}-nachher.png")
        diff = f'<img src="{html.escape(stem)}-diff.png">' if (out / f"{stem}-diff.png").exists() else note
        rows.append((share, f"<h2>{html.escape(stem)} <small>{share:.2f} % verändert {note}</small></h2>"
                            f'<div class="row"><img src="{html.escape(stem)}-vorher.png">'
                            f'<img src="{html.escape(stem)}-nachher.png">{diff}</div>'))
    for name in new:
        shutil.copy(after / name, out / name)
        rows.append((1000, f"<h2>{html.escape(name[:-4])} <small>neu</small></h2>"
                           f'<div class="row"><img src="{html.escape(name)}"></div>'))
    rows.sort(key=lambda r: -r[0])
    page = ("<!doctype html><meta charset='utf-8'><title>Bildvergleich</title>"
            "<style>body{font-family:sans-serif;background:#f4f4f7;margin:16px}"
            ".row{display:flex;gap:8px;overflow-x:auto}.row img{width:300px;border:1px solid #ccc}"
            "small{color:#666;font-weight:normal}</style>"
            f"<h1>Bildvergleich</h1><p>{changed} verändert, {len(new)} neu, {len(gone)} weggefallen"
            f"{': ' + html.escape(', '.join(gone)) if gone else ''}. "
            "Je Bildschirm: vorher (dev), nachher (dieser Pull Request), Unterschiede rot.</p>"
            + "".join(r[1] for r in rows))
    (out / "index.html").write_text(page, encoding="utf-8")
    print(f"Bildvergleich: {changed} verändert, {len(new)} neu, {len(gone)} weggefallen (siehe {out}/index.html)")


if __name__ == "__main__":
    main()
