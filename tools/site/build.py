"""Builds the GitHub Pages site into docs/ from one template and a string table per
language (.spec/SITE.md).

    python tools/site/build.py

- docs/index.html            English (and x-default)
- docs/<lang>/index.html     the other eleven languages
- docs/sitemap.xml, robots.txt, llms.txt (a plain summary for language models)
- docs/assets/               site.css, site.js, and the mod's own SVG icons and vault symbols

Every page is static text — search engines and answer engines read it without
running the 3D scene. Strings live in tools/site/strings/<lang>.json; a key missing
from a language falls back to English, and the build fails on unknown {{keys}}.
"""
import html, json, os, re, shutil, sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SITE = os.path.join(ROOT, 'tools', 'site')
DOCS = os.path.join(ROOT, 'docs')
CONFIG = json.load(open(os.path.join(SITE, 'config.json'), encoding='utf-8'))
BASE = CONFIG['base_url'].rstrip('/') + '/'

# culture → (html lang, OpenGraph locale, Google Fonts families for its script)
LANGS = {
    'en': ('en', 'en_US', ''),
    'ko': ('ko', 'ko_KR', 'family=Noto+Sans+KR:wght@400;500;700&'),
    'ja': ('ja', 'ja_JP', 'family=Noto+Sans+JP:wght@400;500;700&'),
    'zh-Hans': ('zh-Hans', 'zh_CN', 'family=Noto+Sans+SC:wght@400;500;700&'),
    'de': ('de', 'de_DE', ''), 'fr': ('fr', 'fr_FR', ''), 'es': ('es', 'es_ES', ''), 'it': ('it', 'it_IT', ''),
    'pl': ('pl', 'pl_PL', ''), 'pt-BR': ('pt-BR', 'pt_BR', ''), 'ru': ('ru', 'ru_RU', ''), 'tr': ('tr', 'tr_TR', ''),
}
NATIVE = {'en': 'English', 'ko': '한국어', 'ja': '日本語', 'zh-Hans': '简体中文', 'de': 'Deutsch', 'fr': 'Français',
          'es': 'Español', 'it': 'Italiano', 'pl': 'Polski', 'pt-BR': 'Português (Brasil)', 'ru': 'Русский', 'tr': 'Türkçe'}


def url(lang):
    return BASE if lang == 'en' else f'{BASE}{lang}/'


def strings(lang):
    en = json.load(open(os.path.join(SITE, 'strings', 'en.json'), encoding='utf-8'))
    p = os.path.join(SITE, 'strings', f'{lang}.json')
    own = json.load(open(p, encoding='utf-8')) if os.path.exists(p) else {}
    return {**en, **own}


def jsonld(s, lang):
    faq = [{'@type': 'Question', 'name': s[f'faq_q{i}'], 'acceptedAnswer': {'@type': 'Answer', 'text': s[f'faq_a{i}']}}
           for i in range(1, 7)]
    graph = [
        {
            '@type': 'SoftwareApplication', '@id': BASE + '#app', 'name': 'hiumod',
            'alternateName': s['meta_title'], 'applicationCategory': 'GameApplication',
            'operatingSystem': 'Windows 10, Windows 11', 'description': s['meta_description'],
            'inLanguage': list(LANGS), 'offers': {'@type': 'Offer', 'price': '0', 'priceCurrency': 'USD'},
            'isAccessibleForFree': True, 'url': url(lang), 'downloadUrl': CONFIG['releases_url'],
            'softwareRequirements': 'Hell Is Us (Steam, app 1620730)', 'license': CONFIG['license_url'],
            'about': {'@type': 'VideoGame', 'name': 'Hell Is Us', 'author': {'@type': 'Organization', 'name': 'Rogue Factor'},
                      'publisher': {'@type': 'Organization', 'name': 'Nacon'}, 'datePublished': '2025-09-04',
                      'sameAs': 'https://store.steampowered.com/app/1620730/Hell_is_Us/'},
        },
        {'@type': 'FAQPage', 'inLanguage': LANGS[lang][0], 'mainEntity': faq},
        {
            '@type': 'HowTo', 'inLanguage': LANGS[lang][0], 'name': s['install_title'],
            'step': [{'@type': 'HowToStep', 'position': i, 'name': s[f'install_s{i}_t'], 'text': s[f'install_s{i}_d']} for i in range(1, 4)],
        },
        {'@type': 'WebSite', '@id': BASE + '#site', 'url': BASE, 'name': 'hiumod', 'inLanguage': list(LANGS)},
    ]
    return json.dumps({'@context': 'https://schema.org', '@graph': graph}, ensure_ascii=False, indent=1)


def render(lang):
    s = strings(lang)
    tpl = open(os.path.join(SITE, 'template.html'), encoding='utf-8').read()
    hl, og, cjk = LANGS[lang]
    root = '' if lang == 'en' else '../'
    alternates = '\n'.join(f'<link rel="alternate" hreflang="{LANGS[c][0]}" href="{url(c)}">' for c in LANGS)
    alternates += f'\n<link rel="alternate" hreflang="x-default" href="{BASE}">'
    switch = '\n'.join(
        f'<li><a href="{root}{"" if c == "en" else c + "/"}" hreflang="{LANGS[c][0]}" lang="{LANGS[c][0]}"'
        f'{" aria-current=\"page\"" if c == lang else ""}>{NATIVE[c]}</a></li>' for c in LANGS)
    values = {
        **{k: html.escape(v, quote=True) for k, v in s.items()},
        'lang': hl, 'og_locale': og, 'canonical': url(lang), 'alternates': alternates, 'root': root,
        'cjk_fonts': cjk, 'lang_switch': switch, 'jsonld': jsonld(s, lang), 'og_image': BASE + 'assets/og.png',
        'repo_url': CONFIG['repo_url'], 'releases_url': CONFIG['releases_url'], 'base': BASE, 'native': NATIVE[lang],
        'og_alternates': '\n'.join(f'<meta property="og:locale:alternate" content="{LANGS[c][1]}">' for c in LANGS if c != lang),
    }
    unknown = set()

    def sub(m):
        k = m.group(1)
        if k not in values:
            unknown.add(k)
            return m.group(0)
        return values[k]
    out = re.sub(r'\{\{(\w+)\}\}', sub, tpl)
    if unknown:
        sys.exit(f'{lang}: unknown keys {sorted(unknown)}')
    return out


def main():
    os.makedirs(DOCS, exist_ok=True)
    for lang in LANGS:
        d = DOCS if lang == 'en' else os.path.join(DOCS, lang)
        os.makedirs(d, exist_ok=True)
        open(os.path.join(d, 'index.html'), 'w', encoding='utf-8', newline='\n').write(render(lang))
    # assets: the site's own, the mod's icons and symbols (drawn for the mod)
    a = os.path.join(DOCS, 'assets')
    os.makedirs(a, exist_ok=True)
    for f in ('site.css', 'site.js', 'favicon.svg', 'og.png'):
        src = os.path.join(SITE, f)
        if os.path.exists(src):
            shutil.copy(src, os.path.join(a, f))
    for sub in ('icons', 'symbols', 'pins'):
        src = os.path.join(ROOT, 'assets', sub)
        dst = os.path.join(a, sub)
        shutil.rmtree(dst, ignore_errors=True)
        shutil.copytree(src, dst)
    # crawlers and answer engines
    urls = '\n'.join(
        f'  <url><loc>{url(c)}</loc>' + ''.join(f'<xhtml:link rel="alternate" hreflang="{LANGS[o][0]}" href="{url(o)}"/>' for o in LANGS) + '</url>'
        for c in LANGS)
    open(os.path.join(DOCS, 'sitemap.xml'), 'w', encoding='utf-8', newline='\n').write(
        '<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" '
        'xmlns:xhtml="http://www.w3.org/1999/xhtml">\n' + urls + '\n</urlset>\n')
    open(os.path.join(DOCS, 'robots.txt'), 'w', encoding='utf-8', newline='\n').write(
        f'User-agent: *\nAllow: /\n\nSitemap: {BASE}sitemap.xml\n')
    en = strings('en')
    faq = '\n\n'.join(f'### {en[f"faq_q{i}"]}\n{en[f"faq_a{i}"]}' for i in range(1, 7))
    open(os.path.join(DOCS, 'llms.txt'), 'w', encoding='utf-8', newline='\n').write(
        f'# hiumod — {en["meta_title"]}\n\n> {en["meta_description"]}\n\n'
        f'- Site: {BASE} (12 languages: {", ".join(NATIVE.values())})\n- Source: {CONFIG["repo_url"]}\n'
        f'- Download: {CONFIG["releases_url"]}\n- Game: Hell Is Us (Rogue Factor / Nacon, 2025), Steam app 1620730\n'
        f'- Unofficial fan-made tool; not affiliated with Rogue Factor or Nacon.\n\n## What it does\n'
        + '\n'.join(f'- {en[f"feat_{k}_t"]}: {en[f"feat_{k}_d"]}' for k in ('map', 'guide', 'tracker', 'puzzles', 'collect', 'lang'))
        + f'\n\n## FAQ\n\n{faq}\n')
    open(os.path.join(DOCS, '.nojekyll'), 'w').write('')
    print('built', len(LANGS), 'pages into', DOCS)


if __name__ == '__main__':
    main()
