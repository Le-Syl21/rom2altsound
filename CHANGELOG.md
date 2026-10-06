# Changelog

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
