# Game Plan

The Game Plan sound boards: the CPU-less SN76477 boards `SNDBRD_GPSSU1`,
`SNDBRD_GPSSU2`, `SNDBRD_GPSSU3` and `SNDBRD_GPSSU4`, and the 6802 boards
`SNDBRD_GPMSU1` and `SNDBRD_GPMSU3`. The board code is in PinMAME's `src/wpc/gpsnd.c`,
the game side in `src/wpc/gp.c`. rom2altsound has no code of its own for any of them:
they go through the [common method](common.md) (raw sweep `01`..`FF`, stop = board
reset, the board's own level, default pack columns).

**How the game talks to these boards.** The sound lines are four solenoid lines
(`src/wpc/gp.c`, `GP_UpdateSolenoids`): on every write of the second solenoid bank
(solenoids 17 to 30) the game passes the byte to `sndbrd_0_data_w`; its low nibble is
the command, and `0F` (all four lines off) is the idle value the game returns to. The
SSU-1 is wired to the first bank instead, and only gets the values `00`, `01`, `05`,
`06` and the idle `0F`. Every board's `data_w` (also its `manCmd_w`) keeps the low
nibble only.

None of the interfaces (`GPS1`, `GPS2`, `GPS4`, `GPSM`, `GPSM3`) has a stop in
`BUILTIN_STOPS` or a sounds.dat section (`src/extract.rs`, `stop_sends`): after every
sound the tool resets the audio CPUs (`shim_reset_audio_cpus`) and waits for 4 s of
silence. The SSU boards have no CPU, so that reset does nothing: a tone a command left
on keeps playing until the wait for silence times out (4 s plus 10 s), and the next file is flagged
`clean_start: false`. No master volume is known for any of them (`volume::decode`): the
files are at the board's own level, not scaled, not AC-coupled.

## <a name="sndbrd_gpssu1"></a>SNDBRD_GPSSU1

Game Plan SSU-1 (one SN76477, no CPU) · PinMAME interface `GPS1` (`src/wpc/gpsnd.c`) ·
⚠️ · 3 sets, 3 games, no sound ROM, 1979, Game Plan · e.g. Star Trip (`startrip`),
Family Fun! (`famlyfun`), Vegas (`vegasgp`)

- **Hardware**: one SN76477 (`gpSS1_sn76477Int`), no sound CPU.
- **Commands**: `gpss1_data_w`: nibbles `0`, `1`, `5` and `6` set the VCO voltage of a
  chime-like tone and enable the chip; every other nibble (the idle `0F` among them)
  disables it. The tone sustains until then. rom2altsound sends one byte per command.
- **Sound list**: raw sweep `01`..`FF`; only the low nibble counts, so 4 tones (nibble
  `0` from `10`, `20`...), the rest silent or repeats.
- **Stop, boot and resets**: no stop command and nothing to reset: a tone left on plays
  through the wait for silence.
- **Volume**: the chip's level.
- **Loops**: audio only (no sound CPU).
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured** ([board support](../board-support.md)): startrip 17 of 40 (11 files,
  6 blips), 4 distinct levels, 5 not from silence.
- **Limits and what is missing**: the stop. Cheapest fix ([board
  support](../board-support.md#cheapest-fixes), item 6): `0F`, the idle value, which
  turns the tone off in `gpss1_data_w`, as the stop. Not tried.
- **In VPinball**: AltSound receives every write of the first solenoid bank that the
  driver forwards (`00`, `01`, `05`, `06`, `0F`), with no preprocessing for this
  generation. Not tested in VPinball.

## <a name="sndbrd_gpssu2"></a>SNDBRD_GPSSU2

Game Plan SSU-2 (three SN76477, no CPU) · PinMAME interface `GPS2` (`src/wpc/gpsnd.c`)
· ⚠️ · 1 set, 1 game, no sound ROM, 1979, Game Plan · e.g. Sharpshooter (`sshooter`)

- **Hardware**: three SN76477 (`gpSS2_sn76477Int`): one for the explosion, one for the
  effects, one for the tones; no sound CPU.
- **Commands**: `gpss2_data_w`: `07` gunshot, `08` rattlesnake, `0B` horse, `0C` howl,
  `0D` ricochet (second chip), `0E` explosion (first chip); nibbles `0`..`6`, `9` and `A`
  also set a tone on the third chip, which sustains until a nibble without a tone (the
  idle `0F` among them) turns it off.
- **Sound list**: raw sweep `01`..`FF`; 15 distinct nibbles, the rest repeats.
- **Stop, boot and resets**: no stop command and nothing to reset.
- **Volume**: the chips' level.
- **Loops**: audio only.
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured** ([board support](../board-support.md)): sshooter 38 of 40, 24 of them not
  from silence (a tone left on between commands).
- **Limits and what is missing**: the stop; cheapest fix: `0F` as the stop (it turns the
  tone off in `gpss2_data_w`). Not tried.
- **In VPinball**: AltSound receives every write of the second solenoid bank (the
  command in the low nibble, `0F` between commands), no preprocessing. Not tested in
  VPinball.

## <a name="sndbrd_gpssu3"></a>SNDBRD_GPSSU3

Game Plan SSU-3 · PinMAME interface `GPS2` (`src/wpc/gpsnd.c`, sub-type 1) · ⚠️ ·
1 set, 1 game, no sound ROM, 1979, Game Plan · e.g. Coney Island! (`coneyis`)

- **Hardware and commands**: the SSU-2 code (`gpss2_data_w`); on this sub-type the
  rattlesnake (`08`) uses other mixer and SLF settings and `0C` alternates between howl
  and whoop.
- **Everything else**: as [SNDBRD_GPSSU2](#sndbrd_gpssu2).
- **Measured** ([board support](../board-support.md)): same result as sshooter.
- **Limits and what is missing**: as GPSSU2.
- **In VPinball**: as GPSSU2. Not tested in VPinball.

## <a name="sndbrd_gpssu4"></a>SNDBRD_GPSSU4

Game Plan SSU-4 (SN76477, no CPU) · PinMAME interface `GPS4` (`src/wpc/gpsnd.c`) · ⚠️ ·
1 set, 1 game, no sound ROM, 1982, Game Plan · e.g. Super Nova (`suprnova`)

- **Hardware**: SN76477 chips (`gpSS4_sn76477Int`) and a capacitor ramp simulated with
  a timer (`capTimer`); no sound CPU.
- **Commands**: `gpss4_data_w`: `00` stops the wave, `06` starts it, `07` twang, `08`
  spark, `0B` siren, `0C` howl, `0D` warble, `0E` explosion; nibbles `1`..`3` set a tone,
  which a nibble without a tone (`0F` among them) turns off.
- **Sound list**: raw sweep `01`..`FF`; 15 distinct nibbles.
- **Stop, boot and resets**: no stop command and nothing to reset.
- **Volume**: the chips' level.
- **Loops**: audio only.
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured** ([board support](../board-support.md)): suprnova 29 of 40 (26 files,
  3 blips), 6 of them not from silence: partial, as GPSSU1..3.
- **Limits and what is missing**: the 6 files not from silence come from the same
  missing stop; `0F` (and `00` for the wave) as the stop would be the fix. Not tried.
- **In VPinball**: as GPSSU2. Not tested in VPinball.

## <a name="sndbrd_gpmsu1"></a>SNDBRD_GPMSU1

Game Plan MSU-1 (6802 and MC6840) · PinMAME interface `GPSM` (`src/wpc/gpsnd.c`) · ⚠️ ·
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
  logs. rom2altsound sends one byte per command and never sends the idle `0F` in
  between.
- **Sound list**: raw sweep `01`..`FF`; only the low nibble counts (15 distinct inputs).
- **Stop, boot and resets**: no stop command: a 6802 reset after every sound.
- **Volume**: set by the board's program (`pia0a_w`), not by the tool: the board's own
  level.
- **Loops**: audio and sequencer-state (6802, `MRA_RAM` `0000`-`00FF`, `gps_readmem`).
  Not measured.
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured** ([board support](../board-support.md)): lizard 40 of 40 but doubtful: 37
  do not start from silence and all are at -14.9 LUFS (one tone that never stops).
- **Limits and what is missing**: what plays under every command is not determined from
  the code; the reset does not stop it. Cheapest fix to try ([board
  support](../board-support.md#cheapest-fixes), item 6): `0F`, the game's idle value, as
  the stop. The MC6840 emulation itself is incomplete in PinMAME, so even then the
  sounds are PinMAME's approximation.
- **In VPinball**: AltSound receives every write of the second solenoid bank, no
  preprocessing. Not tested in VPinball.

## <a name="sndbrd_gpmsu3"></a>SNDBRD_GPMSU3

Game Plan MSU-3 (6802 and DAC) · PinMAME interface `GPSM3` (`src/wpc/gpsnd.c`) · ❌ ·
6 sets, 3 games, 2 sound ROM ids, 1985, Game Plan · e.g. Andromeda (`andromed`), Lady
Sharpshooter (`ladyshot`), Cyclopes (`cyclopes`)

- **Hardware**: a 6802 (`MACHINE_DRIVER_START(gpMSU3)`, flagged `CPU_AUDIO_CPU`), one PIA
  (`gps_pia[2]`) and a DAC at `3000`.
- **Commands**: `gpsm3_data_w`: as the MSU-1, `F0 | nibble` on the PIA's port B, read on
  the 828 Hz CB1 interrupt. The game's boot sends `0F 0C 00 0F`
  ([board support](../board-support.md)).
- **Sound list**: raw sweep `01`..`FF` (15 distinct nibbles).
- **Stop, boot and resets**: a 6802 reset after every sound.
- **Volume**: the board's own level.
- **Loops**: audio and sequencer-state (6802, `MRA_RAM` `0000`-`007F`, `gps3_readmem`).
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured** ([board support](../board-support.md)): andromed 0 of 40.
- **Limits and what is missing**: why no command plays is not determined from the code:
  PinMAME hands the nibble to the program the same way the game's writes do. The boot's
  `0F 0C 00 0F` suggests the program acts on changes from the idle `0F`; sending `0F`
  before each command (as the stop) is the first thing to try. Not tried.
- **In VPinball**: as GPMSU1. Not tested in VPinball.
