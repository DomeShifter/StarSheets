#!/usr/bin/env python3
"""Downloads the fonts in app/webfonts.txt from Google Fonts as TTF (egui reads TTF/OTF, not
WOFF2) into app/webfonts/<slug>/{regular,bold,italic,bolditalic}.ttf. A style a family lacks is
skipped (the app then draws it from the regular face). Run before `trunk build`.
Usage: scripts/fetch-fonts.py [--check]   (--check: only report what each family has)"""
import os, re, sys, urllib.parse, urllib.request

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "app")
OUT = os.path.join(ROOT, "webfonts")
STYLES = {"regular": "0,400", "bold": "0,700", "italic": "1,400", "bolditalic": "1,700"}
UA = "Mozilla/5.0"  # without a modern browser's user agent the CSS API answers with TTF links


def slug(name):
    return re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-")


def get(url):
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=30) as r:
        return r.read()


def main():
    check = "--check" in sys.argv
    failed = []
    for line in open(os.path.join(ROOT, "webfonts.txt"), encoding="utf-8"):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        menu, family = [x.strip() for x in line.split("|")]
        have = []
        for style, spec in STYLES.items():
            q = urllib.parse.quote(family) + ":ital,wght@" + spec
            try:
                css = get(f"https://fonts.googleapis.com/css2?family={q}").decode()
            except Exception:
                continue
            m = re.search(r"url\((https://[^)]+\.ttf)\)", css)
            if not m:
                continue
            have.append(style)
            if not check:
                folder = os.path.join(OUT, slug(menu))
                os.makedirs(folder, exist_ok=True)
                path = os.path.join(folder, style + ".ttf")
                if not os.path.exists(path):
                    open(path, "wb").write(get(m.group(1)))
        print(f"{menu:18} {family:18} {' '.join(have) or 'NOTHING'}")
        if "regular" not in have:
            failed.append(menu)
    if failed:
        sys.exit(f"no regular face for: {', '.join(failed)}")


if __name__ == "__main__":
    main()
