# Changelog

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
