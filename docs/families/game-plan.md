# Game Plan

The Game Plan sound boards: the CPU-less SN76477 boards `SNDBRD_GPSSU1`,
`SNDBRD_GPSSU2`, `SNDBRD_GPSSU3` and `SNDBRD_GPSSU4`, and the 6802 boards
`SNDBRD_GPMSU1` and `SNDBRD_GPMSU3`. The board code is in PinMAME's `src/wpc/gpsnd.c`,
the game side in `src/wpc/gp.c`. Since the version after 0.2.3, rom2altsound sweeps
only the nibbles the boards read, stops the SSU boards' tones, and sends the MSU boards'
commands framed as the games do (`src/extract.rs`, `sweep`, `BUILTIN_STOPS`,
`gpsm_framed`, `gpsm3_framed`); the rest is the [common method](common.md).

**How the game talks to these boards.** The sound lines are four solenoid lines
(`src/wpc/gp.c`, `GP_UpdateSolenoids`): on every write of the second solenoid bank
(solenoids 17 to 30) the game passes the byte to `sndbrd_0_data_w`; its low nibble is
the command, and `0F` (all four lines off) is the idle value the game returns to. The
SSU-1 is wired to the first bank instead, and only gets the values `00`, `01`, `05`,
`06` and the idle `0F`. Every board's `data_w` (also its `manCmd_w`) keeps the low
nibble only.

No interface has a sounds.dat section. The SSU boards' stops are in `BUILTIN_STOPS`
(`GPS1` and `GPS2`: `0F`, no tone; `GPS4`: `00`, which also stops the wave): the board
reset the tool used before does nothing on these CPU-less boards, so a tone a command
left on kept playing into the next file. The MSU boards (`GPSM`, `GPSM3`) have no stop
known: after every sound the tool resets the 6802 (`shim_reset_audio_cpus`) and waits
for 4 s of silence. No master volume is known for any of them (`volume::decode`): the
files are at the board's own level, not scaled, not AC-coupled.

## <a name="sndbrd_gpssu1"></a>SNDBRD_GPSSU1

Game Plan SSU-1 (one SN76477, no CPU) · PinMAME interface `GPS1` (`src/wpc/gpsnd.c`) ·
✅ · 3 sets, 3 games, no sound ROM, 1979, Game Plan · e.g. Star Trip (`startrip`),
Family Fun! (`famlyfun`), Vegas (`vegasgp`)

- **Hardware**: one SN76477 (`gpSS1_sn76477Int`), no sound CPU.
- **Commands**: `gpss1_data_w`: nibbles `0`, `1`, `5` and `6` set the VCO voltage of a
  chime-like tone and enable the chip; every other nibble (the idle `0F` among them)
  disables it. The tone sustains until then. rom2altsound sends one byte per command.
- **Sound list**: the nibbles `00`..`0E` (`sweep`, `"GPS1"`): 4 tones, the rest silent.
- **Stop, boot and resets**: `0F` (`BUILTIN_STOPS`), which turns the tone off.
- **Volume**: the chip's level.
- **Loops**: audio only (no sound CPU).
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured** ([board support](../board-support.md)): startrip 4 of 15, the four tones,
  all from silence, each held to the 5 s cap (17 of 40 before, 5 not from silence).
- **Limits and what is missing**: a tone is a state, held until the next nibble: the files
  are cut at `--max-secs` or at their loop.
- **In VPinball**: AltSound receives every write of the first solenoid bank that the
  driver forwards (`00`, `01`, `05`, `06`, `0F`), with no preprocessing for this
  generation. Not tested in VPinball.

## <a name="sndbrd_gpssu2"></a>SNDBRD_GPSSU2

Game Plan SSU-2 (three SN76477, no CPU) · PinMAME interface `GPS2` (`src/wpc/gpsnd.c`)
· ✅ · 1 set, 1 game, no sound ROM, 1979, Game Plan · e.g. Sharpshooter (`sshooter`)

- **Hardware**: three SN76477 (`gpSS2_sn76477Int`): one for the explosion, one for the
  effects, one for the tones; no sound CPU.
- **Commands**: `gpss2_data_w`: `07` gunshot, `08` rattlesnake, `0B` horse, `0C` howl,
  `0D` ricochet (second chip), `0E` explosion (first chip); nibbles `0`..`6`, `9` and `A`
  also set a tone on the third chip, which sustains until a nibble without a tone (the
  idle `0F` among them) turns it off.
- **Sound list**: the 15 nibbles `00`..`0E`.
- **Stop, boot and resets**: `0F` (`BUILTIN_STOPS`), which turns the tone off; the
  effects are one-shots.
- **Volume**: the chips' level.
- **Loops**: audio only.
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured** ([board support](../board-support.md)): sshooter 15 of 15, all from
  silence (38 of 40 before, 24 not from silence); 10 are held tones, cut at the 5 s cap.
- **Limits and what is missing**: the held tones (as GPSSU1).
- **In VPinball**: AltSound receives every write of the second solenoid bank (the
  command in the low nibble, `0F` between commands), no preprocessing. Not tested in
  VPinball.

## <a name="sndbrd_gpssu3"></a>SNDBRD_GPSSU3

Game Plan SSU-3 · PinMAME interface `GPS2` (`src/wpc/gpsnd.c`, sub-type 1) · ✅ ·
1 set, 1 game, no sound ROM, 1979, Game Plan · e.g. Coney Island! (`coneyis`)

- **Hardware and commands**: the SSU-2 code (`gpss2_data_w`); on this sub-type the
  rattlesnake (`08`) uses other mixer and SLF settings and `0C` alternates between howl
  and whoop.
- **Everything else**: as [SNDBRD_GPSSU2](#sndbrd_gpssu2).
- **Measured** ([board support](../board-support.md)): coneyis 15 of 15, all from silence,
  as sshooter.
- **Limits and what is missing**: as GPSSU2.
- **In VPinball**: as GPSSU2. Not tested in VPinball.

## <a name="sndbrd_gpssu4"></a>SNDBRD_GPSSU4

Game Plan SSU-4 (SN76477, no CPU) · PinMAME interface `GPS4` (`src/wpc/gpsnd.c`) · ✅ ·
1 set, 1 game, no sound ROM, 1982, Game Plan · e.g. Super Nova (`suprnova`)

- **Hardware**: SN76477 chips (`gpSS4_sn76477Int`) and a capacitor ramp simulated with
  a timer (`capTimer`); no sound CPU.
- **Commands**: `gpss4_data_w`: `00` stops the wave, `06` starts it, `07` twang, `08`
  spark, `0B` siren, `0C` howl, `0D` warble, `0E` explosion; nibbles `1`..`3` set a tone,
  which a nibble without a tone (`0F` among them) turns off.
- **Sound list**: the 15 nibbles `00`..`0E`.
- **Stop, boot and resets**: `00` (`BUILTIN_STOPS`): it stops the wave, mutes the effects
  chip and, having no tone voltage, turns the tone off.
- **Volume**: the chips' level.
- **Loops**: audio only.
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured** ([board support](../board-support.md)): suprnova 10 of 15, all from
  silence (29 of 40 before, 6 not from silence).
- **Limits and what is missing**: the held tones (as GPSSU1).
- **In VPinball**: as GPSSU2. Not tested in VPinball.

## <a name="sndbrd_gpmsu1"></a>SNDBRD_GPMSU1

Game Plan MSU-1 (6802 and MC6840) · PinMAME interface `GPSM` (`src/wpc/gpsnd.c`) · ✅ ·
7 sets, 7 games, 7 sound ROM ids, 1980-1985, Game Plan · e.g. Lizard (`lizard`),
Global Warfare (`gwarfare`), Attila The Hun (`attila`), Captain Hook (`cpthook`)

- **Hardware**: a 6802 (`MACHINE_DRIVER_START(gpMSU1)`, flagged `CPU_AUDIO_CPU`) with two
  PIAs and an MC6840 timer chip. PinMAME's MC6840 is partial: `m6840_w` plays a sine
  sample at the timers' frequencies through the mixer (`playsam1`; "to be implemented
  yet" in the machine driver), and PIA 0's port A sets the two voices' volumes
  (`pia0a_w`, `mixer_set_volume`).
- **Commands**: `gpsm_data_w` puts `F0 | nibble` on PIA 0's port B (`pia0b_r`); a timer
  toggles CB1 at about 828 Hz (`pia_cb1_w`), which interrupts the 6802 (`gps_irq`): the
  program polls the command on that interrupt, there is no strobe. `gpsm_ctrl_w` only
  logs. The game sends one nibble among a stream of idle `0F` (lizard's boot: 1425
  bytes, `08` once). **rom2altsound** sends each nibble between two `0F`
  (`gpsm_framed`); before, one byte per command, never back to `0F`, so the lines kept
  the last nibble and the program played on under every command.
- **Sound list**: the 15 nibbles `00`..`0E` (`sweep`, `"GPSM"`).
- **Stop, boot and resets**: no stop command: a 6802 reset after every sound.
- **Volume**: set by the board's program (`pia0a_w`), not by the tool: the board's own
  level.
- **Loops**: audio and sequencer-state (6802, `MRA_RAM` `0000`-`00FF`, `gps_readmem`).
  Not measured.
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured** ([board support](../board-support.md)): lizard 11 of 15, attila 15 of 15,
  all from silence (lizard 40 of 40 before, 37 not from silence, all at -14.9 LUFS).
- **Limits and what is missing**: the MC6840 emulation is incomplete in PinMAME, so the
  sounds are PinMAME's approximation; no stop known (a 6802 reset after each sound).
- **In VPinball**: AltSound receives every write of the second solenoid bank, no
  preprocessing. Not tested in VPinball.

## <a name="sndbrd_gpmsu3"></a>SNDBRD_GPMSU3

Game Plan MSU-3 (6802 and DAC) · PinMAME interface `GPSM3` (`src/wpc/gpsnd.c`) · ✅ ·
6 sets, 3 games, 2 sound ROM ids, 1985, Game Plan · e.g. Andromeda (`andromed`), Lady
Sharpshooter (`ladyshot`), Cyclopes (`cyclopes`)

- **Hardware**: a 6802 (`MACHINE_DRIVER_START(gpMSU3)`, flagged `CPU_AUDIO_CPU`), one PIA
  (`gps_pia[2]`) and a DAC at `3000`.
- **Commands**: `gpsm3_data_w`: as the MSU-1, `F0 | nibble` on the PIA's port B, read on
  the 828 Hz CB1 interrupt. The game's boot sends `0F 0C 00 0F`
  ([board support](../board-support.md)).
- **The program** (andromed's `850.snd`, interrupt handler at `FAE8`, read with a 6800
  disassembler): on every change of the lines it shifts the new nibble into a byte
  (`$01 = nibble << 4 | $01 >> 4`), and when the lines go back to `F` it plays the byte
  (`FB27`). A command is a byte, sent low nibble, high nibble, then `F`: the boot's
  `0F 0C 00 0F` is command `0C`.
- **What rom2altsound sends** (`gpsm3_framed`): `0F`, the low nibble, the high nibble,
  `0F`. A byte whose two nibbles are equal cannot be sent (the second makes no change),
  nor one with an `F` nibble.
- **Sound list**: the 210 bytes `00`..`EE` made of two different nibbles, neither `F`.
- **Stop, boot and resets**: a 6802 reset after every sound.
- **Volume**: the board's own level.
- **Loops**: audio and sequencer-state (6802, `MRA_RAM` `0000`-`007F`, `gps3_readmem`).
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured** ([board support](../board-support.md)): andromed 39 of 40, cyclopes 39 of
  40 (38 files, 1 blip), all from silence (0 of 40 before: single nibbles).
- **Limits and what is missing**: the bytes with two equal nibbles (if the game ever
  plays them, it is with a step the tool does not know); no stop known.
- **In VPinball**: as GPMSU1. Not tested in VPinball.
