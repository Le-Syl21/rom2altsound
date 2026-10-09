# The common method

What rom2altsound does with **every** sound board family, unless that family's section
says otherwise. Each family document only describes what differs from this. The
reference for every step is [how it works](../how-it-works.md); this page is the same
pipeline seen from the board's side, with the code that implements each step.

Stern SAM is the exception to everything here: it has no sound board and is read from
the ROM image without emulation (see [Stern SAM](stern-sam.md#sndbrd_none_sam)).

## Which family a game belongs to

The family is the sound board PinMAME's machine init starts for the game
(`src/drivers.rs`, `Board::sound_boards`): most machine inits pass on
`core_gameData->hw.soundBoard`, set by the game's init function; WPC, System 3 to 11,
Data East alphanumeric, Whitestar and Pinball 2000 pick the board from the hardware
generation instead, and `Board::sound_boards` mirrors their `switch` statements. Two
boards are listed when the machine runs both (System 11: `SNDBRD_S11XS+SNDBRD_S11CS`).

At run time the extractor does not use that name: it switches on the board
interface's own type string, PinMAME's `sndbrd_typestr()` (`"DCS"`, `"WPCS"`,
`"BSMT"`, `"GTS80B"`...), read once the game has booted (`src/extract.rs`,
`board_typestr`). Several `SNDBRD_*` families share one interface (all the Zaccaria
families after 1982 are `"ZAC1370"`, `SNDBRD_GTS3` is `"GTS80B"`), and are then driven
identically. Two families are told apart from their interface by the board's sub-type:
the Bally Sounds Plus -56 (`SNDBRD_BY56`, interface `"BY51"`, reported `BY56`) and
Pinball 2000 (`SNDBRD_DCSP2K`, interface `"DCS"`, reported `DCSP2K`).

## Running the emulation

`src/extract.rs` runs PinMAME (libpinmame, linked statically, `vendor/pinmame`) in
the process, INT16 audio at 44100 Hz, one machine per child process. libpinmame calls
the audio callback once per emulated video frame (`Extractor::on_audio`); that callback
is both the clock (samples = emulated time) and the only place where the tool acts on
the machine, between two frames, as PinMAME's own sound commander does. `throttle` is
set to 0, so the emulation runs as fast as the host allows.

## Boot

- **Factory mode** (the default): the ROM's nvram and cfg are deleted from the private
  vpm folder, a cold boot (child process, `--cold-boot-only`) lets the game write its
  factory nvram, and the extraction runs from a warm boot on that nvram
  (`src/main.rs`, `cold_boot`).
- **What the game sends**: every byte the game CPU writes to a sound board through
  `sndbrd_data_w` reaches the tool through libpinmame's sound command callback
  (`snd_cmd_log` in `src/wpc/snd_cmd.c`, then `Extractor::on_game_command`) and is
  logged with its emulated time. This is the same stream PinMAME hands to AltSound, so
  the boot log (`boot` in `manifest.json`, "game sent N sound byte(s)" in
  `rom2altsound.log`) shows exactly what AltSound would see from that game.
- **End of the boot** (`Extractor::step`, `Phase::Boot`): at least `--boot-secs`
  (15 s), then until 3 s pass without a byte pair not seen before (`BOOT_QUIET_SECS`),
  at most `--boot-max-secs` (60 s).
- **Halt** (`Extractor::end_boot`): every game CPU is halted (`shim_halt_game_cpus`
  in `shim/shim.c`: a CPU with a type and no flags, PinMAME's commander's own
  selection); the audio CPUs keep running. From then on the board only gets what the
  tool sends.
- **Checks**: a machine without a sound board, or a board whose interface has no manual
  command handler (`shim_board_has_mancmd`, PinMAME's `manCmd_w`), ends the run with an
  error ("nothing can be driven"). A run where no command gives a sound ends with
  `FAILED: no sound was recorded`.
- **Settle**: 0.5 s (`SETTLE_SECS`) so that a half-sent command expires, then the stop
  and a wait for silence.

## Sending a command

- **Path**: one byte per call to PinMAME's `sndbrd_manCmd(board, byte)`, which calls
  the board interface's `manCmd_w` handler, the same path as PinMAME's sound commander
  (`snd_cmd.c`, `playCmd`). On most boards `manCmd_w` is the board's own data handler
  (the latch the game writes), sometimes with a strobe or a conversion; each family
  section says which.
- **Pacing**: one byte every 4th emulated frame (`FRAMES_PER_SEND`, as in `playCmd`;
  `Extractor::tick_sender`).
- **Two boards**: a machine that runs two boards takes (board, byte) pairs, and the id is
  `board << 8 | byte` (`0x0105` is byte `05` on board 1).
- **Recording** starts with the frame after the first byte.

## The list of commands

- **sounds.dat**: if PinMAME's `release/sounds.dat` has a section whose header starts
  with the game's name or its parent's (`src/soundsdat.rs`, `game_entries`, the prefix
  rule of PinMAME's `readCmds`), its commands are played, with their names. Only 20
  sections exist in the PinMAME submodule, so most families have none.
- **Raw sweep** otherwise (`src/extract.rs`, `sweep`): the single bytes `01`..`FF` for
  every interface the function does not name (DCS, BSMT/AT91, BY51/BY32, BYSNT and
  WPCS have their own ranges). `00` is not swept: on most boards it is a stop or an
  idle value. The ranges are printed before the run and in `manifest.json` (`sweep`).
- `--only` and `--limit` restrict the list.

## End of a sound

Per command (`Extractor::step`, `Phase::Record`):

- **Silence**: a sample within 2 LSB (`SILENCE`) of the channel's idle level; the idle
  level follows any frame whose span stays within the dither (`track_idle_level`), so a
  board that idles on a constant non-zero level is silent there.
- `no_sound`: nothing above silence 1.5 s (`--no-sound-secs`) after the last byte: no
  file.
- `silence`: 2 s of emulated silence after the sound (`END_SILENCE_SECS`).
- `loop`: one exact cycle of a loop confirmed (see Loops).
- `max`: no loop found within `--loop-max-secs` (240 s): cut at `--max-secs` (120 s),
  `loop_unresolved` says why.
- **Trim** (`Extractor::analyze`): leading and trailing silence are cut, and a tail of held
  DC levels (`held_dc_start`). What is left under 20 ms is a `blip` (`BLIP_SECS`):
  counted, not written.

## Stop, resets and retries

- **The stop** (`Extractor::stop_sends`): after every recording the tool sends the
  family's stop command and waits for 0.5 s of silence (`QUIET_SECS`), at most 10 s
  (`QUIET_MAX_SECS`). The stop comes from, in this order: `--stop`; an entry named
  "sound off" or "reset sound" in a sounds.dat section named after the interface
  (`family_entries`: only `dcs:` and `wpcs:` exist); the table `BUILTIN_STOPS` (System 11,
  BSMT, the Bally boards, Pinball 2000, Zaccaria's `ZAC1370`, Taito). **A family with none of these has no stop
  command: its stop is a board reset**, after every sound.
- **Board reset** (`Extractor::reset_boards`): DCS, Pinball 2000 and WPCS through their
  control port (`sndbrd_ctrl_w`, as the game resets them); every other board by pulsing
  the reset line of all the audio CPUs (`shim_reset_audio_cpus`: the CPUs flagged
  `CPU_AUDIO_CPU`). After a reset the tool waits for 4 s of silence
  (`QUIET_AFTER_RESET_SECS`; longer for the boards of `REBOOT_SECS`, which ignore
  commands for a while after a reset), at most 10 s more.
- **When the stop does not silence the board** within 10 s, the boards are reset. If
  even the wait after a reset does not end in silence, the next file is flagged
  `clean_start: false`. A board still playing after
  3 waits in a row with no command in between ends the run with an error ("still not
  silent after 3 waits", `MAX_STOP_FAILURES`).
- **Retry pass** (`retry_commands`): every command that gave `no_sound` is played once
  more, after the stop and the volume; `retried: true`.

A board whose program starts a sound by itself after a reset (a background tone, an
attract sound) is never silent between commands when its only stop is a reset: every
file then starts over that sound. This is the usual cause of a "doubtful" ⚠️ family in
[board support](../board-support.md).

## Volume

- **Factory volume**: the master volume the game sends at boot, decoded by
  `src/volume.rs` (`decode`): only DCS, Pinball 2000, WPCS and Whitestar (BSMT/AT91)
  have a known master volume command. The Data East BSMT music level is decoded too
  (`decode_de_music`).
- **Reference volume**: a board with a master volume is recorded at its reference
  volume, then its files are scaled to the factory volume by the measured offset
  (`Extractor::apply_factory_gain`, `scale_files`). The master volume check and the
  factory offset pass only run on those boards.
- **Every other board** is recorded at its only level and not scaled. `volume::full_scale`
  names the boards checked in PinMAME's code to have no volume stage (System 11, Data
  East BSMT, the Bally boards: `reference_volume: "full_scale (no volume stage)"`).
  Any other family gets `"none: recorded at the game's own volume"`: the tool sends no
  volume and does not know whether the board has one.
- **AC coupling**: only on WPCS and System 11 (`ac_couples_dac`), whose DACs are put
  through dac.c's 10 Hz DC correction once the game has booted (`shim_dac_ac_couple`).
  Elsewhere the WAVs keep the DC that PinMAME's mix carries (`--dc-block` removes it;
  always on for `BYSNT`, `DC_BLOCKED`). The levels in the manifest are always measured
  DC-blocked (10 Hz high-pass from the idle level before the command).
- **Clipping** is reported, never corrected: `clipped_samples` per file, `clipped_files`.

## Loops

Three methods, tried in this order on a sound still playing (`loop_check`):

- **DCS track program** (`dcs-catalog`, DCS and Pinball 2000 only, `src/dcsrom.rs`).
- **Audio** (`audio`, every board, `src/looping.rs`): the recording repeats sample-exactly
  (up to the dither and a fraction of a sample) over two cycles and at least 20 s.
- **Sound CPU state** (`sequencer-state`, every board but DCS, `src/seqstate.rs`,
  `src/seqloop.rs`): the registers and the plain RAM (`MRA_RAM` entries of the read map)
  of the audio CPUs with an 8-bit data bus, read at the end of every frame
  (`seqstate::Probe`); a period of that state, checked on the audio, cut where two cycles
  differ least. A board whose audio CPUs have no 8-bit bus or no `MRA_RAM` range has no
  state to read, and only the audio method applies.

"Exact" in the manifest means a loop found by one of these methods (`loops_exact_*`
counts); a sound still playing at `--loop-max-secs` without one is cut at `--max-secs`.

## The pack's columns

Without a family-specific analysis (`src/altsound.rs`, `write_pack`):

- `CHANNEL` 0 (music) for loops, for sounds that never ended (`loop_unresolved`) and for
  sounds.dat `Music:` names; empty (-1, polyphonic) for the rest;
- `DUCK` 100, `STOP` 0, `GAIN` 100;
- G-Sound `TYPE`: `music` for loops and sounds that never ended, `callout` for voice
  lines (quoted sounds.dat names, `VOX:`), `sfx` for the rest.

DCS reads them from the track programs, WPCS and System 11 measure them chip by chip
(the chips pass, `chip_commands`, only on interfaces `WPCS` and `WMSS11*`); every other
family has these defaults.

**Twins** (`altsound::find_twins`): a sound of the same length (one sample), the same
loudness (0.01 LU) and a residual 60 dB below the signal once aligned to a fraction of a
sample is the twin of an earlier one; every command keeps its own file and rows unless
`--merge-twins`.

## In VPinball

VPinball's AltSound plays a pack by the command ids PinMAME hands it: the bytes the game
writes through `sndbrd_data_w` (`snd_cmd_log`). PinMAME's own AltSound
(`src/wpc/altsound/snd_alt.cpp`, `preprocess_commands`) combines bytes into ids for a
few hardware generations (WPC, System 11, Data East, Whitestar, Gottlieb System 80A) and
takes every byte as its own id elsewhere; VPinball's libaltsound plugin was not
checked here and may preprocess differently. The pack's ids are what rom2altsound sent
through `manCmd_w`. They match only where the game's command for a sound is the byte
the tool sent, in one write. Each family section says what is known; unless it says
otherwise, no pack of that family has been tried in VPinball.

## The quick survey

The status of every family in [board support](../board-support.md) comes from one run
per family (one ROM, two where the family spans two boards):

```
rom2altsound <set> --roms fixed --max-secs 5 --limit 40 --no-chip-check --loop-max-secs 0 --no-html
```

the factory boot, then the first 40 commands of the list, each recorded for at most 5 s,
without loop search. "n of 40" is how many of them gave a sound; "not from silence"
means a file that does not start from a silent board (something was still playing when
the command went out). It says whether a family works, not how well.

## Diagnostics

- **Boot sound**: the boot line of the log says how long the board played during the boot
  and its peak (`boot: ... sound for 0.1 s of it, peak 4471`): whether the game's own
  commands played anything.
- **`R2A_TRACE=<n>:<start>-<end>`** (environment, hex addresses): every read and write the
  n-th audio CPU (8-bit bus) makes in that range, with the emulated time and its PC, is
  written to `trace.txt` in the ROM's output folder, between the bytes the game sent at
  boot and the tool's own sends (shim.c `shim_trace_hook`). Pointed at the board's PIA or
  command latch, it shows whether and when the program reads a command. The range must lie
  in one entry of the CPU's read and write maps (the hook sits in front of its handler).
