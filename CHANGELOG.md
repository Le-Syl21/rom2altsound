# Changelog

## Unreleased

- **Jac Van Ham** (JVH): each command is followed by the idle `3F`: Ice Mania 17 of 40 (1
  before), Escape 40 of 40, all from silence.
- **Inder** (SPINB machines with Inder's own handler): stop `00`; Lap By Lap 28 of 40 from
  silence with no board reset. The MSM5205 machines still play their background music
  under every command (documented).
- **Bell Games' -51N** (BY51N, Super Bowl): commands are two nibbles, as on the -56
  (read in its sound program), sent through the same hook: 26 of 40, all from silence,
  distinct (6 of 32 before).
- **Bally Sounds Deluxe** (BYSD): stop `00` instead of a board reset; a full sweep gives
  75 of 255 commands, all from silence (the survey's 12 of 40 were not a protocol problem).
- **Tabart's Sahara Love and Le Grand 8** (TABART2, TABART3): the commands go out through
  the game's own path (the sound lines, then idle), not the manual command, which stored
  them unconverted: 28 of 31 and 26 of 31, all from silence (one held tone before).
- **Zaccaria 1346** (ZAC1346): each command is followed by `00`, as the games send it, and
  `00` is the stop: locomotn 33 of 40 all from silence (39 of 40 over the last sound
  before), ewf and sshtlzac 38 of 40.
- **Williams shuffle alleys** (SNDBRD_S11S), first run: Alley Cats 32 of 40, Gold Mine,
  Top Dawg and Shuffle Inn 40 of 40, all from silence; Tic-Tac-Strike has no sound (its
  sound ROMs are not dumped).
- **Gottlieb System 80 sound and speech boards** (GTS80S, GTS80SP, GTS80SS and its Votrax
  variants: 105 sets): each command is followed by `00`, as the games send it (the board
  replayed a command left on its lines, and the speech board's strobe made no edge for
  the next one), `00` is the stop, and the sweep is the lines the boards read. Survey runs:
  spidermn 15 of 15, hh 38 of 40, blckhole 38 of 40 (5 before), marsp 31 of 40, all from
  silence.
- **Whitestar II (AT91, DE3S: 156 sets) and the Data East alphanumeric board (DE1S, 38
  sets)**: a real stop, `00`, which the games send; on DE1S followed by the board reset.
  The files no longer start over the last sound: lotr 39 of 40, elvis, sopranos, nascar,
  bttf_a28, simp_a27 40 of 40, all from silence (before: up to 38 of 40 over the last
  sound).
- **Playmatic Zira** (PLAYZ): the board reads three lines; the sweep is their 7 values,
  each between two idle `00`: 4 of 7 (0 of 40 before).
- **Baby Pac-Man's Cheap Squeak** (BY45BP: 4 sets, 0 sounds before): commands go out
  as the video CPU sends them, two nibbles with the PIA's CB2 as the strobe (read in the
  sound program); Baby Pac-Man 32 of 40, Granny and the Gators 28 of 40, all from silence.
- **Game Plan boards** (GPMSU1, GPMSU3, GPSSU1..4: 20 sets): the sweep is only the
  nibbles the boards read; the MSU-3 takes a byte as two nibbles then the idle `F` (read
  in its program), the MSU-1 one nibble between idle ones, and the SSU tone boards stop on
  `0F` (SSU-4: `00`). Survey runs: andromed 39 of 40 (0 before), cyclopes 39, attila 15
  of 15, sshooter 15 of 15, all from silence (the tones used to play on).
- **Williams System 3 to 7** (S67S, S7S_ND, S3S, S3DFS, S3WCS: 113 sets, 0 to 3 of 40
  before): the board starts a sound on the change from its idle value, so every command
  now goes out between two idle bytes (`FF cmd FF`), as the games send it, and the sweep
  is only the bits the board reads (`00`..`1E`, Thunderball `00`..`7E`). Survey runs:
  Black Knight, Gorgar, Jungle Lord, Firepower 30 of 31, Thunderball 34 of 40, Phoenix 15
  numbered sounds, all from silence; Disco Fever and World Cup give few distinct sounds.
- **Capcom boards** (SNDBRD_CAPCOMS, 17 sets, 0 sounds before): a command is a serial
  message, `DA 04 07 0F nnnn` (play sample `nnnn`), and `DA 02 03 01` stops; the sweep
  is the sample number `0000`..`03FF`, after sounds.dat's commands where it has some.
  Survey runs: Breakshot, Pinball Magic, Kingpin, Flipper Football 40 of 40, all from
  silence.
- **Stern SB-100 tones** (ST100, ST100B: 26 sets): the stop is now `00`, the empty tone
  mask; a held tone no longer plays under the next command (trident, dracula: 40 of 40,
  all from silence, 38 not before). **SB-300 with speech** (ST300V, 21 sets): the sweep
  is the speech chip's 64 words (`40`..`7F`), 0 sounds before; the effects stay out of
  reach (the game programs the timers itself).
- **Spinball boards** (SNDBRD_SPINB on bushido, mach2, jolypark, vrnwrld: 7 sets, 0
  sounds before): both sound CPUs poll the command latch and take a byte only while its
  bit 7 is set, so the sweep is `81`..`FF`, each command followed by `00`, and `8F`
  (music stop) is the stop. On Jolly Park and Verne's World the game's boot steps the
  board's volume down to 0; the tool steps it back to PinMAME's power-on level once booted.
  Survey runs: bushido 31 of 40, mach2, jolypark and vrnwrld 40 of 40, all from silence.
  Inder's machines, which PinMAME runs on the same interface with a handler of their own,
  are now reported as `INDER` and keep the common method.
- **Taito Sintetizador boards** (TAITO_SINTETIZADOR, TAITO_SINTETIZADORPP: 26 sets, 0
  sounds before): the stop is now `00`, the games' idle value. The board's strobe (CB1)
  only falls on a `00`, and these programs never lower it themselves, so a command sent
  after any other byte made no edge and was never read. Survey runs: shock 35 of 40,
  football 36, snake 35, mrblack 36, all from silence; the Sintevox boards keep their
  counts with fewer board resets.
- **Diagnostics**: the boot line says how long the board played during the boot, and
  `R2A_TRACE=<audio cpu>:<start>-<end>` writes every access the sound CPU makes in that
  range (time, PC, value) to `trace.txt`, next to the game's and the tool's sends.
- **Zaccaria Sounds & Speech boards** (ZAC1370, ZAC13136, ZAC11178, ZAC11178_13181,
  ZAC13181x3: 108 sets, 0 sounds before): bit 7 of the byte is the board's strobe, so
  each command now goes out framed as the games send it (low bits, then with bit 7, then
  without: `7E FE 7E`), the sweep is `FE` down to `80` (commands 01..7F, which the board
  reads inverted), and `FF` (command 00) is the stop. Survey runs: socrking 25 of 40,
  tmachzac 35, clown 40, spooky 38, strsphnx 21, all from silence.
- **Sound ROM catalog site** (https://le-syl21.github.io/rom2altsound/, English and French):
  every sound ROM id of the PinMAME built in (673) and every set without one (Stern SAM,
  games with no sound ROM), 1473 entries for 2961 sets: the sets sharing it and whether
  they were complete in the reference ROM set, the board family with a link to its notes
  and rom2altsound's support level for it (docs/board-support.md), the sound ROM files
  (names, sizes, CRC32, SHA-1), the tracks or calls read statically from the ROMs (DCS and
  Pinball 2000 track catalogs, Stern SAM call tables) and how many commands sounds.dat
  names. Search, filters, sortable columns, one link per entry (`#<sound ROM id>`). Data
  only: no ROM content, no sound, and no sounds.dat name (only their count).
- **`rom2altsound catalog <roms> --out catalog.json`** writes that catalog (one entry per
  line); `python3 docs/build_site.py` writes the pages around it.
- Fixed: Williams' shuffle alleys (alcat_l7, tts_l1/l2, gmine_l2, tdawg_l1, shfin_l1) were
  given no sound board: `GEN_S11` was read as 0x8000000 instead of gen.h's 0x80000000. They
  are SNDBRD_S11S (the System 11 CPU board's sound); a test now checks every generation
  value against gen.h.

## 0.2.3 (2026-10-08)

- **Listening page: a table, and names you can type.** The sounds are now a table (play
  buttons, ID, Name, Type, Duration, LUFS, True peak, Loop, Pack, Flags): click a column
  title to sort by it, again to reverse (`aria-sort`; the page remembers the sort); the
  sort menu is gone. The table scrolls sideways in its own box on a phone. A sound's name
  can be typed in place (✎, F2 or a double-click; Enter keeps, Escape cancels, empty is no
  name): kept by the browser for this ROM, marked "edited", searchable and sortable.
  **Export names** saves a `names.csv` (rom, sound ROM id, version, then `ID,NAME`),
  **Import names** loads one back, **Clear my edits** forgets them.
- **`rom2altsound names <folder> <names.csv>`** puts those names in a pack: the NAME
  column of altsound.csv (cleaned like every name), the names in manifest.json (the
  previous one kept as `sounds_dat_name`) and index.html; the files and the channels,
  DUCK and STOP do not change. Ids not in the pack are reported, an id given twice with
  two names is refused, and so is a file made for another sound ROM (`--force`). Also at
  extraction: `--names <names.csv>` (one ROM; `--force-names`), checked before the boot.
  `rom2altsound names <folder>` alone prints the pack's names as a names.csv.
- `manifest.json` carries the ROM's `sound_rom_id` (as `rom2altsound roms` computes it).

- **Pinball 2000 (Revenge From Mars, Star Wars Episode I: `SNDBRD_DCSP2K`, 52 PinMAME
  sets).** The game is a PC that writes 16-bit words to its DCS2 board without going
  through PinMAME's sound command path, so the boot logged nothing and the tool's 8-bit
  commands did not reach the board. The board's DSP latches are now hooked (shim, PinMAME
  unchanged): the game's words are logged at boot, and the tool sends the game's own
  protocol, read in its code: a request is the track, `FF7F` (volume, pan in the middle),
  `8000` (board channel 0); the master volume `55AA vv~vv` (level 12, `609F`, is the
  factory volume of both games); the stop `55AE 3F00`; after a board reset, the game's
  boot block upload and `ACE1`, which opens the DCS2 protocol. The track catalog is the
  WPC DCS one, in the board's 16-bit words (at $10000 of the sound flash): the sweep plays
  its populated tracks (swep1_130: 690, rfm_120: 1557) and the loops come from the track
  programs. rfm_120's warm boot never sets its board up (XINA 1.12): the tool does it.
  Full runs (factory volume level 12, `55AA 609F`, recorded at the new reference level 20,
  `55AA A05F`, and scaled -10.6 dB): swep1_130 683 sounds from its 690 tracks, 26 loops (24
  exact from the track programs, 2 from the audio); rfm_120 1538 from 1557, 34 loops, all
  exact; no file clipped, loudest true peak -16.4 dBTP on both. No sounds.dat section: the
  files are named by track number, and the pack has the default DUCK/STOP/CHANNEL (a
  program's channels are relative to the one the game picks). The packs do not play in
  VPinball yet: the game's requests do not reach AltSound. `--dcs-volume` also sets the
  Pinball 2000 volume (default `A0`, also with `--no-factory`, where FF would clip).
- `rom2altsound roms`: a set whose missing files are all in other zips of the same folder
  is reported `completable`, and `--fix-names` writes it whole: a Pinball 2000 version zip
  (its four update files) with MAME's `rfmpb.zip` / `swe1pb.zip` (the shared sound and
  Prism ROMs, under other names) gives a complete set.
- Listening page: the A/B "Compare with folder" box takes an absolute path too
  (`C:\packs\taf_l5-edit`, `/home/…/taf_l5-edit`, a `\\server\share` path), not only one
  relative to the page; 0.2.2 turned a drive path into a broken link (reported by goodtwist).
- **`rom2altsound roms <dir|zip>...`: ROM verification.** Identifies every ROM zip (and
  folder of unzipped ROMs) by its content, against the ROM tables of the PinMAME linked
  in, read from the library itself (every driver's `ROM_START` block: name, size, CRC32,
  SHA-1, region). It reports the set(s) a zip holds, good / bad dump (wrong CRC) / missing
  files, files under another name, files that are no ROM (a `.vpx`, a readme, a nested
  zip), zips named after another set, merged zips (clones at the root or in subfolders),
  split clones that need their parent's zip; `--deep` also checks every file's SHA-1.
  Text and `--json` output; `--fix-names <dir>` writes correctly named standalone zips (or
  links) elsewhere, never touching the originals; `--dump-table` writes PinMAME's whole
  table (2971 sets).
- `rom2altsound roms`: the shared **system ROM sets** PinMAME flags `NOT_A_DRIVER` (Gottlieb
  `gts1`, `gts1s`, `gts80`, `gts80s`, `gts80a`, `gts80as`, `allied`, `gp_110`, `recel`,
  `pinheck`: the CPU board ROMs a generation's games load from them, like MAME's BIOS sets)
  are reported as `SUPPORT`, no longer as incomplete or misnamed games; a game whose only
  missing files are in its system set's zip, in the same folder, is complete. Files
  PinMAME knows no dump of (`NO_DUMP`: the 26 Stern SAM colour mods, `acd_168hc`...) are
  matched by name, as the loader does, instead of `NOT PINMAME`. On the full VPinMAME set
  (2804 zips): 2796 OK, 8 support, nothing else.
- **Sound board family and sound ROM id per game**: the board PinMAME's machine init
  starts (`SNDBRD_*`, both boards on System 11), and the SHA-1 of the sorted SHA-1s of
  the game's sound ROMs, the same for every revision that kept its sound ROMs (all 24
  Twilight Zone sets share one): the key of a future pack catalog. See
  [how it works](docs/how-it-works.md#rom-verification).
- **[docs/board-support.md](docs/board-support.md)**: every sound board family of PinMAME
  with its number of games, and a quick survey (first 40 commands, 5 s each) on the full
  VPinMAME set, one ROM per family: of the 85 families tried, 40 give
  sounds (pre-WPC95 DCS and the separate System 11C board among them), 23 partial or
  doubtful (Whitestar's AT91 board: the sounds come out but the board is not silenced
  between commands), 22 none (each with what the game sends, where known), and the
  cheapest fixes by sets gained; only Pinball 2000 is not in the set. Bally -32 (Lost
  World) and -61B (Fathom) now verified.

## 0.2.2 (2026-10-08)

- **Every file is now at its board's factory volume**, the master volume the game itself
  sets at boot from its factory settings, for every family and whatever the table (was
  the per-family reference volume in 0.2.1): DCS `55 AA 67 98` (level 12/31 on Attack from
  Mars; the board's reset default `67` when a game sends none), WPCS `79 0C F3` (level 12,
  read from the boot, which was already the reference), Whitestar `FE 2C FD` (level 3/31 on
  Apollo 13 and Monopoly, instead of `FE 11 FD`), Stern SAM the DAC attenuation the game
  writes, the operator's volume setting (AC/DC `E8`), as PinMAME plays it (see below).
  Boards without a volume stage (System 11, Data East's pot, Bally) stay at full scale,
  their only level; on Data East the music volume is the game's own at boot, else the
  board's default `20`. The files are as loud as the game plays them out of the box, so
  quieter than before: DCS by about 22 dB (afm_113b: peaks -38.9 to -25.8 dBFS, -43.4 LUFS
  in all), Whitestar by 25 to 33 dB.
  - **Recorded at the reference volume, written at the factory volume.** A board with a
    master volume is recorded at its reference volume, and every analysis runs on those
    recordings, as in 0.2.1: silence trimming, end of sound, loops, twins, the volume check,
    the chips pass, the AltSound pack. Then its files (recording, loop body, extended file,
    loop points) are written again scaled by its factory offset, measured on up to 5 files
    played again at the factory volume (afm_113b -22.44 dB, spread 0.001 dB). The gain is
    applied in floating point, then one rounding to 16 bits with a ±1 LSB TPDF dither, as
    PinMAME's mixer rounds its own output. Recorded straight at the factory volume (the
    previous commit), the dither weighed 22 dB more against a DCS file's signal: the loop
    checks' residuals rose (afm_113b `0x0001` -35.5 dB instead of -47.3), the audio loop of
    `0x0013` changed (0.461 s at -1.3 dB) and no twin passed the twin test. Now `0x0001` is
    at -48.0 dB, `0x0013` at -15.6 dB, and the twin test is 0.2.1's again (1 sample of
    length, lags up to 3, -60 dB; which pairs pass still varies a little from run to run,
    at either volume: 6 to 8 of afm_113b's 8 pairs with `--limit 40`). The files are at the factory level all the same: each
    replayed afm_113b file is within 0.02 dB of what PinMAME plays at the factory volume
    (`scaled_minus_replay_db`), and afm_113b's scaled one-shots are within 0.02 dB of a
    recording made at the factory volume. A board the game leaves at its power-on level
    (no volume sent at boot) is recorded there and not scaled; if the offset cannot be
    measured, the files stay at the reference volume and the manifest says so. Fewer than 5
    files of at least 1 s (a short run): topped up with the other written files. A file
    flagged `ignores_master_volume` is played again at the factory volume and scaled by its
    own move (`factory_gain.own_gains`), not by its board's, and so is a replayed file
    whose move is more than 3 dB from its board's median. Whitestar sounds follow the
    master volume less tightly: monopole's replays are within 0.25 dB of the gain.
  - `manifest.json`: `volume_mode` (`factory` or `reference`), `recorded_volume` (per
    board, the volume and where it comes from, including "the game sent none: the board's
    power-on level", and `gain_db`), `factory_gain` (per board the volume recorded at and
    written at, `gain_db`, `gain`, the spread, how the files were rounded, and how many were
    written again), `clipped_files`; `reference_volume`, `factory_offset_db` and
    `factory_offset` are kept (each replayed file now with `scaled_minus_replay_db`). The
    per-sound levels are those of the files as written.
  - A file that clipped in the recording (in PinMAME's own mix) is listed in
    `clipped_files` and on a `CLIPPED` line of the summary; the volume is never lowered.
  - `--volume reference` writes the files at the reference volume, as 0.2.1 did (DCS `55 AA
    EF 10`, WPCS `79 0C F3`, Whitestar `FE 11 FD`, SAM at full scale); `--dcs-volume`,
    `--wpcs-volume` and `--whitestar-volume` imply it unless `--volume factory` is given.
    `--factory-volume` is kept as a hidden alias of the new default.
  - WPCS master volume levels are reported on their 0..31 scale (was /255).
- **Stern SAM: the DAC attenuation as PinMAME plays it.** PinMAME turns the PCM1755's
  attenuation register into a linear mixer level, `(v & 7F) * 100 / 7F` percent (sam.c):
  AC/DC's factory `E8` plays at 81 %, -1.83 dB, where the DAC's datasheet says -11.5 dB
  (0.5 dB per step). The files follow PinMAME, so that a pack sounds like the game in VPX
  today (acd_168h: -11.3 LUFS in all, loudest true peak +0.3 dBTP, no sample at full scale);
  the datasheet's value is in the manifest (`datasheet_offset_db`), and the discrepancy is
  described in docs/how-it-works.md.
- **Listening page: A/B test** (goodtwist's idea). A "Compare with folder" box takes a
  folder relative to the page (`../taf_l5-edit/`, a copy of the pack with some sounds
  swapped; remembered in the browser). Each sound then gets an **A/B** button, and the `b`
  key, that switch the player between this folder's file and the file of the same name in
  the other folder, at the same position and play state; the player shows which side is
  playing, and a file that does not load from the other folder is marked "missing in B".
  No fetch, so it works from `file://`. Hidden rows are now really hidden in browsers (the
  rows' `display: grid` overrode `hidden`).
- **WPCS and System 11: the DAC is AC-coupled, as on the real boards.** PinMAME maps
  their 8-bit DAC unsigned (code 0 = output 0) while the sound programs play around its
  middle code and leave it on the last value a sound wrote: a DC level in the mix that the
  real boards' AC-coupled outputs never passed on. Every file of The Addams Family started
  on one (up to 14216 LSB), and so did 187 of Whirlwind's 189 (up to 10251), a click in
  AltSound; and under a playing effect that DC (about 8750 LSB at level 20) ate the
  headroom, so `C7` clipped 4857 samples at level 20. Once booted, the DAC now goes through
  the 10 Hz DC correction PinMAME already has (`DAC_DC_offset_correction_data_16_w`, same
  scale, set from our shim; PinMAME is not changed), which replaces the parking at code 0
  of the previous commit: 0 files of taf_l5, tz_94h or whirl_l3 start more than 256 LSB
  away from 0 (50, 55 and 157 at most), and `C7` clips 2 samples at level 20.
- **WPCS reference volume: level 12, `79 0C F3`, the game's own factory volume** (was 22,
  `79 16 E9`, in 0.2.1), so the factory offset is 0 dB. With the DAC AC-coupled no WPCS
  source clips on its own, even at level 31 (taf_l5's peaks: DAC 19308, YM2151 10196,
  HC55536 speech 26530 LSB), but their sum in PinMAME's mixer still clips on five effects
  of The Addams Family that play speech, FM and DAC together (`C6`, `C7`, `CD`, `D3`,
  `D4`: 945 samples in all at 31, 43 at 20, 2 at 13), tz_94h at none. Level 12 is the
  loudest at which no file of either ROM clips in full sweeps. `--wpcs-volume 16` gives
  the level of 0.2.1 back.
- **Raw sweeps say what each range gave** (issue #1, suggested by dekay): when a game has
  no sounds.dat section, one line per board and range before the run (what will be tried)
  and in the final summary (what came out of it), e.g.
  `sweep board 0 (WPCS): bank 7A00..7AFF: 137 with sound, 119 silent (last sound 7A88)`,
  for every board family; `manifest.json` has the same in `sweep`.

## 0.2.1 (2026-10-07)

- **The early Bally sound boards** (0 sounds before): Sounds Plus -51 (Viking: 30 sounds
  from its 32 commands, 3 exact loops), Sounds Plus -56 with speech (Xenon: 49 sounds, 20
  of them speech) and Squawk & Talk -61 (Eight Ball Deluxe: 85 sounds, 53 of them speech
  on the TMS5200). Each board's stop was found in its sound program (`1E`, `05`, `05`),
  and after a reset the tool waits for the program's power-up delay (7 s on Sounds Plus,
  4 s of self-test on Squawk & Talk), which swallowed every command before. The -56 takes a
  byte as two nibbles: the C glue hands the board the high one after it read the low one
  (PinMAME is not changed). Squawk & Talk: its background (`06`) is started with the
  command the game would send next, its files are DC-blocked (its DAC holds DC levels up to
  -14 dBFS, which clicked), and its volume lines, which PinMAME does not emulate, are
  reported as such. The -32/-50 is swept too, untested (no ROM). These packs do not play in
  VPinball as they are: PinMAME hands AltSound the raw writes of the lines the game shares
  with its solenoids, not its commands. The fix is proposed upstream:
  [vpinball/pinmame#717](https://github.com/vpinball/pinmame/pull/717) and
  [vpinball/libaltsound#16](https://github.com/vpinball/libaltsound/pull/16).
- **A run that writes no sound now fails** (`FAILED: no sound was recorded`, exit status 1,
  no pack) instead of reporting OK.
- **WPCS games without a sounds.dat section** (The Addams Family, issue #1): the raw sweep
  now also plays the second bank, `7A00`..`7AFF` (keyed `0x7Axx`), where most of their
  voices and effects are.
- WPCS: a command of several bytes (`79 vv ~vv`, `7A xx`) is written to the board back to
  back, as the game writes it. The Addams Family's sound program took the volume's level
  byte, sent a frame later, for a music of its own, which no stop silenced: 0.2.0 reset its
  board after every command and never got anywhere.
- A board that stays loud after 3 stops and resets in a row, with no command played in
  between, now ends the run with an error instead of being reset forever.
- README: PinMAME's sound commander opens with F4, not F6.

## 0.2.0 (2026-10-07)

- **A page to listen to the pack**: `index.html` in each ROM folder plays every sound
  from disk, in any browser, offline, with what the manifest says about it (name, length,
  loudness, loop, channel, DUCK, STOP, twin, flags), a search box, filters and sorting; a
  batch also gets an `index.html` linking each ROM's page. `--no-html` skips them.
- **Reference volume** on the boards without a volume stage (System 11, Cheap Squeak and
  Turbo Cheap Squeak, Data East's hardware pot): full scale, reported as such
  (`reference_volume` in `manifest.json`); WPCS boards are recorded at `79 16 E9`, the
  loudest level at which no Twilight Zone file clips (`--wpcs-volume`).
- **Stern SAM**: the factory volume read in the DAC is now shown to be the operator's volume
  setting (the coin door's volume buttons move it, `factory_offset.verified`).
- **Loops on the boards older than DCS**: found in the state of the board's processor (the
  music's sequencer), checked on the audio and cut where two cycles differ least
  (`method: "sequencer-state"`): Twilight Zone 26 of its 45 musics, Whirlwind 14 of 22,
  Spy Hunter 2 of 2, City Slicker 7 of 10.
- **WPCS and System 11 mix**: a chips pass plays each sound again with only one chip
  heard, and fills CHANNEL, TYPE, DUCK and STOP from what it measures (`mix` in
  `manifest.json`; `--no-chip-check` skips it).
- WPCS: the sounds of Twilight Zone's second bank are sent as the game sends them, `7A xx`,
  without the filler byte sounds.dat puts in front (which faded the music out), and keyed
  `0x7Axx` in the pack, as libaltsound sees them.

## 0.1.0 (first release)

rom2altsound turns a pinball ROM's sounds into an AltSound pack for Visual Pinball: one WAV
file per sound at one reference volume, exact loops, `altsound.csv`, `g-sound.csv`,
`altsound.ini` and a `manifest.json` with every measure.

- **Boards**: Williams/Bally WPC DCS, WPCS, System 11, Data East and Sega/Stern Whitestar
  (BSMT2000, with the chip's own program when `bsmt2000.zip` is found), Bally Cheap Squeak
  and Turbo Cheap Squeak, driven in PinMAME without a window, many times faster than real
  time.
- **Stern SAM** (new in 0.1.0): no sound board to drive, so the sounds are read from the
  ROM image, after Ashram56's reverse engineering of Tron LE: every sound, every version of
  every song as one continuous file, loops from the game's own music scripts, at full
  scale, and the factory volume read in the DAC. The AltSound files are keyed by the game's
  sound calls; PinMAME cannot play them today (SAM sends no sound command). Checked on
  AC/DC LE 1.68.
- **Factory settings**: each ROM boots cold then warm in a private PinMAME folder; the
  game's own volume is reported, with the dB offset to the reference volume.
- **Loops**: intro + one exact cycle with its `smpl` loop points; a loop with an intro of
  its own also gets a 5-minute extended file (the intro, then whole cycles), played once,
  as AltSound loops whole files only.
- **DCS**: channels, ducking (DUCK) and stops read from the ROM's track programs, checked
  with `--check-ducking`.
- Several ROMs per command, `--jobs` at a time; signed Windows and macOS binaries.

The 0.1.0 betas (0.1.0-beta.1 to beta.4) led up to this release; their notes are on the
releases page.
