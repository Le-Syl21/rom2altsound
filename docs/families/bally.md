# Bally sound boards

The Bally (and Bally-derived) boards of `src/wpc/by35snd.c`: the -32/-50 tone board
(`SNDBRD_BY32`), Sounds Plus -51 and its variants (`SNDBRD_BY51`, `SNDBRD_BY56`,
`SNDBRD_BY51N`), Squawk & Talk -61 and its variants (`SNDBRD_BY61`, `SNDBRD_BY61B`,
`SNDBRD_BY61B2`, `SNDBRD_BY61N`), Cheap Squeak (`SNDBRD_BY45`, `SNDBRD_BY45BP`), Turbo
Cheap Squeak (`SNDBRD_BYTCS`) and Sounds Deluxe (`SNDBRD_BYSD`). Everything not said
here is the [common method](common.md).

The interface type string decides how a board is driven (`src/extract.rs`,
`board_typestr`), and several families share one (`src/wpc/sndbrd.h`):

| family | `SNDBRD_TYPE` | interface | reported as |
|---|---|---|---|
| BY32 (BY50 is the same) | (4,0) | `BY32` | `BY32` |
| BY51 | (5,0) | `BY51` | `BY51` |
| BY56 | (5,1) | `BY51` | `BY56` (sub-type 1, `BY56_SUBTYPE`) |
| BY51N | (5,2) | `BY51` | `BY51` |
| BY61, BY61B, BY61B2, BY61N | (7,0..3) | `BYSNT` | `BYSNT` |
| BY45, BY45BP | (8,0..1) | `BY45` | `BY45` |
| BYTCS | (9,0) | `BYTCS` | `BYTCS` |
| BYSD | (10,0) | `BYSD` | `BYSD` |

So BY51N gets everything that was measured on the -51 (sweep, stop, reboot delay), all
four Squawk & Talk variants get what was read in Eight Ball Deluxe's program, and BY45BP
what was measured on the Cheap Squeak.

**How the game talks to these boards** (all but the 6803/6809 games below): the four
lines the game shares with its solenoids (`src/wpc/by35.c` `pia1b_w`:
`sndbrd_0_data_w(0, data & 0x0f)` on every write, solenoids included), a fifth line
"Sound E" (`pia1a_w`, bit 1, when the game has it: `BY35HW_SOUNDE`) and a strobe
(`pia1cb2_w`, `sndbrd_0_ctrl_w`). The later games with a 6803 CPU (`src/wpc/by6803.c`)
write the command on port 1 (`M6803_PORT1` → `sndbrd_0_data_w`) and the strobe on port 2
(`port2_w`).

**In VPinball (every family here)**: AltSound receives the `sndbrd_data_w` writes, i.e.
on the by35.c games every write of the shared lines, 4 bits at a time (solenoids
included), which libaltsound pairs two by two; the packs are keyed by the command byte.
They do not match: these packs do not play in VPinball as they are (README note 12,
CHANGELOG 0.2.1; a fix is proposed upstream in vpinball/pinmame#717 and
vpinball/libaltsound#16). On the 6803 games, what reaches AltSound per command is not
determined from the code. No pack of these families has been tried in VPinball.

## <a name="sndbrd_by32"></a>SNDBRD_BY32

Bally -32 / -50 tone board · PinMAME interface `BY32` (`src/wpc/by35snd.c`) · ✅ · 39
sets, 12 games, 2 sound ROM ids, 1978-2022, Bally (and conversions) · e.g. Kiss
(`kiss`), Lost World (`lostwrld`), Playboy (`playboy`), Star Trek (`startrek`)

- **Hardware**: no sound CPU. A 32-byte PROM gives the pitch of each tone; PinMAME plays
  a sine through a mixer channel with a decay (`by32_custInt`, `setfreq`), plus a
  discrete part (`as2888_discrete`, `MACHINE_DRIVER_START(by32)`).
- **Commands**: 5 bits: the low nibble on the shared lines (`by32_data_w`) and bit 4 on
  "Sound E" with the strobe (`by32_ctrl_w`); a tone starts on the rising strobe, the
  falling strobe mutes it. `by32_manCmd_w` writes the nibble, drops the strobe, then
  raises it with bit 4. rom2altsound sends one byte per command through it.
- **Sound list**: `00`..`1F` (`sweep`, five lines). From the code, only `10`..`1E` can
  give a tone through `by32_manCmd_w`: the falling strobe stores the command without bit
  4 as the last one, and `setfreq` ignores a command equal to the last one and any with
  low nibble `F`; this matches the survey's 15 tones of 32. The tones of `00`..`0E` (the
  other half of the PROM, `cmd ^ 0x10`) are not reached. Not verified beyond the code.
- **Stop, boot and resets**: stop `0F` (`BUILTIN_STOPS`: low nibble F, the strobe drop
  mutes the tone). There is no audio CPU, so a board reset (`shim_reset_audio_cpus`) does
  nothing.
- **Volume**: one level with its own decay, `full_scale (no volume stage)`
  (`volume::full_scale`).
- **Loops**: audio method only (no audio CPU, no state to read).
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured**: kiss, 15 tones from the 32 commands (board-support); README note 15 and
  CHANGELOG name Lost World for the same count.
- **Limits**: half of the PROM's tones are not played (above); one tone per command, no
  sound program.

## <a name="sndbrd_by51"></a>SNDBRD_BY51

Bally Sounds Plus -51 · interface `BY51` (`src/wpc/by35snd.c`) · ✅ · 41 sets, 14 games,
14 sound ROM ids, 1979-2019, Bally · e.g. Space Invaders (`spaceinv`), Viking
(`viking`), Rolling Stones (`rollston`), Silverball Mania (`slbmania`)

- **Hardware**: M6802 at 0.89 MHz, 128 bytes of RAM, a PIA, an AY-3-8910
  (`MACHINE_DRIVER_START(by51)`). The command lines are read through the AY-3-8910's
  port A, inverted (`sp_8910a_r`). The PIA's CB2 is turned by PinMAME into a 75 % mute
  (`sp_pia0cb2_w`).
- **Commands**: five lines (four data + Sound E), one strobe on CA1 (`sp51_data_w`,
  `sp51_ctrl_w`). `sp51_manCmd_w` puts the whole byte on the lines and pulses CA1. The
  game's byte `xx` runs entry `~xx & 1F` of the program's table (vikingb $109B,
  [how it works](../how-it-works.md#per-family)).
- **Sound list**: `00`..`1F` (`sweep`; `00` is the table's last entry, a sound).
- **Stop, boot and resets**: stop `1E` (`BUILTIN_STOPS`: entry 1, the background off,
  vikingb $12C9). After a reset the program waits 7.0 s with its interrupts off; the wait
  after a reset is 8 s (`REBOOT_SECS`).
- **Volume**: `full_scale (no volume stage)`; vikingb and xenon set CB2 once at reset
  and never again.
- **Loops**: audio and sound CPU state (6802, RAM `0000`-`007F` as `MRA_RAM`,
  `sp51_readmem`).
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured**: vikingb 30 sounds from 32 commands, 3 exact loops from the audio (bodies
  of 0.048 to 0.125 s), its background `1D` never repeats (how it works, Loops; README
  note 13); spaceinv 32 of 32 (board-support).
- **Limits**: each game has its own sound program; the stop and the table rule were read
  in vikingb's only.

## <a name="sndbrd_by56"></a>SNDBRD_BY56

Bally Sounds Plus -56 with the -57 Vocalizer · interface `BY51`, sub-type 1, reported
`BY56` (`src/wpc/by35snd.c`) · ✅ · 8 sets, 1 game, 4 sound ROM ids, 1980-2008, Bally ·
Xenon (`xenon`)

- **Hardware**: the -51's M6802 and AY-3-8910 plus an MC3417 CVSD for speech, ROM
  `8000`-`FFFF` (`MACHINE_DRIVER_START(by56)`, `sp_pia0b_w` drives the MC3417).
- **Commands**: the handler reads the lines twice, about 57 us apart, and makes a byte of
  the two nibbles (xenon $F02E-$F078); the byte `xx` runs entry `xx - 4`.
  `sp51_manCmd_w` leaves the same byte on the lines for both reads, so rom2altsound sends
  through `shim_nibble_cmd` (`shim/shim.c`): `shim_nibble_hook` puts a read handler in
  front of the sound CPU's PIA at `0080`; the first read gets the low nibble, then the high
  nibble is put on the lines for the second (`Extractor::tick_sender`, `end_boot`; the
  log says how many reads the first command saw, xenon 3).
- **Sound list**: `01`..`FF` (the generic sweep).
- **Stop, boot and resets**: stop `05` (entry 1); reboot wait 8 s (`REBOOT_SECS`, the same
  7.0 s delay loop, xenon $F013). Speech entries play with the interrupts off, so a stop
  waits for the end of the line.
- **Volume**: `full_scale (no volume stage)`.
- **Loops**: audio and sound CPU state (RAM `0000`-`007F`, `sp56_readmem`).
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured**: xenon 49 sounds, 20 of them speech; 1 loop from the audio; `1A` repeats in
  the state every 2.600 s but its cycles differ by -7.5 dB in the audio (probably the
  AY-3-8910's noise generator), so it is not taken (how it works, Loops; README note 13).
  Survey: 30 of 40 (board-support).
- **Limits**: one game; only its program was read.

## <a name="sndbrd_by51n"></a>SNDBRD_BY51N

Bell Games' -51N (Sounds Plus -51 variant) · interface `BY51`, sub-type 2, reported
`BY51` (`src/wpc/by35snd.c`) · ⚠️ · 2 sets, 1 game, 1 sound ROM id, 1984-2018, Bell
Games · Super Bowl (`suprbowl`, `src/wpc/nuova.c`)

- **Hardware**: the -51's M6802 and AY-3-8910 plus a DAC at `1000`
  (`MACHINE_DRIVER_START(by51N)`).
- **Commands**: the game has no Sound E line (`BY35GD_NOSOUNDE`): four bits only.
  `sp51_data_w` (sub-type 2) keeps `data & 0x0f` and raises the CPU's IRQ when the
  strobe is high and the nibble is not `0F`; the program reads the command through the
  PIA's port A, inverted (`sp_8910r` returns `~lastcmd`). `sp51_manCmd_w` stores the whole
  byte, pulses CA1 and the IRQ.
- **Sound list, stop, reboot**: as reported `BY51`, it gets the -51's: `00`..`1F`, stop
  `1E`, 8 s after a reset. None of these was checked against Super Bowl's program: the
  game only sends `00`..`0F` (`0F` raises no interrupt), and what `1E` and the commands
  `10`..`1F` do in its program is not known.
- **Volume**: `full_scale (no volume stage)` (as `BY51`).
- **Loops**: audio and sound CPU state (RAM `0000`-`007F`, `sp51N_readmem`).
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured**: 6 of 32 (board-support).
- **Limits**: not determined from the code why 26 commands are silent. A start: give
  the sub-type its own sweep (`00`..`0E`) and stop, read in its sound program.

## <a name="sndbrd_by61"></a>SNDBRD_BY61

Bally Squawk & Talk -61 · interface `BYSNT` (`src/wpc/by35snd.c`) · ✅ · 34 sets, 10
games, 12 sound ROM ids, 1981-2019, Bally / Bally Midway · e.g. Eight Ball Deluxe
(`eballdlx`), Flash Gordon (`flashgdn`), Elektra (`elektra`), Mr. & Mrs. Pac-Man
(`m_mpac`)

- **Hardware**: M6802 at 0.89 MHz, 128 bytes of RAM, two PIAs, an AY-3-8910 (or 8912), a
  DAC at `1000` and a TMS5200 speech chip (`MACHINE_DRIVER_START(by61)`). Sub-type 0
  reads the command through the AY's port A (`snt_ay8910Int` → `snt_8910a_r`).
- **Commands**: the same two reads as the -56, the byte `xx` runs entry `xx - 4`.
  PinMAME's manual command already hands the low nibble on the first read and the high
  one on the second (`snt_manCmd_w`, `snt_8910a_r` with `manualSoundcmd`), so
  rom2altsound sends through `sndbrd_manCmd`. `06` (background on) is followed by `04`
  (entry 0, nothing), as the game sends its next sound (`command_sends`,
  `BYSNT_BACKGROUND_ON`, `BYSNT_NOOP`).
- **Sound list**: `01`..`DE` (`sweep`, `BYSNT_LAST`): from `DF` on the commands set the
  volume lines (eballdlx $F915).
- **Stop, boot and resets**: stop `05` (entry 1, the background off, eballdlx $FC90);
  reboot wait 6 s (`REBOOT_SECS`: RAM and AY register test, back 4.0 to 4.25 s later).
- **Volume**: volume lines on the PIAs (port B bits 4-7 of each) that PinMAME stores and
  ignores: `full_scale (volume lines not emulated in PinMAME)`. The files are always
  written DC-blocked (`DC_BLOCKED`): the unsigned DAC holds its last value, up to
  6553 LSB of DC in PinMAME's mix.
- **Loops**: audio and sound CPU state (RAM `0000`-`007F`, `snt_readmem`).
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured**: eballdlx 85 sounds from 222 commands, 53 speech lines; the 84 files not cut
  at 2 minutes start and end within 50 LSB of 0; five speech lines touch full scale for 2
  or 3 samples; the background `06` changes for more than an hour and is cut (how it
  works, Per family and Loops; README note 14). flashgdn: 26 of 40 (board-support).
- **Limits**: the stop, the `06`/`04` rule and `BYSNT_LAST` were read in eballdlx's
  program only, and apply to every `BYSNT` game.

## <a name="sndbrd_by61b"></a>SNDBRD_BY61B

Squawk & Talk -61B · interface `BYSNT`, sub-type 1 (`src/wpc/by35snd.c`) · ✅ · 20 sets,
6 games, 6 sound ROM ids, 1981-2011, Bally · e.g. Centaur (`centaur`), Fathom
(`fathom`), Embryon (`embryon`), Medusa (`medusa`)

- **Hardware**: the -61's (`by61` machine driver); sub-types 1 and 2 read the command
  lines on the first PIA's port A directly (`snt_pia0a_r` → `snt_8910a_r`) instead of
  through the AY.
- **Everything else**: as [SNDBRD_BY61](#sndbrd_by61): same sweep `01`..`DE`, stop
  `05`, `06`+`04`, 6 s reboot wait, DC-blocked files, volume lines not emulated. None of
  these was read in a -61B game's program.
- **Measured**: centaur 30 of 40 (board-support); Fathom 32 of its first 40 (README note
  14).
- **Limits**: as BY61.

## <a name="sndbrd_by61b2"></a>SNDBRD_BY61B2

Squawk & Talk -61B with a second board · interface `BYSNT`, sub-type 2
(`src/wpc/by35snd.c`) · ✅ · 1 set, 1 game, 1 sound ROM id, 1982, Bally · Mysterian
(`mysteria`, prototype)

- **Hardware**: two M6802 boards (`MACHINE_DRIVER_START(by61x2)`), the second with only a
  DAC (no AY, no speech chip; `snt_init` comment).
- **Commands**: one PinMAME board interface for both: `snt_manCmd_w` pulses the CB1 line
  of both boards' PIAs, `snt_ctrl_w` too. A board reset pulses both CPUs
  (`shim_reset_audio_cpus`); the state probe reads both.
- **Everything else**: as [SNDBRD_BY61](#sndbrd_by61).
- **Measured**: 18 of 40, all from silence (board-support, first run).

## <a name="sndbrd_by61n"></a>SNDBRD_BY61N

Bell Games' Squawk & Talk variant · interface `BYSNT`, sub-type 3
(`src/wpc/by35snd.c`) · ✅ · 1 set, 1 game, 1 sound ROM id, 1985, Bell Games · Cosmic
Flash (`cosflash`, `src/wpc/nuova.c`)

- **Hardware**: an M6802 with one PIA and an AY-3-8910, no speech chip
  (`nuova.c` `MACHINE_DRIVER_START(cosflash)`, `cf_writemem`). Its sound ROM is a
  `BAD_DUMP` in PinMAME.
- **Commands**: no Sound E (`BY35GD_NOSOUNDE`); `snt_ctrl_w` pulses the CPU's IRQ on the
  strobe, and `snt_manCmd_w` does too (sub-type 3).
- **Everything else**: as [SNDBRD_BY61](#sndbrd_by61) (sweep `01`..`DE`, stop `05`,
  DC-blocked files...), none of it checked in this game's program.
- **Measured**: 26 of 40, all from silence (board-support, first run).

## <a name="sndbrd_by45"></a>SNDBRD_BY45

Bally Cheap Squeak (-45) · interface `BY45` (`src/wpc/by35snd.c`) · ✅ · 23 sets, 10
games, 12 sound ROM ids, 1983-2021, Bally, Bell Games · e.g. X's & O's (`xsandos`), Spy
Hunter (`spyhuntr`), Kings of Steel (`kosteel`), Black Pyramid (`blakpyra`)

- **Hardware**: M6803 at 0.89 MHz, its 128 bytes of internal RAM, an 8-bit DAC on port 1
  (`MACHINE_DRIVER_START(by45)`, `cs_writeport`).
- **Commands**: a byte as two nibbles with one strobe: the low nibble with the sound
  interrupt (TIN), the high one 70 to 130 us later, read by the same handler on port 2
  (`cs_port2_r`). Upstream PinMAME has no manual command for this board; the fork
  rom2altsound builds adds `cs_manCmd_w`, which hands the high nibble over on the port 2
  read after the first (`cslocals.manHi`). rom2altsound sends through `sndbrd_manCmd`.
- **Sound list**: `01`..`FF` (no sounds.dat section).
- **Stop, boot and resets**: stop `00` (`BUILTIN_STOPS`; the games send it at power-up
  and between sounds). It does not stop every music (spyhuntr `11`, `12`): the board is
  then reset (audio CPU reset line), 4 s wait.
- **Volume**: `full_scale (no volume stage)`.
- **Loops**: audio and sound CPU state (internal RAM `0080`-`00FF`, `cs_readmem`).
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured**: spyhuntr 53 sounds with `--max-secs 20 --loop-max-secs 40` (2 resets),
  2 of 2 musics looped from the state (19.0 s) at the default search (how it works, Per
  family and Loops); xsandos 39 of 40 (board-support).

## <a name="sndbrd_by45bp"></a>SNDBRD_BY45BP

Cheap Squeak behind the Baby Pac-Man video board · interface `BY45`, sub-type 1
(`src/wpc/by35snd.c`, `src/wpc/byvidpin.c`) · ❌ · 4 sets, 2 games, 2 sound ROM ids,
1982-2006, Bally · Baby Pac-Man (`babypac`), Granny and the Gators (`granny`)

- **Hardware**: the Cheap Squeak's M6803 and DAC, on a machine with a main M6800 and a
  video M6809 (both halted by the tool).
- **Commands**: the video CPU, not the main one, writes the board: its PIA's port B
  (`byvidpin.c` `pia2b_w`, low nibble) and CB2 (`pia2cb2_w` → `sndbrd_0_data_w`,
  `sndbrd_0_ctrl_w`). Two things differ from the BY45 in `by35snd.c`: `cs_ctrl_w` reads
  the strobe with the opposite sense for sub-type 1 (`ctrl = (data & 1) == subType`), and
  `by45_p21_w` (set at machine reset, cleared by the video CPU's port B writes) forces
  bit 1 of port 2 on. rom2altsound sends as on BY45 (`cs_manCmd_w`: strobe 0 then 1).
- **Everything else**: as reported `BY45`: sweep `01`..`FF`, stop `00`.
- **Measured**: babypac 0 of 40; the game sent `00`/`0F` at boot (board-support).
- **Limits**: why no command starts a sound is not determined from the code; the strobe
  sense and the `p21` bit above are where to look, then the board's program.

## <a name="sndbrd_bytcs"></a>SNDBRD_BYTCS

Bally Turbo Cheap Squeak · interface `BYTCS` (`src/wpc/by35snd.c`) · ✅ · 12 sets, 5
games, 5 sound ROM ids, 1986-1987, Bally · e.g. Black Belt (`blackblt`), MotorDome
(`motrdome`), City Slicker (`cityslck`), Strange Science (`strngscc`)

- **Hardware**: M6809 at 2 MHz, 8 KiB of RAM (2 KiB on the second memory map, `byTCS2`),
  a PIA, a 10-bit DAC (`tcs_pia0a_w`, `tcs_pia0b_w`).
- **Commands**: two nibbles, one strobe on CA1; `tcs_pia0b_r` gives the low nibble then
  the high one. The fork's `tcs_manCmd_w` stores the whole byte and pulses the strobe.
- **Sound list**: `01`..`FF`.
- **Stop, boot and resets**: stop `00`. After a sound CPU reset the program runs a ROM
  and RAM self-test (about 5 s on cityslck) before it takes commands: the wait after a
  reset is 7 s (`REBOOT_SECS`; with 4 s the next command was swallowed).
- **Volume**: `full_scale (no volume stage)`.
- **Loops**: audio and sound CPU state (RAM `0000`-`1FFF` or `0000`-`07FF`).
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured** (how it works, Per family and Loops; README note 10): with `--max-secs 20
  --loop-max-secs 40`, motrdome 64 sounds (5 resets), cityslck 133 (7 blips, 10
  recovered by the retry, no reset); at the default search cityslck 7 of 10 musics looped
  (27.1 to 28.4 s), motrdome 0 of 5 (a byte of its state drifts up to four frames from
  one cycle to the next). Survey: blackblt 40 of 40, 38 files and 2 blips
  (board-support).

## <a name="sndbrd_bysd"></a>SNDBRD_BYSD

Bally Sounds Deluxe · interface `BYSD` (`src/wpc/by35snd.c`) · ⚠️ · 10 sets, 6 games, 6
sound ROM ids, 1986-1988, Bally · e.g. Special Force (`specforc`), Party Animal
(`prtyanim`), Heavy Metal Meltdown (`hvymetal`), Blackwater 100 (`black100`)

- **Hardware**: an MC68000 at 8 MHz, 4 KiB of RAM, a PIA, a 10-bit signed DAC
  (`MACHINE_DRIVER_START(bySD)`, `sd_pia0a_w`, `sd_pia0b_w`).
- **Commands**: two nibbles, strobe on CA1 (`sd_ctrl_w`), `sd_pia0b_r` giving the low
  nibble then the high one. PinMAME's `sd_man_w` latches the byte, pulses the strobe and
  hands both nibbles.
- **Sound list**: `01`..`FF` (the generic sweep).
- **Stop, boot and resets**: no stop known (not in `BUILTIN_STOPS`): the board is reset
  after every sound (68000 reset line), 4 s wait.
- **Volume**: none decoded: `none: recorded at the game's own volume`.
- **Loops**: audio method only: the probe reads only 8-bit audio CPUs
  (`shim_audio_cpu`), and the 68000's RAM is `MRA16_RAM`.
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured**: specforc 12 of 40 (board-support).
- **Limits**: 28 of the first 40 commands are silent; to check on a full sweep whether
  the board takes a two-byte command (board-support). Not determined from the code.
