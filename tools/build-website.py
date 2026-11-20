#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Builds the website (www.ghulsuite.de) from website/ into a folder ready to upload.

Everything that changes is filled in here, so the site never fetches anything from elsewhere
when it is visited:
  - texts in German and English (website/i18n/*.json, website/pages/<lang>/*.html),
  - the newest stable version and pre-release with date and file sizes (GitHub API),
  - numbers from the project (test functions in the core),
  - the legal notice: name, address, e-mail and hoster come from the environment
    (GitHub secrets IMPRESSUM_NAME, IMPRESSUM_ADRESSE, IMPRESSUM_EMAIL, IMPRESSUM_HOSTER),
    never from the repository.

Usage:
  tools/build-website.py --out dist/website            # live site (needs the IMPRESSUM_* values)
  tools/build-website.py --out dist/preview --preview  # preview: noindex, placeholders allowed
Optional: GITHUB_TOKEN (higher API limit), GHUL_RELEASES_JSON (file instead of the API, for tests).
"""
import argparse
import datetime as dt
import html
import json
import os
import pathlib
import re
import shutil
import sys
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
WEB = ROOT / "website"
REPO = "ghulaschsuppe/GhulSuite"
SITE = "https://www.ghulsuite.de"

# Pages: key -> (German path, English path, template, title key)
PAGES = {
    "home": ("index.html", "en/index.html", None, "home_title"),
    "imprint": ("impressum.html", "en/imprint.html", "impressum|imprint", "imprint_title"),
    "privacy": ("datenschutz.html", "en/privacy.html", "datenschutz|privacy", "privacy_title"),
    "privacy_app": ("datenschutz-app.html", "en/privacy-app.html", "datenschutz-app|privacy-app", "privacy_app_title"),
}
IMAGES = [
    "phone-pass-dark", "phone-docs", "phone-unlock", "phone-pass", "phone-auth-grid",
    "phone-docs-pdf", "phone-scan", "phone-cal-week", "phone-setup", "windows-dark",
]
IMPORTS = ["Proton Pass", "Bitwarden", "KeePass", "KeePassXC", "1Password", "LastPass", "Proton Authenticator",
           "Aegis", "2FAS", "Google Authenticator", "otpauth://", "ICS"]

S = 'viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"'
ICONS = {
    "download": f'<svg {S}><path d="M12 3v12M7 10l5 5 5-5M5 21h14"/></svg>',
    "github": '<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M12 .5a11.5 11.5 0 0 0-3.64 22.41c.58.1.79-.25.79-.56v-2c-3.2.7-3.88-1.37-3.88-1.37-.53-1.33-1.28-1.69-1.28-1.69-1.05-.71.08-.7.08-.7 1.16.08 1.77 1.19 1.77 1.19 1.03 1.77 2.71 1.26 3.37.96.1-.75.4-1.26.73-1.55-2.56-.29-5.25-1.28-5.25-5.69 0-1.26.45-2.29 1.19-3.09-.12-.29-.52-1.46.11-3.05 0 0 .97-.31 3.17 1.18a11 11 0 0 1 5.77 0c2.2-1.49 3.17-1.18 3.17-1.18.63 1.59.23 2.76.11 3.05.74.8 1.19 1.83 1.19 3.09 0 4.42-2.7 5.39-5.27 5.68.41.36.78 1.06.78 2.14v3.17c0 .31.21.67.8.56A11.5 11.5 0 0 0 12 .5z"/></svg>',
    "check": f'<svg {S}><path d="M20 6L9 17l-5-5"/></svg>',
    "key": f'<svg {S}><circle cx="7.5" cy="15.5" r="4.5"/><path d="M10.7 12.3L21 2M17 6l3 3M14 9l2 2"/></svg>',
    "clock": f'<svg {S}><circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/></svg>',
    "doc": f'<svg {S}><path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5M9 13h6M9 17h6"/></svg>',
    "cal": f'<svg {S}><rect x="3" y="5" width="18" height="16" rx="2"/><path d="M3 10h18M8 3v4M16 3v4"/></svg>',
    "phone": f'<svg {S}><rect x="6" y="2" width="12" height="20" rx="3"/><path d="M11 18h2"/></svg>',
    "lock": f'<svg {S}><rect x="4" y="11" width="16" height="10" rx="2"/><path d="M8 11V7a4 4 0 0 1 8 0v4"/></svg>',
    "shield": f'<svg {S}><path d="M12 3l8 3v6c0 4.5-3.2 8.2-8 9-4.8-.8-8-4.5-8-9V6z"/></svg>',
    "code": f'<svg {S}><path d="M8 7l-5 5 5 5M16 7l5 5-5 5M14 4l-4 16"/></svg>',
    "zap": f'<svg {S}><path d="M13 2L4 14h7l-1 8 9-12h-7z"/></svg>',
    "eye": f'<svg {S}><path d="M3 3l18 18M10.6 5.1A10 10 0 0 1 22 12a17 17 0 0 1-3.2 4.2M6.6 6.6A17 17 0 0 0 2 12s3.5 7 10 7a9.7 9.7 0 0 0 5.4-1.6"/><path d="M9.9 9.9a3 3 0 0 0 4.2 4.2"/></svg>',
    "refresh": f'<svg {S}><path d="M21 12a9 9 0 0 1-15.5 6.2L3 16M3 12a9 9 0 0 1 15.5-6.2L21 8M21 3v5h-5M3 21v-5h5"/></svg>',
    "android": f'<svg {S}><path d="M5 10v7M19 10v7M7 9h10v9a1 1 0 0 1-1 1H8a1 1 0 0 1-1-1zM8 21v-2M16 21v-2M7 9a5 5 0 0 1 10 0M9 5L7.5 3M15 5l1.5-2"/></svg>',
    "windows": f'<svg {S}><path d="M3 5.5l7.5-1v7H3zM12.5 4.2L21 3v8.5h-8.5zM3 13.5h7.5v6L3 18.5zM12.5 13.5H21V21l-8.5-1.2z"/></svg>',
    "usb": f'<svg {S}><rect x="7" y="9" width="10" height="13" rx="2"/><path d="M9 9V3h6v6M10.5 5.5h.01M13.5 5.5h.01"/></svg>',
}

MONTHS = {
    "de": ["Januar", "Februar", "März", "April", "Mai", "Juni", "Juli", "August", "September", "Oktober", "November", "Dezember"],
    "en": ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"],
}


def esc(s):
    return html.escape(s or "", quote=True)


def date_text(iso, lang):
    if not iso:
        return ""
    d = dt.datetime.strptime(iso[:10], "%Y-%m-%d")
    return f"{d.day}. {MONTHS['de'][d.month - 1]} {d.year}" if lang == "de" else f"{d.day} {MONTHS['en'][d.month - 1]} {d.year}"


def size_text(n, lang):
    mb = n / 1_000_000
    s = f"{mb:.0f} MB" if mb >= 10 else f"{mb:.1f} MB"
    return s.replace(".", ",") if lang == "de" else s


def releases():
    """(newest stable, newest pre-release) as dicts, or None each."""
    data = None
    local = os.environ.get("GHUL_RELEASES_JSON")
    if local:
        text = pathlib.Path(local).read_text().strip()
        data = json.loads(text) if text else []
    else:
        req = urllib.request.Request(f"https://api.github.com/repos/{REPO}/releases?per_page=30",
                                     headers={"Accept": "application/vnd.github+json", "User-Agent": "ghulsuite-website"})
        token = os.environ.get("GITHUB_TOKEN")
        if token:
            req.add_header("Authorization", f"Bearer {token}")
        try:
            with urllib.request.urlopen(req, timeout=20) as r:
                data = json.load(r)
        except Exception as e:  # the site still builds, with links to the release page
            print(f"Hinweis: Releases nicht abrufbar ({e}); die Seite verlinkt die Release-Seite.", file=sys.stderr)
            return None, None
    published = [r for r in data if not r.get("draft")]
    published.sort(key=lambda r: r.get("published_at") or "", reverse=True)
    stable = next((r for r in published if not r.get("prerelease")), None)
    pre = next((r for r in published if r.get("prerelease")), None)
    # A pre-release older than the stable version is of no interest.
    if pre and stable and (pre.get("published_at") or "") < (stable.get("published_at") or ""):
        pre = None
    return stable, pre


def project_numbers():
    """Number of test functions in the core, from the test protocol."""
    proto = (ROOT / "docs/TESTPROTOKOLL.md").read_text(encoding="utf-8")
    m = re.search(r"Testfunktionen im Kern \| (\d+)", proto)
    return int(m.group(1)) if m else 200


def download_block(lang, t, stable, pre):
    def asset(rel, name):
        for a in (rel or {}).get("assets", []):
            if a.get("name") == name:
                return a
        return None

    def card(icon, title, sub, rel, name, latest):
        a = asset(rel, name)
        if latest:
            href = f"https://github.com/{REPO}/releases/latest/download/{name}"
        elif rel:
            href = f"https://github.com/{REPO}/releases/download/{rel['tag_name']}/{name}"
        else:
            href = f"https://github.com/{REPO}/releases"
        meta = f'<span class="meta">{esc(name)} · {size_text(a["size"], lang)}</span>' if a else f'<span class="meta">{esc(name)}</span>'
        return (f'<article class="card rv"><div class="icon" style="--c:var(--accent)">{ICONS[icon]}</div>'
                f'<h3>{t[title]}</h3><p>{t[sub]}</p>{meta}'
                f'<a class="btn primary" href="{href}" rel="noopener">{ICONS["download"]}{t["dl_get"]}</a></article>')

    rel = stable or pre
    out = ['<div class="channel">']
    if stable:
        out.append(f'<div class="card rv" style="--c:var(--ok)"><span class="badge stable">{t["dl_stable"]}</span>'
                   f'<h3 style="margin-top:12px">{t["dl_stable_title"]} {esc(stable["tag_name"].lstrip("v"))}</h3>'
                   f'<p>{t["dl_stable_text"]} {t["dl_released"]} {date_text(stable.get("published_at"), lang)}. '
                   f'<a href="{esc(stable["html_url"])}" rel="noopener">{t["dl_checksums"]}</a></p></div>')
    else:
        out.append(f'<div class="card rv" style="--c:var(--ok)"><span class="badge stable">{t["dl_stable"]}</span>'
                   f'<h3 style="margin-top:12px">{t["dl_stable_title"]}</h3><p>{t["dl_none_stable"]}</p></div>')
    if pre:
        out.append(f'<div class="card rv" style="--c:var(--warn);--d:.06s"><span class="badge pre">{t["dl_pre"]}</span>'
                   f'<h3 style="margin-top:12px">{t["dl_pre_title"]} {esc(pre["tag_name"].lstrip("v"))}</h3>'
                   f'<p>{t["dl_pre_text"]} {t["dl_released"]} {date_text(pre.get("published_at"), lang)}. '
                   f'<a href="{esc(pre["html_url"])}" rel="noopener">{t["dl_pre_link"]}</a></p></div>')
    else:
        out.append(f'<div class="card rv" style="--c:var(--warn);--d:.06s"><span class="badge pre">{t["dl_pre"]}</span>'
                   f'<h3 style="margin-top:12px">{t["dl_pre_title"]}</h3><p>{t["dl_pre_text"]} '
                   f'<a href="https://github.com/{REPO}/releases" rel="noopener">{t["dl_pre_link"]}</a></p></div>')
    out.append("</div>")
    latest = stable is not None
    out.append('<div class="dl" style="margin-top:18px">')
    out.append(card("android", "dl_android", "dl_android_sub", rel, "GhulSuite.apk", latest))
    out.append(card("windows", "dl_msi", "dl_msi_sub", rel, "GhulSuite.msi", latest))
    out.append(card("usb", "dl_zip", "dl_zip_sub", rel, "GhulSuite-portable.zip", latest))
    out.append("</div>")
    return "\n".join("    " + line for line in out)


def legal_values(preview):
    name = os.environ.get("IMPRESSUM_NAME", "").strip()
    address = os.environ.get("IMPRESSUM_ADRESSE", "").strip()
    email = os.environ.get("IMPRESSUM_EMAIL", "").strip()
    hoster = os.environ.get("IMPRESSUM_HOSTER", "").strip()
    missing = [k for k, v in (("IMPRESSUM_NAME", name), ("IMPRESSUM_ADRESSE", address), ("IMPRESSUM_EMAIL", email)) if not v]
    if missing and not preview:
        sys.exit(f"Fehler: {', '.join(missing)} fehlt. Ohne vollständiges Impressum wird die Website nicht veröffentlicht.")
    if missing:
        name = name or "[Name aus IMPRESSUM_NAME]"
        address = address or "[Anschrift aus IMPRESSUM_ADRESSE]"
        email = email or "kontakt@example.org"
    lines = [ln.strip() for ln in address.replace("\r", "").split("\n") if ln.strip()]
    hl = [ln.strip() for ln in hoster.replace("\r", "").split("\n") if ln.strip()]
    if hl:
        rest = f" ({esc(', '.join(hl[1:]))})" if len(hl) > 1 else ""
        hoster_de = f"<strong>{esc(hl[0])}</strong>{rest}, einem Webhosting-Anbieter mit Sitz in Deutschland"
        hoster_en = f"<strong>{esc(hl[0])}</strong>{rest}, a web hosting provider based in Germany"
    else:
        hoster_de = "einem Webhosting-Anbieter mit Sitz in Deutschland"
        hoster_en = "a web hosting provider based in Germany"
    return {
        "imp_name": esc(name),
        "imp_address": "<br>".join(esc(ln) for ln in lines),
        "imp_email": esc(email),
        "hoster_de": hoster_de,
        "hoster_en": hoster_en,
    }


def rel_link(current, target):
    """Relative link from the page `current` to `target` (both relative to the site root)."""
    up = "../" * current.count("/")
    link = up + target
    if link.endswith("index.html"):
        link = link[: -len("index.html")] or "./"
    if up and link.startswith(up + "en/") and current.startswith("en/"):
        link = link[len(up) + 3:] or "./"
    return link


def render(text, t, v):
    def sub(m):
        kind, key = m.group(1), m.group(2)
        table = {"t": t, "v": v, "i": ICONS}[kind]
        if key not in table:
            raise KeyError(f"{{{{{kind}.{key}}}}} ist nicht bekannt")
        return str(table[key])
    # Twice: values may contain placeholders (e.g. the content in the layout).
    for _ in range(3):
        text = re.sub(r"\{\{([tvi])\.([a-z0-9_]+)\}\}", sub, text)
    left = re.findall(r"\{\{[^}]*\}\}", text)
    if left:
        raise ValueError(f"Platzhalter übrig: {left[:5]}")
    return text


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--preview", action="store_true", help="test server: noindex, no redirects, placeholders allowed")
    args = ap.parse_args()
    out = pathlib.Path(args.out)
    if out.exists():
        shutil.rmtree(out)
    shutil.copytree(WEB / "static", out)
    img = out / "assets/img"
    img.mkdir(parents=True, exist_ok=True)
    for name in IMAGES:
        shutil.copy(ROOT / "docs/images" / f"{name}.png", img / f"{name}.png")
    shutil.copy(ROOT / "docs/images/logo.svg", out / "assets/favicon.svg")
    shutil.copy(ROOT / "docs/images/windows-dark.png", out / "assets/og.png")

    layout = (WEB / "templates/layout.html").read_text(encoding="utf-8")
    home = (WEB / "templates/home.html").read_text(encoding="utf-8")
    stable, pre = releases()
    tests = project_numbers()
    legal = legal_values(args.preview)
    build = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%d%H%M")
    imports = "".join(f"<span>{esc(n)}</span>" for n in IMPORTS)

    for lang in ("de", "en"):
        t = json.loads((WEB / f"i18n/{lang}.json").read_text(encoding="utf-8"))
        for key, (de_path, en_path, tpl, title_key) in PAGES.items():
            path = de_path if lang == "de" else en_path
            root = "../" if path.startswith("en/") else ""

            def page(k, lg):
                return rel_link(path, PAGES[k][0] if lg == "de" else PAGES[k][1])

            v = {
                "lang": lang, "page": key, "root": root, "build": build, "site": SITE,
                "title": esc(t[title_key]) if key == "home" else f"{esc(t[title_key])} · GhulSuite",
                "description": esc(t["home_description"] if key == "home" else t["legal_description"]),
                "robots": '<meta name="robots" content="noindex, nofollow">' if args.preview else "",
                "canonical": f"{SITE}/{path}".replace("/index.html", "/"),
                "url_de": f"{SITE}/{de_path}".replace("/index.html", "/"),
                "url_en": f"{SITE}/{en_path}".replace("/index.html", "/"),
                "home": page("home", lang),
                "href_de": page(key, "de"),
                "href_en": page(key, "en"),
                "cur_de": "true" if lang == "de" else "false",
                "cur_en": "true" if lang == "en" else "false",
                "page_imprint": page("imprint", lang),
                "page_privacy": page("privacy", lang),
                "page_privacy_app": page("privacy_app", lang),
                "version_label": esc(("v" + stable["tag_name"].lstrip("v")) if stable else
                                     (pre["tag_name"] if pre else "Beta")),
                "tests": tests, "imports": imports,
                "download": download_block(lang, t, stable, pre),
                **legal,
            }
            if tpl is None:
                content = home
            else:
                name = tpl.split("|")[0 if lang == "de" else 1]
                content = (WEB / f"pages/{lang}/{name}.html").read_text(encoding="utf-8")
            v["content"] = content
            target = out / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(render(layout, t, v), encoding="utf-8")

    # 404: one page for both languages, with absolute links (it answers for any address).
    nf = (f'<!doctype html><html lang="de"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">'
          f'<title>404 · GhulSuite</title><meta name="robots" content="noindex"><link rel="stylesheet" href="/assets/site.css?v={build}">'
          f'<link rel="icon" href="/assets/favicon.svg" type="image/svg+xml"></head><body>'
          f'<div class="aurora" aria-hidden="true"><i></i><i></i><i></i></div>'
          f'<section class="legal"><div class="wrap center"><p class="eyebrow">404</p><h1>Seite nicht gefunden</h1>'
          f'<p lang="en">Page not found.</p><div class="cta" style="justify-content:center">'
          f'<a class="btn primary" href="/">Zur Startseite</a><a class="btn" href="/en/" lang="en">English</a></div></div></section></body></html>\n')
    (out / "404.html").write_text(nf, encoding="utf-8")

    if args.preview:
        (out / "robots.txt").write_text("User-agent: *\nDisallow: /\n", encoding="utf-8")
        (out / ".htaccess").write_text(
            "# Preview on the test server: not for search engines.\n"
            "Options -Indexes\nAddDefaultCharset utf-8\n"
            "<IfModule mod_headers.c>\n  Header always set X-Robots-Tag \"noindex, nofollow\"\n</IfModule>\n", encoding="utf-8")
    else:
        (out / "robots.txt").write_text(f"User-agent: *\nAllow: /\nSitemap: {SITE}/sitemap.xml\n", encoding="utf-8")
        urls = []
        for de_path, en_path, _, _ in PAGES.values():
            for p in (de_path, en_path):
                urls.append(f"  <url><loc>{SITE}/{p}</loc></url>".replace("/index.html", "/"))
        (out / "sitemap.xml").write_text(
            '<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n'
            + "\n".join(urls) + "\n</urlset>\n", encoding="utf-8")
        shutil.copy(WEB / "htaccess-live", out / ".htaccess")

    # Nothing of the build may stay unresolved, and the legal pages must carry the details.
    for f in out.rglob("*.html"):
        s = f.read_text(encoding="utf-8")
        if "{{" in s:
            sys.exit(f"Fehler: Platzhalter in {f}")
    print(f"Website gebaut: {out} (stabil: {stable['tag_name'] if stable else '-'}, vorab: {pre['tag_name'] if pre else '-'}, "
          f"{tests} Testfunktionen{', Vorschau' if args.preview else ''})")


if __name__ == "__main__":
    main()
