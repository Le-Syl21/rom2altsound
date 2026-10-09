#!/usr/bin/env python3
"""Generate the GitHub Pages site: the sound ROM catalog (English at the root, French under fr/).

Run from anywhere: python3 docs/build_site.py
The table itself is filled by app.js from catalog.json, which `rom2altsound catalog` writes
(see the README): this script writes the two pages around it, the sitemap and .nojekyll,
and marks in catalog.json the entries whose packs cannot play in VPinball today (from
vpx_playback.json). The site holds metadata only: no ROM, no sound, no sounds.dat name.
"""
import json
import sys
from pathlib import Path

DOCS = Path(__file__).resolve().parent
SITE = "https://le-syl21.github.io/rom2altsound/"
# Google Search Console ownership check (the token belongs to the owner's Google account).
GOOGLE_VERIFICATION = "TqbXre6qrm9jaoj6tFwRRiI2vuQilAZLm6kUJA-etmo"
REPO = "https://github.com/Le-Syl21/rom2altsound"
BLOB = REPO + "/blob/main/"
SOUNDS_DAT = "https://github.com/vpinball/pinmame/blob/master/release/sounds.dat"

UI = {
    "en": {
        "title": "Sound ROM catalog · rom2altsound",
        "description": "Every pinball sound ROM PinMAME knows, grouped by sound ROM id: the games and "
                       "revisions that share it, their sound board, how far rom2altsound gets with it, "
                       "the ROM files' checksums and the number of tracks read from the ROMs.",
        "other": ("fr", "Version française", "fr"),
        "nav": [("#catalog", "Catalog"), ("#about", "About"), (REPO, "GitHub")],
        "eyebrow": "rom2altsound",
        "h1": "Sound ROM catalog",
        "catalog": "Catalog",
        "lead": "Every pinball sound ROM the PinMAME of rom2altsound knows, grouped by <strong>sound ROM "
                "id</strong>: one entry per set of sound ROMs, with every game revision that plays it, its "
                "sound board, how far rom2altsound gets with that board, the files' sizes and checksums, and "
                "the number of tracks or calls read straight from the ROMs.",
        "note": "Data only: this site holds no ROM and no sound, only names, sizes, checksums and counts. "
                "ROM images are copyrighted: use your own.",
        "family": "Board family", "support": "rom2altsound support", "maker": "Maker",
        "cols": [("title", "Game", ""), ("maker", "Maker", "hide-sm"), ("year", "Year", ""),
                 ("family", "Board", ""), ("support", "rom2altsound", ""), ("sets", "Sets", "num"),
                 ("sounds", "Sounds", "num"), ("names", "Names", "num"), ("id", "Sound ROM id", "hide-sm")],
        "about": f"""
<h2 id="about">About</h2>
<h3>What a sound ROM id is</h3>
<p>The SHA-1 of the SHA-1s of a game's sound ROMs: the distinct SHA-1s (lowercase hex) of the files
PinMAME loads for the sound board, sorted, each followed by a line feed. Every revision of a game that
kept its sound ROMs has the same id, so one AltSound pack, and one list of sound names, serves all of
them. Stern SAM games have none: their sounds are in the main image (one entry per set here).</p>
<h3>Getting the id of your ROMs</h3>
<pre><code>rom2altsound roms ~/vpinball/roms</code></pre>
<p>identifies each zip by its content and prints, for each verified game, its sound board and its sound
ROM id (<code>--json roms.json</code> for the full report). A pack also carries it, as
<code>sound_rom_id</code> in <code>manifest.json</code>. Type the first characters of an id in the search
box above, or open <code>#&lt;id&gt;</code> on this page.</p>
<h3>Sound names</h3>
<p>Names belong to a sound ROM, not to a game version. Type names on a pack's listening page
(<code>index.html</code>), <strong>Export names</strong> saves a <code>names.csv</code> that carries the
sound ROM id; put it in a pack with</p>
<pre><code>rom2altsound names afm_113b names.csv
rom2altsound afm_113b --names names.csv</code></pre>
<p>A <code>names.csv</code> fits every set of the same entry, and is refused for other sound ROMs.
The <em>Names</em> column counts the commands that PinMAME's <a href="{SOUNDS_DAT}">sounds.dat</a>
names for the entry's sets (the names themselves are not reproduced here).</p>
<h3>The columns</h3>
<ul>
<li><strong>Board</strong>: the sound board family (PinMAME's <code>SNDBRD_*</code>), as the game's machine
init starts it, with a link to the family's notes.</li>
<li><strong>rom2altsound</strong>: the result of the <a href="{BLOB}docs/board-support.md">board survey</a>
for the family: <em>Works</em> (sounds come out, distinct, each from silence), <em>Partial</em>,
<em>No sound yet</em>, <em>No sound board</em>. A second badge, <em>not in VPX</em>, marks the families
whose packs VPinball cannot play today because its AltSound does not receive their sound commands;
its tooltip says why.</li>
<li><strong>Sets</strong>: how many of the entry's PinMAME sets were complete in the reference ROM set the
catalog was built from (2804 VPinMAME zips), out of all.</li>
<li><strong>Sounds</strong>: read from the ROMs without running them, where the layout is known: the populated
tracks of the DCS (and Pinball 2000) track catalog, the records of a Stern SAM call table. Empty for
the other boards: their commands are only found by playing them.</li>
</ul>
<h3>Rebuilding the catalog</h3>
<pre><code>rom2altsound catalog ~/vpinball/roms --out docs/catalog.json
python3 docs/build_site.py</code></pre>
<p>The ROMs never leave your machine: the command reads them and writes metadata only.</p>
""",
        "footer": [(REPO, "Source code and issues on GitHub"), (BLOB + "README.md", "README")],
        "footer_note": "rom2altsound is free software (BSD-3-Clause). PinMAME and Visual Pinball are separate "
                       "projects by their own authors.",
        "loading": "Loading the catalog…",
    },
    "fr": {
        "title": "Catalogue des ROM son · rom2altsound",
        "description": "Toutes les ROM son de flipper que connaît PinMAME, groupées par id de ROM son : les jeux "
                       "et révisions qui la partagent, leur carte son, ce que rom2altsound en tire, les sommes de "
                       "contrôle des fichiers et le nombre de pistes lues dans les ROM.",
        "other": ("en", "English version", "gb"),
        "nav": [("#catalog", "Catalogue"), ("#about", "À propos"), (REPO, "GitHub")],
        "eyebrow": "rom2altsound",
        "h1": "Catalogue des ROM son",
        "catalog": "Catalogue",
        "lead": "Toutes les ROM son de flipper que connaît le PinMAME de rom2altsound, groupées par "
                "<strong>id de ROM son</strong> : une entrée par jeu de ROM son, avec chaque révision du jeu qui "
                "le joue, sa carte son, ce que rom2altsound tire de cette carte, la taille et les sommes de "
                "contrôle des fichiers, et le nombre de pistes ou d'appels lus directement dans les ROM.",
        "note": "Des données seulement : ce site ne contient ni ROM ni son, seulement des noms, des tailles, des "
                "sommes de contrôle et des nombres. Les images de ROM sont protégées par le droit d'auteur : "
                "utilisez les vôtres.",
        "family": "Famille de carte", "support": "Prise en charge", "maker": "Fabricant",
        "cols": [("title", "Jeu", ""), ("maker", "Fabricant", "hide-sm"), ("year", "Année", ""),
                 ("family", "Carte", ""), ("support", "rom2altsound", ""), ("sets", "Jeux de ROM", "num"),
                 ("sounds", "Sons", "num"), ("names", "Noms", "num"), ("id", "Id de ROM son", "hide-sm")],
        "about": f"""
<h2 id="about">À propos</h2>
<h3>Ce qu'est un id de ROM son</h3>
<p>Le SHA-1 des SHA-1 des ROM son d'un jeu : les SHA-1 distincts (hexadécimal minuscule) des fichiers que
PinMAME charge pour la carte son, triés, chacun suivi d'un saut de ligne. Toutes les révisions d'un jeu qui
ont gardé leurs ROM son ont le même id : un seul pack AltSound, et une seule liste de noms de sons, sert à
toutes. Les Stern SAM n'en ont pas : leurs sons sont dans l'image principale (une entrée par jeu de ROM
ici).</p>
<h3>Obtenir l'id de vos ROM</h3>
<pre><code>rom2altsound roms ~/vpinball/roms</code></pre>
<p>identifie chaque zip par son contenu et affiche, pour chaque jeu vérifié, sa carte son et son id de ROM
son (<code>--json roms.json</code> pour le rapport complet). Un pack le porte aussi, dans
<code>manifest.json</code> (<code>sound_rom_id</code>). Tapez les premiers caractères d'un id dans la
recherche ci-dessus, ou ouvrez <code>#&lt;id&gt;</code> sur cette page.</p>
<h3>Les noms des sons</h3>
<p>Les noms appartiennent à une ROM son, pas à une version du jeu. Tapez des noms sur la page d'écoute d'un
pack (<code>index.html</code>) ; <strong>Export names</strong> enregistre un <code>names.csv</code> qui porte
l'id de ROM son ; mettez-le dans un pack avec</p>
<pre><code>rom2altsound names afm_113b names.csv
rom2altsound afm_113b --names names.csv</code></pre>
<p>Un <code>names.csv</code> convient à tous les jeux de ROM d'une même entrée, et il est refusé pour d'autres
ROM son. La colonne <em>Noms</em> compte les commandes que le <a href="{SOUNDS_DAT}">sounds.dat</a> de PinMAME
nomme pour les jeux de ROM de l'entrée (les noms eux-mêmes ne sont pas reproduits ici).</p>
<h3>Les colonnes</h3>
<ul>
<li><strong>Carte</strong> : la famille de carte son (le <code>SNDBRD_*</code> de PinMAME), telle que
l'initialisation de la machine la démarre, avec un lien vers les notes de la famille.</li>
<li><strong>rom2altsound</strong> : le résultat du <a href="{BLOB}docs/board-support.md">relevé des cartes</a>
pour la famille : <em>Fonctionne</em> (les sons sortent, distincts, chacun depuis le silence),
<em>Partiel</em>, <em>Pas encore de son</em>, <em>Pas de carte son</em>. Un second badge, <em>pas dans
VPX</em>, marque les familles dont VPinball ne peut pas jouer les packs aujourd'hui, faute de recevoir
leurs commandes son dans son AltSound ; son infobulle dit pourquoi.</li>
<li><strong>Jeux de ROM</strong> : combien des jeux de ROM PinMAME de l'entrée étaient complets dans la
collection de référence d'où le catalogue est tiré (2804 zips VPinMAME), sur le total.</li>
<li><strong>Sons</strong> : lu dans les ROM sans les faire tourner, là où leur organisation est connue : les
pistes remplies du catalogue DCS (et Pinball 2000), les entrées de la table d'appels d'un Stern SAM. Vide
pour les autres cartes : leurs commandes ne se trouvent qu'en les jouant.</li>
</ul>
<h3>Refaire le catalogue</h3>
<pre><code>rom2altsound catalog ~/vpinball/roms --out docs/catalog.json
python3 docs/build_site.py</code></pre>
<p>Les ROM ne quittent jamais votre machine : la commande les lit et n'écrit que des métadonnées.</p>
""",
        "footer": [(REPO, "Code source et tickets sur GitHub"), (BLOB + "README.md", "README")],
        "footer_note": "rom2altsound est un logiciel libre (BSD-3-Clause). PinMAME et Visual Pinball sont des "
                       "projets distincts, développés par leurs propres auteurs.",
        "loading": "Chargement du catalogue…",
    },
}


def url(lang):
    return SITE + ("fr/" if lang == "fr" else "")


def render(lang):
    u = UI[lang]
    up = "../" if lang == "fr" else ""
    other, other_label, other_flag = u["other"]
    other_href = "../" if lang == "fr" else "fr/"
    verification = (f'<meta name="google-site-verification" content="{GOOGLE_VERIFICATION}">\n'
                    if lang == "en" else "")
    nav = "".join(f'<a href="{h}">{t}</a>' for h, t in u["nav"])
    heads = "".join(
        f'<th data-k="{k}" class="{c}" aria-sort="none"><button type="button">{t}</button></th>'
        for k, t, c in u["cols"])
    footer = "".join(f'<a href="{h}">{t}</a>' for h, t in u["footer"])
    structured = json.dumps({
        "@context": "https://schema.org", "@type": "Dataset", "name": u["h1"],
        "description": u["description"], "url": url(lang), "inLanguage": lang,
        "isAccessibleForFree": True,
        "distribution": {"@type": "DataDownload", "encodingFormat": "application/json",
                         "contentUrl": SITE + "catalog.json"},
    }, ensure_ascii=False)
    return f"""<!doctype html>
<html lang="{lang}" data-base="{up}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{u["title"]}</title>
<meta name="description" content="{u["description"]}">
{verification}<link rel="canonical" href="{url(lang)}">
<link rel="alternate" hreflang="en" href="{url('en')}">
<link rel="alternate" hreflang="fr" href="{url('fr')}">
<link rel="alternate" hreflang="x-default" href="{url('en')}">
<meta property="og:type" content="website">
<meta property="og:site_name" content="rom2altsound">
<meta property="og:title" content="{u["title"]}">
<meta property="og:description" content="{u["description"]}">
<meta property="og:url" content="{url(lang)}">
<meta property="og:locale" content="{'fr_FR' if lang == 'fr' else 'en_GB'}">
<meta name="color-scheme" content="light dark">
<link rel="icon" type="image/svg+xml" href="{up}img/favicon.svg">
<link rel="stylesheet" href="{up}style.css">
<script type="application/ld+json">{structured}</script>
<script defer src="{up}app.js"></script>
</head>
<body>
<header class="site"><div class="wrap wide">
<a class="brand" href="./"><img src="{up}img/favicon.svg" alt="" width="24" height="24"><span>rom2<b>altsound</b></span></a>
<nav class="main">{nav}</nav>
<a class="lang" href="{other_href}" hreflang="{other}" lang="{other}"><img src="{up}img/{other_flag}.svg" alt="" width="21" height="14">{other_label}</a>
</div></header>
<main><div class="wrap wide">
<div class="hero">
<p class="eyebrow">{u["eyebrow"]}</p>
<h1>{u["h1"]}</h1>
<p class="lead">{u["lead"]}</p>
<p class="note">{u["note"]}</p>
</div>
<p id="status" class="count">{u["loading"]}</p>
<div id="app" hidden>
<div id="stats" class="stats"></div>
<section id="detail" hidden aria-live="polite"></section>
<h2 id="catalog">{u["catalog"]}</h2>
<div class="controls">
<label class="grow"><span>&#128269;</span><input type="search" id="q" autocomplete="off" spellcheck="false"></label>
<label>{u["family"]}<select id="family"><option value=""></option></select></label>
<label>{u["support"]}<select id="support"><option value=""></option></select></label>
<label>{u["maker"]}<select id="maker"><option value=""></option></select></label>
<label class="check"><input type="checkbox" id="onlyIds"><span id="lOnlyIds"></span></label>
<label class="check"><input type="checkbox" id="onlyFound"><span id="lOnlyFound"></span></label>
<label class="check"><input type="checkbox" id="hideNoVpx"><span id="lHideNoVpx"></span></label>
</div>
<p id="count" class="count"></p>
<div class="table"><table><thead><tr>{heads}</tr></thead><tbody id="rows"></tbody></table></div>
<p class="more"><button class="btn" type="button" id="more" hidden></button></p>
</div>
<section class="about">
{u["about"].strip()}
</section>
</div></main>
<footer class="site"><div class="wrap wide">
{footer}
<span>{u["footer_note"]}</span>
</div></footer>
</body>
</html>
"""


def mark_vpx_playback():
    """Copy vpx_playback.json into catalog.json: `vpx_not_playable` (label -> reason, per
    language) at the top, `vpx_playable: false` on each entry of those families. Kept in
    the format `rom2altsound catalog` writes (sorted head, one entry per line), so running
    this twice changes nothing."""
    path = DOCS / "catalog.json"
    if not path.exists():
        return
    rules = json.loads((DOCS / "vpx_playback.json").read_text(encoding="utf-8"))
    reasons = {label: rules["reasons"][r] for label, r in rules["families"].items()}
    catalog = json.loads(path.read_text(encoding="utf-8"))
    entries = catalog.pop("entries")
    labels = {e["label"] for e in entries}
    for label in sorted(set(reasons) - labels):
        print(f"vpx_playback.json: no catalog entry has the family {label!r}", file=sys.stderr)
    catalog["vpx_not_playable"] = {k: v for k, v in sorted(reasons.items()) if k in labels}
    for e in entries:
        e.pop("vpx_playable", None)
        if e["label"] in reasons:
            e["vpx_playable"] = False
    compact = {"ensure_ascii": False, "separators": (",", ":")}
    head = json.dumps(catalog, sort_keys=True, **compact)
    text = (head[:-1] + ',"entries":[\n'
            + ",\n".join(json.dumps(e, **compact) for e in entries) + "\n]}\n")
    path.write_text(text, encoding="utf-8")


def main():
    mark_vpx_playback()
    (DOCS / "fr").mkdir(exist_ok=True)
    (DOCS / "index.html").write_text(render("en"), encoding="utf-8")
    (DOCS / "fr" / "index.html").write_text(render("fr"), encoding="utf-8")
    alts = "".join(f'<xhtml:link rel="alternate" hreflang="{l}" href="{url(l)}"/>' for l in ("en", "fr"))
    alts += f'<xhtml:link rel="alternate" hreflang="x-default" href="{url("en")}"/>'
    urls = "\n".join(f"<url><loc>{url(l)}</loc>{alts}</url>" for l in ("en", "fr"))
    (DOCS / "sitemap.xml").write_text(
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:xhtml="http://www.w3.org/1999/xhtml">\n'
        + urls + "\n</urlset>\n", encoding="utf-8")
    (DOCS / ".nojekyll").write_text("", encoding="utf-8")


if __name__ == "__main__":
    main()
