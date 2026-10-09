# Playmatic and E.F.O. sound boards

`SNDBRD_PLAY1`, `SNDBRD_PLAY2`, `SNDBRD_PLAY3`, `SNDBRD_PLAY4`, `SNDBRD_PLAYZ` (Playmatic,
`src/wpc/playsnd.c`, driven from `src/wpc/play.c`) and `SNDBRD_ZSU` (E.F.O.'s ZSU board,
used by Playmatic's last games and by E.F.O./Maibesa, `src/wpc/efosnd.c`). Each is its own
PinMAME interface. rom2altsound drives all of them with the [common method](common.md):
none has a sounds.dat section, none is named in `sweep`, `BUILTIN_STOPS` or
`volume::full_scale`, so each gets the raw sweep `01`..`FF` through `sndbrd_manCmd`, a
**board reset as the stop** (`shim_reset_audio_cpus`), no volume command (recorded at the
board's level, not scaled; DAC/mixer output not AC-coupled by the tool), and the default
pack columns. What the manual command handler does, and so what the sweep really
exercises, differs per board.

## <a name="sndbrd_play1"></a>SNDBRD_PLAY1

Playmatic tone board (four discrete tones, no sound CPU) · PinMAME interface `PLAY1`
(`src/wpc/playsnd.c`) · status ✅ · 7 sets, 7 games, 0 sound ROM ids, 1978-1979,
Playmatic, Sonic (Spain) · e.g. Space Gambler (`spcgambl`), Big Town (`bigtown`), Last
Lap (`lastlap`), Night Fever (`ngtfever`)

- **Hardware**: no sound CPU and no sound ROM. PinMAME models four triangle waves (C, E,
  G, B notes) in its discrete sound system (`DISCRETE_SOUND_START(play_tones)`), enabled
  by the low four bits of the command; a timer (`play1s_timer_callback`) fades the mixer
  volume down by 1 % every 5 ms.
- **Commands**: the game writes the byte of its "sound & player up lights" output with
  `sndbrd_0_data_w` (`play.c`, `out1_n`, digit select 1); bits 5-7 also drive the
  player-up lamps. `play1sIntf`'s manual handler is the data handler `play1s_data_w`:
  bits 0-3 switch the tones on, bit 4 clear starts a fade, set holds the tone; a byte
  whose low nibble is 0 stops the tones, unless a fade is running.
- **Sound list**: raw sweep `01`..`FF`: 16 tone combinations x fade/no fade, repeated for
  each value of bits 5-7 (which the board ignores): the twin test pairs the repeats.
- **Stop, boot and resets**: the stop is a board reset, but the machine has no audio CPU
  (the only CPU is the game's CDP1802), so `shim_reset_audio_cpus` resets nothing and a
  held tone (bit 4 set) keeps sounding. The board's own stop is a byte with low nibble
  `0` (`play1s_data_w`, else branch).
- **Volume**: the mixer level the fade leaves; full at the start of each new command
  (`sndlocals.volume = 100`).
- **Loops**: audio only (no sound CPU, no state to read); a held tone is a steady
  waveform that the audio method can find.
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured**: `spcgambl`, 39 of 40, 15 not from silence: a tone held between commands
  ([board support](../board-support.md)).
- **Limits and what is missing**: no stop. The cheapest fix, from `play1s_data_w`: `00`
  (or any byte with low nibble 0) as the `PLAY1` stop in `BUILTIN_STOPS`; a fade in
  progress still ends by itself (0.5 s). The sweep could also stop at `1F`.
- **In VPinball**: AltSound gets the game's byte including the lamp bits 5-7, so the ids
  of a sound can differ from the sweep's in those bits. Not tested in VPinball.

## <a name="sndbrd_play2"></a>SNDBRD_PLAY2

Playmatic square-wave board (no sound CPU) · PinMAME interface `PLAY2`
(`src/wpc/playsnd.c`) · status ✅ · 6 sets, 5 games, 0 sound ROM ids, 1979-1980,
Playmatic, Sonic (Spain) · e.g. Antar (`antar`), Storm (`storm`), Evil Fight
(`evlfight`), Black Fever (`blkfever`)

- **Hardware**: no sound CPU; PinMAME plays a 32-step square wave sample
  (`squareWave`, custom sound `play2s_custInt`) at `2950000 / 4 / (freq + 1)` Hz.
- **Commands**: the game writes the frequency byte with `sndbrd_0_data_w` (`play.c`,
  `out2_n`, `SOUND` port) and the enable with `sndbrd_0_ctrl_w` (bit 7 of the display
  column port); enable on starts the tone at full volume, enable off starts a fade at
  120 Hz steps (`play2s_ctrl_w`). The manual handler `play2s_man_w` sets the frequency,
  then the enable on and off at once: every command is a tone that fades out by itself
  (100 steps at 120 Hz, about 0.83 s).
- **Sound list**: raw sweep `01`..`FF`: 255 pitches of the same tone.
- **Stop, boot and resets**: the reset resets nothing (no audio CPU); the tones end by
  themselves.
- **Volume**: the board's only level. **Loops**: none expected (each command fades).
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured**: `antar`, 40 of 40, tones ([board support](../board-support.md)).
- **Limits**: the files are pitches, not the game's sounds: the game shapes its sounds by
  changing the frequency and the enable over time, which a single command does not
  reproduce.
- **In VPinball**: AltSound gets the frequency bytes (the enable goes through
  `sndbrd_ctrl_w`, which is not logged). Not tested in VPinball.

## <a name="sndbrd_play3"></a>SNDBRD_PLAY3

E.F.O. Sound-3 (CDP1802 + TMS5200 speech) · PinMAME interface `PLAY3`
(`src/wpc/playsnd.c`) · status ✅ · 2 sets, 2 games, 2 sound ROM ids, 1982, Playmatic ·
e.g. Cerberus (`cerberus`), Spain 82 (`spain82`)

- **Hardware**: a CDP1802 at 2.95 MHz (`MACHINE_DRIVER_START(PLAYMATICS3)`, RAM
  `2000`-`201F`, `MRA_RAM`), a TMS5200 (`play3s_5220Int`, `TMS5220_IS_5200`); the command
  is read on port 2 (`in_snd_3`), the strobe on EF2 (`play3s_ctrl_w`).
- **Commands**: the game writes the command and the strobe from its lamp port (`play.c`,
  `out2_n`: `(data & 0x70) >> 4` then `!enX` on older CPUs; on Spain 82 the `m8020_w`
  "hack to make sound work"). The manual handler `play3s_man_w` is game-specific: on
  `cerberus` it sends the high nibble, 0, the low nibble, 0, with timeslices between
  (`delay`), then the strobe; elsewhere the byte with the strobe low then high.
- **Sound list**: raw sweep `01`..`FF`.
- **Stop, boot and resets**: board reset (the CDP1802's reset line).
- **Volume**: the board's only level. **Loops**: audio, sequencer state (CDP1802 RAM).
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured**: `cerberus`, 19 of 40 ([board support](../board-support.md)).
- **Limits**: only the quick survey.
- **In VPinball**: the game writes 3-bit values from its lamp port (`(data & 0x70) >> 4`
  on the older CPU boards) or its lamp column (the other branch of `out2_n`), not the
  bytes `play3s_man_w` splits into nibbles on Cerberus: the ids are not expected to match.
  Not tested in VPinball.

## <a name="sndbrd_play4"></a>SNDBRD_PLAY4

E.F.O. Sound IV (CDP1802 + 2 AY-3-8910) · PinMAME interface `PLAY4`
(`src/wpc/playsnd.c`) · status ✅ · 16 sets, 13 games, 13 sound ROM ids, 1984-1987,
Playmatic, JocMatic · e.g. Meg-Aaton (`megaaton`), Nautilus (`nautilus`), Mad Race
(`madrace`), Star Fire (`starfire`)

- **Hardware**: a CDP1802 at 3.58 MHz (`MACHINE_DRIVER_START(PLAYMATICS4)`, RAM
  `8000`-`80FF`, `MRA_RAM`), two AY-3-8910 in stereo; each AY's port A sets its volume in
  four steps (`ay8910_0_porta_w`: `100 - 25 * (data >> 6)`), a programmable interrupt
  clock (`clk_snd`).
- **Commands**: the game writes its lamp column byte with `sndbrd_0_data_w` and the
  enable with `sndbrd_0_ctrl_w` (`play.c`, `out2_n`, when ENSN is low). The board reads the
  command on port 2 (`in_snd_4`, which sets the EF4 flag the program polls). The manual
  handler `play4s_man_w`: the byte, then the enable at 0.
- **Sound list**: raw sweep `01`..`FF`.
- **Stop, boot and resets**: board reset.
- **Volume**: the board's AY volumes are set by its own program; no master volume command
  is known.
- **Loops**: audio, sequencer state (CDP1802 RAM).
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured**: `madrace`, 30 of 40 ([board support](../board-support.md)).
- **Limits**: only the quick survey.
- **In VPinball**: the game's data byte is the one the sweep sends; not tested in VPinball.

## <a name="sndbrd_playz"></a>SNDBRD_PLAYZ

Playmatic Zira board (COP420 + AY-3-8910) · PinMAME interface `PLAYZ`
(`src/wpc/playsnd.c`) · status ❌ · 1 set, 1 game, 1 sound ROM id, 1981, Playmatic · e.g.
Zira (`zira`)

- **Hardware**: a National COP420 microcontroller at 2.01216 MHz / 16
  (`MACHINE_DRIVER_START(PLAYMATICSZ)`), one AY-3-8910 (whose ports also drive two lamp
  rows, `ay8910_z_porta_w`/`portb_w`), 2 KB of ROM banked by port D (`romsel_w`).
- **Commands**: the game writes bits 4-6 of its lamp output to the board with
  **`sndbrd_0_ctrl_w`** (`play.c`, `out2_n`: `data & 0x70`); `playzsIntf` has no data
  handler (`NULL`), and its manual handler is the control handler `playzs_ctrl_w`. The
  COP420 reads `(~cmd >> 4) & 7` on its IN port (`in_snd_z`): **8 commands**, inverted,
  bits 4-6 only.
- **Sound list**: raw sweep `01`..`FF`: only bits 4-6 reach the board, so the 255 bytes are
  8 distinct values, each repeated 32 times; `01`..`0F` read as 7, the same as the
  power-on value 0.
- **Stop, boot and resets**: board reset (the COP420's reset line).
- **Volume**: the board's only level.
- **Loops**: audio only: the COP420's RAM is internal, its read map has no `MRA_RAM`
  range (`playsound_readmemz`), so there is no state to read.
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured**: `zira`, 0 of 40; "the game sent no sound byte at boot"
  ([board support](../board-support.md)): expected, since the game uses `sndbrd_ctrl_w`,
  which is not logged.
- **Limits and what is missing**: the first 40 commands of the sweep (`01`..`28`) cover
  only the values 7, 6 and 5. Whether the board starts a sound on a level or on a change
  of level is not determined from the code. Cheapest next step: sweep the 8 values
  `00`, `10`, ... `70` with the idle value between them; nothing in the boot log shows the
  game's own sequence, since nothing is logged on this path.
- **In VPinball**: **the pack cannot play**: the game's commands go through
  `sndbrd_ctrl_w`, which does not call `snd_cmd_log`, so AltSound receives nothing.

## <a name="sndbrd_zsu"></a>SNDBRD_ZSU

E.F.O. ZSU Sound Control Unit (Z80 + 2 AY-3-8910 + MSM5205) · PinMAME interface `ZSU`
(`src/wpc/efosnd.c`) · status ✅ · 6 sets, 6 games, 6 sound ROM ids, 1987-1988,
Playmatic, Maibesa · e.g. Skill Flight (`sklflite`), Phantom Ship (`phntmshp`), Cobra
(`cobrapb`), Come Back (`comeback`)

- **Hardware**: a Z80 at 4 MHz with two Z80 CTCs (`MACHINE_DRIVER_START(ZSU)`, RAM
  `7000`-`77FF`, `MRA_RAM`), two AY-3-8910 (the first's port A banks the ROM, the
  second's controls the MSM5205), an OKI MSM5205 ADPCM fed through a 16-byte FIFO
  (`clock_pulse`, `fifo_w`); ported from MAME's `efo_zsu.cpp`.
- **Commands**: the game writes a byte with `sndbrd_0_data_w` (`play.c`, `out2_n` when
  ENSN is low, Playmatic; `efo.c`, E.F.O. and Maibesa). `zsuIntf`'s manual handler is the
  data handler `zsu_data_w`: the byte in the latch and an interrupt with vector `FF` to
  the Z80, which reads it on port `14` (`snd_r`). One byte, one command.
- **Sound list**: raw sweep `01`..`FF`.
- **Stop, boot and resets**: board reset (the Z80's reset line).
- **Volume**: the board's only level.
- **Loops**: audio, sequencer state (Z80 RAM).
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured**: `sklflite`, 24 of 40, all from silence ([board support](../board-support.md)).
- **Limits**: only the quick survey.
- **In VPinball**: the game writes one byte per command through `sndbrd_0_data_w`, the
  byte the sweep sends: the ids should match. Not tested in VPinball.
