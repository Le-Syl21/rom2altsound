# Gottlieb

The Gottlieb sound boards of System 1, System 80/80A, System 80B and System 3:
`SNDBRD_GTS80S`, `SNDBRD_GTS80SP`, `SNDBRD_GTS80SS`, `SNDBRD_GTS80SS_VOTRAX`,
`SNDBRD_GTS80SS_VOTRAX_OLD`, `SNDBRD_GTS80B` and `SNDBRD_GTS3`. All the board code is in
PinMAME's `src/wpc/gts80s.c`. rom2altsound has no code of its own for any of them: they
go through the [common method](common.md) (raw sweep `01`..`FF`, stop = board reset,
recorded at the board's own level, default pack columns). Each section says what the
board does with that.

PinMAME interfaces: `GTS80` (`gts80sIntf`: GTS80S, GTS80SP), `GTS80SS` (`gts80ssIntf`:
the three Sound & Speech variants), `GTS80B` (`gts80bIntf`: GTS80B and GTS3). None has
a stop in `BUILTIN_STOPS` or a sounds.dat section (`src/extract.rs`, `stop_sends`), so
after every sound the tool pulses the reset line of the audio CPUs
(`shim_reset_audio_cpus`) and waits for 4 s of silence.

**How the game talks to these boards.** On System 80/80A/80B the sound lines are bits of
the RIOT port that also drives solenoids: `src/wpc/gts80.c` `riot6532_2b_w` calls
`GTS80_sndCmd_w` → `sndbrd_0_data_w` on **every** write of that port. With a System
80/80A board it sends `(lamp bit 0x10) | (data & 0x0F)` while the strobe bit `0x10` of
the port is set, and `0` (or the lamp bit alone) otherwise; with a System 80B board it
sends `(lamp bit 0x10) | (data & 0x0F)` only while the strobe is set. So the game sends
5-bit commands, returns the lines to idle between them, and repeats the same value at
every solenoid write: spidermn sent 4956 bytes during its boot
([board support](../board-support.md)). System 1 (`src/wpc/gts1.c` `snd_w`) sends a bit
pattern of lamps, chimes and DIP switches. System 3 (`src/wpc/gts3.c` `xvia_1_a_w`)
sends a full byte, inverted.

## <a name="sndbrd_gts80s"></a>SNDBRD_GTS80S

System 80/80A Sound Board (and the System 1 sound board) · PinMAME interface `GTS80`
(`src/wpc/gts80s.c`) · ✅ · 37 sets, 19 games, 20 sound ROM ids, 1979-2008, Gottlieb,
Oliver · e.g. The Amazing Spider-Man (`spidermn`), Buck Rogers (`buckrgrs`), The
Incredible Hulk (`hulk`), Totem (`totem`)

- **Hardware**: one 6502 (`MACHINE_DRIVER_START(gts80s_s)`, about 905 kHz, flagged
  `CPU_AUDIO_CPU`) with a 6530 RIOT; the RIOT's port A is the board's 8-bit DAC
  (`gts80s_riot6530_0a_w`), rendered by PinMAME as a timestamped sample buffer through
  a FIR low-pass (`GTS80S_Update`). No sound chip. Two DIP switches (S1 sound/tones,
  S2 attract mode tunes, `gts80s_init`) are read by the board.
- **Commands**: `gts80s_data_w` is both `data_w` and `manCmd_w`. On System 80/80A games
  it keeps the low 4 bits and puts `dips | 0x20 | (data & 0x0F)` on the RIOT's port B,
  which the sound program reads; no interrupt is raised (only the PiggyPack board,
  `SNDBRD_GTS80SP`, pulses the IRQ). On System 1 games (`coreDips < 32`) all 8 bits go
  to port B as they are. **rom2altsound** sends each command followed by `00`, as the
  game does (`gts80_released`, in `board_sends`); until 0.2.3 it left the command on the
  latch, and the program played it again and again.
- **Sound list** (`sweep`, `"GTS80"`): `01`..`0F` on System 80/80A (the four lines the
  board reads); the whole byte, `01`..`FF`, on System 1 games (`GEN_GTS1`).
- **Stop, boot and resets**: `00` (`BUILTIN_STOPS`), the lines back to idle.
- **Volume**: no master volume command known (`volume::decode`): recorded at the board's
  own level, `reference_volume: "none: recorded at the game's own volume"`. No AC
  coupling.
- **Loops**: the audio method; the sequencer-state method applies (6502, `MRA_RAM` at
  `0000`-`01FF` and `1000`-`10FF` in `GTS80S_readmem`). Not measured: the survey runs
  without loop search.
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured** (survey settings, [board support](../board-support.md)): spidermn 15 of 15,
  all from silence, 4 to the 5 s cap (40 of 40 before, 37 over the last sound);
  buckrgrs (System 1) 30 of 40, all from silence, but at two levels (-13.8 LUFS before as
  now: the System 1 board's tones).
- **Limits and what is missing**: the ids are the four command bits; the game's own bytes
  carry the lamp bit `10` (`01`..`1F`).
- **In VPinball**: libaltsound's `GEN_GTS80` case drops the `00` bytes (the game writes
  thousands) and takes every other byte as an id ([In VPinball](common.md#in-vpinball)):
  measured on `spidermn`, `05 05 05 04` among 5517 `00` looked up as `0005 0005 0005
  0004`. The game's ids are `01`..`1F` (with the lamp bit), the pack's are the swept
  bytes: they match where the two are the same byte. Not played in VPinball itself.

## <a name="sndbrd_gts80sp"></a>SNDBRD_GTS80SP

System 80A Sound Board with the PiggyPack · PinMAME interface `GTS80`
(`src/wpc/gts80s.c`, sub-type 1) · ✅ · 26 sets, 11 games, 11 sound ROM ids, 1983-2022,
Gottlieb, Flipprojets · e.g. Alien Star (`alienstr`), El Dorado City of Gold
(`eldorado`), Rack 'Em Up (`rackemup`), Ice Fever (`icefever`)

- **Hardware**: as `SNDBRD_GTS80S` (`MACHINE_DRIVER_START(gts80s_sp)`, the 6502 at about
  962 kHz), with the PiggyPack, whose 6530 RIOT ROM is not used (`gts80s_init` skips its
  set-up for sub-type 1).
- **Commands**: `gts80s_data_w`, low 4 bits on port B; on this sub-type a non-zero
  nibble also pulses the 6502's IRQ when the program has enabled it
  (`IRQEnabled`, set by `gts80s_riot6530_0b_w`). So each command starts on its own,
  without the board having to see the lines return to idle. rom2altsound sends one byte
  per command.
- **Sound list**: `01`..`0F`, each followed by `00`, as on GTS80S.
- **Stop, boot and resets**: `00`, as on GTS80S.
- **Volume**: as GTS80S: the board's own level, not scaled.
- **Loops**: audio and sequencer-state (same memory map as GTS80S). Not measured.
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured** ([board support](../board-support.md)): alienstr 15 of 15, all from silence
  (38 of 40 before, from the sweep `01`..`28`).
- **Limits and what is missing**: only 15 distinct commands exist; the other swept
  bytes repeat them. Loops, ducking and the full sweep are not verified.
- **In VPinball**: as GTS80S (`GEN_GTS80`; alienstr: `05 05 05 04` looked up as such). Not
  played in VPinball itself.

## <a name="sndbrd_gts80ss"></a>SNDBRD_GTS80SS

System 80/80A Sound & Speech Board, without the Votrax chip · PinMAME interface
`GTS80SS` (`src/wpc/gts80s.c`, sub-type 0) · ✅ · 17 sets, 8 games, 8 sound ROM ids,
1982-2023, Gottlieb, Flipprojets · e.g. Haunted House (`hh`), Krull (`krull`), Spirit
(`spirit`), Amazon Hunt (`amazonh`)

- **Hardware**: one 6502 at 3.58 MHz / 4 (`MACHINE_DRIVER_START(gts80s_ss)`) with a 6532
  RIOT; two DAC latches (`GTS80SS_da1_latch_w`, the sound; `GTS80SS_da2_latch_w`, which
  on the Votrax variants sets the speech chip's clock), mixed in `GTS80SS_Update`.
- **Commands**: `gts80ss_data_w` (`data_w` and `manCmd_w`) keeps the low 6 bits and puts
  `data | 0x80` on the RIOT's port A when the low nibble is not 0, `data` alone when it
  is: bit 7 is the strobe. The RIOT raises the 6502's IRQ on an **edge of PA7**
  (`src/machine/6532riot.c`, `check_pa7_interrupt`; the edge is the one the program
  selects in the RIOT's edge control register, rising or falling). Either way PA7 must
  change for a command to start: the game gets that for free, since it returns the lines
  to `00` between commands (`riot6532_2b_w`, above). rom2altsound sends one byte per command and never
  sends `00` in between: from one non-zero nibble to the next, PA7 stays at 1 and the
  board sees no new command. Only a command that follows one whose low nibble is 0
  (`10`, `20`, `30`...) makes an edge, which fits the 5 of 40 of the survey (not
  verified command by command).
- **Sound list** (`sweep`, `"GTS80SS"`): `01`..`3F`, the six bits that reach the board,
  each followed by `00` (`gts80_released`), which drops PA7 so that the next command
  makes its edge.
- **Stop, boot and resets**: `00` (`BUILTIN_STOPS`). Until 0.2.3 a board reset, which did
  not change the RIOT's port A input: PA7 stayed high and the next commands made no edge.
- **Volume**: no master volume known: the board's own level.
- **Loops**: audio; sequencer-state does not apply as the code stands: `GTS80SS_readmem`
  reads the RIOT RAM through a handler (`GTS80SS_riot6532_3_ram_r`), not `MRA_RAM`, so
  `shim_cpu_ram_ranges` finds no plain RAM and only the registers are in the state.
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured** (survey settings, [board support](../board-support.md)): hh 38 of 40, all
  from silence (5 of 40 before).
- **Limits and what is missing**: the commands with a zero low nibble (`10`, `20`, `30`)
  raise no strobe; they are swept but are not commands of their own.
- **In VPinball**: `GEN_GTS80` case (`00` dropped, every other byte an id; amazonh: `05 05
  05 04` looked up as such), so the game's ids are its 5-bit commands. Not played in
  VPinball itself.

## <a name="sndbrd_gts80ss_votrax"></a>SNDBRD_GTS80SS_VOTRAX

System 80/80A Sound & Speech Board with the SC-01-A Votrax · PinMAME interface `GTS80SS`
(`src/wpc/gts80s.c`, sub-type 1) · ✅ · 24 sets, 9 games, 13 sound ROM ids, 1981-2008,
Gottlieb, Flipprojets · e.g. Black Hole (`blckhole`), Mars - God of War (`mars`),
Volcano (`vlcno_ax`), Devil's Dare (`dvlsdre`)

- **Hardware**: the `SNDBRD_GTS80SS` board plus a Votrax SC-01-A speech chip
  (`GTS80SS_votrax_sc01a_interface`), written by `GTS80SS_vs_latch_w` (inverted
  phonemes), its clock set by the second DAC latch; the chip's request line is wired to
  the 6502's NMI (`GTS80SS_nmi`) and its status to RIOT port B bit 7
  (`GTS80SS_riot3b_r`).
- **Commands**, **sound list**, **stop**, **volume**, **loops**, **columns**: as
  [SNDBRD_GTS80SS](#sndbrd_gts80ss): the same `gts80ss_data_w`, the same PA7 strobe.
- **Measured** ([board support](../board-support.md)): blckhole 38 of 40, all from
  silence (5 of 40 before the `00` after each command).
- **Limits and what is missing**: as GTS80SS.
- **In VPinball**: as GTS80SS (blckhole: `05 05 05 04` looked up as such). Not played in
  VPinball itself.

## <a name="sndbrd_gts80ss_votrax_old"></a>SNDBRD_GTS80SS_VOTRAX_OLD

System 80 Sound & Speech Board with the older SC-01 Votrax · PinMAME interface `GTS80SS`
(`src/wpc/gts80s.c`, sub-type 2) · ✅ · 1 set, 0 games (a clone), 1 sound ROM id, 1981,
Gottlieb · e.g. Mars - God of War prototype (`marsp`)

- **Hardware**: as GTS80SS_VOTRAX with the older SC-01 (`MACHINE_DRIVER_START(gts80s_ss_old)`,
  `GTS80SS_votrax_sc01_interface`).
- **Everything else**: as [SNDBRD_GTS80SS](#sndbrd_gts80ss).
- **Measured** ([board support](../board-support.md)): marsp 31 of 40 (29 files, 2
  blips), all from silence, 11 clipped (3 of 40 before).
- **Limits and what is missing**: as GTS80SS; the clipping is the old Votrax's level.
- **In VPinball**: as GTS80SS. Not played in VPinball itself.

## <a name="sndbrd_gts80b"></a>SNDBRD_GTS80B

System 80B Sound Board (three generations) · PinMAME interface `GTS80B`
(`src/wpc/gts80s.c`) · ✅ · 148 sets, 32 games, 31 sound ROM ids, 1985-2021, Gottlieb,
Flipprojets · e.g. Raven (`raven`), Bad Girls (`badgirls`), Rock (`rock`), Spring Break
(`sprbreak`)

- **Hardware**: two 6502s flagged `CPU_AUDIO_CPU`, the "Y" CPU (music, speech) and the
  "D" CPU (DAC) (`gts80s_b1`, `gts80s_b2`, `gts80s_b3`; a third CPU for the second DAC
  on Bone Busters, `gts80s_b3a`). Generation 1: DAC, AY-3-8910 and the SP0250 speech
  chip; generation 2: DAC and AY-3-8910; generation 3: DAC and YM2151. The DACs are
  written through `DAC_DC_offset_correction_data_16_w` (`s80bs_dac_data_w` and
  friends): PinMAME already high-passes them, so they carry no DC.
- **Commands**: `gts80b_data_w` (`data_w` and `manCmd_w`) ignores the very first byte
  after the board starts (`firstCmd`), inverts the byte and calls `s80bs_sh_w`, which
  (with `test1` defined) latches every value but `FF` and raises the IRQ of both CPUs.
  The latch holds the full byte. rom2altsound sends one byte per command; the game's
  first byte has already been taken by the time the tool sends.
- **Sound list**: raw sweep `01`..`FF`: every byte is a distinct command on this board
  (the game's own commands are 5 bits, `riot6532_2b_w`, so most of the upper range is
  expected to be silent or to repeat; not measured).
- **Stop, boot and resets**: no stop command: after every sound both (or three) audio
  CPUs are reset (`shim_reset_audio_cpus`), then 4 s of silence.
- **Volume**: the board's program sets its own DAC volume registers (`s80bs_dac_vol_w`);
  no master volume command is known: recorded at the board's own level, not scaled.
- **Loops**: audio and sequencer-state (6502s, `MRA_RAM` `0000`-`07FF` on each CPU,
  `GTS80BS1_readmem` and following). Not measured.
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured** ([board support](../board-support.md)): badgirls 28 of 40, raven 37 of 40.
- **Limits and what is missing**: loops, ducking and the full sweep are not verified.
- **In VPinball**: **the pack plays from VPinball master 3abe805 on**, not in 10.8.1-5436 and older:
  their libaltsound has no case for `GEN_GTS80B` and joins the bytes two by two ([In VPinball](common.md#in-vpinball)).
  Measured on `amazonh2`: the game sent `1A 1A 1A 1B`, AltSound looked up `1A1A 1A1B`;
  with one byte per command
  ([vpinball/libaltsound#20](https://github.com/vpinball/libaltsound/pull/20), in VPinball master from 3abe805) it
  looks up `001A 001A 001A 001B`. Each byte the game writes is the same byte as the
  tool's (both go through `gts80b_data_w`), so with that change the ids would match.

## <a name="sndbrd_gts3"></a>SNDBRD_GTS3

System 3 Sound Board · PinMAME interface `GTS80B` (`src/wpc/gts80s.c`, sub-type 1) · ✅
· 51 sets, 23 games, 23 sound ROM ids, 1989-2023, Gottlieb, Gottlieb / Vifico · e.g.
Cue Ball Wizard (`cueball`), Super Mario Bros. (`smb`), Cactus Jack's (`cactjack`),
Operation Thunder (`opthund`)

- **Hardware**: the GTS80B design with two 6502s, a DAC, a YM2151 and an OKI MSM6295
  (`MACHINE_DRIVER_START(gts80s_s3)`; `gts80s_s3_no` without the OKI).
- **Commands**: the game writes a VIA port (`src/wpc/gts3.c`, `xvia_1_a_w`), inverts the
  byte and passes it to `sndbrd_0_data_w` (Strikes N' Spares, with two DMDs, sends it
  later); `gts80b_data_w` inverts it back. rom2altsound sends one byte per command
  through the same handler.
- **Sound list**: raw sweep `01`..`FF`.
- **Stop, boot and resets**: a board reset after every sound (no stop command).
- **Volume**: as GTS80B: the board's own level, not scaled.
- **Loops**: audio and sequencer-state (as GTS80B). Not measured.
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured** ([board support](../board-support.md)): cueball 40 of 40.
- **Limits and what is missing**: loops, ducking and the full sweep are not verified.
- **In VPinball**: **the pack plays from VPinball master 3abe805 on**, not in 10.8.1-5436 and older:
  their libaltsound has no case for `GEN_GTS3` and joins the bytes two by two ([In VPinball](common.md#in-vpinball)).
  Measured on `barbwire`: the game sent `00 FF 00 55 00 FF`, AltSound looked up `00FF 0055
  00FF`; with one byte per command
  ([vpinball/libaltsound#20](https://github.com/vpinball/libaltsound/pull/20), in VPinball master from 3abe805) it
  looks up `0000 00FF 0000 0055`. The game writes the value the tool sends
  (`GTS3locals.sound_data`), so with that change the ids would match.
