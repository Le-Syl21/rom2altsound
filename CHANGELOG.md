# Changelog

## Unreleased

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
  with its solenoids, not its commands.
- **A run that writes no sound now fails** (`FAILED: no sound was recorded`, exit status 1,
  no pack) instead of reporting OK.

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
