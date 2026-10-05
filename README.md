<img src=".github/flags/gb.svg" height="14" alt="GB"> [English](#english) | <img src=".github/flags/fr.svg" height="14" alt="FR"> [Français](#français)

# rom2altsound

## <a name="english"></a><img src=".github/flags/gb.svg" height="14" alt="GB"> English

**rom2altsound turns a pinball ROM's sounds into an AltSound pack for Visual Pinball.**

> **Beta.** This is a first public version. The packs it writes play in VPinball, but
> expect rough edges, and please report what you find (see [Help and feedback](#help-and-feedback)).

### What it does

Give it a ROM zip (`afm_113b.zip`) and it:

1. runs PinMAME (the emulator VPinball uses) inside the program, without a window;
2. sends every sound command of the ROM to the emulated sound board, one after another;
3. records each one to its own WAV file, all at the same reference volume;
4. finds where music loops and cuts it to its intro plus **one exact cycle**, so that it
   loops without a seam;
5. writes a folder that VPinball's AltSound plugin reads as is: the WAV files,
   `altsound.csv`, `g-sound.csv`, `altsound.ini`, and `manifest.json` with everything
   that was measured.

### Why not PinMAME's own sound dump?

PinMAME can record its output while you play sounds by hand (the sound commander, F6).
That gives one long recording, or one file per sound at best, that you then cut, name,
level and loop yourself, sound by sound. rom2altsound does it all in one go:

- **every command**, named from PinMAME's `sounds.dat`, plus the DCS tracks it does not list;
- **clean files**: each sound starts from silence, with the silence before and after trimmed;
- **one reference volume** for the whole ROM, set the same way the game sets it, loud but
  without clipping, so every sound keeps its level relative to the others;
- **exact loops**, found in the audio and, on DCS boards, in the sound program of the ROM itself;
- **a ready AltSound pack**, not just a pile of WAV files;
- it runs **many times faster than real time** (an AFM ROM takes about 2 minutes).

### Install

**Release binaries** (Linux x86_64/aarch64, Windows x86_64, macOS arm64/x86_64): download
the archive for your system from the
[releases page](https://github.com/Le-Syl21/rom2altsound/releases), unpack it and run
`rom2altsound` from a terminal. The Windows and macOS binaries are signed.

**With Cargo** (needs Rust, CMake and a C/C++ compiler; PinMAME is built along the way,
which takes a few minutes):

```
cargo install --git https://github.com/Le-Syl21/rom2altsound
```

### Usage

```
rom2altsound afm_113b
```

looks for `afm_113b.zip` in the current folder, then in `./roms`, and writes the pack to
`./afm_113b/`. More examples:

```
rom2altsound ~/vpinball/roms/mm_109c.zip           # a ROM given by its zip
rom2altsound afm_113b cv_20h rs_l6 --roms ~/vpinball/roms --out ~/packs
                                                   # three ROMs, two at a time, in ~/packs/<rom>/
rom2altsound afm_113b --jobs 1 --merge-twins       # see "Twins" below
rom2altsound --help                                # every option
```

Several ROMs run two at a time by default (`--jobs` to change it); then each ROM's progress
goes to `rom2altsound.log` in its folder. A ROM that fails does not stop the others, and a
recap is printed at the end.

Then copy the ROM's folder next to your table, as `<table folder>/altsound/<rom>/` (for
example `Tables/Attack from Mars/altsound/afm_113b/`), and turn on the AltSound plugin in
VPinball.

### What you get

```
afm_113b/
├── 0x0001-afm_113b.wav         intro + one loop cycle, with its loop points (WAV smpl chunk)
├── 0x0001-afm_113b-loop.wav    the loop alone (what AltSound plays)
├── 0x0064-afm_113b.wav         one file per command
├── ...
├── altsound.csv                AltSound format (selected in altsound.ini)
├── g-sound.csv                 G-Sound format (set format = g-sound in altsound.ini to use it)
├── altsound.ini                format, and the ROM's volume control turned off
├── manifest.json               every measure: names, lengths, loudness, loops, twins...
├── cold-boot.json              how the factory boot went
└── factory-nvram/afm_113b.nv   the factory settings the ROM was played with
```

In the CSVs, music (loops and sounds named "Music:") goes on the music channel, the rest
plays as sound effects; voice lines are callouts in `g-sound.csv`. Every gain is 100, and
nothing ducks or stops anything else.

**The pack is a starting point.** The artistic pass, i.e. which sounds lower the music
(ducking), which ones stop it, and the gains, is yours to do in an AltSound editor such as
**VPin Studio**.

### Supported boards

| family | examples | status |
|---|---|---|
| Williams/Bally DCS (WPC, 1993-1999) | Attack from Mars, Medieval Madness, Cirqus Voltaire | supported, exact loops |
| Williams WPC89 / WPCS | earlier WPC games | supported (WPCS not tested yet) |
| Williams System 11 | Whirlwind | supported |
| Data East (BSMT) | Guns N' Roses, Batman | supported |
| Sega / Stern Whitestar (BSMT) | Apollo 13, The X-Files | supported |
| Stern SAM | | **not supported** (the sound comes from the game CPU) |
| Bally Cheap Squeak / Turbo Cheap Squeak | | **not supported** (PinMAME cannot drive them) |

### Limits

- **AltSound loops whole files only**, so a looping music plays its loop without the intro
  ([libaltsound issue #14](https://github.com/vpinball/libaltsound/issues/14)). The full
  file, intro + loop with its loop points, is kept next to it for when this is fixed, or for
  other players.
- **Data East and Whitestar music never repeats exactly** (the board's timing drifts a
  little), so it cannot be looped without a seam: those tracks are cut at 2 minutes
  (`--max-secs`). They are on the music channel, so the next music replaces them.
- **Twins**: some ROMs contain the same sound under two or more commands (Attack from Mars
  lists every sound effect twice). They are kept as separate files, because why the ROM has
  them is not known (probably channel or priority variants); `manifest.json` marks them
  with `twin_of`. `--merge-twins` makes both commands use the first file instead.
- About one DCS command in 200 plays nothing on its first try; every silent command is
  played a second time, which recovers them.
- The volume is the one the game uses in attract mode, raised to a common reference level;
  a game that changes its volume during play is not followed.

How it all works, measured ROM by ROM: [docs/how-it-works.md](docs/how-it-works.md).

### Help and feedback

- Project: <https://github.com/Le-Syl21/rom2altsound> (issues welcome)
- Discord: <https://discord.gg/T37DYHmt2j>, channel **#rom2altsound**

### License

GPL-3.0-or-later (see [LICENSE](LICENSE)). rom2altsound includes PinMAME
(<https://github.com/vpinball/pinmame>), under its own license (see
[vendor/pinmame/LICENSE](https://github.com/vpinball/pinmame/blob/master/LICENSE): BSD-3-Clause
for new code, the former MAME license for the rest). You need your own ROM files; none are
included.

---

## <a name="français"></a><img src=".github/flags/fr.svg" height="14" alt="FR"> Français

**rom2altsound transforme les sons d'une ROM de flipper en pack AltSound pour Visual Pinball.**

> **Bêta.** C'est une première version publique. Les packs qu'il écrit se jouent dans
> VPinball, mais il reste sûrement des défauts : merci de signaler ce que vous trouvez
> (voir [Aide et retours](#aide-et-retours)).

### Ce qu'il fait

Donnez-lui une ROM (`afm_113b.zip`), et il :

1. fait tourner PinMAME (l'émulateur utilisé par VPinball) à l'intérieur du programme, sans fenêtre ;
2. envoie une à une toutes les commandes de son de la ROM à la carte son émulée ;
3. enregistre chacune dans son propre fichier WAV, toutes au même volume de référence ;
4. repère les boucles des musiques et les coupe à leur introduction plus **un cycle exact**,
   pour qu'elles bouclent sans raccord audible ;
5. écrit un dossier que le plugin AltSound de VPinball lit tel quel : les fichiers WAV,
   `altsound.csv`, `g-sound.csv`, `altsound.ini`, et `manifest.json` avec toutes les mesures.

### Pourquoi pas l'enregistrement de PinMAME ?

PinMAME sait enregistrer sa sortie pendant que l'on joue les sons à la main (le commandeur
de sons, touche F6). On obtient un long enregistrement, ou au mieux un fichier par son,
qu'il faut ensuite découper, nommer, mettre au bon niveau et faire boucler soi-même, son
par son. rom2altsound fait tout d'un coup :

- **toutes les commandes**, nommées d'après le `sounds.dat` de PinMAME, plus les pistes DCS
  qu'il oublie ;
- **des fichiers propres** : chaque son part du silence, le silence avant et après est retiré ;
- **un seul volume de référence** pour toute la ROM, réglé comme le jeu le règle, fort mais
  sans saturer, pour que chaque son garde son niveau par rapport aux autres ;
- **des boucles exactes**, trouvées dans le son et, sur les cartes DCS, dans le programme
  sonore de la ROM elle-même ;
- **un pack AltSound prêt**, pas seulement un tas de fichiers WAV ;
- il tourne **bien plus vite que le temps réel** (une ROM d'Attack from Mars prend environ 2 minutes).

### Installation

**Binaires prêts à l'emploi** (Linux x86_64/aarch64, Windows x86_64, macOS arm64/x86_64) :
téléchargez l'archive pour votre système sur la
[page des versions](https://github.com/Le-Syl21/rom2altsound/releases), décompressez-la
et lancez `rom2altsound` depuis un terminal. Les binaires Windows et macOS sont signés.

**Avec Cargo** (il faut Rust, CMake et un compilateur C/C++ ; PinMAME est compilé au
passage, ce qui prend quelques minutes) :

```
cargo install --git https://github.com/Le-Syl21/rom2altsound
```

### Utilisation

```
rom2altsound afm_113b
```

cherche `afm_113b.zip` dans le dossier courant, puis dans `./roms`, et écrit le pack dans
`./afm_113b/`. D'autres exemples :

```
rom2altsound ~/vpinball/roms/mm_109c.zip           # une ROM donnée par son fichier zip
rom2altsound afm_113b cv_20h rs_l6 --roms ~/vpinball/roms --out ~/packs
                                                   # trois ROM, deux à la fois, dans ~/packs/<rom>/
rom2altsound afm_113b --jobs 1 --merge-twins       # voir « Jumeaux » plus bas
rom2altsound --help                                # toutes les options
```

Plusieurs ROM sont traitées deux à la fois par défaut (`--jobs` pour changer) ; le détail
de chacune va alors dans `rom2altsound.log`, dans son dossier. Une ROM qui échoue n'arrête
pas les autres, et un récapitulatif s'affiche à la fin.

Copiez ensuite le dossier de la ROM à côté de votre table, en
`<dossier de la table>/altsound/<rom>/` (par exemple
`Tables/Attack from Mars/altsound/afm_113b/`), et activez le plugin AltSound dans VPinball.

### Ce que vous obtenez

```
afm_113b/
├── 0x0001-afm_113b.wav         intro + un cycle de boucle, avec ses points de boucle (bloc WAV smpl)
├── 0x0001-afm_113b-loop.wav    la boucle seule (ce que joue AltSound)
├── 0x0064-afm_113b.wav         un fichier par commande
├── ...
├── altsound.csv                format AltSound (celui choisi dans altsound.ini)
├── g-sound.csv                 format G-Sound (mettre format = g-sound dans altsound.ini pour l'utiliser)
├── altsound.ini                le format, et le contrôle du volume par la ROM désactivé
├── manifest.json               toutes les mesures : noms, durées, niveaux, boucles, jumeaux...
├── cold-boot.json              le déroulé du démarrage en réglages d'usine
└── factory-nvram/afm_113b.nv   les réglages d'usine avec lesquels la ROM a été jouée
```

Dans les CSV, la musique (les boucles et les sons nommés « Music: ») va sur la voie
musique, le reste se joue comme des effets sonores ; les voix sont des « callouts » dans
`g-sound.csv`. Tous les gains sont à 100, et aucun son ne baisse ni n'arrête les autres.

**Le pack est un point de départ.** Le travail artistique, c'est-à-dire quels sons baissent
la musique (« ducking »), lesquels l'arrêtent, et les gains, reste à faire dans un éditeur
AltSound comme **VPin Studio**.

### Cartes son prises en charge

| famille | exemples | état |
|---|---|---|
| Williams/Bally DCS (WPC, 1993-1999) | Attack from Mars, Medieval Madness, Cirqus Voltaire | oui, boucles exactes |
| Williams WPC89 / WPCS | premiers jeux WPC | oui (WPCS pas encore testé) |
| Williams System 11 | Whirlwind | oui |
| Data East (BSMT) | Guns N' Roses, Batman | oui |
| Sega / Stern Whitestar (BSMT) | Apollo 13, The X-Files | oui |
| Stern SAM | | **non** (le son vient du processeur du jeu) |
| Bally Cheap Squeak / Turbo Cheap Squeak | | **non** (PinMAME ne sait pas les piloter) |

### Limites

- **AltSound ne fait boucler que des fichiers entiers** : une musique qui boucle est donc
  jouée sans son introduction
  ([ticket libaltsound n° 14](https://github.com/vpinball/libaltsound/issues/14)). Le
  fichier complet, intro + boucle avec ses points de boucle, est gardé à côté, pour le
  jour où ce sera corrigé ou pour d'autres lecteurs.
- **La musique Data East et Whitestar ne se répète jamais exactement** (le rythme de la
  carte dérive un peu), on ne peut donc pas la faire boucler sans raccord : ces morceaux
  sont coupés à 2 minutes (`--max-secs`). Ils sont sur la voie musique, la musique
  suivante les remplace.
- **Jumeaux** : certaines ROM contiennent le même son sous deux commandes ou plus (Attack
  from Mars liste chaque effet sonore deux fois). Ils sont gardés en fichiers séparés, car
  on ne sait pas pourquoi la ROM les a (sans doute des variantes de voie ou de priorité) ;
  `manifest.json` les signale avec `twin_of`. `--merge-twins` fait utiliser le premier
  fichier aux deux commandes.
- Environ une commande DCS sur 200 ne joue rien au premier essai ; chaque commande muette
  est rejouée une seconde fois, ce qui les récupère.
- Le volume est celui que le jeu utilise en mode attraction, monté à un niveau de
  référence commun ; un jeu qui change de volume en cours de partie n'est pas suivi.

Le fonctionnement détaillé, mesuré ROM par ROM (en anglais) :
[docs/how-it-works.md](docs/how-it-works.md).

### Aide et retours

- Projet : <https://github.com/Le-Syl21/rom2altsound> (tickets bienvenus)
- Discord : <https://discord.gg/T37DYHmt2j>, salon **#rom2altsound**

### Licence

GPL-3.0 ou ultérieure (voir [LICENSE](LICENSE)). rom2altsound contient PinMAME
(<https://github.com/vpinball/pinmame>), sous sa propre licence (voir
[vendor/pinmame/LICENSE](https://github.com/vpinball/pinmame/blob/master/LICENSE) :
BSD-3-Clause pour le code récent, l'ancienne licence MAME pour le reste). Il vous faut vos
propres fichiers de ROM ; aucun n'est fourni.
