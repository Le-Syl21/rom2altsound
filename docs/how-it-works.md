# How rom2altsound works

Technical reference of the extraction engine. For installing and using rom2altsound, see
the [README](../README.md).

rom2altsound extracts a pinball ROM's sounds by running PinMAME's emulation in-process and
driving its sound board directly from Rust: no keyboard simulation, no `-key_script`, no use
of PinMAME's sound-commander UI, and no patch to PinMAME.

```
rom2altsound <rom>... [--roms <dir>] [--out <dir>] [--jobs N] [--no-factory | --factory-volume
             | --whitestar-volume HH | --no-volume-init] [--dcs-volume HH] [--only 0x0186,0x0002,...]
             [--limit N] [--boot-secs S] [--boot-max-secs S] [--max-secs S] [--loop-max-secs S]
             [--no-sound-secs S] [--stop 0xHHHH] [--dc-block] [--vpm <dir>] [--no-altsound]
             [--merge-twins] [--intro-loop-secs S] [--check-ducking]
rom2altsound loop-scan [--hint SECS | --hint-frames F] <wav>...    # the loop detector alone
rom2altsound dcs-effects <region.bin> <rom> [--json F]   # DCS track programs (diagnostic)
rom2altsound duck-fit <M.wav> <C.wav> <MC.wav> <at_secs> [--win S]  # music gain under a sound
rom2altsound drift-check <A.wav> <B.wav>                 # does a board replay sample-exactly?
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
   (`cb_OnSoundCommand`). The boot lasts at least `--boot-secs` (15 s), then until 3 s pass
   without a byte that is new (a (previous byte, byte) pair not sent before: afm_113b polls
   the silent track `03 D3` three times a second forever, whirl_l3 repeats `1F`), and on DCS
   until the game's master volume was seen; at most `--boot-max-secs` (60 s). The manifest
   says which of `quiet`, `repeats` or `max` ended it. Then every game CPU is halted, using
   the commander's own selection (`cpu_type && cpu_flags == 0`). Audio CPUs keep running.
   If the halt split a Whitestar `FE xx FD` (see Volume), the missing `FD` is sent.
4. After 0.5 s, so that any half-sent command expires, the tool sends the stop command and
   waits for silence. With the factory settings (the default) it then sends the reference
   volume (see Factory mode). With `--no-factory` it sends `55 AA HH ~HH` to every DCS board
   (`--dcs-volume HH`, default `FF` = 0 dB) and waits for silence again. With
   `--factory-volume` or `--no-volume-init` it sends no volume: the
   boards play at whatever the game set.
5. Before every command, outside its recording, the tool sends what keeps one command's
   state from leaking into the next (`refreshed_before_each_command`), then waits for
   silence again: the Data East music volume, or the Whitestar master volume (the
   reference one with the factory settings, else the game's when it kept re-sending it during boot;
   see the per-family notes).
6. For each command, it sends one byte per `sndbrd_manCmd` call every 4th frame, as in
   `snd_cmd.c` `playCmd`, except on DCS: one byte per frame (see DCS below). Two-board
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
   again since a reset loses it: our `--dcs-volume`, or else the game's own last volume
   commands, byte for byte (`volume_replays` in the manifest). If even that fails, the next
   file is marked `clean_start: false`. Nothing is only reported on stderr.
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
   `g-sound.csv`, `altsound.ini`.

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
  file is at the same reference volume). `NAME` is the sounds.dat name without commas and
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
0 dB. Each sound is compared to the earlier sounds that are not twins themselves, so
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
on other boards. The research behind this: [ducking-study.md](ducking-study.md).

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
   reason. It is kept as metadata only: the files are recorded at the **reference volume**
   (below), so that they can be listened to (at its factory `FE 2C FD`, apollo13's music
   `0A` peaked at -43 dBFS and looked like silence in an editor). `--factory-volume` records
   at the game's own volume instead (the old behavior: no volume of ours is ever sent).
4. Factory offset pass (after the retry pass and the master volume check), per board that
   has a master volume: up to 5 written non-loop files are played again at the factory
   volume (the game's last master volume at boot, or on DCS the board's reset default `67`
   if the game sent none). They are the loudest files at most 5 LU above the median file
   (louder ones are the volume check's suspects), at least 1 s long and not clipped: loud,
   so that they stay far above the silence threshold at a low factory volume. These replays
   are measured, never written. `factory_offset_db` is the median of their level moves
   (factory `level_lufs` minus reference `level_lufs`, negative); `factory_offset` in the
   manifest lists the files, both levels, the move and the spread. Boards without a master
   volume (Data East, System 11) record at their only level: offset 0.
   `loudness.as_shipped` is the loudness report shifted by the offset: what the files
   measure at the game's factory volume.

### Reference volume

Per board family, the loudest master volume at which no file of our ROMs clips in emulation,
apart from isolated clicks. Measured with full sweeps (written files with raw samples at
+32767/-32768):

| family | volume | clipped files at that volume | one step louder |
|---|---|---|---|
| DCS | `55 AA EF 10` (level 29/31, `--dcs-volume`) | afm_113b `0186` (1 sample), cv_20h `03DE` (2 samples, a 77 ms click that ignores the master volume) | `FF`: afm 5 files (`0186` 68 samples), cv_20h 18 (its loop `0016` 575), mm_109c 24 (`01AB` 99), rs_l6 5 (`0240` 33) |
| Whitestar | `FE 11 FD` (level 30/31, `--whitestar-volume`) | xfiles `1F` (56 samples, a 50 ms click that ignores the master volume) | `FE 10 FD`: apollo13 `5C` 172 samples, `68` 13 (xfiles: only `1F`) |
| System 11, Cheap Squeak / Turbo Cheap Squeak, Data East | no software volume stage: always full scale, which is the reference (`reference_volume: "full_scale (no volume stage)"`); on Data East the music level is set to its loudest, `20` | | |

**Boards without a volume stage.** System 11 (`s11s`, `s11cs`, `s11js` in wmssnd.c) and
Bally's Cheap Squeak and Turbo Cheap Squeak (`by45`, `byTCS` in by35snd.c) write their DACs,
CVSD and YM2151 directly: no volume register, no `mixer_set_volume`, no volume command in
their sound programs that PinMAME would see. Data East's BSMT board (`de2s` in desound.c)
has its master volume on a pot in the power junction box ("it was not done through the
software"); its bytes `20`..`2F` are a music level the game drives (a music may fade it as
it ends), which the tool sets to its loudest, `20`, before every command. These boards are
always at full scale: that is their reference volume, and their factory offset is 0.

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
| WPCS | `79 vv ~vv`: the game's is read (tz_94h `79 0C F3`), no reference volume (the files are at the game's) | none | `00` |
| Whitestar BSMT (Sega/Stern) | `FE xx FD`, level = 2F - xx, 0..31 | the master volume (ours with the factory settings, else the game's `FE xx FD`, which it re-sends every 0.5 s) | `00` |
| Data East BSMT | none (hardware pot in the power box) | the music volume `20`..`2F` (the loudest, `20`, with the reference volume), then the stop `00` | `00` |
| System 11 (WMSS11, 11C, 11J) | none (no volume stage) | none | `00` / `20` (11C) |
| Bally Cheap Squeak (BY45), Turbo Cheap Squeak (BYTCS) | none (no volume stage) | none | `00` |
| Stern SAM | PCM1755 DAC attenuation, `FF` = 0 dB, -0.5 dB per step (read, not driven) | none | none (static, see Stern SAM) |

**DCS**: `55 AA vv ~vv` sets the master volume (`~vv` must be the complement, else the
firmware drops it). The bytes of a command go out one frame apart. The DCS firmware drops
the first byte of a two-byte command if the second comes 13 main-loop passes later (13 x
7.68 ms, about 100 ms; mjrgh's DCSExplorer, `dataPortTimeout`): with 6 frames between the
bytes every command of rs_l6 was lost, 4 frames (PinMAME's commander) is within the limit,
1 frame is closer to the game, which sends both bytes within a millisecond. Independently of
the pacing, about one command in 200 plays nothing on its first try and normally later; see
Limits. The sweep without a sounds.dat section plays the ROM's catalog (below).

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

**Levels**: the files are the samples as decoded, at full scale: what the DAC plays at 0
dB (attenuation `FF`), the reference volume. 855 of the 1037 acd_168h files reach
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
boot, cold or warm, and nothing more in 120 s of attract mode: -11.5 dB
(`factory_offset_db`). PinMAME plays that register as a linear mixer level,
`(v & 7F) * 100 / 7F` = 81 % (-1.8 dB), not as the DAC does. That `E8` follows the
operator's volume setting is not verified (`factory_offset.verified` false): pressing the
coin door's `+` key in libpinmame changed no DAC register, and the game may scale its mix
in software as well. `E8` = `80 + 2 x 52`. Boot cost: 12-13 s wall per boot (15 s
emulated, the ARM7 interpreter; the asmjit JIT is off in this build).

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
leaving out those that change the board's state (`commands_from` says what was swept):

- DCS: the populated tracks of the ROM's own track catalog (`src/dcsrom.rs`, layout from
  mjrgh's DCSExplorer: catalog in U2 at $3000/$4000/$6000, track index pointer at +$40,
  track count at +$46, `FFxxxx` = empty slot). A DCS command below the track count plays
  that track; `0000` (stop) and the `55 xx` specials are left out. Without a catalog:
  0001..03FF. **A DCS game with a sounds.dat section gets the union**: the section's
  commands (with their names) plus the catalog's populated tracks it leaves out, sorted by
  track number. afm_113b: 1130 tracks, 589 populated besides `0000`; its section lists 575
  of them and misses 14, among them `0013`, a 120 s loop at -47.6 LUFS (the 13 others play
  nothing).
- Whitestar / Data East (BSMT): bytes 01..FB. `00` is the stop; `FC`..`FF` start two-byte
  commands (`FE xx FD` is the volume). Probed: `FF xx` plays the same sound as `xx` (apollo13,
  xfiles), and `FC xx` starts a loop for every `xx` on apollo13, gnr_300 and xfiles alike, so
  neither is swept.
- WPCS: bytes 01..FF without the tempo/volume bytes of sounds.dat `wpcs:` (1E-2F, 60-72) and
  the prefixes 79 (volume) and 7A (16-bit commands).
- Other boards: bytes 01..FF.

On two-board machines the id is `board<<8 | byte`, so `0x0105` means byte 05 on board 1.
`--stop` overrides the stop. It uses the same notation as `--only`.

### Silence

The upstream mixer adds +/-1 LSB TPDF dither, so a sample within 2 LSB of the idle level counts
as silence. The idle level is not always 0: System 11 boards hold their DAC at a constant level
(whirl_l3 idles at +2056, and +6264 or +10248 after some sounds). Each frame whose span stays
within the dither updates the per-channel idle level. All decisions use emulated time.

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

At the top level: `mode` (`factory` or `normal`), `factory` (vpm, saved nvram path and size,
cold boot report), the boards, `boot` (length, what ended it, every byte per board as
`seconds:byte`), `factory_volume` (factory mode) or `game_volume` (the last command per board
and kind: master, DCS channel, Data East music), `volume_init` (what the files were recorded
at), `volume_replays`, `refreshed_before_each_command`, `commands_from`, `counts` (tried,
with_sound, written, blips, no_sound, loops, loops_exact_dcs_catalog, loops_exact_audio,
loops_unresolved, not_clean, clipped (written files only), retried,
recovered_by_retry, ignores_master_volume), `loudness`, the `stop` actually sent,
`board_resets` and `dc_blocked_wav`. DCS: `dcs` (the catalog summary and
`ducking_check`). `recording_cap`: `max_secs` (120 s by default),
`loop_max_secs` (240 s), `loop_hint_max_secs` (900 s) and what they do (see Loops).

Reference mode (the default) adds `reference_volume` (what each board is recorded at: our
master volume command, or `full_scale (no volume stage)`; `board N: ...` per board when they
differ), `levels_note`, `factory_offset_db` (the ROM's offset: 0
without a master volume, null if not measured), `factory_offset` (`method`, per board the
`reference_volume`, `factory_volume` and where it comes from, `factory_offset_db`,
`spread_db` and the `samples`: `id`, `reference_lufs`, `factory_lufs`, `delta_db`, plus a
`note`) and `loudness.as_shipped` (`factory_offset_db`, `all_lufs`, `excluding_loops_lufs`,
`median_file_lufs`, `loudest_true_peak_dbtp`). Every per-sound level is at the reference
volume (`volume_init`, e.g. `reference 55AAEF10`).

## Factory results for our ROMs

`rom2altsound <rom> --roms roms-all` (factory settings, reference volume), three runs in
parallel. Loudness at the reference volume: integrated LUFS of all written files / of the
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

Cold boots: WPC DCS 60 s (max, the game sends nothing) and 12334-byte nvram; System 11 15 s,
2094 bytes; Data East and Whitestar 15 s, 8238 bytes. Warm boots: 15 s (rs_l6 37.9 s, its
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
- **Stern SAM**: the AltSound files do not play in PinMAME (no sound command); the scripts'
  volume ramps and the game's mixing are not reproduced; the factory volume is the DAC's,
  not verified against the operator setting; only acd_168h was checked.
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
- **WAVs keep the emulated DC** of System 11 DACs (whirl_l3 idles at 2056, 4015, 10248...
  depending on the last sound) unless `--dc-block` is given. Real machines AC-couple their
  output; PinMAME does not model that. The levels in the manifest are always DC-blocked.
- **WPCS (DOUBLECMD) boards**: `sndbrd_manCmd` only acts on byte pairs and `wpcs_manCmd_w`
  writes both bytes to the board, so padding a one-byte command would also send `00` ("Reset
  Sound System"). One-byte commands (all of sounds.dat's WPCS entries, and the stop `00`)
  therefore go through `sndbrd_data_w`, the path the WPC game CPU uses (wpc.c
  `WPC_SND_DATA`). Tested on tz_94h (Twilight Zone): 307 commands from sounds.dat, 302
  written, 5 silent, no blip, no clipped file, no board reset. That byte also comes back
  through libpinmame's sound-command callback while the extractor's state is held; the
  callback skips it (a blocking lock there hung the extraction). None of its 45 music
  tracks repeats exactly within 240 s (YM2151 + DAC): they are cut at
  `--max-secs`. Its master volume is not set by the tool: the files are at the game's
  factory level (`79 0C F3`), and the master volume check has no other level to replay at.
- **Several commands can map to one sound** (whirl_l3 `0x0001` = `0x0004`). They are marked
  as twins (above) but kept, and the loudness totals count every copy.
- **The factory volume is the boot (attract mode) volume.** Nothing is played, so whether a
  game uses another level during play is not seen. apollo13 re-sends `FE 2C FD` (level 3/31)
  every 0.5 s in attract mode; its sounds then measure around -55 LUFS, hence the reference
  volume for the files and `factory_offset_db` (-32.6 dB) for that level.
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
