# The common method

What rom2altsound does with **every** sound board family, unless that family's section
says otherwise. Each family document only describes what differs from this. The
reference for every step is [how it works](../how-it-works.md); this page is the same
pipeline seen from the board's side, with the code that implements each step.

Stern SAM is the exception to everything here: it has no sound board and is read from
the ROM image without emulation (see [Stern SAM](stern-sam.md#sndbrd_none_sam)). The
boards that take no sound command at all (Stern SB-300, Atari, Stern's Astro tester,
Romstar) are the other one: the game itself is asked for its sounds, see
[Game-driven boards](#game-driven-boards).

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

What VPinball's AltSound looks up, read in the code it builds (VPinball master, which pins
PinMAME 2150eab and libaltsound f908262 since 2026-10-07) and measured:

1. **PinMAME** logs a sound command with `snd_cmd_log(board, byte)`, which
   `sndbrd_data_w` calls for every write of a sound board's data (unless the main board
   logs the commands itself, `sndbrd_logData`: by35.c since vpinball/pinmame#717).
   libPinMAME turns each one into an `OnAudioCmd` message (board, byte), once the game is
   running and its controller registered.
2. **VPinball's AltSound plugin** (`plugins/altsound/AltSoundPlugin.cpp`) starts
   libaltsound when the game's controller appears, passes the game's generation
   (`core_gameData->gen`, through `GetMachineState`) to `AltSoundSetHardwareGen`, and
   calls `AltSoundProcessCommand(cmd, 0)` for every message: **the board number is
   dropped**.
3. **libaltsound** (`altsound_preprocess_commands`, the same cases as PinMAME's
   `src/wpc/altsound/snd_alt.cpp`) builds the id it looks up (`getSample`, an exact match
   on the CSV's ID):

| generation | id looked up |
|---|---|
| `GEN_WPCDCS`, `WPCSECURITY`, `WPC95DCS`, `WPC95` | two bytes, `0x0186`; `55 AA vv ~vv` (volume) and the other `55 xx` filtered |
| `GEN_WPCALPHA_2`, `WPCDMD`, `WPCFLIPTRON` | one byte; `7A xx` as `0x7Axx`; `79 vv ~vv` volume |
| `GEN_WPCALPHA_1`, `S11`, `S11X`, `S11B2`, `S11C` | one byte; a byte equal to the one before is skipped |
| `GEN_DE`, `DEDMD16`, `DEDMD32`, `DEDMD64` | one byte; `FF` and a lone `00` skipped |
| `GEN_WS`, `WS_1`, `WS_2` | `FC`..`FF xx` as `0xFCxx`..; `FE 10`..`2F` volume; other bytes paired |
| `GEN_GTS80` | one byte; `00` filtered |
| `GEN_BY17`, `BY35` | one byte (since libaltsound#16) |
| every other generation, 0 included | **no case: the bytes are paired**, `(previous << 8) \| byte` on every second byte, which pair depending on how many bytes came since AltSound started |

The last line covers System 3 to 9, the Bally 6803 machines, Stern MPU-100/200, Gottlieb
System 1, 80B and 3, Zaccaria (and the machines on its generations: Jac van Ham,
Rowamet, Tecnoplay's Scramble), Hankin, Alvin G., Mr. Game, Capcom, and every machine
whose generation is 0 (Atari, Game Plan, Playmatic, Taito, Inder/Spinball, Tabart,
Jeutel, Barni, Joctronic...): a pack keyed by the command byte does not play there as
written. One byte per command for them is proposed in
[vpinball/libaltsound#20](https://github.com/vpinball/libaltsound/pull/20) (draft).

**Measured.** One ROM per family ran in libPinMAME (PinMAME 2150eab) for 45 s of attract
mode, with coins and start (keys 3, 5, then 1), its commands fed to libaltsound
(f908262) exactly as the plugin does, with a test pack holding every id
`0x0000`..`0x01FF` and libaltsound's debug log giving each id it looked up. The commands
were also replayed through libaltsound offline (the same ids as live). Each family
section gives what its game sent and what AltSound looked up. A run in which the game
sent no command (many older games are silent in attract mode, and coins do not always
register) only tells the generation.

**What rom2altsound writes for it.** The pack's ids are what the tool sent through
`manCmd_w`. Where AltSound looks a sound up under another fixed id, the pack has that id
too (`altsound::Aliases`, rows with the same file): on Whitestar (BSMT2000 and AT91, not
Data East), `0xFDxx` for each one-byte row, since the games send their sounds as `FD xx`
([SNDBRD_DE2S](data-east-sega-stern.md#sndbrd_de2s)); on System 11 machines with two
sound boards, `0x00xx` for each board 1 row where board 0 has no row for that byte
([System 11](williams-system11.md)). Where the ids are paired, no fixed id can be written:
the families concerned are listed as not playable in `docs/vpx_playback.json` and on the
site. Unless a section says otherwise, no pack of a family has been played in VPinball
itself: what is measured is the id AltSound looks up.

## Game-driven boards

On five families the game CPU makes every sound itself, by writing the sound chip's
registers over time, and sends no sound command: Stern's SB-300
([ST300](stern-early.md#sndbrd_st300)) and its board tester
([ASTRO](stern-early.md#sndbrd_astro)), Atari's generation 1 and 2
([ATARI1](atari.md#sndbrd_atari1), [ATARI2](atari.md#sndbrd_atari2)) and Romstar's Goofy
Hoops ([ROMSTAR](capcom-romstar.md#sndbrd_romstar)). There is nothing to sweep: a byte
sent to these boards is a register value, not a sound. What every one of these programs
has is a sound layer: a routine that plays a sound and a request the rest of the game uses
to ask for one. `src/gamesound.rs` reads that layer in the game's program image (the game
CPU's memory region, Romstar's `REGION_USER1`):

- **The request**: what the game's own code writes or calls to start a sound: a script
  pointer and a delay byte in RAM (Stern), a counter, a slot or a pending count per sound
  (Atari), the game's own play routines (Romstar).
- **The catalog**: every sound the program asks for, found where the program refers to it
  (thread instructions, direct loads, tables), each checked against the format the sound
  routine reads; or the game's own sound table where it has one (Atari generation 2).
- **The stop**: what the game does to silence its sounds (Stern: the script op that
  silences the board; Atari: every counter or slot back to 0; Romstar: the sound system
  reset its sound test calls).

The extraction then differs from the method above in four places
(`Extractor::end_boot`, `gamesound::game_driven`):

- **No halt**: the game CPU keeps running, idling in its attract mode, since it is the one
  that plays the sounds. None of these games plays a sound in attract mode once booted:
  every file of the runs the family sections give starts from silence, and every wait for
  quiet succeeded at once.
- **Sending**: a request is written into the game's RAM through the game CPU's own memory
  map, all its bytes between two frames (`Send::Pokes`, shim.c `shim_game_pokes`, which
  calls PinMAME's `cpunum_write_byte`), so the game never sees half of one. On Romstar the
  game's routine is called (`Send::Call`, shim.c `shim_m68k_call`): a few instructions
  written in the stack space below the stack pointer save the registers, push the
  arguments as the game does, call the routine, restore the registers and return with
  `RTE`; the shim enters them as an exception would, when the 68306 is in supervisor mode
  with its interrupts unmasked, outside the game's sound code, and with the sound
  system's lock word at 0 (the state in which the game's own code makes the call).
- **The ids** are the game's own internal sound ids: a script address (Stern), a counter
  and its length or a slot number (Atari generation 1), a sound number (Atari generation
  2), a sequence address (Romstar). They are written in `altsound.csv` as they are, but
  they are not sound commands: AltSound never receives them (no command reaches it on
  these machines), so **the packs cannot play in VPinball**. They are a recording of every
  sound of the game, for listening, measurement and archive, and for the loudness of each
  ROM. `manifest.json` (`game_sound`) gives the layer read, what an id is, and where the
  program refers to each sound.
- **Volume**: no volume command; the files are at the level the game plays them.

The boot, the recording, the end of a sound, the loops (from the audio only: these
machines have no sound CPU whose state could be read) and the pack's columns are the
common ones.

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
