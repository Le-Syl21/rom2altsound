# Synthetic test ROM set: `flash_l1.zip`

**These are not Williams ROMs.** The four files in `flash_l1.zip` are a tiny program
written for rom2altsound's tests (by `make_testrom.py`, next to this file, under the
repository's BSD-3-Clause license). They contain no original code or data of any game:
no byte of them comes from a ROM dump. The zip only carries the set's name and file
names (`gamerom.716`, `green1.716`, `green2.716`, `sound1.716`), so that PinMAME's
`flash_l1` driver (Williams System 6, sound board `SNDBRD_S67S`) loads it.

What the program does:

- **game CPU** (`green2.716`, reset vector at `FFFE`): masks its interrupts and loops
  forever. rom2altsound halts the game CPU after the boot and drives the sound board
  itself, so nothing more is needed. `gamerom.716` and `green1.716` hold a banner only.
- **sound CPU** (`sound1.716`, the board's 6808): on a command (PIA port B, CB1 rising
  edge), commands `00` to `04` play a square wave on the DAC of a known frequency and
  length (`expected.json`), then fall silent; every other command plays nothing.

PinMAME checks the CRC and SHA-1 of each file: they do not match, so it logs
`WRONG CHECKSUMS` and `WARNING: the game might not run correctly.`, then runs the set as
usual (libpinmame refuses a set only when files are *missing*).

Regenerate the zip and `expected.json` with `python3 make_testrom.py`; `--check` tells
whether the committed files match the script (CI runs it). `tests/check_pack.py` runs a
rom2altsound binary on the set and checks the pack: exit status, `manifest.json`,
`altsound.csv` (one row per tone), `index.html`, and every WAV file's length and
frequency. `cargo test` runs it on the freshly built binary (`tests/synthetic_rom.rs`),
CI on every released binary, on its own system.
