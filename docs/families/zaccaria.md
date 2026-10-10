# Zaccaria

The Zaccaria sound boards: `SNDBRD_ZAC1311`, `SNDBRD_ZAC1125`, `SNDBRD_ZAC1346`, and the
five families of the 1370 board and its successors, `SNDBRD_ZAC1370`,
`SNDBRD_ZAC13136`, `SNDBRD_ZAC11178`, `SNDBRD_ZAC11178_13181` and `SNDBRD_ZAC13181x3`,
which PinMAME runs through one interface, `ZAC1370`. The board code is in PinMAME's
`src/wpc/zacsnd.c`, the game side in `src/wpc/zac.c`. The `ZAC1125` and `ZAC1346`
boards go through the [common method](common.md) (raw sweep `01`..`FF`, stop = board
reset, the board's own level, default pack columns); the `ZAC1370` interface has its own
framing, sweep and stop (`src/extract.rs`, `zac_strobed`, `sweep`, `BUILTIN_STOPS`), see
[SNDBRD_ZAC1370](#sndbrd_zac1370).

None has a sounds.dat section. `ZAC1125` and `ZAC1346` have no stop in `BUILTIN_STOPS`
(`src/extract.rs`, `stop_sends`): after every sound the tool pulses the reset line of the
audio CPUs (`shim_reset_audio_cpus`) and waits for 4 s of silence. No master volume is
known for any of them (`volume::decode`): the files are at the board's own level
(`"none: recorded at the game's own volume"`), not AC-coupled.

## <a name="sndbrd_zac1311"></a>SNDBRD_ZAC1311

Zaccaria 1311 sound board (discrete) · no PinMAME sound board interface
(`src/wpc/zacsnd.c`, `zac1311Intf = {0}`) · — · 6 sets, 3 games, no sound ROM,
1978, Zaccaria · e.g. Future World (`futurwld`), House of Diamonds (`hod`), Winter
Sports (`wsports`)

- **Hardware**: no sound CPU; a discrete sound circuit (`MACHINE_DRIVER_START(zac1311)`,
  `zac1311_discInt`).
- **Commands**: the game plays its sounds by firing solenoids 21 to 24: `src/wpc/zac.c`
  writes them straight to the discrete circuit (`discrete_sound_w`, in the solenoid
  write path) and never calls `sndbrd_data_w`.
- **What rom2altsound does**: the board interface exists but is empty, so it has no
  manual command handler: `Extractor::end_boot` (`shim_board_has_mancmd`) ends the run
  with "sound board 0 () has no manual command handler in PinMAME: nothing can be
  driven". No pack.
- **What would be needed**: driving the four solenoid lines of the discrete circuit,
  which is not a sound command; nothing in rom2altsound does that.
- **In VPinball**: **the pack cannot play**: AltSound receives nothing from these games
  (no `sndbrd_data_w`; futurwld: no command in 45 s of attract mode with a coin and
  start).

## <a name="sndbrd_zac1125"></a>SNDBRD_ZAC1125

Zaccaria 1125 sound board (SN76477, no CPU) · PinMAME interface `ZAC1125`
(`src/wpc/zacsnd.c`) · ✅ · 8 sets, 4 games, no sound ROM, 1979-1980, Zaccaria · e.g.
Fire Mountain (`firemntn`), Hot Wheels (`hotwheel`), Shooting the Rapids (`strapids`),
Star God (`stargod`)

- **Hardware**: one SN76477 (`zac1125_sn76477Int`) and an NE555 ramp simulated with a
  timer (`ne555_timer`); no sound CPU.
- **Commands**: the game writes a byte of its RAM (`src/wpc/zac.c`, `ram1_w`, offset
  `0x16`), which calls `sndbrd_data_w` at every write. `zac1125_data_w` (`data_w` and
  `manCmd_w`) keeps the low 4 bits; a non-zero value selects one of 8 settings of the
  SN76477 (`state = data >> 1`) and pulses its enable line; `0` does nothing.
  rom2altsound sends one byte per command.
- **Sound list**: raw sweep `01`..`FF`. Only the low nibble counts, and two nibbles
  share each state (`02`/`03`, `04`/`05`...), so the 255 commands hold 8 distinct
  sounds; the rest are repeats.
- **Stop, boot and resets**: no stop command, and no audio CPU to reset
  (`shim_reset_audio_cpus` resets none): the tool relies on the one-shot sounds ending
  by themselves.
- **Volume**: the chip's level, not scaled.
- **Loops**: audio only (no sound CPU, no state to read).
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured** ([board support](../board-support.md)): firemntn 38 of 40, all from
  silence.
- **Limits and what is missing**: the sweep plays every state many times; a sweep of the
  8 states (`02`, `04`... `0E` and `01`) would be enough.
- **In VPinball**: **the pack does not play as written**: libaltsound has no case for
  `GEN_ZAC1` and joins the bytes two by two ([In VPinball](common.md#in-vpinball)).
  Measured on `firemntn`: the game sent `08 09 0A 0B 0A 0B`, AltSound looked up `0809 0A0B
  0A0B`; with one byte per command
  ([vpinball/libaltsound#20](https://github.com/vpinball/libaltsound/pull/20), draft) it
  would look up `0008 0009 000A 000B`. AltSound gets every write of the RAM byte.

## <a name="sndbrd_zac1346"></a>SNDBRD_ZAC1346

Zaccaria 1346 sound board (i8035 MCU and DAC; 1146 with an extra SN76477 on Locomotion)
· PinMAME interface `ZAC1346` (`src/wpc/zacsnd.c`) · ✅ · 6 sets, 3 games, 3 sound ROM
ids, 1980-1981, Zaccaria · e.g. Locomotion (`locomotn`), Earth, Wind & Fire (`ewf`),
Space Shuttle (`sshtlzac`)

- **Hardware**: an i8035 (`MACHINE_DRIVER_START(zac1346)`, flagged `CPU_AUDIO_CPU`)
  writing a DAC on its port 1 (`dac_w`), with 256 bytes of external RAM; Locomotion adds
  an SN76477 and an NE555 tone (`zac1146`).
- **Commands**: as ZAC1125, the game writes its RAM byte `0x16` (`ram1_w`).
  `sp1346_data_w` stores the byte for the program (read on ports `80`..`FF`,
  `sp1346_data_r`, with the DIP switches in the high nibble) and sets the MCU's test
  input T1 to "command waiting" for a non-zero byte, "idle" for `00`; on Locomotion `00`
  also mutes the SN76477. The games follow every command with `00` (locomotn's boot:
  `07 00 05 00 0A 00`...); rom2altsound does the same (`gts80_released`, in
  `board_sends`). Until 0.2.3 it sent one byte per command and never sent `00`.
- **Sound list**: raw sweep `01`..`FF`.
- **Stop, boot and resets**: `00` (`BUILTIN_STOPS`), the idle value.
- **Volume**: the board's own level, not scaled.
- **Loops**: audio; sequencer-state reads the external RAM (`MRA_RAM` `0800`-`08FF` in
  `i8035_readmem`), not the MCU's internal RAM. Not measured.
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured** (survey settings, [board support](../board-support.md)): locomotn 33 of 40,
  ewf 38 of 40, sshtlzac 38 of 40, all from silence (locomotn before: 40 of 40, 39 not
  from silence, all 40 to the 5 s cap; with the stop `00` alone, still all 40 to the cap:
  the command left on the lines replays).
- **Limits and what is missing**: no full run.
- **In VPinball**: **the pack does not play as written**: libaltsound has no case for
  `GEN_ZAC1` and joins the bytes two by two ([In VPinball](common.md#in-vpinball)).
  Measured on `ewf`: the game sent `01 00 01 00 01 00`, AltSound looked up `0100 0100
  0100`; with one byte per command
  ([vpinball/libaltsound#20](https://github.com/vpinball/libaltsound/pull/20), draft) it
  would look up `0001 0000 0001 0000`.

## <a name="sndbrd_zac1370"></a>SNDBRD_ZAC1370

Zaccaria 1370 Sounds & Speech board · PinMAME interface `ZAC1370` (`src/wpc/zacsnd.c`)
· ✅ · 25 sets, 3 games, 8 sound ROM ids, 1982-1987, Zaccaria, Apple Time · e.g. Soccer
Kings (`socrking`), Pinball Champ (`pinchamp`), Thunder Man (`thndrman`)

- **Hardware**: one 6802 (`MACHINE_DRIVER_START(zac1370)`, flagged `CPU_AUDIO_CPU`)
  with three PIAs, a TMS5220 speech chip, an AY-3-8910 and a DAC.
- **Commands**: the game writes its data port (`src/wpc/zac.c`, `data_port_w` →
  `sndbrd_0_data_w`). `sns_data_w` (`data_w` and `manCmd_w`) stores the byte and feeds
  **bit 7 to the CB1 input of the first PIA**: CB1 is the strobe, and the PIA raises the
  6802's IRQ on the CB1 edge the program has armed (`sns_irq0b`). The program reads the
  command through the AY-3-8910's port A, **inverted** (`sns_8910a_r` returns
  `~lastcmd`). The game's boot sends `00 FE FE 7E` ([board
  support](../board-support.md)): bit 7 up, then down with the same low bits, around
  each command.
- **What rom2altsound sends** (`zac_strobed`, in `board_sends`, so the sweep, `--only`
  and the stop all go through it): every byte with bit 7 set goes out as three bytes,
  its low bits with bit 7 clear, the byte itself, then bit 7 clear again (`7E FE 7E`
  for `FE`), one per send (4 frames apart). CB1 then makes one rising and one falling
  edge per command, whichever edge the program arms, and the AY-3-8910's port reads the
  command's bits while the strobe is up. A byte with bit 7 clear is sent as is. Before
  this (0.2.3 and earlier), one byte per command, `01`..`FF` in order: `01` to `7F`
  never moved CB1 and `80` on made only one edge, the 0 of 40 of the survey.
- **Sound list**: sweep `FE` down to `80` (`sweep`, `"ZAC1370"`): commands `01` to `7F`
  as the program reads them (inverted), 127 commands; the id is the byte with bit 7
  set (`0xFE`). `FF` (command `00`) is left out: it is the stop.
- **Stop, boot and resets**: `FF` (`BUILTIN_STOPS`), framed `7F FF 7F`: command `00`,
  what the 13136 games send at boot (`00 FF FF 7F`). On socrking, tmachzac, clown,
  spooky and strsphnx it silenced every sound within the wait (no board reset in the
  survey runs but one on pinchamp).
- **Volume**: the board's own level.
- **Loops**: audio and sequencer-state (6802, `MRA_RAM` `0000`-`007F`, `sns_readmem`);
  nothing recorded.
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured** (survey settings, `--max-secs 5 --limit 40`; [board
  support](../board-support.md)): socrking 25 of 40 (24 files, 1 blip), pinchamp 26 of
  40, all from silence; 0 of 40 before the framing.
- **Limits and what is missing**: no full sweep run yet; which edge of CB1 the program
  arms was not needed (both are made). No names (no sounds.dat section).
- **In VPinball**: **the pack does not play as written**: libaltsound has no case for
  `GEN_ZAC2` and joins the bytes two by two ([In VPinball](common.md#in-vpinball)).
  Measured on `pinchamp`: the game sent `8C 8C 0C E3 E3 63`, AltSound looked up `8C8C 0CE3
  E363`; with one byte per command
  ([vpinball/libaltsound#20](https://github.com/vpinball/libaltsound/pull/20), draft) it
  would look up `008C 008C 000C 00E3 00E3 0063`. The game writes each command with and
  without bit 7 (`8C 8C 0C`), `zac.c` `data_port_w` → `sndbrd_0_data_w`.

## <a name="sndbrd_zac13136"></a>SNDBRD_ZAC13136

Zaccaria 13136 Sounds & Speech board · PinMAME interface `ZAC1370`
(`src/wpc/zacsnd.c`, sub-type 1) · ✅ · 45 sets, 5 games, 20 sound ROM ids, 1983-1985,
Zaccaria · e.g. Time Machine (`tmachzac`), Farfalla (`farfalla`), Devil Riders
(`dvlrider`), Magic Castle (`mcastle`), Robot (`robot`)

- **Hardware**: as ZAC1370 (`MACHINE_DRIVER_START(zac13136)`: 6802, TMS5220,
  AY-3-8910, DAC).
- **Commands**: `sns_data_w` drives the 6802's **IRQ line directly from bit 7** (asserted
  while bit 7 is 1, cleared when it is 0); the command is read inverted
  (`sns2_8910a_r`). The game's boot sends `00 FF FF 7F`.
- **What rom2altsound sends**: as ZAC1370 (`7F FF 7F` framing): the IRQ is asserted for
  one send, then cleared. Before, one byte per command: `01`..`7F` never asserted the
  IRQ (0 of 40 in the survey).
- **Everything else** (sound list, stop, volume, loops, columns): as
  [SNDBRD_ZAC1370](#sndbrd_zac1370).
- **Measured** ([board support](../board-support.md)): tmachzac 35 of 40, farfalla 34
  of 40, all from silence, no board reset (0 of 40 before).
- **Limits and what is missing**: as ZAC1370.
- **In VPinball**: **the pack does not play as written**: libaltsound has no case for
  `GEN_ZAC2` and joins the bytes two by two ([In VPinball](common.md#in-vpinball)).
  Measured on `dvlrider`: the game sent `FF FF 7F F9 F9 79`, AltSound looked up `FFFF 7FF9
  F979`; with one byte per command
  ([vpinball/libaltsound#20](https://github.com/vpinball/libaltsound/pull/20), draft) it
  would look up `00FF 00FF 007F 00F9 00F9 0079`.

## <a name="sndbrd_zac11178"></a>SNDBRD_ZAC11178

Zaccaria 11178 Sounds & Speech board · PinMAME interface `ZAC1370`
(`src/wpc/zacsnd.c`, sub-type 2) · ✅ · 18 sets, 4 games, 9 sound ROM ids, 1985-1986,
Zaccaria · e.g. Clown (`clown`), Pool Champion (`poolcham`), Black Belt (`bbeltzac`),
Mexico 86 (`mexico`)

- **Hardware**: a 6802 (`MACHINE_DRIVER_START(zac11178)`) with a TMS5220 and a custom
  stream (`sns_custInt`) fed through DAC latches (`storelatch`, `dacxfer`...).
- **Commands**: `sns_data_w` feeds bit 7 to the **CA1** input of the second PIA (the
  strobe); the 11178 reads the command inverted at its own address (`readcmd`,
  `lastcmd ^ 0xFF`).
- **What rom2altsound sends**, **sound list**, **stop**, **volume**, **loops**,
  **columns**: as [SNDBRD_ZAC1370](#sndbrd_zac1370) (sequencer-state: `MRA_RAM`
  `0000`-`007F`, `sns3_readmem`).
- **Measured** ([board support](../board-support.md)): clown 40 of 40, poolcham 40 of 40,
  all from silence, no board reset (0 of 40 before the ZAC1370 framing).
- **Limits and what is missing**: as ZAC1370.
- **In VPinball**: **the pack does not play as written**: libaltsound has no case for
  `GEN_ZAC2` and joins the bytes two by two ([In VPinball](common.md#in-vpinball)).
  Measured on `bbeltzac`: the game sent `FF FF 7F FC FC 7C`, AltSound looked up `FFFF 7FFC
  FC7C`; with one byte per command
  ([vpinball/libaltsound#20](https://github.com/vpinball/libaltsound/pull/20), draft) it
  would look up `00FF 00FF 007F 00FC 00FC 007C`.

## <a name="sndbrd_zac11178_13181"></a>SNDBRD_ZAC11178_13181

Zaccaria 11178 board with the 13181 daughter board · PinMAME interface `ZAC1370`
(`src/wpc/zacsnd.c`, sub-type 3) · ✅ · 12 sets, 2 games, 6 sound ROM ids, 1986-1987,
Zaccaria · e.g. Spooky (`spooky`), Zankor (`zankor`)

- **Hardware**: the 11178 (6802, TMS5220) plus a Z80 daughter board with its own DACs
  (`MACHINE_DRIVER_START(zac11178_13181)`).
- **Commands**: two strobes in the top bits (`sns_data_w`): CA1 of the 11178's PIA is
  `(data & 0xC0) == 0xC0` (bits 7 and 6 both set), and a byte with bit 7 set and bit 6
  clear pulses the Z80's NMI, after which the Z80 reads the command (`readcmd`). The
  game's boot sends `00 7F FF 7F 3F BF`.
- **What rom2altsound sends**: the ZAC1370 framing, which is the game's own here
  (`3F BF 3F`): the low bits are on the lines before the strobe byte, so the Z80, which
  reads the command after its NMI, reads the right one. `FE`..`C0` go to the 11178's
  6802 (CA1), `BF`..`80` to the Z80 (NMI). Before, one byte per command, `01`..`FF`:
  `01`..`7F` reached neither board (0 of 40).
- **Everything else**: as [SNDBRD_ZAC1370](#sndbrd_zac1370) (stop `7F FF 7F`, command 00
  of the 6802);
  sequencer-state reads both CPUs (Z80 `MRA_RAM` `FC00`-`FFFF`, `z80_readmem`).
- **Measured** ([board support](../board-support.md)): spooky 38 of 40, zankor 39 of 40,
  all from silence (the first 40 commands, `FE`..`D7`, are the 6802's).
- **Limits and what is missing**: the Z80 half (`BF`..`80`) is not in the survey's first
  40 commands; not measured.
- **In VPinball**: **the pack does not play as written**: libaltsound has no case for
  `GEN_ZAC2` and joins the bytes two by two ([In VPinball](common.md#in-vpinball)).
  Measured on `spooky`: the game sent `7F FF 7F 3F BF 3F`, AltSound looked up `7FFF 7F3F
  BF3F`; with one byte per command
  ([vpinball/libaltsound#20](https://github.com/vpinball/libaltsound/pull/20), draft) it
  would look up `007F 00FF 007F 003F 00BF 003F`.

## <a name="sndbrd_zac13181x3"></a>SNDBRD_ZAC13181x3

Zaccaria board with three 13181 Z80 boards · PinMAME interface `ZAC1370`
(`src/wpc/zacsnd.c`, sub-type 4) · ✅ · 8 sets, 2 games, 2 sound ROM ids, 1987,
Zaccaria · e.g. Star's Phoenix (`strsphnx`), New Star's Phoenix (`nstrphnx`)

- **Hardware**: three Z80s flagged `CPU_AUDIO_CPU` (`zac11183` and following), DACs and
  a TMS5220; no 6802.
- **Commands**: `sns_data_w`: a byte with bit 7 set and bit 6 clear pulses the NMI of the
  second Z80, bits 7 and 6 both set the NMI of the third; each reads the command after
  its NMI.
- **What rom2altsound sends**: as ZAC11178_13181 (`FE`..`C0` to the third Z80, `BF`..`80`
  to the second). Before, one byte per command: `01`..`7F` reached no CPU (0 of 40).
- **Everything else**: as [SNDBRD_ZAC1370](#sndbrd_zac1370) (sequencer-state: the
  Z80s' `MRA_RAM`).
- **Measured** ([board support](../board-support.md)): strsphnx 21 of 40 (20 files, 1
  blip), nstrphnx the same, all from silence, no board reset.
- **Limits and what is missing**: the second Z80's half (`BF`..`80`) not measured.
- **In VPinball**: **the pack does not play as written**: libaltsound has no case for
  `GEN_ZAC2` and joins the bytes two by two ([In VPinball](common.md#in-vpinball)).
  Measured on `strsphnf`: the game sent `7F FF 7F 3F BF 3F`, AltSound looked up `7FFF 7F3F
  BF3F`; with one byte per command
  ([vpinball/libaltsound#20](https://github.com/vpinball/libaltsound/pull/20), draft) it
  would look up `007F 00FF 007F 003F 00BF 003F`.
