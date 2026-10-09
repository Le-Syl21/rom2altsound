# Other makers' sound boards

The sound boards of the smaller makers: `SNDBRD_SPINB`, `SNDBRD_NUOVA`, `SNDBRD_MRGAME`,
`SNDBRD_JVH`, `SNDBRD_JVH2`, `SNDBRD_TABART`, `SNDBRD_TABART2`, `SNDBRD_TABART3`,
`SNDBRD_HANKIN`, `SNDBRD_GRAND`, `SNDBRD_JEUTEL`, `SNDBRD_BARNI`, `SNDBRD_TECHNO`,
`SNDBRD_TECNOPLAY`, `SNDBRD_JOCTRONIC` and `SNDBRD_ROWAMET`. Each has its own PinMAME
interface, and none of them is named anywhere in rom2altsound's code: all go through the
[common method](common.md) unchanged. That is, for every family below:

- **Sound list**: no sounds.dat section, so the raw sweep `01`..`FF` (`sweep`, default
  range), one byte per `sndbrd_manCmd` call every 4 frames, through the interface's manual
  handler (what that handler does is the family-specific part).
- **Stop**: the interface is not in `BUILTIN_STOPS` and has no sounds.dat family section:
  **the stop is a board reset** after every sound (`stop_sends` → `Send::Reset` →
  `reset_boards` → `shim_reset_audio_cpus`, the reset line of every `CPU_AUDIO_CPU`), then
  4 s of silence (`QUIET_AFTER_RESET_SECS`).
- **Volume**: no master volume command is known (`volume::decode` returns nothing;
  `none_reason`: "no known volume command for this board family"); recorded at the
  board's own level, not scaled; `reference_volume` "none: recorded at the game's own
  volume". No AC coupling by the tool (`ac_couples_dac` is only WPCS and System 11):
  where the board's DAC is unsigned, the WAVs keep its DC; the manifest's levels are
  DC-blocked.
- **Loops**: audio, and sequencer state where the audio CPU has an 8-bit bus and plain
  RAM in its read map (said per family).
- **DUCK / STOP / CHANNEL**: the defaults (DUCK 100, STOP 0, loops and unfinished sounds
  on the music channel).
- **Measured**: only the quick survey of [board support](../board-support.md) (one ROM,
  first 40 commands, 5 s each); no full run is recorded for any of these families.

## <a name="sndbrd_spinb"></a>SNDBRD_SPINB

Spinball / Inder sound board (two Z80, two MSM5205 or MSM6585) · PinMAME interface `SPINB`
(`src/wpc/spinb.c`) · status ⚠️ · 27 sets, 16 games, 14 sound ROM ids, 1985-1996, Inder
(Spain), Spinball (Spain) · e.g. Bushido (`bushido`), Mach 2, Jolly Park, Verne's World

Two hardware lines share the interface: Spinball's (`spinbgames.c`: bushido, mach2,
jolypark, vrnwrld, 7 sets), whose manual command is the interface's own
`spinb_sndCmd_w`, and Inder's (`indergames.c`, `bowlgames.c`: 20 sets, brvteam to
metalman), whose machine inits replace it (`sndbrd_setManCmd`: `snd_w`, `snd2_w`, in
`src/wpc/inder.c`). rom2altsound tells them apart at run time (shim.c `shim_spinb_own`)
and names the second `"INDER"` (`board_typestr`).

### Spinball (`"SPINB"`)

- **Hardware**: two Z80 sound CPUs at 5 MHz, one for the effects and one for the music
  (`spinbsnd1_readmem`, `spinbsnd2_readmem`, RAM `2000`-`3FFF` each, `MRA_RAM`), each
  feeding its own ADPCM chip from its sample ROMs: MSM5205 on Bushido and Mach 2
  (`SPINB_msm5205Int`), MSM6585 on Jolly Park and Verne's World (`SPINB_msm6585Int`); an
  8051 runs the DMD. A step volume (`digvol_w`, written by the music CPU at `A000`, 0 to
  142 steps, applied to both ADPCM chips; `SPINBlocals.volume` starts at 122).
- **Commands**: the game's Z80 writes the command byte to a latch at `6C20` or `CC20`
  (`soundbd_w`, in the game CPU's memory map): **not through `sndbrd_data_w`**, so nothing
  is logged at boot. Both sound CPUs poll that latch at `8000` (`sndcmd_r`; no interrupt,
  no strobe) and take a byte **only while its bit 7 is set** (read in the programs:
  bushido's effects loop at `006C`, the music's at `0093`, `AND 80`); the latch keeps the
  byte until the next write. The effects program plays `cmd & 7F`, the music program
  `cmd & 3F` (`0C` starts a music, `0F` stops it; `8F` and, inside a music, any `xF`
  interrupt it).
- **What rom2altsound sends** (`spinb_released`, in `board_sends`): every byte with bit 7
  set, followed by `00`, which releases the latch. A byte left in the latch with bit 7 set
  plays nothing (bushido: `81` alone silent, `81 00` plays): the program takes it, plays,
  and takes it again. Before (0.2.3), the sweep `01`..`FF` sent single bytes: `01`..`7F`
  reach neither program, the 0 of 40 of the survey.
- **Sound list**: `81`..`FF` without `8F` (the stop) and the step volume's `C3`, `C4`,
  `DE`, `DF` (none of which the effects programs take).
- **Stop**: `8F 00` (`BUILTIN_STOPS`, `SPINB_STOP`), the music stop; the effects end by
  themselves. No board reset in the survey runs.
- **Volume** (`spinb_level`): on the MSM6585 boards the music program steps the volume,
  `C3` one step up, `C4` one down (once per release of bit 7), `DF` locks it and `DE`
  unlocks it, and its reset adds 8 steps. The games' boot steps it down to 0 (traced on
  jolypark: 127 steps down from 2.95 s), which left every sound 30 to 40 dB down
  (jolypark -64 to -41 LUFS). Once booted, and after a board reset, the tool sends `DE`,
  142 steps down and 122 up: PinMAME's power-on level, the one Bushido and Mach 2 (no
  step volume) play at. The steps go out within a few frames (`Send::Pairs`, shim.c
  `shim_mancmd_pairs`, 30 timeslices after each byte: with 4, only 24 of 142 steps
  reached `digvol_w`). The level the game itself sets later (its volume setting) is not
  known.
- **Loops**: audio, sequencer state (both Z80s' RAM).
- **Measured** (survey settings, [board support](../board-support.md)): bushido 31 of 40,
  mach2 40 of 40, jolypark 40 of 40 (-32 to -8 LUFS), vrnwrld 40 of 40, all from
  silence, no board reset.
- **In VPinball**: **the pack cannot play**: the game writes its latch directly, never
  through `sndbrd_data_w`, so AltSound receives nothing.

### Inder (`"INDER"`)

- **Commands**: `snd2_w` stores the byte in a latch and pulses the sound CPU's NMI (the
  INDER2 machine: Lap By Lap...), `snd_w` only stores it (`inder.c`); on the MSM5205
  machines (INDERS1: Moon Light, Pin Clown, Corsario, Mundial, Atleta...) the Z80 polls
  the latch at `8000`, as Spinball's programs do (their successors): read in corsario's
  sound program (`a-corsar.bin`, poll at `009D`), a byte with bit 7 set is a command
  (`cmd & 3F`, `8F` stops), and **while bit 7 is clear the program plays command `0C`,
  its background music** (`00A8` → `0268`).
- **What rom2altsound sends**: the common method (one byte per command, `01`..`FF`), stop
  `00` (`BUILTIN_STOPS`, since the version after 0.2.3).
- **Measured** (survey settings): lapbylap 28 of 40, all from silence, no board reset (30
  of 40, 2 not from silence, 40 resets with the reset as the stop). moonlght, pinclown,
  corsario, atleta: 40 of 40 but all at one level, none from silence: the background
  music, under every command. canasta and brvteam: "no sound board on this machine" (their
  SN76489 is not started as a board).
- **Limits and what is missing**, tried on corsario: `8F` as the stop silences the
  background, and an effect sent as `8x 00` then plays from silence, but the `00` that
  releases the latch starts the background again under it; with `8F` as the release
  (`8x 8F`) the effect is cut at once, and a byte left with bit 7 set replays from its
  start (a stutter, -58 LUFS). The background is the game's own state between commands;
  separating it from the effects needs a release value that neither restarts nor
  interrupts, not found.
- **In VPinball**: not tested.

## <a name="sndbrd_nuova"></a>SNDBRD_NUOVA

Nuova Bell sound board (6803 + DAC) · PinMAME interface `NUOVA` (`src/wpc/nuova.c`) ·
status ✅ · 6 sets, 6 games, 6 sound ROM ids, 1986-1988, Nuova Bell Games · e.g. F1 Grand
Prix (`f1gp`), Skill Flight (`skflight`), Cobra (`cobra`), Top Pin (`toppin`)

- **Hardware**: Bally -35 game hardware (`MDRV_IMPORT_FROM(by35)`), a 6803 at 3.58/4 MHz
  (`snd_readmem`, RAM `0080`-`00FF`, `MRA_RAM`), an unsigned DAC on port 1 (`dac_w`,
  `DAC_0_data_w`), banked sample ROMs (`bank_w`); F1 Grand Prix has a second 6803, U-Boat
  65 an 8752 and a TMS5220.
- **Commands**: the game writes nibbles on the Bally lines (`by35.c`,
  `sndbrd_0_data_w(0, data & 0x0f)`) and the strobe with `sndbrd_0_ctrl_w`;
  `nuova_ctrl_w` pulses the 6803's timer input. The manual handler `nuova_man_w`
  ("needs two nybbles sent in perfect sync"): the strobe, the high nibble, 50
  timeslices, the low nibble. One byte = one command.
- **Stop, boot, volume**: as the common part; the DAC's DC stays in the WAVs.
- **Loops**: audio, sequencer state (6803 RAM).
- **Measured**: `f1gp`, 15 of 40, all from silence ([board support](../board-support.md)).
- **Limits**: only the quick survey.
- **In VPinball**: the game's writes reach AltSound 4 bits at a time (the Bally lines),
  not as the byte the pack is keyed by: as for the early Bally boards, the ids are not
  expected to match. Not tested in VPinball.

## <a name="sndbrd_mrgame"></a>SNDBRD_MRGAME

Mr. Game sound board (two Z80, DAC, M114S, TMS5220) · PinMAME interface `MRGAME`
(`src/wpc/mrgame.c`) · status ✅ · 5 sets, 4 games, 4 sound ROM ids, 1988-1990, Mr. Game
(Italy) · e.g. Dakar (`dakar`), Motor Show (`motrshow`), Mac Attack (`macattck`), World
Cup '90 (`wcup90`)

- **Hardware**: a 68000 game CPU; two Z80 sound CPUs at 4 MHz (RAM `FC00`-`FFFF`, and
  `7C00`-`7FFF` on the second CPU of generation 2, `MRA_RAM`), two DACs written through
  `DAC_DC_offset_correction_data_16_w` (PinMAME already AC-couples them), an M114S
  wavetable chip, a TMS5220 on generation 1.
- **Commands**: the 68000 writes the latch directly (`sound_w`, a 16-bit handler in its
  memory map): three writes per command, bit 7 set, cleared, set; the cleared-to-set
  transition fires the sound CPUs' NMI. **Not through `sndbrd_data_w`**: the interface has
  no data handler. The manual handler `mrgame_sndcmd` reproduces the three writes
  (`data | 0x80`, `data`, `data | 0x80`), so one byte of the sweep is one command (its low
  7 bits).
- **Sound list**: raw sweep `01`..`FF`: `80`..`FF` are the same commands as `00`..`7F`
  (bit 7 is the strobe): the twin test pairs them.
- **Loops**: audio, sequencer state (Z80 RAM).
- **Measured**: `dakar`, 26 of 40 ([board support](../board-support.md)).
- **Limits**: only the quick survey; the sweep could stop at `7F`.
- **In VPinball**: **the pack cannot play**: the game's writes go to `sound_w`, never
  through `sndbrd_data_w`, so AltSound receives nothing.

## <a name="sndbrd_jvh"></a>SNDBRD_JVH

Jac Van Ham sound board (6802 + AY-3-8912) · PinMAME interface `JVH` (`src/wpc/jvh.c`) ·
status ✅ · 3 sets, 3 games, 2 sound ROM ids, 1986-1987, Jac Van Ham (Royal) · e.g. Ice
Mania (`icemania`), Escape (`escape`), Movie Masters (`movmastr`)

- **Hardware**: a TMS9980 game CPU; a 6802 at 1 MHz (`snd_readmem`, RAM `0000`-`007F`,
  `MRA_RAM`) with a 6522 VIA and an AY-3-8912 written through the VIA (`jvh_via_a_w`).
- **Commands**: the game sets the command bit by bit (`snd_w`) and writes it with
  `sndbrd_0_data_w` when bit 5 is written. `jvhIntf`'s manual handler is the data handler
  `jvh_data_w`: only bits 0-5 are kept, inverted, put on the VIA's port A as a level, with
  no interrupt; a command that would read `00` is replaced by `FF` ("avoid passing in 0x00
  as a command because it stops all sound forever").
- **What rom2altsound sends** (`board_sends`, `"JVH"` sub-type 0): each command, then
  `3F`, which the handler turns into "no line" (`FF` on the port): the program acts on a
  change of the level. Before (0.2.3), one byte per command, the level left on.
- **Sound list**: `01`..`3E`, the six lines (`sweep`).
- **Loops**: audio, sequencer state (6802 RAM).
- **Measured** (survey settings, [board support](../board-support.md)): `icemania` 17 of
  40, `escape` 40 of 40, all from silence (icemania 1 of 40 before). The game's boot sends
  no byte, so the idle value was found by trying: `3F` and `00` after a command both work.
- **Limits and what is missing**: no stop command known (a board reset after each sound).
- **In VPinball**: the game writes its 6-bit command through `sndbrd_0_data_w`; not
  tested in VPinball.

## <a name="sndbrd_jvh2"></a>SNDBRD_JVH2

Jac Van Ham Formula 1 sound board (two 6809, YM2203, YM3812, DAC) · PinMAME interface
`JVH` (`src/wpc/jvh.c`, sub-type 1) · status ✅ · 1 set, 1 game, 1 sound ROM id, 1988, Jac
Van Ham (Royal) · e.g. Formula 1 (`formula1`)

- **Hardware**: a TMS9995 game CPU; two 6809 at 2 MHz (RAM `6000`-`63FF` and
  `0000`-`00FF`, `MRA_RAM`), a YM2203, a YM3812, a DAC (on the second VIA, whose port B
  sets a mixer volume, `vol2_w`).
- **Commands**: the game writes the byte with `sndbrd_0_data_w` (output `0x85` of its
  CRU map); `jvh_data_w` on sub-type 1 latches the whole byte and pulses the first 6809's
  IRQ. One byte = one command.
- **Loops**: audio, sequencer state (6809 RAM).
- **Measured**: `formula1`, 27 of 40, all from silence ([board support](../board-support.md)).
- **Limits**: only the quick survey.
- **In VPinball**: the game's byte is the byte the sweep sends; not tested in VPinball.

## <a name="sndbrd_tabart"></a>SNDBRD_TABART

Christian Tabart L'Hexagone board (Z80 + YM2203 + YM3526) · PinMAME interface `TABART`
(`src/wpc/tabart.c`) · status ⚠️ · 1 set, 1 game, 1 sound ROM id, 1986, Christian Tabart
(France) · e.g. L'Hexagone (`hexagone`)

- **Hardware**: Gottlieb System 1 game hardware (`gts1.c`); a Z80 at 3.58 MHz
  (`tabart1_readmem`, RAM `4000`-`407F`, `MRA_RAM`), a YM2203 (its ports read the
  command and the switch returns) and a YM3526.
- **Commands**: the game writes its sound lines with `sndbrd_0_data_w` (`gts1.c`;
  `tabart_data_w`: `data ^ 0xC7`) and the switch strobe with `sndbrd_0_ctrl_w`, which on
  strobe 1 raises the Z80's NMI. The manual handler `tabart_manCmd_w` (sub-type 0)
  **alternates**: one call stores the byte as the value read on YM2203 port B
  (`manCmd`), the next one sets the command byte and pulses the NMI (`toggle`).
- **Sound list**: raw sweep `01`..`FF`, one byte per command: with the alternation, every
  other command of the sweep only stores a byte, and the next one plays with the byte
  before it. The toggle is kept across board resets (`sndlocals` is only cleared in
  `tabart_init`).
- **Loops**: audio, sequencer state (Z80 RAM).
- **Measured**: `hexagone`, 31 of 40, 22 not from silence, 5 distinct levels, then "still
  not silent after 3 waits" ([board support](../board-support.md)).
- **Limits and what is missing**: tried after 0.2.3 and dropped: each command as a pair
  through the toggle (`00`, then the line state as `tabart_data_w` stores it, `data ^
  C7`), then the idle `10` the same way (hexagone's boot: `58` between `10`s): 8 of 40,
  all the same held sound at -19.4 LUFS, 105 board resets. The NMI the program reads the
  command in comes from the game's switch strobe (`tabart_ctrl_w`, strobe 1), which the
  halted game no longer makes; the command would need that strobe sequence. Not
  determined further.
- **In VPinball**: AltSound gets the game's line writes (`data` before the `^ 0xC7` of
  `tabart_data_w`), not the manual handler's pairs; not tested in VPinball.

## <a name="sndbrd_tabart2"></a>SNDBRD_TABART2

Christian Tabart Sahara Love board (Z80 + AY-3-8912) · PinMAME interface `TABART`
(`src/wpc/tabart.c`, sub-type 1) · status ✅ · 1 set, 1 game, 1 sound ROM id, 1984,
Christian Tabart (France) · e.g. Sahara Love (`sahalove`)

- **Hardware**: Gottlieb System 1 game hardware; a Z80 at 19.6608/8 MHz
  (`MACHINE_DRIVER_START(TABART2)`, RAM `4000`-`407F`, `MRA_RAM`), an AY-3-8912 whose port
  A reads the command (`ay8912a_r`); a periodic NMI at about 150 Hz.
- **Commands**: the game writes its lines with `sndbrd_0_data_w`; `tabart_data_w` builds
  the command from them and DIP bank 3 (sub-type 1). The manual handler (sub-type
  non-zero) only stores the byte as the command: no strobe, the program reads the latch
  at every NMI.
- **What rom2altsound sends** (`tabart_sends`): the game's path, `sndbrd_data_w` with the
  lines, then `00`, the idle lines (sahalove's boot: `48`, then `00`). Until 0.2.3 the
  manual command stored the byte as the command itself, unconverted, and nothing put the
  lines back to idle.
- **Sound list**: `01`..`0F` and `40`..`4F`, the lines `tabart_data_w` reads (bits 0-3
  and 6).
- **Stop, boot and resets**: board reset after each sound (the lines are already idle).
- **Loops**: audio, sequencer state (Z80 RAM).
- **Measured** (survey settings, [board support](../board-support.md)): `sahalove` 28 of
  31, all from silence, 17 levels (before: 40 of 40, 39 not from silence, all at -17.0
  LUFS).
- **Limits and what is missing**: no stop command known (the board reset is enough).
- **In VPinball**: AltSound gets the game's raw line writes, not the converted command;
  not tested in VPinball.

## <a name="sndbrd_tabart3"></a>SNDBRD_TABART3

Christian Tabart Le Grand 8 board (Z80 + AY-3-8912) · PinMAME interface `TABART`
(`src/wpc/tabart.c`, sub-type 2) · status ✅ · 1 set, 1 game, 1 sound ROM id, 1985,
Christian Tabart (France) · e.g. Le Grand 8 (`grand8`)

- **Hardware**: Gottlieb System 80 game hardware (`gts80games.c`, `INIT_S80(grand8, ...)`),
  the same Z80 + AY-3-8912 board as Sahara Love.
- **Commands**: as Sahara Love; `tabart_data_w` builds the command with DIP bank 4
  (sub-type 2) instead of 3.
- **What rom2altsound sends**, **sound list**, **stop**: as Sahara Love.
- **Measured**: `grand8` 26 of 31, all from silence (before: as sahalove, one tone)
  ([board support](../board-support.md)).
- **Limits and what is missing**: as Sahara Love.
- **In VPinball**: as Sahara Love; not tested.

## <a name="sndbrd_hankin"></a>SNDBRD_HANKIN

Hankin sound board (6802 + wavetable) · PinMAME interface `HNK` (`src/wpc/hnks.c`) ·
status ✅ · 5 sets, 5 games, 5 sound ROM ids, 1978-1981, Hankin · e.g. FJ Holden
(`fjholden`), Orbit 1 (`orbit1`), Howzat (`howzat`), The Empire Strikes Back (`empsback`)

- **Hardware**: a 6802 at 900 kHz (`hnks_readmem`, RAM `0000`-`007F`, `MRA_RAM`) with a
  6821 PIA; the sound is a 32-step waveform from a wavetable ROM, played by PinMAME's
  mixer at a rate set by the program (`start_samples`; the file header says the waveform
  and the 16000 Hz base frequency are guesses).
- **Commands**: the game (Bally -35 style CPU board, `by35.c`) writes a nibble with
  `sndbrd_0_data_w(0, data & 0x0f)` and the strobe with `sndbrd_0_ctrl_w` (PIA CA1). The
  manual handler `hnks_manCmd_w`: strobe low, the data, strobe high. Only PA0-PA3 are read
  (`hnks_data_w`: `data & 0x0f`): **16 commands**.
- **Sound list**: raw sweep `01`..`FF`: the 15 non-zero nibbles, each repeated for every
  high nibble (twins).
- **Loops**: audio, sequencer state (6802 RAM).
- **Measured**: `fjholden`, 38 of 40 ([board support](../board-support.md)).
- **Limits**: only the quick survey; the sweep could stop at `0F`.
- **In VPinball**: the game writes the nibble, which equals the pack's ids `01`..`0F`;
  not tested in VPinball.

## <a name="sndbrd_grand"></a>SNDBRD_GRAND

Grand Products 301/Bullseye board (6802 + DAC) · PinMAME interface `GRAND`
(`src/wpc/by35games.c`) · status ✅ · 4 sets, 1 game, 1 sound ROM id, 1986-2021, Grand
Products Inc. · e.g. 301/Bullseye (`bullseye`)

- **Hardware**: Bally -17 game hardware; a 6802 at 3.58/4 MHz (`gpsnd_readmem`, RAM
  `0000`-`007F`, `MRA_RAM`), a PIA whose port A is an unsigned DAC (`DAC_0_data_w`).
- **Commands**: the game writes a whole byte at `0080` with `sndbrd_0_data_w` (`by35.c`,
  `install_mem_write_handler(..., sndbrd_0_data_w)` for `SNDBRD_GRAND`); `grand_data_w`
  adds bit 7 from DIP 4 ("Sound mode": chimes or effects) and calls `grand_man_w`, which
  latches the byte and raises the 6802's IRQ. The manual handler is `grand_man_w` itself:
  the sweep's bit 7 replaces the DIP's.
- **Loops**: audio, sequencer state (6802 RAM).
- **Measured**: `bullseye`, 39 of 40 ([board support](../board-support.md)).
- **Limits**: only the quick survey.
- **In VPinball**: AltSound gets the game's byte before the DIP bit is added: with the
  DIP off, the same as the pack's ids `01`..`7F`. Not tested in VPinball.

## <a name="sndbrd_jeutel"></a>SNDBRD_JEUTEL

Jeutel sound board (Z80 + AY-3-8910 + TMS5110) · PinMAME interface `JEUTEL`
(`src/wpc/jeutel.c`) · status ✅ · 3 sets, 3 games, 2 sound ROM ids, 1983-1984, Jeutel ·
e.g. Le King (`leking`), Olympic Games (`olympic`), Papillon (`jpapillon`)

- **Hardware**: two Z80 game CPUs; a Z80 sound CPU at 4 MHz (RAM `4000`-`43FF`,
  `MRA_RAM`), an AY-3-8910 and a TMS5110 speech chip.
- **Commands**: the game writes the byte on a PPI port with `sndbrd_data_w`
  (`ppi2_portc_w`) and the control lines with `sndbrd_ctrl_w` (`ppi2_porta_w`, bits 6-7):
  `jeutel_ctrl_w` resets the sound CPU on bit 0 low and drives its NMI with bit 1. The
  manual handler `jeutel_manCmd_w`: the byte, then control `3` (NMI on), then `1` (NMI
  off).
- **Loops**: audio, sequencer state (Z80 RAM).
- **Measured**: `leking`, 40 of 40 ([board support](../board-support.md)).
- **Limits**: only the quick survey.
- **In VPinball**: AltSound gets every write of the PPI port; whether the game writes one
  byte per sound is not determined. Not tested in VPinball.

## <a name="sndbrd_barni"></a>SNDBRD_BARNI

Barni sound board (6802 + TMS5220 + DAC) · PinMAME interface `BARNI` (`src/wpc/barni.c`)
· status ✅ · 2 sets, 2 games, 2 sound ROM ids, 1985, Barni · e.g. Red Baron (`redbaron`),
Champion (`champion`)

- **Hardware**: two 6809 game CPUs; a 6802 at 3.58/4 MHz (`snd_readmem`, RAM
  `0000`-`007F`, `MRA_RAM`), two PIAs, a TMS5220 (as 5220C) and a DAC.
- **Commands**: the game writes the inverted byte with `sndbrd_0_data_w(0, ~data)`.
  `barniIntf`'s manual handler is the data handler `snd_data_w`: the byte in the latch
  (read at `2000`), CB1 pulsed high then low on the first PIA. One byte = one command.
- **Loops**: audio, sequencer state (6802 RAM).
- **Measured**: `redbaron`, 26 of 40, all from silence ([board support](../board-support.md)).
- **Limits**: only the quick survey.
- **In VPinball**: AltSound gets the byte as the board receives it (already inverted), the
  same byte the sweep sends: the ids should match. Not tested in VPinball.

## <a name="sndbrd_techno"></a>SNDBRD_TECHNO

Tecnoplay Scramble board (two 6502 + TMS7000, System 80B-like) · PinMAME interface
`TECHNO` (`src/wpc/gts80s.c`, `technoIntf`) · status ✅ · 1 set, 1 game, 1 sound ROM id,
1987, Tecnoplay · e.g. Scramble (`scram_tp`)

- **Hardware**: a Zaccaria-type game CPU (S2650, `zac.c`); the board copies Gottlieb's
  System 80B generation 1 sound board (two 6502, RAM `0000`-`07FF`, `MRA_RAM`; AY-3-8913s,
  an SP0250 speech chip, a DAC) and adds a TMS7000 with its own DAC (comment above
  `techno_locals`; Scramble uses Raven's D and Y ROMs).
- **Commands**: the game writes the byte on its S2650 data port with `sndbrd_0_data_w`
  (`zac.c`, `data_port_w`). `tsns_data_w` (data and manual handler) inverts it ("Data is inverted
  from main cpu") and calls `techno_sh_w`: the latch, both 6502s' IRQ, and the TMS7000's
  IRQ1 when bit 6 is clear. Bit 7 is the game's strobe, sent set then cleared; the
  handler does not filter it ("Doesn't seem to matter much").
- **Loops**: audio, sequencer state (6502 RAM).
- **Measured**: `scram_tp`, 37 of 40 ([board support](../board-support.md)).
- **Limits**: only the quick survey; a byte and the same byte with bit 7 changed may be
  the same command.
- **In VPinball**: AltSound gets the game's two writes per command (with and without the
  strobe); not tested in VPinball.

## <a name="sndbrd_tecnoplay"></a>SNDBRD_TECNOPLAY

Tecnoplay X Force board (TMS7000 + Y8950 + DAC) · PinMAME interface `TECNOPLAY`
(`src/wpc/techno.c`) · status ❌ · 2 sets, 2 games, 1 sound ROM id, 1987-1988, Tecnoplay ·
e.g. X Force (`xforce`), Space Team (`spcteam`)

- **Hardware**: a 68000 game CPU; a TMS7000 at 4 MHz (`MACHINE_DRIVER_START(tecno)`), a
  Y8950 (OPL with ADPCM, samples in `REGION_USER1`) and a DAC.
- **Commands**: the game's 68000 writes a 16-bit word to `016000` (`sound_w`): D0-D7 the
  sound data, D8 the strobe, D9 a reset, D10 the display data clock, D11-D15 auxiliary
  outputs. `sound_w` passes the low byte of **every** write to `sndbrd_data_w`, display
  clocking included: hence the 76978 bytes logged during the survey's boot. The board's
  handler `tecsnd_data_w` (data and manual): a non-zero byte is latched and **asserts**
  the TMS7000's IRQ3; a zero byte clears it. The strobe bit D8 never reaches the board.
- **Sound list**: raw sweep `01`..`FF`: each command asserts IRQ3 and nothing clears it
  until a `00`, which the sweep never sends (the stop is a reset).
- **Loops**: audio only: the TMS7000's read map (`snd_readmem`) has no `MRA_RAM` range
  (its RAM is internal), so there is no state to read.
- **Measured**: `xforce`, 0 of 40 ([board support](../board-support.md)).
- **Limits and what is missing**: tried after 0.2.3: each command followed by `00`
  (`--only 0x0100,0x0200,...`, which releases IRQ3 as the game's next write does) plays
  nothing either, and the game's boot writes only `00` (all 76923 bytes: its display
  clocking), so there is no sound request to copy; the boot plays no sound. Whether the
  program needs the game's D8 strobe, which PinMAME's `sound_w` drops, or something else,
  is not determined: the TMS7000 program was not read (no disassembler at hand).
- **In VPinball**: AltSound gets every low byte the game writes to that port, display
  clocking included: the stream does not isolate the sound commands. Not tested in
  VPinball.

## <a name="sndbrd_joctronic"></a>SNDBRD_JOCTRONIC

Joctronic sound board (Z80 + AY-3-8910, DAC; YM2203 on some) · PinMAME interface
`JOCTRONIC` (`src/wpc/joctronic.c`) · status ❌ · 3 sets, 3 games, 3 sound ROM ids, 1986,
Joctronic · e.g. Punky Willy (`punkywil`), Walkyria (`walkyria`), Pin Ball (`jpinball`)

- **Hardware**: a Z80 game CPU; a Z80 sound CPU at 6 MHz (`snd_readmem`, RAM
  `8000`-`87FF`, `MRA_RAM`, the command at `C000`); `joctronicS1`: two AY-3-8910 whose
  ports feed two DACs; `joctronicS2`: an AY-3-8910, a YM2203 and the DACs.
- **Commands**: the game writes the byte at `E000` (`sndbrd_0_data_w` in its memory map).
  `snd_data_w` (data and manual): latch, then an NMI pulse to the sound CPU. One byte =
  one command, as far as the handler shows.
- **Loops**: audio, sequencer state (Z80 RAM).
- **Measured**: `punkywil`, 0 of 40; the game sent one byte (`01`) at boot
  ([board support](../board-support.md)).
- **Limits and what is missing**: the handler is a plain latch and NMI, and the NMI part
  works: punkywil's sound program (`pw_sound.bin`, read with a Z80 disassembler) queues
  every non-zero byte in a ring at `8010` (NMI handler at `0066`). Its main loop, though,
  only moves on each time the IRQ handler (`RST 38` → `003E`) has counted `28` interrupts
  at `8002`, and the IRQ comes from the main CPU's CTC channel 0 (`joctronic.c` `to0_w`,
  `cpu_set_irq_line(1, ...)`). The boot plays no sound either (0.0 s of sound, one click),
  with the game CPU running: the queued commands are never played in PinMAME, whose CTC
  output to the sound CPU does not seem to run. Not fixable from rom2altsound's side
  without inventing that interrupt; left as is.
- **In VPinball**: the game's byte is the byte the sweep sends; not tested in VPinball.

## <a name="sndbrd_rowamet"></a>SNDBRD_ROWAMET

Rowamet sound board (Z80 + DAC) · PinMAME interface `ROWAMET` (`src/wpc/rowamet.c`) ·
status ✅ · 1 set, 1 game, 1 sound ROM id, year unknown, Rowamet · e.g. Heavy Metal
(`heavymtl`)

- **Hardware**: Taito game hardware with a Z80 (`MDRV_IMPORT_FROM(taito)`); a Z80 sound
  CPU at 1.89 MHz (`snd_readmem`, RAM `1000`-`17FF`, `MRA_RAM`), a DAC on port 1.
- **Commands**: the game writes through Taito's `taito_sndCmd_w` (a byte assembled from
  two output nibbles, sent when it changes, see [Taito](taito.md)). `rowamet_data_w`
  (data and manual): latch, mixer volume back to 100, NMI pulse.
- **Loops**: audio, sequencer state (Z80 RAM).
- **Stop**: `00` (`BUILTIN_STOPS`), what the game sends after each command (heavymtl's
  boot: `12 00 12 00`...). Until 0.2.3 a board reset, which left the latch on the last
  command.
- **Measured**: `heavymtl` 38 of 40, all from silence, no board reset (5 of 40 before,
  [board support](../board-support.md)).
- **Limits and what is missing**: no full run.
- **In VPinball**: AltSound gets the game's changed bytes; not tested in VPinball.
