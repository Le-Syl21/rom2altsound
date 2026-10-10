# How rom2altsound works

Technical reference of the extraction engine. For installing and using rom2altsound, see
the [README](../README.md).

rom2altsound extracts a pinball ROM's sounds by running PinMAME's emulation in-process and
driving its sound board directly from Rust: no keyboard simulation, no `-key_script`, no use
of PinMAME's sound-commander UI, and no patch to PinMAME.

```
rom2altsound <rom>... [--roms <dir>] [--out <dir>] [--jobs N] [--no-factory | --volume factory|reference
             | --no-volume-init] [--dcs-volume HH] [--wpcs-volume HH] [--whitestar-volume HH]
             [--only 0x0186,0x0002,...]
             [--limit N] [--boot-secs S] [--boot-max-secs S] [--max-secs S] [--loop-max-secs S]
             [--no-sound-secs S] [--stop 0xHHHH] [--dc-block] [--vpm <dir>] [--no-altsound]
             [--merge-twins] [--intro-loop-secs S] [--check-ducking]
rom2altsound loop-scan [--hint SECS | --hint-frames F] <wav>...    # the loop detector alone
rom2altsound dcs-effects <region.bin> <rom> [--json F]   # DCS track programs (diagnostic)
rom2altsound duck-fit <M.wav> <C.wav> <MC.wav> <at_secs> [--win S]  # music gain under a sound
rom2altsound drift-check <A.wav> <B.wav>                 # does a board replay sample-exactly?
rom2altsound roms <dir|zip>... [--json F] [--fix-names DIR] [--deep] [-q] [--dump-table F]
                                                         # identify and check ROM zips
```

The three diagnostics are not in `--help`. `dcs-effects` reads a region dumped with the
hidden `--dump-sound-region <file>`; `duck-fit` takes recordings of `--only` scenarios
(`--only 0x000C,0x01B6,0x000C+3+0x01B6`: the last one sends `000C`, waits 3 s and sends
`01B6`, in one recording).

Each ROM runs in a child process of its own (`--in-process`, internal): libpinmame runs one
machine per process.

## Build

`build.rs` builds PinMAME's static library from the `vendor/pinmame` submodule with the
`cmake` crate. The submodule points at a fork (Le-Syl21/pinmame, branch `bsmt2000-lle`):
upstream master plus the BSMT2000 low level emulation and the Cheap Squeak / Turbo Cheap
Squeak manual commands (see "Per family" and "BSMT2000: the chip's own program"). `cmake/libpinmame/CMakeLists.txt` expects to be at the root of the source
tree (PinMAME's CI copies it there), so a patched copy is generated in Cargo's `OUT_DIR`
with every tree-relative path made absolute; nothing is written into the submodule. Every
patch must match, so a PinMAME update that moves things fails the build instead of
producing a different library. Link-time optimization is turned off: the upstream file turns
it on for Release, which would leave GCC bytecode (or MSVC `/GL` objects) in the archive.

PinMAME compiles its own zlib (`ext/zlib`), but its CMake file only puts those headers on
the include path on Windows; elsewhere the build picked the system's `zlib.h` and failed on
a Linux host without `zlib1g-dev`. `build.rs` adds `ext/zlib` on every platform, so the
headers always match the vendored sources and no zlib development package is needed.

`build.rs` also reads PinMAME's SAM driver (`src/wpc/sam.c`) for its `SAM1_ROM32MB` /
`SAM1_ROM128MB` sets (set name, image file, CRC32, length) and writes them to
`OUT_DIR/sam_sets.rs`: a SAM ROM is extracted statically, so it must be known as one
before PinMAME runs.

`build.rs` compiles `shim/shim.c` with the exact defines and include paths CMake used for the
library (CMake writes them out with `file(GENERATE)`, whatever the generator), so the shim
sees PinMAME's structures with the right layout.

## How it works

Linking libpinmame statically makes its internal globals reachable: `throttle`,
`sndbrd_manCmd`, `sndbrd_typestr`, `sndbrd_exists`. The few things that need PinMAME's structs
(the machine's CPU list, the sound board interface table, the game's hardware generation)
are in a C shim.

1. The ROM zip (and its parent's) is linked (copied on Windows) into a private `--vpm` dir,
   so nvram/cfg writes stay there. Default: the user cache directory (`$XDG_CACHE_HOME` or
   `~/.cache` on Linux, `~/Library/Caches` on macOS, `%LOCALAPPDATA%` on Windows) +
   `rom2altsound/vpm-factory/<rom>` (`rom2altsound/vpm` with `--no-factory`), never the
   current directory. libpinmame runs with INT16 audio at 44100 Hz and no message API, so
   `osd_update_audio_stream` calls our audio callback once per emulated frame, on the emulation
   thread. That callback is both the clock (samples = emulated time) and the place where the
   hardware is driven, between two frames. This is the same spot as PinMAME's own commander,
   since `sound_update()` and the commander both run from `updatescreen()`.
2. In the first callback `throttle` is set to 0. Measured speed: x21 to x55 real time
   (whirl_l3 x21-24, afm_113b x36-54), against x1.0 with `--throttled`.
3. Boot: every byte the game CPU sends to a sound board is logged with its emulated time
   (`cb_OnSoundCommand`; on Pinball 2000, whose PC writes 16-bit words to the board
   without that callback, every word the board's DSP takes, as its two bytes: see Pinball
   2000 below). The boot lasts at least `--boot-secs` (15 s), then until 3 s pass
   without a byte that is new (a (previous byte, byte) pair not sent before: afm_113b polls
   the silent track `03 D3` three times a second forever, whirl_l3 repeats `1F`), and on DCS
   until the game's master volume was seen; at most `--boot-max-secs` (60 s). The manifest
   says which of `quiet`, `repeats` or `max` ended it. Then every game CPU is halted, using
   the commander's own selection (`cpu_type && cpu_flags == 0`). Audio CPUs keep running.
   On the boards that take no command (Stern SB-300) the game CPU is the
   one that plays the sounds: it keeps running, and each sound is asked for through the
   game's own sound layer, read in its program
   ([game-driven boards](families/common.md#game-driven-boards)).
   If the halt split a Whitestar `FE xx FD` (see Volume), the missing `FD` is sent.
4. After 0.5 s, so that any half-sent command expires, the tool sends the stop command and
   waits for silence. With the factory settings (the default) it then sends the reference
   master volume on the boards that have one, where the files are recorded before they are
   scaled to the game's factory volume (`--volume reference`: the same, not scaled; a
   WPCS or Whitestar board the game left at its power-on level gets nothing; see Factory
   mode). With `--no-factory` it sends `55 AA HH ~HH` to every DCS board
   (`--dcs-volume HH`, default `FF` = 0 dB) and waits for silence again. With
   `--no-volume-init` it sends no volume: the boards play at whatever the game set.
5. Before every command, outside its recording, the tool sends what keeps one command's
   state from leaking into the next (`refreshed_before_each_command`), then waits for
   silence again: the Data East music volume, or the Whitestar master volume (with the
   factory settings the reference one, as recorded; else the game's when it kept re-sending it during boot; see the per-family notes).
6. For each command, it sends one byte per `sndbrd_manCmd` call every 4th frame, as in
   `snd_cmd.c` `playCmd`, except on DCS: one byte per frame (see DCS below), and on WPCS,
   where the bytes of a command go out back to back, as the game sends them (see WPCS
   below). Two-board
   machines take (board, byte) pairs. Recording starts with the frame after the first byte.
   It ends when no sound started `--no-sound-secs` (1.5 s) after the last byte
   (`ended_by: "no_sound"`, no file), after 2 s of emulated silence (`"silence"`), once one
   exact cycle of a loop is confirmed (`"loop"`, see Loops) or, when none is found within
   `--loop-max-secs` (240 s), cut at `--max-secs` (`"max"`); both are
   `looping_or_truncated: true`. Leading and trailing silence are
   trimmed, and so is a tail of held DC levels (see Trimming); what is left under 20 ms is a
   `blip` (counted, not written: System 11 DAC steps of 1-2 ms). Then the stop command goes
   out and the tool waits for 0.5 s of silence (at most 10 s).
7. If the stop does not bring silence, the tool resets the sound boards (DCS and WPCS through
   their control port, which is how the game resets them and reboots the DCS DSP; other boards
   by pulsing the audio CPUs' reset line), waits for 4 s of silence, and sends the volume
   again since a reset loses it: the master volume the files are recorded at (reference,
   or `--dcs-volume`), and the game's own last other volume commands, byte for
   byte (`volume_replays` in the manifest). If even that fails, the next
   file is marked `clean_start: false`. Nothing is only reported on stderr. A board that
   is still not silent after 3 waits for quiet in a row with no command played in between
   (each ending in a reset) ends the run with an error instead of being reset forever:
   what keeps it playing is then something sent between two commands (the volume, the
   refresh), or a stop that does not work there (`--no-volume-init`, `--stop`).
8. Retry pass: every command that played nothing (`no_sound`) is played once more, after
   the stop, the volume and the refresh; the result says `retried: true` (and, if it played
   then, holds the second try).
9. Master volume check, where the board has a master volume (DCS, Whitestar): the written
   files more than 5 LU above the ROM's median file are played again 8 master volume levels
   away, together with a reference (the non-loop file closest to the median, same board).
   It needs at least 3 written non-loop files (else the manifest says why it did not run).
   A file whose level does not follow the reference's (it moves by more than half the
   reference's move away from it, and at least 3 LU) is flagged `ignores_master_volume` and
   left out of the loudness totals. These replays are only measured, never written.
10. Output: `<out>/0xHHHH-<rom>.wav` (16-bit, the stream's channel count) and
   `<out>/manifest.json`, rewritten after every command, then a summary on stdout.
11. AltSound pack (unless `--no-altsound`, see below): loop points, twins, `altsound.csv`,
   `g-sound.csv`, `altsound.ini`, from the recordings.
12. Factory mode: the files written again at the factory volume (see Factory mode, step 5).
13. `sound_rom_id` into `manifest.json`, then `--names` (see "Sound names" below).
14. Listening page (unless `--no-html`, see below): `<out>/index.html`.

### AltSound pack

Written in the ROM's folder once the extraction is done (`src/altsound.rs`), so that the
folder can be dropped as `<table folder>/altsound/<rom>/` (the AltSound plugin of VPinball
looks there first, then in `<table folder>/pinmame/altsound/<rom>/`, then in the global
AltSound folder).

- **Loop points.** `<id>-<rom>.wav` of a loop (intro + one exact cycle) gets a `smpl`
  chunk with one forward loop: start = `intro_samples`, end = `intro_samples +
  period_samples - 1` (the chunk's end is inclusive), sample frames. libaltsound loops
  whole files only, from their first sample (its decoder seeks back to frame 0 at the end;
  the `smpl` chunk is not read: libaltsound issue
  [#14](https://github.com/vpinball/libaltsound/issues/14)). So what the CSVs reference
  depends on the intro (`pack.file_kind` in the manifest, with `pack.file_reason`):
  - **`intro_loop_extended`**: a loop with an **intro of its own**, at least
    `OWN_INTRO_MIN_SECS` (50 ms) before the repetition starts (`repeats_from_samples`, see
    Loops) and louder than `OWN_INTRO_MIN_PEAK` (32 LSB) there. The row plays
    `<id>-<rom>-extended.wav`: the intro, then the body copied back to back until the file
    is `--intro-loop-secs` long (300 s by default; whole cycles, so a little longer), with
    the same `smpl` loop points. Each joint is the body's own end-to-start joint, the one a
    player looping `-loop.wav` plays, so it is as seamless. `LOOP` = 0: played once, the
    game's next music replaces it. In `g-sound.csv` its `TYPE` stays `music` (the type is
    what makes it the music, which other types duck and the next music replaces), and
    libaltsound loops every music sample: past the file's length it starts again from the
    intro. Attack from Mars reported by deadmanworking: `0009` (Martian attack: a fanfare,
    then the loop) and `000A` (the same loop, played on) had the same `-loop.wav`, and the
    fanfare was lost.
  - **`body_loop`**: a loop without an intro of its own (or `--intro-loop-secs 0`): the
    body alone, `<id>-<rom>-loop.wav` (or the file itself for a loop that holds from the
    first sample), `LOOP` = 100.
  - **`one_shot`**: a sound that ends, or one cut at `--max-secs` without a loop:
    `<id>-<rom>.wav`, `LOOP` = 0.

  On afm_113b, 11 of the 20 loops have an intro of their own (`0002`, `0004`-`0006`,
  `0009`-`000E`, `0013`), exactly those whose DCS track program has more than one intro
  frame, with the same length (`0009`: 3.749 s, 489 frames = 3.756 s); the 9 others start
  repeating within 9 ms. Size: an extended file of 300 s is 26 MB (mono, 16-bit,
  44.1 kHz); the afm_113b pack goes from 143 MB to 429 MB. libaltsound decodes
  through miniaudio's `ma_decoder_init_file`, which also reads FLAC, MP3 and Ogg Vorbis
  (`stb_vorbis` is built in), so an editor can recompress the pack (FLAC is lossless and
  smaller); the tool writes WAV only.
- **`altsound.csv`** (`ID,CHANNEL,DUCK,GAIN,LOOP,STOP,NAME,FNAME`, libaltsound's
  `altsound_csv_parser`). Boards other than DCS: `CHANNEL` 0 (music: one at a time, a new
  one replaces it) for loops, for sounds that never ended (`loop_unresolved`, not looped)
  and for sounds.dat `Music:` names; empty (= -1, voice/SFX) for the rest; `DUCK` 100 (no
  ducking), `STOP` 0. Their sound programs are code for the board's own CPU, with nothing
  that says how one sound changes another's level, so nothing is made up. DCS: from the
  track programs, see "Ducking, stops and channels" below. Everywhere: `GAIN` 100 (every
  file is at the same master volume, the game's factory one by default). `NAME` is the sounds.dat name without commas and
  quotes (the parser splits on commas and deletes quotes), or `sound <id>`.
- **`g-sound.csv`** (`ID,TYPE,GAIN,DUCKING_PROFILE,FNAME`, `gsound_csv_parser`). Boards
  other than DCS: `music` for loops and sounds that never ended (in G-Sound every music
  sample loops), `callout` for voice lines (quoted sounds.dat names, `VOX:`), `sfx` for the
  rest, including one-shot `Music:` jingles; `DUCKING_PROFILE` 0 (none). DCS: below.
- **`altsound.ini`**: `format = altsound` (change it to `g-sound` to use the other CSV)
  and `rom_volume_ctrl = 0`: the files already carry the right relative levels, and the
  ROM's volume commands must not change them. Boards other than DCS get libaltsound's own
  G-Sound template; DCS gets the ROM's ducking profiles (below).
- Ids that are not a single number (none so far) are left out of the CSVs.

### Listening page

`index.html` (`src/listen.rs`, template `src/listen.html`) is written last, from the final
`manifest.json` (after the AltSound pack has added `pack` and `twin_of`), in the ROM's
folder; Stern SAM packs get one too. It opens from disk: a page loaded as `file://` cannot
fetch `manifest.json`, so the data it needs is spliced into the page as JSON in a
`<script type="application/json">`, with every `<` written `\u003c` (it only occurs inside
JSON strings, where it reads back the same), so no sound name can close the element. The
CSS and JS are inline and nothing is loaded from elsewhere: it works offline, with the
system's fonts, in light and dark.

Per sound: `id`, `name` (and `dat_name`, sounds.dat's, when names were applied), `board`, `duration`, `lufs`, `true_peak` and `peak`, `ended_by`,
`loop` (`method`, `period_secs`, `intro_secs`, `confidence`), `loop_unresolved`, `pack`
(`channel`, `type` = `gsound_type`, `duck`, `stop`, `kind` = `file_kind`, `reason`, and on Stern SAM the game's `calls`), the DCS
channel, the chips pass's `chip`, `twin_of`, `flags` (`clipped (n)`, `blip`, `silent`, `not
clean`, `ignores master volume`, `cut at max`, `loop unresolved`), and `files`: the
recording, the loop body, the extended file and the file the pack plays (a merged twin's
original), each only when it is on disk when the page is written, as a path relative to the
page, with `pack: true` on the one the pack plays (its button is outlined). The header shows the volume the files are at (`volume_init`: in factory mode "factory 55AA6798 (from reference 55AAEF10, -22.44 dB)"), the other volume (reference or factory) and `factory_offset_db`.

The sounds are a `<table>`, one row per sound: play buttons, ID, Name, Type, Duration,
LUFS, True peak, Loop (period, "from" the intro when it is 50 ms or more, the method),
Pack (channel, DUCK, STOP, calls, file kind, DCS channel, chip) and Flags (with the twin
link). The table is in a box that scrolls sideways (`overflow-x: auto`, at least 1100 px
wide), so on a phone the page itself never does. Each column title but Play is a
`<button>` in its `<th>`: a click sorts by that column, a second click reverses, and the
sorted `<th>` has `aria-sort`. Each column has a first direction (ID, Name, Type, Pack:
ascending; Duration, LUFS, True peak, Loop, Flags: the largest first), a sound with no
value (no loop, no LUFS, no name) is always last, ties keep the id order, and names sort
with `localeCompare` (case-insensitive, numbers as numbers). The sort is kept in
`localStorage` under `rom2altsound.sort.<rom>`.

One shared `<audio>` element plays them; the `loop` button plays the body looped (the
browser loops the whole file, as libaltsound does). Rows are built once, and the search
(every word must be in the id, name, type, board, loop method or flags; "edited" finds the
names typed on the page), the type filter (`pack.type`, or the name's prefix as in the pack
when there is no pack), "loops only", "with sound" (rows with a file, on by default) and
the sorting only hide and reorder them. Keyboard: one row is in the tab order at a time
(its buttons with it); Up/Down/Home/End move, Space or Enter plays the row's first file
(again: pause), F2 renames, `/` goes to the search.

**Names typed on the page.** The Name cell has a ✎ button (also F2 on the row, or a
double-click): the name becomes a text field; Enter or leaving the field keeps it, Escape
cancels, and the field's keys do not reach the row (arrows, Space). A name is cleaned as
`manifest.json` keeps them (control characters to spaces, spaces collapsed, trimmed);
empty means no name. Edits are kept in `localStorage` under `rom2altsound.names.<rom>`,
one JSON object `{id: name}` (`""`: the name cleared); an edit equal to the pack's name is
dropped, so after `rom2altsound names` the marks go. An edited name shows "edited" (its
tooltip gives the pack's name) and is what the search, the sort and the player show.
**Export names** writes `names.csv` with a Blob and `<a download>` (which works from
`file://`): the header comment lines, then `ID,NAME` for every sound with a name, edited or
not, and an empty NAME for a pack name cleared on the page. **Import names** reads one with
a file input and `FileReader` (no fetch); a file whose `sound_rom_id` differs from the
page's asks before importing; ids not in the pack are counted and named in the status
line. **Clear my edits** asks, then forgets them. Without storage, edits and sorting work
until the page is closed.

**A/B** (goodtwist's idea: copy the folder, swap sounds in the copy, compare): the "Compare
with folder" box takes a path relative to the page (`../taf_l5-edit/`, a trailing `/` is
added), kept in `localStorage` under `rom2altsound.compare.<rom>` (every access in a
`try`: in a private window or with storage blocked the page works, it just does not
remember). With a folder set, every row with a file gets an **A/B** button and the player
a badge (`A`, `B`, `B: missing`). The button of the playing row, or the `b` key (not in a
text field, no modifier), switches the shared `<audio>` between `<file>` and
`<folder><file>`: the position (`currentTime`, set again on `loadedmetadata`, which with
`preload="none"` comes when it plays) and the play state are kept, so a paused player stays
paused at the same point. On a row that is not playing, the button plays its pack file on
the side shown. An `error` on the element while on B marks the row "missing in B" (with the
path in its tooltip) and the badge. Nothing is fetched: a plain `src` change, which works
from `file://`. The batch index has no player, so no A/B. Tested in Boa with a stub DOM (the
same as for the rest of the page): switching both ways while playing and paused, the
position kept, the error mark, `b` ignored with Ctrl or in the search box, the stored
folder restored, and storage that throws.

The table and the names were tested the same way, in Boa with a stub DOM, on the pages of
afm_113b (589 sounds) and swep1_130 (690, no names): sorting both ways with the empty
values last and `aria-sort` moved, the sort kept across a reload, renaming with the button
and F2 (arrows staying in the field, Escape, an empty name), search and sort on the
edited name, the edit kept across a reload, the exported CSV (quoting, the cleared name),
importing it back with an unknown id, a file for another sound ROM declined, a file
without header, the twin jump, Space playing, and storage that throws.

A batch (several ROMs) also writes `<out>/index.html`, a table of every folder under
`--out` (default `.`) that has a page of ours and a `manifest.json`, when there are at least
two; an `index.html` at that root that is not ours (no `<meta name="generator"
content="rom2altsound">`) is left alone.

`rom2altsound page <folder>...` writes the page again in existing ROM folders, from their
`manifest.json` (a pack made before the page existed, or after editing the manifest), then
the index of their parent folder.

### Sound names

`manifest.json` carries the ROM's `sound_rom_id`, the key `rom2altsound roms` groups games
by (the SHA-1 of its sound ROMs' sorted SHA-1s, one per line; see "ROM verification"),
taken from the PinMAME table built in after the extraction (Stern SAM has none: its sound
data is in the main image). For a pack made before, `rom2altsound page` and
`rom2altsound names` look it up by the manifest's `rom`. The page exports it in the names
file, because names belong to a sound ROM: all the revisions of a game that share their
sound ROMs take the same names.

`names.csv` (`src/names.rs`): comment lines starting with `#`, of which `# rom:`,
`# sound_rom_id:` and `# rom2altsound:` are read, a header with `ID` and `NAME` (in any
order, other columns ignored), then RFC 4180 rows (quoted fields may hold commas, `""` and
line breaks; a BOM is skipped). Ids are compared as numbers (`0x2`, `0x0002` and `0X0002`
are one id, written `0x0002` as in `altsound.csv`).

`rom2altsound names <folder> <names.csv>` and `--names` at extraction apply it:

- a file whose `sound_rom_id` differs from the pack's is refused before anything is
  written (at extraction, before the boot), unless `--force` (`--force-names`); with one
  of the two ids missing, a different `# rom:` is only a warning;
- an id given twice with two different names is refused; twice with the same name, and
  ids not in the pack, are reported (the latter left out, as for a pack made with
  `--only`);
- `manifest.json`: the sound's `name` becomes the new one (cleaned as on the page), the
  previous one is kept once as `sounds_dat_name`, and a `names` record says where the
  names came from (`from`, `rom`, `sound_rom_id`, `rows`, `matched`, `unknown`);
- `altsound.csv`: only the NAME column of the listed ids changes, through the same
  `csv_name` as the extraction (no comma, quote or control character; empty becomes
  `sound <id>`); the columns are found by the header, every row must have as many fields,
  and the line endings are kept. `g-sound.csv` has no names. The WAV files keep their
  id-based names, and the channels, DUCK, STOP and types stay as extracted (they come from
  sounds.dat's names and the board, not from these labels);
- the page is written again (`--no-html`: not).

`--names` takes one ROM (the extraction refuses it with several). `rom2altsound names
<folder>` alone prints the pack's names as a `names.csv` (what the page exports without
edits).

### Twins

Some ROMs hold several commands that play the same audio (sounds.dat says of afm_113b
"Every sound effect appears twice"). A sound is the **twin** of an earlier one when:

- their lengths differ by at most one sample;
- their integrated loudness differs by at most 0.01 LU;
- once aligned to a fraction of a sample (whole-sample lags up to 3, then a golden-section
  search with a 48-tap windowed-sinc interpolator on the loudest 16384 samples), the
  residual over the whole overlap is at least 60 dB below the signal.

Measured on afm_113b: the 303 pairs that pass the first two tests are at -64 to -80 dB once
aligned (a linear interpolator left them at -25 to -55 dB: the copies differ by the
fractional phase of the resampler); different sounds of nearly the same length are above
0 dB. The test runs on the recordings at the reference volume, in factory mode too (the
files are scaled to the factory volume after the pack is built, see "Factory mode"): at
the DCS factory volume, 22 dB lower, PinMAME's ±1 LSB dither is 22 dB closer to the signal
and the silence trim falls a few samples apart, so the same pairs measured -43 to -50 dB
(about 1 LSB RMS) and up to 105 samples apart in length, and none passed. Which pairs pass
can still change a little from one run to the next, at any volume: the emulation is not
sample-exact from run to run (the sound's start against the DCS frame), and a pair whose
copies were recorded in a different context can miss the test (afm_113b `--limit 40`,
four runs, two at each volume: 6, 8, 7 and 6 of the same 8 pairs; `0x0070`/`0x0071` are at
-78 dB played alone, and were missed in one of the four). The thresholds are in `manifest.json`
(`altsound.twin_test`). Each sound is compared to the earlier sounds that are not twins themselves, so
`twin_of` always names an original. On DCS the reason is the board's channels (see
"Ducking, stops and channels"): a new command on a channel cuts what was playing there, so
afm_113b puts its sound effects on channels 1 and 2 and its Martian voices and effects on
4 and 5 as identical pairs (107 and 82), and the game sends a sound to the free channel of
the pair: two copies overlap instead of cutting each other. The General (channel 3) has no
twin, so a new line cuts the previous one. `twin_reason` says which channels. By default
every command keeps its own file and its own CSV rows, because they carry this channel
information; `--merge-twins` only shares the WAV file: a twin's rows (kept distinct) point
at its original's file, and the twin's own files are not kept.

### Ducking, stops and channels (DCS)

A DCS track program (the bytecode the board runs for each command, see Loops) says in
plain opcodes what it does to the other channels (from mjrgh's DCSExplorer, `ExecTrack`,
`MixingLevelOp`, `LoadTrack`). Once booted, `dcsrom::command_effects` follows every
populated track of the catalog from a silent board for up to 60 s (0.1 s for AFM's 590)
and records:

- **the home channel** (header byte 2). A type 1 track replaces the program on its channel
  and clears that channel's stream: **a new command on a channel cuts the previous one
  there, and nothing else**. Type 2 tracks only leave a deferred track for their channel,
  which a music track starts at its next phrase boundary (opcode `05`).
- **ducks**: opcodes `07`-`0C` set, raise or lower a channel's mixing level, at once or with
  a fade over N frames. Each channel keeps one contribution per source channel, and its
  level is their sum; one unit is `0.9733` in gain, **0.2352 dB**, whatever the channel's
  own level. A contribution is dropped when its program ends, is stopped or is replaced, so
  a duck lasts as long as the program that set it; programs give it back with a fade
  (`0B`) just before they end (0.15 s for most AFM commands).
- **stops**: opcode `02 c` stops channel `c`; a track that plays nothing on its home
  channel only clears it (AFM `0x03E3` clears channel 0, the music).

Per command, `manifest.json` gets `dcs`: `track_type`, `channel`, `streams` (channels it
plays on), `stops`, `ducks` (per other channel: `units`, `db`, `start_s`, `full_s`,
`end_s`, `restore` = `fade` / `program end` / `step` / `held`, `release_s`), `deferred`,
`queues`, `own_level`, `length_s`, `error` (a program that could not be followed: the
command keeps the plain rows). At the top level, `dcs` sums up the catalog: per channel
the tracks, how many play a stream and how many duck the music, the `stop_commands`
(what libaltsound does with each), the `deferred` tracks, the `unreadable` ones.

On afm_113b: channel 0 holds the 20 music tracks, 1 and 2 the same sound effects twice
(107 pairs), 3 the General's 171 lines, 4 and 5 the Martians' voices and effects twice.
313 of the 576 written commands lower the music: by 10 units (-2.35 dB, DUCK 76: most of
the General's lines), 15 (-3.53, DUCK 67: most voices), 20 (58), 30 (44), and the fanfares
and big effects by 70 to 100 units (-16.5 to -23.5 dB, DUCK 15, 11 and 7). Nothing stops
the music but the stop commands, which play nothing.

**Twins** are the explanation of AFM's "every sound effect appears twice": the same sound
on two channels (1 and 2, 4 and 5), so that two of them can play at once; the game picks
the free one (see Twins above).

How the programs map onto the pack (libaltsound's `altsound_processor.cpp` and
`gsound_processor.cpp`):

| column | DCS rule | what is exact, what is lost |
|---|---|---|
| `CHANNEL` | 0 for a track on channel 0 that plays a stream (the music); 1 (jingle: one at a time) for the **voice channel**, the channel with the most voice lines (quoted names, most of its rows) that has no twin channel (more than half of its sounds also on another channel); -1 otherwise | AFM: channel 3, so a General line cuts the previous one, as on the board. The other channels cut their own previous sound on the board, but AltSound has one music and one jingle channel only: they play polyphonic |
| `DUCK` | `round(100 * 0.9733^units)` of the deepest contribution to channel 0; 100 when none (and on music rows, which the parser forces to 100 anyway) | The depth is exact. AltSound keeps it while the file plays and gives the level back **at once** when it ends, where the board fades it back (0.15 s mostly; 17 AFM effects give it back 0.2 to 2 s before their end). Overlapping ducks **add up** on the board (-2.35 and -3.53 give -5.9 dB) but AltSound uses **only the deepest** ([libaltsound issue #15](https://github.com/vpinball/libaltsound/issues/15)). Only the music is ducked: a duck of another channel (3 AFM commands) is in the manifest only |
| `STOP` | 1 when the row is on the jingle channel and its program stops channel 0 | AltSound can only stop the music, and only from a jingle; such a stop on another row is listed in `altsound.dcs.limits`. AFM: none. The stop commands play nothing, so they have no row: libaltsound stops the music on `0x03E3` itself; `0x0000` (all channels) and the per-channel stops (`0x03E1`, `0x03E2`, `0x03E4`...) are lost |
| `LOOP` | as before: 100 for a loop played from its body, 0 for a loop with an intro of its own, played from its extended file (see AltSound pack) | The intro is played once, then `--intro-loop-secs` of cycles; past that the music stops ([issue #14](https://github.com/vpinball/libaltsound/issues/14)) |
| `TYPE` | `music` for channel 0 loops (an extended file too, which G-Sound then loops whole, intro included), `callout` for the voice channel, `sfx` for the rest (a one-shot channel 0 track too) | Same limits as `CHANNEL`: callouts cut each other, sfx are polyphonic |
| `DUCKING_PROFILE` | per type, one profile per distinct DUCK value, lightest first: `ducking_profileN = music:<DUCK>` in `[callout_ducking_profiles]` / `[sfx_ducking_profiles]`, with `ducks = music` in `[callout]` / `[sfx]` (left empty for a type without profile: libaltsound refuses `ducks` without one) | Same limits as `DUCK`. AFM: callout `76, 67, 58`, sfx `76, 67, 58, 44, 15, 11, 7` |

The deferred tracks (a music change on the beat) cannot be expressed in either format: they
are only listed. The 1993 DCS software reads opcodes `04` and `06` differently (see Loops),
and DCS-95 opcodes `10`-`12` are not modelled: such a program ends in `error` and its
command keeps the plain rows.

**`--check-ducking`** plays it back: once the extraction is done, the loudest written music
loop alone, then per duck depth one written command (the one whose full depth holds longest,
up to 6 s) alone and sent 3 s into the music, all at the reference volume. The music's gain
is fitted by least squares (`MC = gm * M + gc * C`) in 30 ms windows, the median `gm` over
the full depth (60 ms kept from each end) is compared with the program's depth, and a
difference over 0.5 dB is a `mismatch` (manifest `dcs.ducking_check`, and the summary). The
fit before the command must be within 0.1 dB of 0 (`before_db`), or the check is not
measured and counts as a mismatch. The boards are reset once before the check (one more
`board_resets`): the main pass leaves a deferred track armed that the stop does not clear,
and on afm_113b the first take of music `0011` then played another track. It adds about a minute of emulation. Only DCS boards
replay a sound sample-exactly (two takes correlate at 0.998+); on Whitestar (apollo13) they
correlate at 0.3 to 0.99 and the fit means nothing, so there is no check, and no reading,
on the BSMT boards. WPCS and System 11 are measured chip by chip instead (below). The research behind this: [ducking-study.md](ducking-study.md).

### Ducking, stops and channels (WPCS and System 11)

These boards have no track programs to read: their sound program is 6809 (or 6808) code.
What it does to the music is measured instead, chip by chip, with PinMAME's mixer: each
chip's output is a mixer channel (`YM2151 #0 Ch1/Ch2`, `DAC #0`, `HC55536 #0`; System 11
has a DAC and a CVSD per board), and setting a channel's mixing level to 0 mutes it without
touching the emulation (`mixer_set_mixing_level`, called from Rust; the levels the boot
left are restored before PinMAME stops, since it saves them in the machine's cfg and the
next boot would start muted). After the factory offset pass, the **chips pass** (on by
default, `--no-chip-check` skips it) plays every written sound that is not a loop:

1. with only the **voice chip** (the HC55516/HC55536 CVSD, `HC555*`) heard, at most 2.5 s:
   its level against the written file over the same length. Within 6 dB, most of the sound
   is on that chip (`mix.voice_db`, `mix.chip` `voice`);
2. sent 2 s into the loudest written music loop, with only the **music chip** (the YM2151)
   heard; the music alone is played once first, the same way. The two takes are compared
   as envelopes (rms per 0.1 s; FM replays are not sample-exact, two takes of the same
   music differ by +2.9 dB at the sample level): the median gain before the command is the
   takes' own drift, and its spread, as 2.5 standard errors of the medians, the smallest
   move told from it (`music_noise_db`, 0.2 to 1.7 dB). Relative to before, the music's
   gain while the sound plays (`music_during_db`, at most its first 6 s) and from 0.3 to
   1.3 s after it ends (`music_after_db`) say whether it **ducks** the music (by 1.5 dB
   or more, and more than the drift: `ducks_music_db`), **stops** it (-20 dB or less
   after: `stops_music`) or **plays on the music chip** itself (+1.5 dB or more: `chip`
   `fm`; its ducking cannot be told from its own sound).

The pass adds two takes per sound: 1976 s of emulation on Twilight Zone (257 sounds,
about 10 minutes of wall time), 1337 s on Whirlwind (167). The manifest's `mix_check` sums it up (the music used, the channels, the
counts).

On Twilight Zone (WPCS) speech and booms are on the CVSD (a boom is a CVSD sample like a
voice line, so the chip alone does not tell a voice line from an effect: the name does),
FM jingles and menu sounds on the YM2151, the drums of the music on the DAC. Over music
`1C`: 141 of the 257 sounds are on the voice chip (91 of them voice lines), 7 on the
music chip; no voice line lowers the music by more than 2.4 dB, but 36 jingles and effects
lower it by 1.7 to 13.8 dB while they play (they take some of the YM2151's voices: the
same Clock Chime raised music `03`, quieter, by 3 dB instead), and the tilts `D3` and `D4`
stop it. The second bank's filler (`01` before `7A xx`, see Commands) used to make all its
141 sounds stop the music. On Whirlwind (System 11, two boards) 53 of 167 sounds are on a
voice chip, 44 on the music chip, none lowers the music `0166`, and 11 sounds of the music
board end it: a command of that board replaces the music it was playing.

How the measures map onto the pack:

| column | rule | what is exact, what is lost |
|---|---|---|
| `CHANNEL` | 1 (jingle: one at a time) for a sound on the voice chip, which plays one sound at a time on the board, a new one cutting the previous; 0 for a non-voice sound that ends the music (System 11C: a command of the music board ends the music it was playing), which takes the music's place as on the board; -1 otherwise | The board may also refuse a sound of lower priority, which AltSound cannot. A sound on the music chip takes some of its voices from the music; AltSound plays both in full |
| `TYPE` | `callout` for a sound on the voice chip unless its name says it is an effect (`SFX:`); `sfx` for the rest; `music` as before | Without sounds.dat names (System 11), every voice-chip sound is a callout |
| `DUCK` | `round(100 * 10^(dB/20))` of the measured duck, 100 when none | Measured on one music: a sound may duck another music differently |
| `STOP` | 1 for a jingle that stops the music | A non-voice sound that stops it goes on the music channel instead (above) |

### Factory mode (the default; `--no-factory` turns it off)

The goal is the volume the game sets by itself from its factory settings, not from an nvram
a player may have changed. So the user's nvram/cfg are never used:

1. The private vpm is `<cache>/rom2altsound/vpm-factory/<rom>` (or `--vpm`).
   This ROM's `nvram/<rom>.nv` and `cfg/<rom>.cfg` are deleted first.
2. Cold boot, in a child process (`--cold-boot-only`; libpinmame runs one machine per
   process): no nvram, so the game runs its factory reset. Same boot rule as above, then
   `PinmameStop`, which is when libpinmame writes `<vpm>/nvram/<rom>.nv`. The parent checks
   that the file exists (else it stops with an error naming it) and copies it to
   `<out>/factory-nvram/<rom>.nv`, the path the manifest reports (`factory.nvram`): the
   warm boot rewrites the vpm's copy when it stops. The child's boot log goes to
   `<out>/cold-boot.json` and into the manifest.
3. Warm boot from that nvram. `factory_volume` in the manifest and the summary is what the
   game sent on this boot (the last command per board and kind), or `seen: false` with the
   reason. **The files end up at that factory volume** (`volume_mode: "factory"`), on every
   board family and whatever the game: DCS `55 AA vv ~vv` (afm_113b `55 AA 67 98`, level
   12/31), WPCS `79 vv ~vv` (tz_94h and taf_l5 `79 0C F3`), Whitestar `FE xx FD` (apollo13
   and monopole `FE 2C FD`, level 3/31), Stern SAM the DAC attenuation as PinMAME plays it
   (acd_168h `E8`, -1.8 dB). But a board with a master volume is **recorded at its
   reference volume** (below), and every analysis runs on those recordings: the silence
   trim, the end of a sound, loops (cut points, checks, seams), the volume check, the chips
   pass, the AltSound pack (twins, extended files, `smpl`). Only then are its files written
   again at the factory volume (step 5). Once booted the tool sends its volume again before
   every command where the game kept re-sending its own (Whitestar), and after a board
   reset. A board with no volume stage (System 11, Data East's pot, the Bally boards) is
   recorded at its only level, full scale, and not scaled. When the game sends no master
   volume at boot, the board stays at its power-on level: on DCS the factory volume is
   then the board's reset default, `55 AA 67 98`, and the board is recorded at the
   reference and scaled as above; on WPCS and Whitestar the tool sends nothing, records
   the board at that power-on level and does not scale it. `recorded_volume` in the
   manifest says, per board, which volume the files are at, where it comes from and the
   gain applied (`gain_db`). Nothing is lowered to avoid clipping: a file that reached full
   scale in the recording (PinMAME's mixer sums the chips in float and clips once, to 16
   bits) is listed in `clipped_files` and on a `CLIPPED` line of the summary.
   `--volume reference` writes the files at the reference volume (below) instead, as 0.2.1
   did; giving `--dcs-volume`, `--wpcs-volume` or `--whitestar-volume` implies it, unless
   `--volume factory` is given too.
4. Factory offset pass (after the retry pass and the master volume check), per board that
   has a master volume: up to 5 written non-loop files are played again at the factory
   volume (on DCS the board's reset default `67` if the game sent none). They are the
   loudest files at most 5 LU above the median file (louder ones are the volume check's
   suspects), at least 1 s long and not clipped; when fewer qualify (a short run, `--only`,
   `--limit`) they are topped up with the other written non-loop files, the loudest first.
   In factory mode the files flagged `ignores_master_volume` (step 9 of "How it works")
   are played again at the factory volume too, each for its own gain (they would not
   follow the board's). These replays are measured, never written. When the factory volume is the reference
   (WPCS `79 0C F3`) nothing is played and the offset is 0. The board's offset is the
   median of factory `level_lufs` minus reference `level_lufs` (negative where the factory
   volume is quieter: afm_113b -22.44 dB, spread 0.00 to 0.02 dB); `factory_offset` in the
   manifest lists the files, both levels, the move and the spread, and `factory_offset_db`
   is the ROM's. Boards without a master volume record at their only level: offset 0.
   With `--volume reference`, `loudness.as_shipped` is the loudness report shifted by the
   offset: what the files measure at the game's factory volume.
5. Factory mode: each board's files are scaled by its offset (`factory_gain`: `gain_db`,
   the linear `gain`, per board), a file flagged `ignores_master_volume` by its own move
   from the reference to the factory volume (`factory_gain.own_gains`: `id`, both levels,
   `gain_db`; 0 dB when its replay could not be measured). So is a replayed file of step 4
   whose move is more than 3 dB away from the board's median (a sound that does not follow
   the master volume but that the volume check could not flag, on a short run: monopole
   `--only` with `0x1E`, which moves by 0.0 dB when the others move by -24.7); the board's
   gain is then the median of the other files. First the levels (every per-sound level and the loudness
   totals move by the gain, so `manifest.json` and the summary describe the files as
   written); then, after the pack, every file of the board: the recording, read back,
   times the gain in floating point, plus a TPDF dither of ±1 LSB (the difference of two
   uniform values from xorshift128 generators seeded by the file name, as PinMAME's
   `mixer_sh_update` dithers its own output), rounded to the nearest 16-bit value and
   clamped (a gain above 0 dB only can clamp; `factory_gain.rewritten.clamped_samples`).
   The loop body (`-loop.wav`) and the extended file are cut again from the scaled
   recording, sample for sample consistent with it, and the `smpl` loop points written
   again. Each replayed file of step 4 checks the result: `scaled_minus_replay_db`, the
   scaled file's level minus what PinMAME plays at the factory volume (afm_113b: -0.008
   to +0.017 dB; monopole -0.24 to +0.23 dB over a full run: Whitestar sounds follow the
   master volume within a few tenths of a dB, depending on the context they are played in,
   since the same files measure within 0.05 dB of each other when played alone). If the offset could not be measured, the board's files stay at the
   reference volume and `recorded_volume` says so.

**Why record at the reference and scale.** Recorded straight at the DCS factory volume,
22 dB lower, the files' content is the same but PinMAME's ±1 LSB dither (added once, to
16 bits) weighs 22 dB more against the signal: the loop checks' residuals rose (afm_113b
`0x0001`: -35.5 dB instead of -47.3), the audio method picked another, worse loop on
`0x0013` (0.461 s at -1.3 dB, against -15.5 dB at the reference), the silence trim fell a
few samples apart and no twin passed the twin test. Recorded at the reference and scaled
after, every analysis is that of the reference volume (`0x0001` -48.0 dB, `0x0013` -15.6
dB, the twin test back to 0.2.1's) and the files are at the factory level all the same.
The scaled file and a recording at the factory volume differ only by their dither: both
are PinMAME's float mix at that level rounded once to 16 bits (the recording at the
reference carries its own dither too, 22 dB down after the gain). A 24-bit or float WAV
would not hold more: libpinmame's float output is the same 16-bit mix converted
(`src_short_to_float_array`), and its mixer has no wider output. The consumers would read
one (libaltsound decodes with miniaudio, whose WAV decoder reads 16-, 24- and 32-bit PCM
and float), so the format stays 16-bit PCM. At -22 dB the dither floor is about 74 dB
below the files' own peaks instead of 96.

### Reference volume

Used with `--volume reference` (the default until 0.2.1), and in factory mode as the volume
the boards are recorded and analysed at, before their files are scaled to the factory
volume. Per board family, the loudest master volume at
which no file of our ROMs clips in emulation, apart from isolated clicks. Measured with full
sweeps (written files with raw samples at +32767/-32768):

| family | volume | clipped files at that volume | one step louder |
|---|---|---|---|
| DCS | `55 AA EF 10` (level 29/31, `--dcs-volume`) | afm_113b `0186` (1 sample), cv_20h `03DE` (2 samples, a 77 ms click that ignores the master volume) | `FF`: afm 5 files (`0186` 68 samples), cv_20h 18 (its loop `0016` 575), mm_109c 24 (`01AB` 99), rs_l6 5 (`0240` 33) |
| WPCS | `79 0C F3` (level 12, the game's own, `--wpcs-volume`; the volume runs from `00`, silent, to `1F`, and the board ignores `20` and above; the DAC is AC-coupled) | tz_94h: none (307 commands). taf_l5: none (472 commands) | `0D`: taf_l5 `C6` and `CD` (1 sample each); at `14` 5 effects (`D3` 20, `CD` 10, `C6` 8, `D4` 3, `C7` 2); at `1F` `C7` 430, `D3` 341, `CD` 93, `C6` 50, `D4` 31 |
| Whitestar | `FE 11 FD` (level 30/31, `--whitestar-volume`) | xfiles `1F` (56 samples, a 50 ms click that ignores the master volume) | `FE 10 FD`: apollo13 `5C` 172 samples, `68` 13 (xfiles: only `1F`) |
| System 11, Cheap Squeak / Turbo Cheap Squeak, Data East, Bally -32/-50 and Sounds Plus -51/-56 | no software volume stage: always full scale, which is the reference (`reference_volume: "full_scale (no volume stage)"`); on Data East the music level is set to its loudest, `20` | | |
| Bally Squawk & Talk -61 | volume lines PinMAME does not emulate: always full scale (`reference_volume: "full_scale (volume lines not emulated in PinMAME)"`) | eballdlx: 5 speech files, 2 or 3 samples each, in PinMAME's own mix | |

**Boards without a volume stage.** System 11 (`s11s`, `s11cs`, `s11js` in wmssnd.c) and
Bally's Cheap Squeak and Turbo Cheap Squeak (`by45`, `byTCS` in by35snd.c) write their DACs,
CVSD and YM2151 directly: no volume register, no `mixer_set_volume`, no volume command in
their sound programs that PinMAME would see. Data East's BSMT board (`de2s` in desound.c)
has its master volume on a pot in the power junction box ("it was not done through the
software"); its bytes `20`..`2F` are a music level the game drives (a music may fade it as
it ends), which the tool sets to its loudest, `20`, before every command. These boards are
always at full scale: that is their reference volume, and their factory offset is 0.

**The early Bally boards** (by35snd.c). The -32/-50 (`by32`) has no sound CPU: one tone per
command at one level, with its own decay. The Sounds Plus -51 and -56 (`sp51`) feed the
AY-3-8910 (and the -56's MC3417 speech) straight to the mixer; their PIA's CB2 line is
turned by PinMAME into a 75 % mute, but vikingb and xenon set it low once, at reset
(`CRB = 34`), and never touch it again. The Squawk & Talk -61 (`snt`) has volume lines on
its PIAs (port B bits 4-7 of the first for the sounds, of the second for the speech), set by
commands: eballdlx $F915 maps `DF`..`EE` to the sound volume and `EF`..`FE` to the speech
volume, 16 steps each. PinMAME stores those port writes and does nothing with them, so in
emulation the board is at full scale whatever the game sends, and the game's own volume
cannot be read back from the emulation: the reference is full scale, reported as
`full_scale (volume lines not emulated in PinMAME)`.

**WPCS** (tz_94h): a sweep of `79 vv ~vv` on three loud sounds (two booms and music `03`)
gave silence at `00`, -2.5 dB from `0C` (the game's factory value) at `06`, then +0.8,
+1.5, +2.0, +2.3 and +2.5 dB at `10`, `14`, `18`, `1C` and `1F`; `20`, `24`, `2F` and `FF`
change nothing (the board keeps the `0C` the game sent at boot). The steps are not even:
the program turns a digital pot (`wpcs_volume_w`, one step per write, the mixer at
`pot * 100 / 127` %). Whole-ROM sweeps (10 s per sound) found no clipped file at `14`,
`15` and `16`, and the booms `A3` and `A4` clipped at `17` and `18` (then `A5` too at
`1C`): they start on the level the DAC was left at by the sound before (Twilight Zone's
DAC holds its last value, 9291 LSB before `A3` at `1F`), so a boom that fits alone can
clip in the ROM's order. `16` was the reference until The Addams Family (taf_l5, raw
sweep): at `16` eight of its effects clipped, then six at `14` (level 20).

Two causes. First, the board's program leaves its DAC (an AD7524, 8 bits, `DAC_0_data_w`)
on the last value a sound wrote, and PinMAME maps the DAC unsigned (code 0 = output 0,
`UnsignedVolTable`): the held value is a DC level in the mix, on which the next sound
starts. At `14` all 268 taf_l5 files started more than 256 LSB away from 0 (up to 14216
LSB), and `A1` clipped 56 samples on it (`82`, `8D` and `A1` do not clip alone). Second,
the sounds play on the DAC around its mid code, whose DC level (8750 LSB at `14`) adds to
the voice and the music: five taf_l5 effects (`C6`, `C7`, `CD`, `D3`, `D4`) clipped on
it on their own (at `14`, with the DAC parked at code 0 between sounds: `C7` 4857, `D3`
1227, `D4` 414, `CD` 89, `C6` 79 samples; still `CD` 2 at `08`). The real board's output
is AC-coupled, so none of that DC ever reached the speaker.

So once the game has booted, the tool AC-couples the DAC (`shim_dac_ac_couple`): dac.c
already has a one-pole 10 Hz high-pass, opt-in per channel, used by the Gottlieb, Taito
and Mr. Game drivers (`DAC_DC_offset_correction_data_16_w`). Its input is the raw level
`data * 0x101 / 2`, exactly what `DAC_data_w` stores, so one write through it at 0 (the
DAC's power-on level) switches the channel for good and the board's own `DAC_data_w`
writes then go through the filter, at the same scale; the volume stage (`wpcs_volume_w`,
`mixer_set_volume` on every channel) and the mixing levels (DAC 70 %, HC55536 100 %,
YM2151 16 %) apply as before. PinMAME is not changed. 10 Hz costs -0.46 dB at 30 Hz
(PinMAME's figure). Then 0 of 268 taf_l5 files and 0 of 302 tz_94h files start more than
256 LSB away from 0 (50 and 55 LSB at most), whatever the order of the sweep.

What still clips is the sum of the sources in PinMAME's mixer (float, clipped to 16 bits
once at the end). Played alone with the other channels muted (`--solo`, taf_l5 effects and
`82`, `8D`, `A1`, tz_94h `A3`, `A4`, `A5` and four musics), no source clips even at `1F`:
peaks DAC 19308 (`C6`), YM2151 10196 (`C7`), HC55536 26530 LSB (the speech, the same in
every one of these effects); at `14`, 18515, 9644 and 23446. Mixed, the five effects play
speech, FM and DAC together and clip on the sum (`C7` at `1F`: clipped from 0.07 to 1.21
s, while its speech and FM play; its DAC part lasts 0.28 s). Clipped samples, DAC
AC-coupled, `C6` / `C7` / `CD` / `D3` / `D4` (`--only`, 10 s per sound): `1F` 50 / 430 /
93 / 341 / 31; `1C` 59 / 173 / 57 / 224 / 17; `19` 23 / 51 / 33 / 108 / 9; `16` 14 / 4 /
13 / 42 / 1; `14` (full sweep) 8 / 2 / 10 / 20 / 3; `13` 5 / 1 / 0 / 9 / 0; `12` 7 / 0 /
4 / 15 / 2; `11` 2 / 0 / 0 / 7 / 0; `10` 3 / 0 / 0 / 3 / 0; `0F` `D3` 1; `0E` `C6` 1,
`CD` 1; `0D` none, but 1 sample of `C6` and of `CD` in a full sweep (the speech is not
sample-exact from one run to the next); `0C` none. tz_94h clips at no level, `1F`
included (its loudest, the boom `A3`, peaks at 26508). The reference is `79 0C F3` (level
12), the game's own factory volume and the loudest level at which no file of either ROM
clips in full sweeps (taf_l5 472 commands, tz_94h 307; loudest true peaks -0.3 and -2.0
dBTP): the factory offset is 0 dB.

**WPCS command pacing** (taf_l5): the game writes the bytes of a multi-byte command (`79 vv
~vv`, `7A xx`) to the board back to back, within the same frame. The Addams Family's sound
program (`tafu18l1.rom`) does not wait a frame for the next byte: with 4 frames (67 ms)
between them, and still with 1 frame (17 ms), `79 0C F3` played `0C`, a music, which no
stop then silenced (0.2.0 reset the board after every wait, and the volume it sent again
after the reset started the music again, forever). Twilight Zone's program waits. So a
WPCS command of several bytes goes out in one burst (`shim_data_burst`: the board's data
handler, 12 timeslices between the bytes, as PinMAME's `wpcs_manCmd_w` sends its pairs); a
single byte goes through `sndbrd_data_w`, as before.

Whitestar's top step is not like the others: `FE 10` is 4.8 dB above `FE 11` (apollo13
offsets to the factory `FE 2C`: -37.4 dB from `FE 10`, -32.6 dB from `FE 11`, i.e. about
1.2 dB per level below). The DCS steps measure about 1.3 dB.

The Whitestar reference is re-sent before every command, as the game re-sends its own
volume (`refreshed_before_each_command`), and both are sent again after a board reset.

Measured cold boots: every one of our nine ROMs wrote its nvram (WPC: 12334 bytes, DE/Sega:
8238, System 11: 2094). The WPC DCS games send no sound byte at all on the cold boot (60 s);
on the warm boot they send `55 AA 67 98` after 6-12 s.

### Per family

| family | master volume | other state re-sent before each command | stop |
|---|---|---|---|
| DCS (WPC) | `55 AA vv ~vv`, level = (vv - 7) / 8, 8..31 (`67` = 12) | none | `00 00` |
| DCS channel mix | `55 AB..B0 vv ~vv` (rs_l6 fades `55 AB` FF to 07 and back to FF at boot) | none | |
| Pinball 2000 DCS2 (`DCSP2K`, 16-bit words) | `55AA vv~vv`, level = vv / 8, 0..31 (`60` = 12, `FF` = 31); `55AB mm vv` (channel volumes) and `55AC mm pp` (pans) replayed as the game sent them | none | `55AE 3F00` |
| WPCS | `79 vv ~vv`, `vv` 00..1F (20 and above ignored): the game's is read (tz_94h `79 0C F3`) and sent again once (`--volume reference`: the reference, also `79 0C F3`); the DAC is AC-coupled once booted | none | `00` |
| Whitestar BSMT (Sega/Stern) | `FE xx FD`, level = 2F - xx, 0..31 | the master volume (with the factory settings the game's factory `FE xx FD`, or the reference with `--volume reference`; else the game's, which it re-sends every 0.5 s) | `00` |
| Data East BSMT | none (hardware pot in the power box) | the music volume `20`..`2F` (the game's last one at boot, else the board's default `20`; the loudest, `20`, with `--volume reference`), then the stop `00` | `00` |
| System 11 (WMSS11, 11C, 11J) | none (no volume stage) | none | `00` / `20` (11C) |
| Bally Cheap Squeak (BY45), Turbo Cheap Squeak (BYTCS) | none (no volume stage) | none | `00` |
| Bally Sounds Plus -51 (BY51) | none (no volume stage) | none | `1E` |
| Bally Sounds Plus -56 (BY56, named BY51 by PinMAME) | none (no volume stage) | none | `05` |
| Bally Squawk & Talk -61 (BYSNT) | its volume lines are not emulated | none | `05` |
| Bally -32 / -50 (BY32) | none (no volume stage) | none | `0F` (unmeasured) |
| Stern SAM | PCM1755 DAC attenuation, `FF` = 0 dB, -0.5 dB per step: the operator's volume setting (read, not driven) | none | none (static, see Stern SAM) |

**DCS**: `55 AA vv ~vv` sets the master volume (`~vv` must be the complement, else the
firmware drops it). The bytes of a command go out one frame apart. The DCS firmware drops
the first byte of a two-byte command if the second comes 13 main-loop passes later (13 x
7.68 ms, about 100 ms; mjrgh's DCSExplorer, `dataPortTimeout`): with 6 frames between the
bytes every command of rs_l6 was lost, 4 frames (PinMAME's commander) is within the limit,
1 frame is closer to the game, which sends both bytes within a millisecond. Independently of
the pacing, about one command in 200 plays nothing on its first try and normally later; see
Limits. The sweep without a sounds.dat section plays the ROM's catalog (below).

**Pinball 2000** (swep1_130, rfm_120; PinMAME's `SNDBRD_DCSP2K`, the same `DCS` board
interface with 16-bit words, reported as `DCSP2K`). The game is a PC (src/p2k) that writes
the board's 16-bit host port directly (p2k.c `p2k_dcs_write`), never through
`sndbrd_data_w`, so libpinmame's sound command callback never sees a word, and
`sndbrd_manCmd` truncates to 8 bits. The board's DSP, though, is a PinMAME CPU whose data
map holds the host latches: the shim puts hooks in front of its handlers (as for Stern
SAM's DAC), and logs every word the DSP acknowledges (data `0400`) and every reply it writes
(`0401`), with the emulated time; the tool sends words with `dcs_p2k_data_w`, as the PC
does, and takes the board's replies in the halted PC's place (the DSP waits for that before
it goes on). The words go through the byte machinery as two bytes each, high byte first, so
`55AA 609F` is the DCS volume `55 AA 60 9F`. What the game sends was read in its own code
(`game.rom`, a flat x86 image loaded at `0x100000`, with `symbols.rom`) and in the boot logs:

- **its boot** (swep1_130, cold and warm; rfm_120's cold boot ends the same): `000E` (a boot block upload: the flash's own
  boot page, last word first, three words per program word), `003A`, `001B` (the ROM
  checksums, 3.5 s), `00AA` (2.3 s), `000E` again (the board answers `EE07`, then `000A`),
  then `ACE1` twice, which the board answers `0100 000C`: the DCS2 protocol is open. Then
  `55AA 609F` (master volume level 12), `55AB 3FFF` (the six channels' volumes, FF),
  `55AC 3F7F` (their pans, the middle), and the requests `03E7` and `03E8` (two tracks
  that only write to the host, opcode `04`). Nothing more in 200 s of attract mode. rfm_120
  (XINA 1.12) sends requests in the middle of its first boot block, which the board then
  receives for real, and on its warm boot it never opens the protocol (its boot stops at
  `000E 8180 001B`, 3.7 s in): the tool then resets the board and sets it up itself
  (`boot.p2k` in `manifest.json` says which).
- **a request** (`DCSRequest`): the track number, then `vol_pan` = `FF pp` (the volume is
  always FF, `pp` the pan, `7F` the middle), then `trk_pri` = `8000 | trk << 7`, the board
  channel ("track", 0..7) the host plays it on. The tool sends every track on channel 0
  (the game's boot tracks use 4 and 5; the channel does not change the sound: two tracks
  played on channels 0 and 4 gave the same length and level).
- **the volumes** (`DCSSetVolume`, `DCSSetTrackVolume`, `DCSSetTrackPan`): `55AA vv~vv`
  with `vv` = level * 8 (FF for 31); `55AB mm vv`, `mm` the mask of the channels (`3F` = all
  six); `55AC mm pp`. Each level is about 1.3 dB (12 to 20 and 12 to 4: 10.6 dB; 12 to 31:
  26.2 dB). FF clips most sounds in emulation: the reference volume is `A0`, level 20
  (both games' loudest sound peaks there at -5.8 dBFS).
- **the stop** (`DCSQuietAllTracks`): `55AE 3F00`; one channel: `55AE`, then its bit in
  the high byte.

The sound flash, U109 and U110 hold the DCS track catalog and the track programs of the
WPC boards in 16-bit words (`dcsrom::p2k_image`, see Commands): DCS2 adds opcodes `13 ll`
and `14 ll nnnn` (a level and a fade of the program's own channel, which the host chooses),
followed but not modelled. The programs' loops are found as on DCS (`dcs-catalog`); the
ducking, stop and channel analysis of the pack is not done (a program's channels are
relative to the one the host picks): the pack has the defaults. After a board reset the
DSP is back in its loader: the tool sends the game's boot block upload, then `ACE1` twice,
then the volumes (`p2k_reboot`).

**Whitestar** (apollo13, xfiles): `FE xx` sets the master volume and **must** be completed
by `FD`, which the game always sends right after: without it the board waits for it and
swallows every later command, the `00` stop included; so every replay sends `FE xx FD`, and
a halt that split the game's own command gets its `FD`. Measured on apollo13 with music
`06`: after the game's own `FE 2C`, `-54.4` LUFS; `FE 10 FD` gives `-17.0`, `FE 2C FD`
`-54.7`, `FE 2F FD` silence. The games re-send their `FE xx FD` every 0.5 s in attract mode,
and the tool does too before each command: on xfiles, music `0F` ends after 22 s with the
board muted, and without the refresh every later command was silent (one-byte sweep: 17
sounds instead of 170).

**Data East** (gnr_300, btmn_106; same BSMT board, told apart by the game's hardware
generation): no master volume, but the single bytes `20`..`2F` set the music volume, `20`
loudest (also the board's default) and each step quieter. The stop `00` does not reset it,
and some music tracks fade it out when they end by themselves (gnr_300 `10`, btmn_106
`01`). Without a reset between commands, the old sweep left later music 17 to 50 dB too
quiet and recorded six gnr_300 loops as `no_sound`. So before every command the tool sends
the music volume the game sent last at boot (gnr_300: `20`), else `20` (btmn_106 sends
none), **then the stop `00`**, which does not reset the music volume. Without that `00`, the
`20` alone left btmn_106's BSMT output on a held DC level (idle at +14462 LSB): 140 of its
141 WAVs started more than 256 LSB away from 0 (a click in VPX) and `7B` clipped 6151
samples; with `20 00`, 0 of 141 start off zero and `7B` does not clip. gnr_300 was not
affected (0 of 161 either way). `21`..`2F` are still swept: pure volume bytes end as `no_sound` or a blip, and the
real sounds among them are kept (gnr_300 `2E` is a loop). Measured on gnr_300: music `30`
after `10` ended went from -46.1 to -15.7 LUFS, `11`, `12` and `2E` from silent to
-17.5/-17.7/-17.7 LUFS; on btmn_106, `03` after `01` from -60.7 to -15.6 LUFS.

**System 11**: no volume stage. Stop: `00` on WMSS11 (the game sends it at power-up; it cuts
a looping sound within 0.6 s). WMSS11C ignores `00`; on S11_SNDOVERLAY games (whirl_l3) the
game never even sends `00`-`1F` to it (s11.c `pia5cb2_w` routes those to the solenoid
overlay). A sweep of all 256 bytes used as the stop after the looping `0x22` found `20`,
`93`, `94`, `98` and `9E` silent within 0.5 s; `20` then stopped all 22 looping commands of
the board. WMSS11J: `00`, an unmeasured guess (the reset fallback covers a wrong guess).

**Bally Cheap Squeak / Turbo Cheap Squeak** (spyhuntr, motrdome, cityslck): PinMAME's
`BY45` and `BYTCS` boards had no manual-command handler; the PinMAME fork rom2altsound
builds adds one (`by35snd.c`). The game sends a byte as two nibbles with one strobe: the low
nibble with the sound interrupt, the high one 70 to 130 us later, read by the same interrupt
handler; the handler hands the high nibble over on the read that follows. No sounds.dat
section: the sweep is 01..FF. Stop `00` (the games send it at power-up and between
sounds). It does not stop every music (motrdome `21`, `34`, `50`..`52`, spyhuntr `11`,
`12`), and the board is then reset; after a sound CPU reset the TCS program runs a ROM and
RAM self-test (about 5 s on cityslck) before it enables its command interrupt, so the wait
for quiet after a reset is 7 s on `BYTCS` (`REBOOT_SECS`; with the old 4 s, and with a
reset as the only stop, the next command was swallowed). Defaults, `--max-secs 20
--loop-max-secs 40`: spyhuntr 53 sounds (2 resets), motrdome 64 (5 resets), cityslck 133
(7 blips, 10 recovered by the retry, no reset); no loop repeated exactly within 40 s.

**The early Bally boards** (vikingb, xenon, eballdlx; read in their sound programs with a
6800 disassembler, then measured). The game drives the board from the four lines it
shares with its solenoids, plus "Sound E" (a fifth line) and a strobe (by35.c `pia1b_w`,
`pia1cb2_w`); the board's interrupt handler reads the lines through the AY-3-8910's port A,
inverted (by35snd.c `sp_8910a_r`). Every handler first silences the AY-3-8910, so a command
cuts the sound before it.

- **Sounds Plus -51** (BY51, vikingb): one read of five lines, so 32 commands; the byte
  `xx` the game sends runs entry `~xx & 1F` of the program's table ($109B). PinMAME's
  manual command sends it as is. Sweep `00`..`1F` (`00` is the table's last entry, a
  sound). Stop `1E`: entry 1, which turns the background off ($12C9; entry 2, `1D`, turns
  it on, and the program then loops on it, $12CD). After a reset the program waits with
  its interrupts off for 7.0 s (a delay loop of 50 x 15661 x 8 cycles at 894886 Hz, $1013),
  then clears its interrupt flag: a command sent meanwhile is lost. This is why 0.2.0 got
  nothing out of these boards: with no known stop, every recording ended with a reset, and
  the next command came 4 s later, into the delay loop. The wait after a reset is now 8 s
  (`REBOOT_SECS`), and the stop makes resets rare.
- **Sounds Plus -56** (`SNDBRD_BY56`, xenon), which PinMAME also names "BY51" (the same
  interface, variant 1; the tool reports it as BY56): the handler reads the lines twice,
  about 57 us apart, and makes a byte of the two nibbles ($F02E-$F078): the game puts the low
  nibble on the lines with the strobe and the high one right after. The byte `xx` runs entry
  `xx - 4` ($F0D5; `00`..`03` and `3C`..`FF` do nothing). PinMAME's manual command
  (`sp51_manCmd_w`) leaves the same byte on the lines for both reads, so only the bytes
  whose nibbles are equal reached the board. The C shim (`shim_nibble_hook`) puts a read
  handler in front of the sound CPU's PIA (as it does for the SAM DAC): once a command is
  armed, the first read of the PIA's port A goes through unchanged, then the high nibble is
  put on the lines through the board's own data handler, for the second read. PinMAME is
  not changed. xenon reads the lines a third time, after both nibbles, to clear the
  interrupt (the log says how many reads the first command saw). Sweep `01`..`FF`. Stop
  `05` (entry 1, which does nothing beyond the silencing). Same 7.0 s delay after a reset.
  Speech: entries `24` and up (`28`..`3B`) play the MC3417 lines, some with the interrupts
  off, so a stop waits for the end of the line.
- **Squawk & Talk -61** (BYSNT, eballdlx): the same two reads, the same `xx - 4`; PinMAME's
  manual command already hands the board the low nibble on the first read and the high one
  on the second (`snt_8910a_r`). Sweep `01`..`DE`: from `DF` on the commands set the volume
  lines (above). The interrupt handler rewrites its return address to the main loop
  ($F6D7), so a command abandons whatever was playing. Stop `05`: entry 1, the background
  off ($FC90). `06` (entry 2) turns the background on but returns straight to the main loop
  ($FC8D), which only looks at the background flag after a command is done: alone, `06`
  plays nothing; the tool sends `04` (entry 0, nothing) after it, as the game would send
  its next sound (`command_sends`). The background then ramps up for more than an hour (its
  step counter $60 goes up by one every 48 s or so, to `78`), so it never loops within a
  search. Speech: entries `24`..`37` ($F99D) and `38`..`58` ($FB0B), commands `28`..`5C`,
  53 lines on the TMS5200. After a reset the program tests its RAM and the AY-3-8910's
  registers: back in its main loop 4.0 to 4.25 s later; the wait is 6 s.
  The board's DAC is unsigned and keeps the last value a sound wrote; PinMAME's DAC passes
  that on as a DC level (`UnsignedVolTable`, 0..32767, at a mixing level of 20: up to 6553
  LSB). Raw, eballdlx's files started and ended on levels from 0 to 6553 LSB, a click in
  AltSound, which plays a file from 0 and stops it at 0. On this board the files are
  written DC-blocked (`DC_BLOCKED`, the `--dc-block` filter, as the board's AC-coupled
  output), and a file that ends on a step of the held level ends once the filtered step
  has decayed to silence (about 0.1 s) instead of in the middle of it; with `--dc-block`
  every board's files end that way. Measured: the 84 files that are not cut at 2 minutes
  start and end within 50 LSB of 0 (raw: 84 of 85 started or ended more than 256 LSB away).
- **-32 / -50** (BY32): no sound CPU; `by32_manCmd_w` plays a tone from the 32-byte PROM.
  Sweep `00`..`1F`; `xF` plays nothing, and `0F` mutes the tone at once (the strobe drop
  in `by32_ctrl_w`), hence the stop. Measured on lostwrld (quick survey): 15 tones from the
  32 commands.

What PinMAME hands AltSound on these machines is not these commands: `sndbrd_data_w`
logs every write of the shared lines (by35.c `pia1b_w`, solenoids included), 4 bits at a
time, and libaltsound pairs them two by two (its BY35 generation has no preprocessing). The
packs' ids are the game's commands, as on the other boards; they do not match what
VPinball would look up today.

**SAM**: no sound board; the sounds are read from the image (see Stern SAM).

### BSMT2000: the chip's own program

The BSMT2000 (Data East, Sega, Stern Whitestar, Alvin G.) is a TMS320C15 DSP with the
sound program in its mask ROM. PinMAME emulated it at a high level (HLE: voices, ADPCM and
mixing rewritten in C, with known approximations). The fork built here also runs the chip's
real program on a TMS320C1x core (LLE, after MAME's `bsmt2000.cpp`) when it finds MAME's
`bsmt2000.bin` (8 KiB, CRC `c2a265af`): in `bsmt2000.zip` or a `bsmt2000/` folder of its
ROM path, else inside the game's zip (then its parent's). The file is never shipped nor
embedded. rom2altsound links (copies, on Windows) the first valid `bsmt2000.zip` or
`bsmt2000/bsmt2000.bin` it finds next to the ROM zip, in `--roms` or in `./roms` into its
private `vpm/roms`, then does PinMAME's lookup itself (`src/bsmtfw.rs`: the zip's central
directory CRC, or the file's, in the same order as `lle_load_firmware`) to report which
emulation ran: `manifest.json` `bsmt2000.emulation` is `lle` (with `firmware_crc` and where
it was found) or `hle` (with why), on machines with the chip only (`shim_has_bsmt2000`).
`--bsmt-hle` sets `PINMAME_BSMT2000_HLE=1`, which makes PinMAME use the HLE anyway.

Measured against the HLE (same packs otherwise): the HLE without the file is byte-identical
to the previous PinMAME on apollo13, btmn_106, gnr_300, hook_408, monopole, rctycn,
trek_201 and xfiles. With the program, Monopoly writes 208 files instead of 165 (sounds
the HLE left silent), ADPCM sounds (`5F`) change, the rest keeps its counts within a file
or two; the LLE runs about 1.6 times slower (apollo13: 1050 s instead of 650 s).

### Stern SAM

A SAM machine has one CPU, an Atmel AT91 (ARM7) at 40 MHz, and no sound board: the FIQ
handler (4 kHz) mixes up to 8 voices in software and writes the mix to a TI PCM1755 DAC
through a Xilinx FIFO, 24 kHz stereo. The game never sends a sound command (sam.c's board
interface has empty handlers), so neither the method of the other families nor PinMAME's
AltSound can work. `run` hands every set of the SAM driver (`sam_sets.rs`) to
`src/sampack.rs`, which reads the sounds from the flash image (`src/sam.rs`) without
emulation, and boots the game only for its volume.

**Image**: the largest member of the zip (acd_168h: `ACD168LE.BIN`, 119,685,024 bytes, 14.3
banks of 8 MB; the name differs from sam.c's `acd_168h.bin`, PinMAME matches the CRC),
inflated in memory (`src/zipread.rs`, `miniz_oxide`) and checked against the zip's CRC32,
then compared with the driver's.

**Format** (Ashram56's reverse engineering of Tron LE 1.74,
<https://github.com/Ashram56/Tron-Legacy-LE-ROM-Decryption>, confirmed on acd_168h):

- banked pointer `p`: file offset `(p >> 24) * 0x800000 + (p & 0xFFFFFF)`;
- **sample directory**: the longest run of u32 words of the first 8 MB that are 0 or banked
  pointers to a valid script header (acd_168h: file `0x1200EC`, 5270 words). The language
  count is the largest stride for which at least half of the groups of entries hold one
  pointer (5 on acd_168h: 1054 samples x 5 languages, all five the same on this US ROM);
- **script**: `05 <voice mask> <voices> <len32>` (length in FIQ ticks) then opcodes, each
  with a fixed operand count (`sam::SCRIPT_ARGS`; bytes above `10` are padding). Every one
  of acd_168h's 1054 distinct scripts parses to its end:

| op | operands | meaning |
|---|---|---|
| `00` | - | end |
| `01` | 2 | ? (`01 00 00`, `01 00 01`) |
| `02` | 2 | `<voice> <bus>` routing (music = bus 3) |
| `03` | 1 | loop: back to the `07` mark |
| `04` | 2 | bus link |
| `06` | - | ? (once) |
| `07` | 1 | loop start mark |
| `08` | 4 | u32 marker the game polls |
| `09` | 15 | volume ramp `<bus> <period u32> <steps u32> <start> <delta> <final>` |
| `0a` | 5 | start a stream: `<voice> <banked ptr32>` (replaces the voice's stream) |
| `0b` | 2 | channel stop stub (samples 1-8) |
| `0d` | 4 | u32 song position the game polls: `song << 24 \| chunk index` |
| `0e` | 1 | stop the voices in the mask |
| `0f` | 4 | wait N ticks |
| `10` | 1 | wait until the voices in the mask have finished |

- **stream**: `u32 samples, u16 1, u8 divisor, u8 divisor`, then IMA ADPCM, 4 bits, low
  nibble first, the standard step table, predictor and index from 0 at the start of every
  stream; 24000 / divisor Hz (24 kHz, 12 kHz for most voices);
- **sound call table**: 20-byte records whose `+8` points (CPU address `0x04xxxxxx`) to a
  0-terminated u16 list of sample ids; the game's code plays a call, which picks one of its
  samples. The table is the longest run of records with a non-empty list, then grown over
  neighbours with an empty list whose first word (a pointer to the call's state in nvram)
  continues the sequence: acd_168h's starts at file `0x10ADF4` with call 0, which plays
  nothing, for 413 calls. (The study script that came first started one record later, so
  its call numbers are one lower.)

**Sounds**: a script with at most one stream and no loop is a sound; each distinct stream
is decoded once at its own rate and named after the first directory entry that plays it
(`s<sample>-<rom>.wav`, `s<sample>-l<language>-<rom>.wav` beyond the first language). The
script's own volume ramps (`09`) are not applied. acd_168h: 951 streams (1854 s), 8 stub
scripts (no stream).

**Music**: a script with two streams or more, or a `07`...`03` loop, is music. It runs once
on a 24 kHz timeline: `0f` adds 6 frames per tick, `10` waits for the voice's stream end,
`0a` on a busy voice cuts its stream; 12 kHz chunks are brought to 24 kHz by linear
interpolation (the FIQ's). Chunks chained with `10` are joined gaplessly; a gap under one
tick holds the last value, a longer one is silence; overlapping streams (two short loop
beds) are mixed. Every chunk restarts its ADPCM decoder at 0, so its first nibbles saturate
(`x7`/`xF`) and the join dips for about 0.3 ms, on the machine too: those leading samples
(at most 16) are replaced by a line from the previous sample. Roles: `teaser` (song select:
markers 0 then 1, loops), `full` (chunk 1 to the end, once), `main` (the script that picks
up right after the teaser's last chunk; loops on songs 1-12 and 18), `resume` (another
partial version, once), `bed` (a loop without song positions). A looping script gets a
`smpl` loop from the `07` mark to its end; with an intro before the mark (the two beds),
`-loop.wav` holds the cycle (its first chunk's ramp redrawn from the cycle's own end) and
`-extended.wav` the intro then whole cycles for `--intro-loop-secs`, each cycle's ramp
redrawn from the previous one; the CSVs play the extended file once (LOOP 0), as for the
other families. acd_168h: 86 scripts, 24 songs (teaser, main and full each; resume for
songs 1-12), 268.5 minutes, 4255 distinct chunks, 39 loops.

**Checked** against the study script (`sam_study.py`, Python, written from the same
format): the 951 sound files and the 86 music files are sample-identical, `smpl` loop
points, roles and songs included. Its `export` wrote 1872 files: the same 951, plus 917
music chunks that its byte-by-byte pointer search found in the first 256 bytes of the
music scripts, plus 4 false positives (headers with field 4 != 1, decoding to noise); the
opcode parser finds neither.

**Levels**: the files are the samples as decoded, scaled by the factory DAC attenuation
the game writes on the warm boot, as PinMAME plays it: sam.c turns the register into a
mixer level of `(v & 7F) * 100 / 7F` percent (integer division), so acd_168h's `E8` plays
at 81 %, -1.83 dB (`factory_gain`), then rounded to the nearest 16-bit value (no dither:
these samples do not come from PinMAME's mixer). That is what VPX players hear today. The
PCM1755 datasheet gives -0.5 dB per step from `FF` (`E8` = -11.5 dB, `datasheet_offset_db`
and `factory_volume.attenuation_db` in the manifest): the real machine plays about 9.7 dB
quieter than PinMAME at the factory setting, and the two drift further apart as the
attenuation grows (`C0`: -6.0 dB in PinMAME, -31.5 dB by the datasheet); PinMAME's own
comment in sam.c has the datasheet's rule ("For ATx[7:0]DEC = 0 through 128, attenuation
is set to infinite attenuation") but maps the rest linearly. The files follow PinMAME, so
that a pack sounds like the game in VPX; should PinMAME follow the datasheet one day, the
gain would follow it. With
`--volume reference`, or when the attenuation is not known (the boot failed, `--no-factory`),
they are at full scale: what the DAC plays at 0 dB (attenuation `FF`), and
`recorded_volume` says so. At full scale, 855 of the 1037 acd_168h files reach
+32767/-32768 as decoded (median 6 samples, at most 3.6 % of a file): the ADPCM data is
mastered that hot; the ARM's decoder clamps the same way. Loudness is measured as for the
other families, except for the true peak: `ebur128`'s precise true-peak resampler costs
about 0.13 s of CPU per second of audio (more than 2000 s for acd_168h's 5 hours), so SAM
files use `loudness::measure_fast`: the same R128 meter for the loudness and
`true_peak_interpolated` for the peak (x8 at 24 kHz, a 16-tap Lanczos-windowed sinc; within
0.1 dB of the meter in the tests). The totals count the first language's sounds and every
music file.

**Factory volume**: the PCM1755's attenuation registers (`0x10` left, `0x11` right; FF =
0 dB, -0.5 dB per step, 80 and below mute, per the datasheet) are written by bit-banging
the PIO lines, which PinMAME sees as writes to the CPU's single I/O port (`sam_port_w`).
sam.c keeps the result in its private struct, so the shim installs its own port handler
in front of it (`shim_sam_hook_dac`, from the audio-available callback, before the CPU
runs): it decodes the same serial words, logs them with the emulated time and calls sam.c's
handler, which still does the work; PinMAME is not modified. The cold boot (child process,
no nvram) and the warm boot (from the cold boot's nvram) each run until the DAC has been
quiet for 3 s after `--boot-secs`. acd_168h writes `10 E8` and `11 E8` 0.37 s into every
boot, cold or warm, and nothing more in 120 s of attract mode: PinMAME plays it at
`(v & 7F) * 100 / 7F` = 81 %, -1.83 dB (`factory_offset_db`, the files' gain), where the
DAC's datasheet says -11.5 dB (`datasheet_offset_db`). `E8` = `80 + 2 x 52`.

That register **is the operator's volume setting** (`factory_offset.verified` true,
`verified_by`), measured with the hidden `--sam-volume-test N`: it boots from the nvram in
the vpm (cold without one), and once the DAC is quiet opens the coin door, presses its
`Plus` button N times (`Minus` when N is negative), closes the door and logs every DAC
write; stopping saves the nvram, so a second run with `0` shows what the game writes at
its next power-up. The coin door buttons only reach the game through libpinmame's
keyboard handling (sam.c maps them to keys: `END` toggles the door, `8` Minus, `9` Plus,
and the door state exists only there), so the test turns it on and answers the key
callback; a switch set with `PinmameSetSwitch` with the door closed did nothing to the
DAC. On acd_168h, from the factory nvram: `+4` gives `EA`, `EC`, `EE` (the first press
only shows the setting, then 1 dB, two DAC steps, per press), and the next boot writes
`EE` at 0.37 s; `-2` then gives `EC`, written again at the next boot. Boot cost: 12-13 s
wall per boot (15 s emulated, the ARM7 interpreter; the asmjit JIT is off in this
build).

**AltSound**: one row per (call, sample of the call), the first language: ID = call id,
music rows on channel 0 (looping ones LOOP 100, the extended ones LOOP 0), the rest
polyphonic, DUCK 100, STOP 0; a sample whose languages differ is a callout. acd_168h: 1129
rows for 412 calls, 561 files. **They do not play**: PinMAME's AltSound is fed by sound
commands and SAM has none. A PinMAME that reported the call ids would need the address of
the game's `snd_play` (per ROM, like sam.c's `fastflipaddr`) or a signature search; until
then the CSVs are for editing and measurement.

**Cost** on the 8-core Xeon test host: acd_168h in 49 s wall (24 s for the files, 4
threads; two boots of 12 s), peak RSS 956 MB (the 120 MB image, plus up to four files in
flight, the longest a 17.7-minute song).

### Commands

They come from PinMAME's `release/sounds.dat`: the sections whose header starts with the game
name or the parent name (afm_113b uses `afm_113:`). The generic `dcs:` / `wpcs:` sections
supply the stop command. If a game has no section, the tool sweeps raw commands instead,
leaving out those that change the board's state (`commands_from` says what was swept).
Each board's sweep is made of one or more ranges (WPCS: its single bytes, then the `7A`
bank), printed before the run with how many commands each holds, and again in the summary
with what came out of it, as in
`sweep board 0 (WPCS): bank 7A00..7AFF: 137 with sound, 119 silent (last sound 7A88)`;
`manifest.json` has the same in `sweep` (per range: `board`, `family`, `range`,
`commands`, `tried`, `with_sound`, `silent`, `written`, `last_sound`):

- DCS: the populated tracks of the ROM's own track catalog (`src/dcsrom.rs`, layout from
  mjrgh's DCSExplorer: catalog in U2 at $3000/$4000/$6000, track index pointer at +$40,
  track count at +$46, `FFxxxx` = empty slot). A DCS command below the track count plays
  that track; `0000` (stop) and the `55 xx` specials are left out. Without a catalog:
  0001..03FF. **A DCS game with a sounds.dat section gets the union**: the section's
  commands (with their names) plus the catalog's populated tracks it leaves out, sorted by
  track number. afm_113b: 1130 tracks, 589 populated besides `0000`; its section lists 575
  of them and misses 14, among them `0013`, a 120 s loop at -47.6 LUFS (the 13 others play
  nothing).
- Pinball 2000: the same catalog, in the board's 16-bit words read as two bytes each, high
  byte first (`dcsrom::p2k_image`; PinMAME's region holds the 1 MiB sound flash at 0, U109
  at $400000 and U110 at $800000, little-endian words), at $10000 of that image: the flash's
  own entry ($100 * 4 KiB, chip 0, checksum 0), then U109's and U110's with their chip
  selects `04` and `08` and their checksums, the track index pointer at +$40, the count at
  +$46. ROM pointers are plain 24-bit offsets into the image. swep1_130: 2301 tracks
  (`0000`..`08FC`), 690 populated besides `0000`; rfm_120: 2940, 1557. No sounds.dat
  section: the catalog is the command list.
- Whitestar / Data East (BSMT): bytes 01..FB. `00` is the stop; `FC`..`FF` start two-byte
  commands (`FE xx FD` is the volume). Probed: `FF xx` plays the same sound as `xx` (apollo13,
  xfiles), and `FC xx` starts a loop for every `xx` on apollo13, gnr_300 and xfiles alike, so
  neither is swept.
- WPCS: bytes 01..FF without the tempo/volume bytes of sounds.dat `wpcs:` (1E-2F, 60-72) and
  the prefixes 79 (volume) and 7A (second bank), then the second bank itself,
  `7A00`..`7AFF` (ids `0x7Axx`), where most voices and effects of some games are (taf_l5:
  137 sounds, `7A00`..`7A88`). All 256 are swept, the empty ones ending as `no_sound` after
  1.5 s: the program's own table of the bank was not read for its length.
- Bally Sounds Plus -51 and -32/-50: bytes 00..1F (five lines). Squawk & Talk: 01..DE
  (`DF`..`FF` are its volume commands). Sounds Plus -56: 01..FF (see "Per family").
- Other boards: bytes 01..FF.

**WPCS second bank**: sounds.dat writes the sounds of the `7A` bank with a filler byte in
front, `01 7A xx` (Twilight Zone's 142 entries), so that PinMAME's commander, which sends
byte pairs, puts `7A` second. The game sends `7A xx`, and libaltsound keys it `0x7Axx`.
Sent as is, the filler is a command of its own (on Twilight Zone `01` fades the music out
over a second), so it is dropped: the command and its id are `7A xx` (`--only` takes either
form).

On two-board machines the id is `board<<8 | byte`, so `0x0105` means byte 05 on board 1.
`--stop` overrides the stop. It uses the same notation as `--only`.

### Silence

The upstream mixer adds +/-1 LSB TPDF dither, so a sample within 2 LSB of the idle level counts
as silence. The idle level is not always 0: WPCS and System 11 boards hold their DAC on the
last value a sound wrote (whirl_l3 idled at +2056, and +6264 or +10248 after some sounds).
Each frame whose span stays within the dither updates the per-channel idle level. All
decisions use emulated time.

On those boards the DACs are AC-coupled once the game has booted (`shim_dac_ac_couple`:
every DAC of the machine goes through dac.c's 10 Hz DC correction, see Reference volume),
so that no sound starts on the level the one before left (the real boards' outputs are
AC-coupled: that level never reached the speaker), and the idle level settles back to 0
within about 60 ms. Measured on whirl_l3 (`--max-secs 10`): 187 of its 189 files started
more than 256 LSB away from 0 (up to 10251 LSB), none do now (157 LSB at most), with the
same 189 files and 219 blips and no clipped file. On taf_l5 (WPCS) see Reference volume.
The Data East games have no DAC (gnr_300 and trek_201 files start within 8 LSB of 0
without it).

### Trimming

Leading and trailing silence (relative to the idle level) is cut. Some BSMT sounds then hold
the output at flat DC offsets for seconds, stepping between them, before coming back to 0
(gnr_300 `67`: three levels between -1100 and -6500 LSB for 2 s, then 0 at 5.9 s; xfiles
`69`: -114, -288, -90, -16).
The idle tracker treats each step as sound, so the file used to run until the last one. The
end is now walked back over such steps: a burst of moving samples of at most 10 ms that
changes the level, after a level held for at least 40 ms within 16 LSB and with no more
motion than the dither (standard deviation at most 1 LSB; the real plateaus measure 0.5).
Without that last condition a quiet decaying tail around 0 passed for held levels (apollo13
`76`, sd 2 to 3.7 LSB at about -74 dBFS, was cut from 3.92 to 3.53 s). The file ends where
the first held level starts (gnr_300 `67`: 5.95 to 3.95 s; xfiles `69`: 2.29 to 1.83 s). A
click that comes back to its level is not a step.

### Loops

A sound that keeps playing is written as **one exact cycle**, for a seamless loop (an
AltSound pack's LOOP column): the file is the intro (if any) followed by exactly one loop
body, cut at the sample where the body's end joins its start; when there is an intro, the
body alone also goes to `<id>-<rom>-loop.wav`. The manifest's `loop` says how it was found.
The pack then plays the body alone, or for a loop with an intro of its own, an extended
file (see AltSound pack).

- **Audio** (`method: "audio"`, every board, `src/looping.rs`). The emulation is
  deterministic, so after an intro `x[n] = x[n + period]`, up to PinMAME's +/-1 LSB TPDF
  dither. The period is rarely a whole number of output samples: the board's stream is
  resampled to 44.1 kHz with libsamplerate's sinc converter (a DCS frame, 240 samples at
  31250 Hz, is 338.688 output samples), so each cycle is the previous one delayed by a
  constant fraction of a sample. The comparison applies that fraction (a 33-tap
  windowed-sinc fractional delay) before measuring the residual, in 2048-sample windows; a
  window passes when its residual is within 4 LSB rms or 30 dB below its own level. At a
  whole-sample lag, the 0.4-sample offset of cv_20h `0030` (232 frames, 78575.616 samples)
  left -25 dB in bright windows, and only every 8th cycle (0.07 sample off) passed; with
  the fractional delay, the residual of our DCS loops is -30 to -73 dB.
  Candidates come from an envelope (rms over 4096 samples every 1024) whose last two spans
  match; each is refined to the sample, then its fraction is fitted. The period must hold
  from some point to the end of the recording over **two full cycles and at least 20 s**
  (`looping::CONFIRM_SECS`): that span keeps a phrase played twice inside a longer loop
  from passing for the loop. A verified lag is often a multiple of the period (the envelope
  matches best there), so its divisors are tried and the shortest that verifies wins. The
  body starts one window into the repeating part, at the point of the next second where two
  cycles differ least (preferably a quiet one), or at the very start when the repetition
  holds from the first sample (no intro, no `-loop.wav`). A body is rounded to whole
  samples, so each repeat shifts the waveform by the period's fraction: harmless on long
  loops, audible on a short bright one (the BSMT test tone `F2`, 2984.1 samples, jumped by
  209 LSB at the joint on btmn_106, where its steps reach 2400). A loop of at most 2 s
  whose cycle is not within 0.05 sample of a whole number therefore takes the fewest cycles
  that are (`cycles`: 10 x 2984.1 = 29841.0).
- **DCS track program** (`method: "dcs-catalog"`, `src/dcsrom.rs`). A DCS command plays a
  track, a byte-code program (mjrgh's DCSExplorer, `ExecTrack`) that loads audio streams
  into channels, waits a number of 7.68 ms frames between opcodes, sets and fades mixing
  levels and loops (`0E nn` ... `0F`, `nn = 0` forever). The tool runs the program frame by
  frame without decoding audio (streams are only counted down), from a silent board, until
  the whole state (program positions and counters, loop stacks, stream positions, mixing
  levels and fades, queued commands) repeats: that gives the period (an LCM of program and
  stream loops comes out by itself) and the frame where it starts. ROM pointers carry the
  chip in bits 21-23 (U2 = 0, 1 MiB per chip). The program's period is then checked on the
  audio, with the same windows, over one full period (at most 60 s, at least 20 s) after
  the intro; for such a track the recording goes on until that is possible (up to 900 s),
  past `--loop-max-secs`. A period found in the audio alone is taken when it equals the
  program's (`dcs-catalog`), divides it (`audio`, with a note), or once the program's
  period failed on the audio (`audio`, with a note). The program period is only a hint:
  1993 ROMs read opcodes 04 and 06 differently (not modelled), type 3 tracks are not
  modelled, and the board may hold a deferred track left by an earlier command (below).
- **Sound CPU state** (`method: "sequencer-state"`, every board but DCS,
  `src/seqstate.rs`, `src/seqloop.rs`). On these boards the audio of a music never repeats
  sample-exactly: the music is a program on the sound CPU, a sequencer that walks its score
  at each tick of a timer interrupt, and the ticks are not locked to the chips' sample
  clocks, so each note of the next cycle starts a little off (Twilight Zone `02`: the same
  notes 55 to 91 samples later than one 47.2 s cycle before) and the FM oscillators are at
  other phases (the two cycles differ by -15 to -19 dB). Nothing from outside causes it:
  the game CPUs are halted for the whole extraction, and with the volume sent only once
  after the boot (the hidden `--no-refresh`, instead of before every command) the music of
  apollo13 (`06`, `0A`), xfiles (`01`, `04`), gnr_300 (`01`, `04`) and btmn_106 (`03`,
  `08`) still did not repeat sample-exactly within 240 s, with the BSMT2000's own program.
  What does repeat is the
  sequencer's state. At the end of every emulated frame the shim reads the audio CPUs'
  registers and RAM (the entries of the CPU's read map that are plain RAM, `MRA_RAM`, read
  straight from the CPU's memory region: no handler runs; 8 KB on WPCS and BSMT boards, 4
  + 8 KB on System 11, 128 bytes on Cheap Squeak), and keeps the bytes that change. A loop
  of that state is looked for on the bytes that carry the music's position, leaving out
  the registers and the stack (where the CPU happened to be when the frame ended), the
  free-running counters (bytes that only count up, or down: Twilight Zone has one 16-bit
  tick counter per voice) and the bytes that change in most frames (tick countdowns: at a
  frame boundary their value depends on where the frame fell between two ticks).
  Candidate periods are how far back the state of a group of those bytes was last entered
  (8 interleaved groups, so that a byte with a clock of its own only hides the period from
  its group); a period holds when, from some frame to the end, at least 99.8 % of the
  bytes equal the same byte one period later, give or take a frame (the period is rarely a
  whole number of frames), over at least one period and 20 s (two cycles in the
  recording). A value held for a single frame is a working variable caught mid-update and
  is not compared (apollo13 `06`: `13 13 0E 13 13`, one frame in 15 on some bytes). Of two
  neighbouring periods, the one with the most frames exactly equal one period later wins.
  The period that holds on every byte comes first; one that leaves out up to 2 bytes (and
  5 % of them) that keep a clock of their own (whirl_l3 `0121`: two bytes counting down
  over several cycles) is only taken at `--loop-max-secs`, once every period of the whole
  state up to half the recording has had its chance (with 4 bytes left out, a 1.88 s bar
  of Twilight Zone `02` passed for its 47.2 s loop).

  The audio must then follow the state: per half second, over a cycle and at least 20 s,
  the lag within one frame (and 48 samples) of the state's period that best maps the
  audio onto itself; 75 % of these lags must agree with their median (within 100 samples)
  and the median residual must be at most -3 dB. On the BSMT boards the 6809's state can
  repeat while the audio does not (xfiles `04` every 41.0 s: the residual about 0 dB, 60 %
  of the lags agree), as the music also lies in the BSMT2000's own sample streams; there
  the audio follows every other state cycle (82.05 s: 99 % of the lags agree, -3.8 dB).
  The cut is then where the two cycles differ least: the 512-sample window (at its half
  second's lag, refined within 8 samples where it carries signal) with the least squared
  difference, looked for in the first 10 s of the cycle first and taken there when it is
  at least 30 dB below the music around it, then over the whole cycle; within it, the
  sample where the two cycles are closest. The body is that lag long, a whole number of
  samples. `loop.sequencer_state` in the manifest: `period_frames`, `repeats_from_frame`,
  `byte_share`, `own_clock_bytes`, `mask` (the bytes that changed and those left out by
  kind), `cycle_residual_db`, `lag_agreement`; `residual_db` is the cut window's. A state
  loop the audio did not follow is named in `loop_unresolved`. The audio method is tried
  first, so the loops it finds (the BSMT test tones) stay sample-exact.
  `rom2altsound seq-scan <dump> [<raw.wav>]` runs the finder on a recording dumped with
  `R2A_SEQ_DUMP=<dir>` (every loop search writes `<id>.seq` and `<id>.raw.wav` there).
- **No loop**: a sound still playing at `--loop-max-secs` (or past its DCS period's check)
  is cut at `--max-secs` as before (`ended_by: "max"`), with `loop_unresolved` saying why.

`loop` in the manifest: `intro_samples`, `period_samples` (both in sample frames; the
body's length), `repeats_from_samples` (where the repetition starts: the intro's own audio;
the body starts one verification window after it at least, then at the quietest joint
within a second, so `intro_samples` is never 0 when this is not, and a loop without an
intro of its own has it near 0), `period_exact_samples` (with the fraction), `period_secs`, `cycles`, `method`, `confidence` (1 minus
the worst window's residual-to-signal ratio: 0.999 at -60 dB, 0.968 at -30 dB, lower only
for a near-silent loop judged within the dither), `residual_db`, `verified_secs`,
`loop_file`, `seam` (`joint_step`: the step played at the joint, last frame of the body to
its first; `natural_step`: the step the recording makes there, last frame of the body to the
next one; `error`: the difference between the two, i.e. the discontinuity the loop adds;
`body_p99_step` and `body_max_step`, all in LSB on the raw recording, largest over the
channels), `dcs_track` (what the program does: `loops` with `intro_frames` and
`period_frames`, `ends` after `frames`, or `unknown` with a `reason`), and for
`dcs-catalog` `audio_period_samples` (the audio alone on the same recording, null when it
holds too few cycles) and an optional `note`.

Results (factory settings, same eight ROMs as below; seam `error` in LSB):

| ROM | loops | dcs-catalog / audio / cut at 120 s | periods | worst residual | worst seam error | WAV size before / after |
|---|---|---|---|---|---|---|
| afm_113b | 20 | 19 / 1 / 0 | 0.23 to 42.9 s | -15.6 dB (`0013`, a near-silent tail) | 1 | 301 / 148 MB |
| cv_20h | 27 | 26 / 1 / 0 | 0.58 to 52.3 s | -48.8 dB | 4 | 364 / 127 MB |
| mm_109c | 17 | 17 / 0 / 0 | 1.0 to 45.9 s | -31.4 dB | 1 | 334 / 205 MB |
| rs_l6 | 34 | 26 / 8 / 0 | 0.31 to 58.1 s | -30.6 dB | 4 | 415 / 148 MB |
| apollo13 | 41 | 0 / 3 / 38 | 0.068 s (test tones) | -34.0 dB | 1 | 1013 / 958 MB |
| xfiles | 40 | 0 / 3 / 37 | 0.068 s | -33.0 dB | 1 | 905 / 841 MB |
| gnr_300 | 40 | 0 / 3 / 37 | 0.068 s | -72.7 dB | 1 | 909 / 846 MB |
| btmn_106 | 34 | 0 / 3 / 31 | 0.068 s | -31.2 dB | 1 | 778 / 715 MB |

Every DCS loop came out as an exact cycle, and the audio alone gives the same period as the
track program wherever both apply. rs_l6's 8 `audio` loops: `0004`, whose program the
simulation cannot follow, `041E` (178 audio cycles per program cycle), and six tracks whose
program "period" of 129 to 336 s comes from streams of 16887 to 43733 frames (`FFFF` for
`000D`'s first one) that the audio contradicts over up to 397 s: the 1994 software probably
lays out those streams differently. apollo13 `D2`..`D5` used to be 120 s "loops": recorded
on, they end by themselves after 130.9 s. Retry and clipping counts move as between any two
runs (rs_l6 26 to 25 retried, apollo13 70 retried / 1 recovered to 70 / 0, gnr_300 and
btmn_106 ADPCM files).

`rom2altsound loop-scan [--hint SECS | --hint-frames F] <wav>...` runs the detector on
existing files (one line per file, with the seam).

Sound CPU state results (factory settings, the BSMT2000's own program; `--loop-max-secs`
240 s by default, 600 s for the second count; seam `error` in LSB):

| ROM | musics | from the state | with 600 s | periods | seam error (median / worst) | audio: lags agreeing, cycles apart |
|---|---|---|---|---|---|---|
| tz_94h (WPCS) | 45 | 26 | 31 | 2.5 to 272 s | 1 / 50 | 75 to 100 %, -3.8 to -16.8 dB |
| whirl_l3 (System 11) | 22 | 14 | 17 | 20.6 to 268 s | 1 / 11 | 83 to 100 %, -8.0 to -17.3 dB |
| spyhuntr (Cheap Squeak) | 2 | 2 | | 19.0 s | 0 / 0 | 100 %, -15 to -16 dB |
| cityslck (Turbo Cheap Squeak 2) | 10 | 7 | | 27.1 to 28.4 s | 0 / 0 | 100 %, -23.8 to -32.9 dB |
| motrdome (Turbo Cheap Squeak) | 5 | 0 | | | | |
| xfiles (Whitestar) | 40 | 5 (+ 3 test tones from the audio) | | 11.6 to 82 s | 9 / 16 | 93 to 100 %, -3.2 to -5.9 dB |
| vikingb (Sounds Plus -51) | 4 | 0 (+ 3 short tone loops from the audio) | | | | |
| xenon (Sounds Plus -56) | 2 | 0 (+ 1 from the audio) | | | | |
| eballdlx (Squawk & Talk) | 1 | 0 | | | | |

On the Bally Sounds Plus and Squawk & Talk boards (6802, 128 bytes of RAM, all in the
state) the search runs, and finds what there is: vikingb's `06`, `07` and `08` repeat
exactly in the audio (bodies of 0.048 to 0.125 s after a 0.9 s intro); its background
`1D` follows a byte of RAM ($00) that does not come back within the search; xenon's `1A` repeats in the
state every 2.600 s, but its cycles differ by -7.5 dB in the audio (probably the
AY-3-8910's noise generator, which is in the chip, not in the state), so it is not taken; eballdlx's background
(`06`) changes for more than an hour.

A long loop is found with a longer search only: Twilight Zone `03` repeats after 271.9 s
(the state every 16312 frames), Whirlwind `0121` and `0122` after 267.8 s, eight times the
33.5 s cycle of the rest of their state (two bytes count the cycles). On motrdome a byte
of the music's state changes up to four frames off from one cycle to the next, which the
one-frame tolerance does not accept (four frames made the other boards' periods come out a
frame off). On the BSMT boards five more xfiles musics had a state loop that the audio did
not follow, and apollo13, gnr_300 and btmn_106 (two musics each) none at all.

### Loudness

Every level in the manifest is measured on the DC-blocked signal (a 10 Hz one-pole
high-pass starting from the idle level before the command, i.e. what an AC-coupled output
plays): `peak_dbfs`, `rms_dbfs`, and the EBU R128 integrated loudness (`lufs`) and true peak
(`true_peak_dbtp`), with the `ebur128` crate (`precision-true-peak`, as in the loudness
plugin). The true peak used to be measured on the raw samples, so it could read below the
DC-blocked sample peak (xfiles `69`: -28.5 dBTP for a -27.5 dBFS peak); now it never does.
A mono ROM stream is measured as two identical channels, because that is what VPX plays (it
reads +3 LU above a mono-only measurement); stereo stays stereo. Files under 400 ms have no
complete gating block and no `lufs`; their `level_lufs` is the loudness of the file padded
with silence to 400 ms, the level used to compare files.

Per ROM (`loudness` in the manifest, and the summary), without the files flagged
`ignores_master_volume`:

- `all`: the integrated loudness of the written files' R128 gating blocks together (the
  files played back to back, minus the blocks straddling two files; a file under 400 ms
  counts as one padded block), with the loudest file's true peak;
- `excluding_loops`: the same without the loops (`looping_or_truncated`: one cycle, or the
  first `--max-secs` of an unresolved one);
- `median_file_lufs`: the median of the per-file `lufs` of those non-loop files, the level
  of a typical sound, which one loud file cannot move.

### Manifest fields

Per sound: `id`, `name`, `file` (null if the command produced no audio), `duration`,
`blip`, `lufs`, `true_peak_dbtp`, `level_lufs`, `peak_dbfs` and `rms_dbfs` (all DC-blocked;
the peak can read slightly above 0 dBFS when the raw output clipped), `clipped_samples` (raw
samples at +32767/-32768), `dc_offset` (mean of the raw samples, in LSB), `onset` (s from the
first byte to the first sound), `ended_by`, `looping_or_truncated`, `loop` and
`loop_unresolved` (see Loops), `clean_start` (false if
the previous sound could not be stopped), `retried`, `ignores_master_volume`,
`master_volume_check` (the volume it was played at again, the levels away, its level and
move, the reference's id and move), `board`, `volume_init`, `idle_level` (output level at
the end, in LSB).
DCS: `dcs` (see "Ducking, stops and channels").
With the AltSound pack: `twin_of` (the original's id), on DCS `twin_reason` (the two
channels), and `twin` (`residual_db`,
`lag_samples`, `length_diff_samples`, `lufs_diff`) on twins, `pack` on every row
(`file`, `file_kind`, `file_reason`, `loop`, `channel`, `duck`, `stop`, `gsound_type`,
`ducking_profile`: what it became in the CSVs), and at the top level `altsound` (`files`,
`rows`, `loops_with_smpl`, `intro_loops_extended`, `intro_loop_secs`, `twins`,
`merged_twins`, `files_referenced`, `dcs`: `voice_channel`, row counts, `duck_values`,
`callout_profiles`, `sfx_profiles`, `limits`; `twin_test`).
With names applied (see "Sound names"): `sounds_dat_name` on a renamed sound, and `names`
at the top level.

At the top level: `sound_rom_id` (see "Sound names"), `mode` (`factory` or `normal`), `factory` (vpm, saved nvram path and size,
cold boot report), the boards, `boot` (length, what ended it, every byte per board as
`seconds:byte`), `factory_volume` (factory mode) or `game_volume` (the last command per board
and kind: master, DCS channel, Data East music), `volume_init` (what the files were recorded
at), `volume_replays`, `refreshed_before_each_command`, `commands_from`, `sweep` (raw
sweep only, see Commands), `counts` (tried,
with_sound, written, blips, no_sound, loops, loops_exact_dcs_catalog, loops_exact_audio,
loops_unresolved, not_clean, clipped (written files only), retried,
recovered_by_retry, ignores_master_volume), `loudness`, the `stop` actually sent,
`board_resets` and `dc_blocked_wav`. DCS: `dcs` (the catalog summary and
`ducking_check`). `recording_cap`: `max_secs` (120 s by default),
`loop_max_secs` (240 s), `loop_hint_max_secs` (900 s) and what they do (see Loops).

With the factory settings: `volume_mode` (`factory`, the default, or `reference`),
`recorded_volume` (per board: `volume`, the command the files are recorded at or
`full_scale (no volume stage)`, and `from`, where it comes from), `clipped_files` (`id`,
`file`, `clipped_samples`, `ignores_master_volume`, the most clipped first; also in the
other modes), `reference_volume` (the reference master volume command per board, or
`full_scale (no volume stage)`; `board N: ...` per board when they differ: what the files
are recorded at with `--volume reference`, what the offset is measured against in factory
mode), `levels_note`, `factory_offset_db` (the ROM's offset: 0
without a master volume, null if not measured), `factory_offset` (`method`, per board the
`reference_volume`, `factory_volume` and where it comes from, `factory_offset_db`,
`spread_db` and the `samples`: `id`, `reference_lufs`, `factory_lufs`, `delta_db`, in
factory mode `scaled_minus_replay_db`, plus a `note`), `factory_gain` (factory mode:
`method`, `rounding`, per board `recorded_at`, `written_at`, `gain_db`, `gain`,
`spread_db`, `files`, `note`, `own_gains`, and `rewritten`: `files` written again, `clamped_samples`;
`gain_db` is also in each `recorded_volume`) and `loudness.as_shipped` (`factory_offset_db`, `all_lufs`, `excluding_loops_lufs`,
`median_file_lufs`, `loudest_true_peak_dbtp`; `--volume reference` only). Every per-sound
level is that of the file as written (`volume_init`, e.g. `factory 55AA6798 (from
reference 55AAEF10, -22.44 dB)` or `reference 55AAEF10`): in factory mode, measured on the
recording and moved by the board's gain; `clipped_samples` and `master_volume_check` are
those of the recording.

## ROM verification

`rom2altsound roms <dir|zip>...` identifies ROM zips by their content, against the ROM
tables of the PinMAME linked in (`src/romcheck.rs`, `src/drivers.rs`, the driver table part
of `shim/shim.c`). Each zip given, each `*.zip` and each subfolder of a folder given is one
unit; a folder that holds only files is a unit of unzipped ROMs.

**The tables.** The shim walks PinMAME's `drivers[]` and, for each game, its `ROM_START`
block as MAME's own ROM loader does (`rom_first_region` / `rom_first_file` /
`rom_first_chunk`): name, size (the sum of the file's chunks, `ROM_CONTINUE` included),
CRC32 and SHA-1 (`hash_data_extract_printable_checksum`), region, `NO_DUMP`, `BAD_DUMP`,
`ROM_OPTIONAL`. A file loaded into two regions (Pinball 2000's boot ROM, in the CPU's and
the sound board's) is one file. The machine driver is expanded (`expand_machine_driver`,
which only fills a structure) for the CPUs and sound chips. The data is the emulator's own:
a PinMAME update changes it with no code to touch.

**System sets.** PinMAME flags with `NOT_A_DRIVER` the sets that are no game but hold the
ROMs a hardware generation's games share, as MAME's BIOS sets do: `gts1`, `gts1s`, `gts80`,
`gts80s`, `gts80a`, `gts80as` (Gottlieb System 1 and 80's CPU board ROMs), `allied`,
`gp_110`, `recel`, `pinheck` (10 sets). A game is a clone of its system set (`spidermn` of
`gts80s`, itself of `gts80`), its ROM table lists the shared ROMs too, and the ROM loader
looks for each file in the game's zip, then its parent's, then up the chain to the system
sets' zips (`common.c`, `open_rom_file`). The table keeps these sets, flagged `system`, and
gives every set the system sets of its chain (`systems`, nearest first); a game's parent is
never a system set (the shim, like PinMAME's own front-ends, stops the parent at the first
`NOT_A_DRIVER`).

**Sound ROMs.** A ROM is a sound ROM when its region is marked sound-only
(`ROMREGION_SOUNDONLY`, what `SOUNDREGION` declares: PinMAME does not load it with sound
off), is a `REGION_SOUNDn`, or is the program region (`REGION_CPUn`) of a CPU flagged
`CPU_AUDIO_CPU`.

**Matching.** A zip's members are looked up by CRC32 and size, as its central directory
lists them (no decompression); names play no part in the identification. A set is a
candidate when one of its files is there. For each candidate, each file with a known dump
is: good (CRC and size match; one under another name is reported as such), wrong (a
member has its name but another CRC: a bad dump), or missing (optional files do not
count). A file PinMAME knows no dump of (`NO_DUMP`: the Stern SAM colour mods such as
`acd_168hc`, `mtl_170hc`) is taken by its name alone, as the loader does (a missing one, or
another length, is only a warning there), and reported as not verifiable. The sets reported
are the complete ones, without those whose files are all part of another complete set's,
and without the system sets when a game is complete (the shared ROMs are part of the
game's). A zip named after a complete system set, or where only system sets are complete,
is a support zip (`SUPPORT`, as bsmt2000.zip): the games that have no ROM of their own
beyond it (Allied's, Game Plan's Model 110 games) are listed under it. When none is
complete, the closest one (most files found, then
fewest wrong or missing; ties are listed). Members no reported set uses are extras: the
ROM of another set, a duplicate, a known non-ROM (`.vpx`, `.txt`, a nested `.zip`...), or
unknown (no PinMAME ROM has its CRC). Then, per unit: misnamed (no reported set has the
zip's name), merged (several sets; their folders inside the zip when not the root), and
split (a clone zip whose missing files are all in its parent's zip, in the same folder). The
missing files of a set are looked up, as the loader does, in the zips of its parent and then
of its system sets, in the same folder: a game whose only missing files are in its system
set's zip is complete (`OK`, the zip named in the issues), one that needs its parent's is
split. When some are in none of those, the other zips of the folder are searched by content:
a set whose missing files are all there is reported `completable` (`complete_with` in the
JSON), and `--fix-names` writes it whole, its files under their PinMAME names. This is how
Pinball 2000 versions are completed from MAME's base zips: a version zip (`rfm_120.zip`,
`swep1_130.zip`) holds only its four update files, and PinMAME looks for the shared sound
and Prism ROMs in its parent's zip (`rfm_160.zip`, `swep1_150.zip`), while MAME's
`rfmpb.zip` / `swe1pb.zip` hold them under other names (`28f800.bin` for
`rfm_28f800.rom`, `rfm_u109.rom` for `rfm_u109.bin`), with MAME's BIOS (`awdbios.bin`,
`cga.chr`, no PinMAME ROM). `rom2altsound roms rfm_120.zip rfmpb.zip --fix-names fixed`
writes a complete `fixed/rfm_120.zip` (15 files; the SHA-1s checked with `--deep`), and
`rfm_010`, `rfm_080` and `swep1_040` (no file of their own) from the base zips alone.
`--deep` decompresses the matched files, which checks their stored CRC, and compares their
SHA-1 with PinMAME's.

**Sound board.** The family of a game is the board its machine init starts. Most machine
inits pass on `core_gameData->hw.soundBoard`; that game data is only set by the game's
init function (`driver_init`), which can also need a machine (the WPC simulators install
memory handlers there) and crash or exit. So the inits run in a throwaway child process
(`__driver-boards <start>`, internal), with `Machine->gamedrv` and `Machine->drv` set (some
inits read the game's name): its first statement sets `core_gameData`, and an exit or a
crash handler still reports it; the parent starts another child after that driver. A few
CPU families choose the board in their machine init from the generation instead (WPC,
System 3 to 11, Data East alphanumeric, Whitestar, Pinball 2000); the shim names a
driver's machine init by comparing its address with the init of a machine driver of each
of these families, and `Board::sound_boards` applies that family's `switch`. The whole
table (2971 sets, 10 of them system sets) takes a few seconds.

### Sound ROM id

The key under which the games that share their sound ROMs are grouped (all the revisions
of Twilight Zone: 24 sets, one id):

```
sound_rom_id = SHA-1( s1 + "\n" + s2 + "\n" + ... + sn + "\n" )
```

where `s1` .. `sn` are the distinct SHA-1s of the game's sound ROMs (as above; `NO_DUMP`
ROMs left out), each as 40 lowercase hexadecimal digits, sorted in ascending order, and the
result is written as 40 lowercase hexadecimal digits. It comes from PinMAME's table, not
from the files, so it is the same for every correct dump; a zip only gets it when all its
sound ROMs are good (`sound_roms_good`). There is none for a game without sound ROMs (Stern
SAM, whose sound data is in the main image; discrete boards), or when a sound ROM has no
SHA-1 in PinMAME's table. The ids of all PinMAME games are in `--dump-table`'s output, and
the report's `sound_groups` lists those of the zips checked.

### Output

One line per unit (`OK`, `MISNAMED`, `BAD DUMP`, `INCOMPLETE`, `SPLIT`, `NOT PINMAME`,
`SUPPORT` for bsmt2000.zip and the system sets' zips), its sets with description, maker and year, the wrong and
missing files, one `sound:` line per sound ROM id (board, PinMAME's board interface name,
the first 12 digits of the id, how many other PinMAME sets share it) and the issues; then
a summary and the verified sound ROMs by board. `--json` writes it all (`units[]` with
`sets[]`, each set's `wrong`, `missing`, `renamed`, `no_dump`, `folders`, `split_with` (the
other zips it loads from), `system`, `system_users` (how many sets load from a system set),
`systems` and `sound`:
`board`, `board_values`, `interfaces`, `generation`, `core_init`, `sound_rom_id`,
`sound_roms`, `sound_roms_good`, `shared_with`; `extras[]`; `issues[]`), `--dump-table`
every PinMAME set with its ROMs, board and id.

`--fix-names <dir>` writes one zip per complete (or split) set into another folder, named
after the set: a link to the original (a copy on Windows) when it already holds exactly
that set at its root under the right names, else a new zip with the set's files under
their PinMAME names, copied as stored (no recompression), the parent's files added to a
split clone. Nothing is written into a folder being checked, and an existing file is never
replaced.

## Factory results for our ROMs

Measured with 0.2.1, at the reference volume (now `--volume reference`): `rom2altsound
<rom> --roms roms-all`, three runs in parallel. The "as shipped" columns are what the
default factory mode now writes. Loudness at the reference volume: integrated LUFS of all written files / of the
non-loop files / median non-loop file, without the files flagged `ignores_master_volume`;
the loudest true peak (DC-blocked); mono measured as two identical channels. "As shipped":
the same shifted by `factory_offset_db`.

| ROM | boards | factory volume | reference volume | factory offset (spread) | tried / sound / written / blips / loops | retried (recovered) | flagged | all | no loops | median file | loudest TP | as shipped: all / no loops / median / TP | clipped files | wall |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| afm_113b | DCS | `55AA6798` 12/31 | `55AAEF10` | -22.45 dB (0.02) | 589 / 576 / 576 / 0 / 20 | 13 (0) | - | -19.2 | -17.0 | -17.4 | 0.7 dBTP | -41.6 / -39.4 / -39.9 / -21.8 | `0186` (1 sample) | 95.6 s |
| cv_20h | DCS | `55AA6798` 12/31 | `55AAEF10` | -22.44 dB (0.01) | 567 / 558 / 558 / 0 / 27 | 9 (0) | - | -16.4 | -14.8 | -14.8 | 1.2 dBTP | -38.9 / -37.2 / -37.3 / -21.3 | `03DE` (1, the click) | 113.6 s |
| mm_109c | DCS | `55AA6798` 12/31 | `55AAEF10` | -22.44 dB (0.01) | 893 / 871 / 871 / 0 / 17 | 22 (0) | - | -18.2 | -16.3 | -17.3 | 0.1 dBTP | -40.7 / -38.8 / -39.7 / -22.3 | - | 113.5 s |
| rs_l6 | DCS | `55AA6798` 12/31 (+ `55ABFF00`) | `55AAEF10` | -22.45 dB (0.03) | 480 / 455 / 455 / 0 / 34 | 26 (1) | - | -19.5 | -17.0 | -17.4 | -1.0 dBTP | -41.9 / -39.4 / -39.8 / -23.5 | - | 116.7 s |
| apollo13 | BSMT | `FE2C` 3/31 | `FE11FD` | -32.56 dB (0.06) | 251 / 182 / 178 / 4 / 45 | 70 (1) | - | -24.9 | -19.6 | -23.0 | -1.6 dBTP | -57.4 / -52.2 / -55.6 / -34.2 | - | 126.4 s |
| xfiles | BSMT | `FE20` 15/31 | `FE11FD` | -9.18 dB (0.03) | 251 / 170 / 169 / 1 / 40 | 81 (0) | 0x1F | -23.8 | -23.3 | -25.9 | -5.4 dBTP | -32.9 / -32.5 / -35.1 / -14.6 | `1F` (56, the click) | 118.3 s |
| gnr_300 | BSMT (Data East) | `20` music 15/15 | none (offset 0) | 0 | 251 / 161 / 161 / 0 / 40 | 90 (0) | - | -16.5 | -14.3 | -17.4 | 2.1 dBTP | same | 12 (`67`: 4966) | 117.0 s |
| btmn_106 | BSMT (Data East) | none | none (offset 0) | 0 | 251 / 157 / 141 / 16 / 34 | 94 (0) | - | -17.2 | -16.9 | -19.6 | 0.9 dBTP | same | 3 (`7A`: 384) | 102.0 s |
| whirl_l3 | WMSS11 + WMSS11C | none | none (offset 0) | 0 | 510 / 408 / 189 / 219 / 22 | 102 (0) | - | -20.4 | -16.0 | -17.8 | -2.3 dBTP | same | - | 229.6 s (not re-run) |
| swep1_130 | DCSP2K | `55AA609F` 12/31 | `55AAA05F` | -10.56 dB (0.00) | 690 / 683 / 683 / 0 / 26 | 7 (0) | - | -22.6 | -21.2 | -21.4 | -5.8 dBTP | -33.2 / -31.7 / -31.9 / -16.4 | - | 1638 s |
| rfm_120 | DCSP2K | `55AA609F` 12/31 (none at the warm boot: `P2K_FACTORY_DEFAULT`, the cold boot's level) | `55AAA05F` | -10.57 dB (0.00) | 1557 / 1538 / 1538 / 0 / 34 | 19 (0) | - | -19.8 | -18.9 | -19.3 | -5.8 dBTP | -30.4 / -29.5 / -29.9 / -16.4 | - | 2842 s |

The two Pinball 2000 rows were measured later, with the Pinball 2000 support, in the
default factory mode (`rom2altsound swep1_130 rfm_120 --jobs 2`; x2.9 and x3.0 real
time): every loop exact (swep1_130: 24 from the track programs and 2 from the audio, of the
29 tracks whose program loops, 2 of the others playing nothing and one ending silent;
rfm_120: 34 of 35, all from the programs), all files from silence, no reset but rfm_120's set-up after its warm boot (`boot.p2k`). The
master volume check replayed swep1_130's 11 loudest files 8 levels down: -10.6 LU each,
the volume scales everything (about 1.32 dB per level there; levels 12 to 31: +26.2 dB).
rfm_120 has 631 twins (the same sound on several tracks). The silent tracks: swep1_130
`000E`, `0028`..`002A`, `03E3`, and `03E7`/`03E8`, which only write to the host.

Cold boots: WPC DCS 60 s (max, the game sends nothing) and 12334-byte nvram; System 11 15 s,
2094 bytes; Data East and Whitestar 15 s, 8238 bytes; Pinball 2000 15 s, 196946 bytes
(rfm_120's warm boot 60 s, max: it never sets its board up). Warm boots: 15 s (rs_l6 37.9 s, its
`55 AB` fade). No board reset and no unclean start in any run.

What changed with the reference volume (against the previous table, recorded at the
factory volume):

- Same written files on every ROM (afm 576, cv 558, mm 871, rs 455, apollo13 178, xfiles
  169, gnr 161, btmn 141). The "as shipped" totals match the old factory totals within
  0.1 LU, and the offsets are consistent (spread 0.01 to 0.06 dB over 5 files): the volume
  scales the whole output.
- apollo13 `0A` (a music loop) peaks at -9.5 dBFS instead of about -43; its loudest file,
  `5C`, at -1.7 dBFS. afm_113b `0186` at +0.1 dBFS DC-blocked (one raw sample at full scale).
- Flags: apollo13 `5F`, cv_20h `03DE` and xfiles `1E` are no longer flagged. At the
  reference volume they are no longer more than 5 LU above the median file (`5F`: 2.5 LU),
  so the master volume check does not replay them, and they stay in the totals. In the
  "as shipped" figures they are shifted by the offset like the others, which is wrong for
  them (they do not follow the master volume: `5F` measured -11.4 LUFS at apollo13's level
  3). The "as shipped" totals still match the old ones within 0.1 LU; cv_20h's "as shipped"
  true peak, -21.3 dBTP, is `03DE`'s (the other files': -22.7, as before).
- apollo13: 70 retried (1 recovered) instead of 73 (0), and 4 blips (sub-20 ms output now
  above the silence threshold, still not written). At `FE 10 FD` (not used), apollo13
  wrote two such near-silent files (`E2`, `F3`, 24 ms at -62 and -74 dBFS).
- gnr_300: 12 clipped files instead of 13, and other sample counts: Data East ADPCM state
  carries over between commands (Limits); the level there is unchanged (no master volume).

The round before (factory volume):

- btmn_106: the refresh is now `20 00`. The WAVs start at 0 (0 of 141 more than 256 LSB away,
  140 before) and `7B` no longer clips (6151 samples before; -19.5 to -18.6 LUFS). Levels
  match the reviewer's patched sweep exactly; against the old table four files move by 0.5
  to 0.9 LU. gnr_300 with the same refresh: four ADPCM files move by up to 0.7 LU
  (`6D`, `70`, `79`, `A2`), "no loops" -14.3 to -14.2, median -17.5 to -17.4.
- apollo13 `76` (the reference of its volume check) is no longer cut: 3.53 to 3.92 s, the
  length of its solo recording. gnr_300 `67`, xfiles `56` and `69` are cut as before.
- xfiles `1E` is flagged too (threshold 5 LU instead of 15); the totals move by less than
  0.05 LU. No other ROM flags anything new.

Two rounds before:

- gnr_300: 155 to 161 written: six music loops (`12` `14` `16` `1A` `1C` `1E`) were silent
  after a music volume byte, and 21 files (`11`, `2E`, `30`..`3E`, `64`, `EB`, `F0`..`FB`)
  were 30 to 40 LU too quiet. btmn_106: same 141 files, `14` 17 LU louder. Both match the
  reviewer's reference sweeps (a `20` before every command), plus gnr_300 `2E`.
- afm_113b: 589 commands instead of 575 (the 14 catalog tracks missing from sounds.dat), one
  more file: `0013`.
- rs_l6: 455 written in every run (the old runs gave 453 or 454, never the same ones); the
  retry recovered `0247`, `024E`, `0803`. mm_109c: 871 in every run.
- cv_20h `03DE`, apollo13 `5F` and xfiles `1F` are left out of the totals. apollo13's
  "no loops" total goes from -12.5 LUFS (mostly `5F`) to -52.2; cv_20h's loudest true peak
  from +0.6 to -22.7 dBTP.
- whirl_l3: `clipped` counts written files (0, it said 172).
- Totals are now gated over the files' blocks and measured DC-blocked, so they move by up to
  0.3 LU on unchanged ROMs.

## Limits

- **Boards without a manual-command handler** produce nothing. The tool checks `manCmd_w` in
  PinMAME's board table before starting and stops with an error naming the board. Machines
  with no sound board report that too. SAM sets never get there: they are read statically
  (see Stern SAM).
- **A run that writes no sound fails**: when every command stayed silent (a board that
  takes its commands some other way, as the early Bally boards did before their support),
  the manifest is written for the diagnosis, no pack is, and the ROM is reported `FAILED:
  no sound was recorded` (exit status 1).
- **Early Bally boards**: the packs do not play in VPinball as they are (PinMAME hands
  AltSound the raw writes of the lines the game shares with its solenoids, see "Per
  family"); only one program per board was tried (vikingb, xenon, eballdlx), and the
  -32/-50 not at all.
- **Stern SAM**: the AltSound files do not play in PinMAME (no sound command); the scripts'
  volume ramps and the game's mixing are not reproduced; the factory volume is PinMAME's
  linear reading of the DAC register, about 10 dB louder than the datasheet's at `E8`;
  only acd_168h was checked.
- **DCS first-try losses.** About one command in 200 on rs_l6 (none of 893 on mm_109c with
  the current pacing) plays nothing on its first try and plays normally on the retry. Which
  ones depends on the boot's timing, which varies by a frame or two from run to run (the WPC
  clock comes from the host's clock): 060D, or 080E and 08BE, with the commander's 4-frame
  pacing; 0247, 024E and 0803 with 1 frame (mm_109c: 00BF, 0107, 03B7 with 4 frames, none
  with 1). Ruled out: a byte lost on the way in (the DSP had
  read every byte: its IRQ2 was never still pending at the next one), the 100 ms inter-byte
  timeout (bytes are 17 ms apart), and PinMAME's DCS decoder speedup (the same three were
  lost with `DCS_useSpeedup = 0`). So it happens inside the DCS firmware as emulated; the
  root cause is not found. The retry pass recovers them all: rs_l6 wrote the same 455 files
  in three full sweeps compared file by file (455 in two more), mm_109c the same 871 in two
  (871 in two more).
- **Sounds that do not follow the master volume.** cv_20h's `03DE` (a 77 ms click, +1.0
  dBTP) and xfiles' `1F` (a 50 ms click near full scale, -14.4 LUFS padded at `FE10`,
  `FE20`, `FE28` and `FE2C` alike) and `1E` (9.3 LU above the median; -0.7 LU moved where
  the reference moved -5.5) keep their level whatever the master volume; apollo13's
  ADPCM `5F` follows the Whitestar volume backwards and non-monotonically (-8.7 LUFS at
  level 0, where everything else is silent; -11.4 at the game's 3; -39.9 at 11; -15.8 at
  31). The check flags them and keeps
  them out of the totals, when it replays them: only files more than 5 LU above the median
  are replayed, so at the reference volume only xfiles `1F` is (see the results). It needs a master volume: on Data East (hardware pot) the BSMT
  ADPCM `5F` of gnr_300 and btmn_106 cannot be checked and stays in the totals; the median
  is the robust figure there. The check also needs the other files: a run of a few
  commands with `--only` (fewer than 3 non-loop files) does not run it and says so, so
  `--only 0x03DE` on cv_20h keeps the click in that run's totals.
- **BSMT ADPCM state carries over between commands** on Data East. gnr_300 `67`'s raw
  `dc_offset` reads -4198, -5608 or +103 depending on what played before it; the WAVs still
  start at 0 and the DC-blocked level moves by 0.16 LU at most. 12 or 13 gnr_300 files clip
  in the emulated output itself (`67`: 3500 to 5000 samples), at the board's only level.
- **Very short files can move by about 0.7 LU between runs** (mm_109c `0164` and `0165`,
  0.975 s: -41.97 or -41.26 LUFS, same length and RMS to 0.001 dB): such a file has only a
  few R128 gating blocks, so a tiny difference can flip one block across the relative gate.
- **DCS at `FF` clips some sounds** (hence the reference `EF`). One volume step (`08`) is
  1.3 dB. On afm_113b:

  | `--dcs-volume` | `0x0186` clipped / rms | `0x01B5` clipped / rms |
  |---|---|---|
  | `FF` (default, 0 dB) | 65-73 / -17.8 dBFS | 6 / -12.5 |
  | `F7` | 29 / -18.9 | 0 / -13.9 |
  | `EF` | 1 / -20.2 | 0 / -15.2 |
  | `E7` | 0 / -21.5 | 0 / -16.5 |
  | `67` (game's nvram setting, = reset default) | 0 / -42.6 | 0 / -37.6 |

  The clipping is at the output only (levels follow the volume step for step), so a lower
  `--dcs-volume` gives the same sound without distortion. Clipped files are counted in the
  summary and flagged per file in the manifest.
- **WAVs keep the emulated DC** of a DAC that PinMAME mixes unsigned, unless `--dc-block`
  is given, except on WPCS and System 11, whose DACs are AC-coupled once booted (see
  Silence: their files no longer start or sit on a held level), and on Squawk & Talk,
  whose files are always DC-blocked. Real machines AC-couple their output; PinMAME models
  that only on the DACs whose drivers opt in. The levels in the manifest are always
  DC-blocked.
- **WPCS (DOUBLECMD) boards**: `sndbrd_manCmd` only acts on byte pairs and `wpcs_manCmd_w`
  writes both bytes to the board, so padding a one-byte command would also send `00` ("Reset
  Sound System"). One-byte commands (all of sounds.dat's WPCS entries, and the stop `00`)
  therefore go through `sndbrd_data_w`, the path the WPC game CPU uses (wpc.c
  `WPC_SND_DATA`). Tested on tz_94h (Twilight Zone): 307 commands from sounds.dat, 302
  written, 5 silent, no blip, no clipped file, no board reset. That byte also comes back
  through libpinmame's sound-command callback while the extractor's state is held; the
  callback skips it (a blocking lock there hung the extraction). The audio of its 45 music
  tracks never repeats sample-exactly (YM2151 + DAC), so their loops come from the sound
  CPU's state: 26 loop within 240 s, 31 with `--loop-max-secs 600`, the others are cut at
  `--max-secs` ([Loops](#loops)). The tool sends the reference master volume `79 0C F3`
  (level 12), which is also the game's own factory volume, so the factory offset is 0 dB;
  the master volume check has no WPCS case, so no file is flagged `ignores_master_volume`.
- **Several commands can map to one sound** (whirl_l3 `0x0001` = `0x0004`). They are marked
  as twins (above) but kept, and the loudness totals count every copy.
- **The factory volume is the boot (attract mode) volume.** Nothing is played, so whether a
  game uses another level during play is not seen. apollo13 re-sends `FE 2C FD` (level 3/31)
  every 0.5 s in attract mode; its sounds then measure around -55 LUFS, and that is the
  level the files are recorded at by default (`--volume reference` for the loud
  `FE 11 FD`; `factory_offset_db`, -32.6 dB, says how far apart they are).
- **BSMT music does not repeat exactly** (Whitestar and Data East: apollo13, xfiles,
  gnr_300, btmn_106), so its loops stay cut at `--max-secs` with `loop_unresolved`. The
  envelope repeats (xfiles `01`: every 6.70 s, envelope difference 1.7 %), but the
  waveform does not: the best lag moves from window to window (295493 to 295517 samples
  over 100 s) and leaves -2 to -20 dB at best; a brute-force search over every lag up to
  110 s back found nothing better than -20 dB on xfiles `01` and -5 to -6 dB on apollo13
  `01`, gnr_300 `01` and btmn_106 `02`. The event timing of the sound CPU's sequencer
  jitters against the output by a few tenths of a millisecond, which is enough to break a
  sample-exact cycle. Only the BSMT test tones (`F0`..`F2`) loop exactly.
- **A DCS board keeps a deferred track between commands.** A command whose track has type 2
  only arms a link that the next opcode `05` on that channel starts. On cv_20h, `0001` arms
  the music `0012` and plays nothing; `0015`, played later, consumes it: its file holds
  `0015` and then that music (its program alone loops every 4.047 s; the audio repeats every
  24.184 s, `0012`'s period, so the loop is `method: "audio"` with a note). The stop
  `00 00` does not clear such a link.
- **The factory offset assumes every sound follows the master volume.** Files flagged
  `ignores_master_volume` are out of the totals, but a click that ignores it and is not
  loud enough at the reference volume to be checked (apollo13 `5F`, cv_20h `03DE`, xfiles
  `1E`) stays in them, shifted by the offset like the others.
- **Cold boot**: with no nvram, the WPC DCS games send no sound byte at all (60 s emulated);
  The factory mode only uses the cold boot to create the nvram and extracts from the warm boot.
- **Why the old extractor peaked at -22.9 dBFS**: the DCS board was at `67`. Whether the game
  sent `55 AA 67 98` (warm nvram, at 10.4 s) or nothing (cold boot) makes no difference: the
  board's reset default gives the same level (`0x0186` peaks at -23.0 either way). `67`
  (level 12/31) is also the factory default that the factory mode finds on all four of our WPC DCS
  games (afm_113b, cv_20h, mm_109c, rs_l6).
- **Repeated sounds are not sample-identical on System 11C** (the YM2151 keeps running state),
  so files can only be compared by duration and level there. On DCS a sound recorded after a
  stopped loop matches its solo recording to 3 samples in length and identical levels.
