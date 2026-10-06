<img src=".github/flags/gb.svg" height="14" alt="GB"> [English](#english) | <img src=".github/flags/fr.svg" height="14" alt="FR"> [Français](#français)

# rom2altsound

## <a name="english"></a><img src=".github/flags/gb.svg" height="14" alt="GB"> English

**rom2altsound turns a pinball ROM's sounds into an AltSound pack for Visual Pinball.**

> **Beta.** The packs it writes play in VPinball, but
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
rom2altsound afm_113b --check-ducking              # DCS: replay the ducking and check it
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

Every gain is 100: the files already carry the ROM's own levels.

**On DCS boards (Williams/Bally 1993-1999), the mix comes from the ROM itself.** Each DCS
sound command is a small program that says which of the board's channels it plays on and
how much it lowers the others while it plays. rom2altsound reads those programs, so in the
pack:

- the music is the music channel (a new music replaces the previous one);
- the one voice channel that has no twin (on Attack from Mars, the General) becomes the
  jingle channel, where a new line cuts the previous one, as on the machine;
- each sound lowers the music (**DUCK**) exactly as much as the ROM does: on Attack from
  Mars a callout lowers it by 2.4 or 3.5 dB (DUCK 76 or 67), a fanfare by 16 to 24 dB
  (DUCK 15 to 7). `g-sound.csv` gets the same depths as ducking profiles in
  `altsound.ini`;
- `--check-ducking` plays the music with one sound per depth on top and checks that the
  emulated board really lowers it that much (within 0.5 dB).

AltSound cannot do everything the board does: it gives the music its level back at once
when a sound ends, where the board fades it back over about 0.15 s, and when two sounds
overlap it keeps only the deepest duck, where the board adds them up
([libaltsound issue #15](https://github.com/vpinball/libaltsound/issues/15)).

**On the other boards**, the sound programs are code for the board's own processor, with
nothing that says how one sound changes another, so the pack makes nothing up: music on
the music channel (loops and sounds named "Music:"), the rest as sound effects, voice lines
as callouts in `g-sound.csv`, no ducking (DUCK 100) and no stops (STOP 0).

**The pack is a starting point.** The artistic pass, i.e. changing what the ROM does, the
ducking on non-DCS boards, and the gains, is yours to do in an AltSound editor such as
**VPin Studio**.

### Supported boards

What rom2altsound gets out of each sound board family:

| family | sounds | exact loops | reference volume | ducking (DUCK) | stops (STOP) | channels (CHANNEL / TYPE) |
|---|---|---|---|---|---|---|
| Williams/Bally WPC DCS (1993-1999) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ ¹ |
| Williams WPCS (1991-1993) | ✅ ² | ⚠️ ⁵ | ⚠️ ⁸ | ❌ | ❌ | ❌ |
| Williams System 11 | ✅ | ⚠️ ³ | ❌ ⁴ | ❌ | ❌ | ❌ |
| Data East (BSMT) | ✅ | ⚠️ ⁵ | ⚠️ ⁶ | ❌ | ❌ | ❌ |
| Sega / Stern Whitestar (BSMT) | ✅ | ⚠️ ⁵ | ✅ | ❌ | ❌ | ❌ |
| Stern SAM | ❌ ⁷ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Bally Cheap Squeak / Turbo Cheap Squeak | ❌ ⁷ | ❌ | ❌ | ❌ | ❌ | ❌ |

✅ verified, ⚠️ partial, ❌ not available. "Reference volume": every sound recorded at one
loud volume set the way the game sets it, and the game's own (factory) volume read and
reported. Where ducking, stops and channels are ❌, the pack has the defaults: DUCK 100,
STOP 0, music (loops and "Music:" names) on the music channel, the rest polyphonic.

1. Read in the ROM's own sound programs and measured (`--check-ducking`) on Attack from Mars;
   sounds, loops and volume also on Cirqus Voltaire, Medieval Madness and Red & Ted's
   Road Show.
   AltSound keeps the music and one voice channel exclusive; see Limits.
2. Verified on Twilight Zone (302 of its 307 commands, named from sounds.dat). The
   earlier WPC89 sound board was not tested.
3. Found in the audio only; this board does not replay a sound sample-exactly.
4. The board has no volume stage.
5. The music never repeats exactly (Twilight Zone: none of its 45 music tracks within
   4 minutes): it is cut at 2 minutes (`--max-secs`).
6. Music volume only: the master volume is a hardware knob.
7. Cannot be driven: on SAM the sound comes from the game CPU, and PinMAME cannot send
   commands to Cheap Squeak / Turbo Cheap Squeak.
8. The game's factory volume (`79 vv ~vv`) is read, but the sounds are recorded at it: no
   reference volume for this board yet.

### Limits

- **AltSound loops whole files only**, so a looping music plays its loop without the intro
  ([libaltsound issue #14](https://github.com/vpinball/libaltsound/issues/14)). The full
  file, intro + loop with its loop points, is kept next to it for when this is fixed, or for
  other players.
- **Data East and Whitestar music never repeats exactly** (the board's timing drifts a
  little), so it cannot be looped without a seam: those tracks are cut at 2 minutes
  (`--max-secs`). They are on the music channel, so the next music replaces them.
- **Twins**: some ROMs contain the same sound under two or more commands (Attack from Mars
  lists every sound effect twice). On DCS the reason is the board's channels: each command
  has a home channel, and a new command on a channel cuts what was playing there. Attack
  from Mars puts its sound effects on channels 1 and 2, and its Martian voices and effects
  on 4 and 5, as identical pairs (107 and 82 pairs), so the game can send a sound to the
  free channel of the pair and let two copies overlap instead of cutting each other. The
  General's voice (channel 3) has no twin, so a new line cuts the previous one. Twins stay
  separate by default because they carry this channel information; `manifest.json` marks
  them with `twin_of` and `twin_reason`. `--merge-twins` only shares the WAV file: the CSV
  rows stay distinct.
- **What AltSound cannot reproduce on DCS**: the ROM's fade back after a duck (AltSound
  restores the music at once), overlapping ducks adding up (AltSound keeps the deepest,
  [issue #15](https://github.com/vpinball/libaltsound/issues/15)), a music change that waits
  for the end of a musical phrase, and the board's other exclusive channels (only the music
  and one voice channel cut their previous sound).
- About one DCS command in 200 plays nothing on its first try; every silent command is
  played a second time, which recovers them.
- The volume is the one the game uses in attract mode, raised to a common reference level;
  a game that changes its volume during play is not followed.

How it all works, measured ROM by ROM: [docs/how-it-works.md](docs/how-it-works.md).

### Help and feedback

- Project: <https://github.com/Le-Syl21/rom2altsound> (issues welcome)
- Discord: <https://discord.gg/T37DYHmt2j>, channel **#rom2altsound**

### License

BSD-3-Clause (see [LICENSE](LICENSE)), the license PinMAME is moving to. rom2altsound includes PinMAME
(<https://github.com/vpinball/pinmame>), under its own license (see
[vendor/pinmame/LICENSE](https://github.com/vpinball/pinmame/blob/master/LICENSE): BSD-3-Clause
for new code, the former MAME license for the rest). You need your own ROM files; none are
included.

The release binaries embed PinMAME, so they are distributed under PinMAME's terms as well: free of charge, with the source available here.

---

## <a name="français"></a><img src=".github/flags/fr.svg" height="14" alt="FR"> Français

**rom2altsound transforme les sons d'une ROM de flipper en pack AltSound pour Visual Pinball.**

> **Bêta.** Les packs qu'il écrit se jouent dans
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
rom2altsound afm_113b --check-ducking              # DCS : rejoue le ducking et le vérifie
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

Tous les gains sont à 100 : les fichiers ont déjà les niveaux de la ROM.

**Sur les cartes DCS (Williams/Bally 1993-1999), le mixage vient de la ROM elle-même.**
Chaque commande de son DCS est un petit programme qui dit sur quelle voie de la carte elle
joue et de combien elle baisse les autres pendant qu'elle joue. rom2altsound lit ces
programmes ; dans le pack :

- la musique va sur la voie musique (une nouvelle musique remplace la précédente) ;
- la seule voie de voix sans jumelle (sur Attack from Mars, le Général) devient la voie
  « jingle », où une nouvelle phrase coupe la précédente, comme sur la machine ;
- chaque son baisse la musique (**DUCK**) exactement autant que la ROM : sur Attack from
  Mars une voix la baisse de 2,4 ou 3,5 dB (DUCK 76 ou 67), une fanfare de 16 à 24 dB
  (DUCK 15 à 7). `g-sound.csv` reçoit les mêmes profondeurs, en profils de ducking dans
  `altsound.ini` ;
- `--check-ducking` joue la musique avec un son par profondeur par-dessus et vérifie que la
  carte émulée la baisse vraiment d'autant (à 0,5 dB près).

AltSound ne sait pas tout faire comme la carte : il rend son niveau à la musique d'un coup
quand un son se termine, alors que la carte le remonte en fondu sur environ 0,15 s, et quand
deux sons se chevauchent il ne garde que la baisse la plus forte, alors que la carte les
additionne ([ticket libaltsound n° 15](https://github.com/vpinball/libaltsound/issues/15)).

**Sur les autres cartes**, les programmes sonores sont du code pour le processeur de la
carte, sans rien qui dise comment un son en change un autre : le pack n'invente donc rien.
La musique va sur la voie musique (les boucles et les sons nommés « Music: »), le reste se
joue comme des effets sonores, les voix sont des « callouts » dans `g-sound.csv`, sans
ducking (DUCK 100) ni arrêt (STOP 0).

**Le pack est un point de départ.** Le travail artistique, c'est-à-dire modifier ce que
fait la ROM, le ducking sur les cartes non DCS, et les gains, reste à faire dans un éditeur
AltSound comme **VPin Studio**.

### Cartes son prises en charge

Ce que rom2altsound sait tirer de chaque famille de carte son :

| famille | sons | boucles exactes | volume de référence | ducking (DUCK) | arrêts (STOP) | voies (CHANNEL / TYPE) |
|---|---|---|---|---|---|---|
| Williams/Bally WPC DCS (1993-1999) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ ¹ |
| Williams WPCS (1991-1993) | ✅ ² | ⚠️ ⁵ | ⚠️ ⁸ | ❌ | ❌ | ❌ |
| Williams System 11 | ✅ | ⚠️ ³ | ❌ ⁴ | ❌ | ❌ | ❌ |
| Data East (BSMT) | ✅ | ⚠️ ⁵ | ⚠️ ⁶ | ❌ | ❌ | ❌ |
| Sega / Stern Whitestar (BSMT) | ✅ | ⚠️ ⁵ | ✅ | ❌ | ❌ | ❌ |
| Stern SAM | ❌ ⁷ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Bally Cheap Squeak / Turbo Cheap Squeak | ❌ ⁷ | ❌ | ❌ | ❌ | ❌ | ❌ |

✅ vérifié, ⚠️ partiel, ❌ non disponible. « Volume de référence » : tous les sons sont
enregistrés à un même volume fort, réglé comme le jeu le règle, et le volume d'usine du jeu
est lu et indiqué. Là où ducking, arrêts et voies sont à ❌, le pack a les valeurs par
défaut : DUCK 100, STOP 0, la musique (boucles et noms « Music: ») sur la voie musique, le
reste joué en parallèle.

1. Lus dans les programmes sonores de la ROM et mesurés (`--check-ducking`) sur Attack from
   Mars ; sons, boucles et volume aussi sur Cirqus Voltaire, Medieval Madness et
   Red & Ted's Road Show. AltSound ne garde exclusives que la musique et une voie de voix ; voir
   Limites.
2. Vérifié sur Twilight Zone (302 de ses 307 commandes, nommées d'après sounds.dat). La
   carte son WPC89, plus ancienne, n'a pas été testée.
3. Trouvées dans le son seulement ; cette carte ne rejoue pas un son à l'échantillon près.
4. La carte n'a pas d'étage de volume.
5. La musique ne se répète jamais exactement (Twilight Zone : aucun de ses 45 morceaux en
   4 minutes) : elle est coupée à 2 minutes (`--max-secs`).
6. Volume de la musique seulement : le volume général est un bouton matériel.
7. Impossible à piloter : sur SAM le son vient du processeur du jeu, et PinMAME ne sait pas
   envoyer de commandes aux Cheap Squeak / Turbo Cheap Squeak.
8. Le volume d'usine du jeu (`79 vv ~vv`) est lu, mais les sons sont enregistrés à ce
   volume : pas encore de volume de référence pour cette carte.

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
  from Mars liste chaque effet sonore deux fois). Sur DCS, la raison vient des voies de la
  carte : chaque commande a sa voie, et une nouvelle commande sur une voie coupe ce qui y
  jouait. Attack from Mars met ses effets sonores sur les voies 1 et 2, et les voix et
  effets des Martiens sur 4 et 5, en paires identiques (107 et 82 paires) : le jeu peut
  envoyer un son sur la voie libre de la paire, et deux copies se superposent au lieu de se
  couper. La voix du Général (voie 3) n'a pas de jumeau : une nouvelle phrase coupe la
  précédente. Les jumeaux restent séparés par défaut, car ils portent cette information de
  voie ; `manifest.json` les signale avec `twin_of` et `twin_reason`. `--merge-twins` ne
  fait que partager le fichier WAV : les lignes des CSV restent distinctes.
- **Ce qu'AltSound ne reproduit pas sur DCS** : la remontée en fondu de la musique après
  une baisse (AltSound la remet d'un coup), les baisses qui s'additionnent quand des sons se
  chevauchent (AltSound garde la plus forte,
  [ticket n° 15](https://github.com/vpinball/libaltsound/issues/15)), un changement de
  musique qui attend la fin d'une phrase musicale, et les autres voies exclusives de la
  carte (seules la musique et une voie de voix coupent leur son précédent).
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

BSD-3-Clause (voir [LICENSE](LICENSE)), la licence vers laquelle PinMAME migre. rom2altsound contient PinMAME
(<https://github.com/vpinball/pinmame>), sous sa propre licence (voir
[vendor/pinmame/LICENSE](https://github.com/vpinball/pinmame/blob/master/LICENSE) :
BSD-3-Clause pour le code récent, l'ancienne licence MAME pour le reste). Il vous faut vos
propres fichiers de ROM ; aucun n'est fourni.

Les binaires publiés embarquent PinMAME : ils sont donc aussi distribués selon ses conditions, gratuitement et avec les sources disponibles ici.
