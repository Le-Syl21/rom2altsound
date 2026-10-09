# Data East, Sega and Stern Whitestar

The three generations of Data East's sound hardware, which Sega and then Stern kept
(`src/wpc/desound.c`: "Generation 1: YM2151 & MSM5205", "Generation 2: BSMT 2000",
"Generation 3: AT91 CPU"): [`SNDBRD_DE1S`](#sndbrd_de1s), [`SNDBRD_DE2S`](#sndbrd_de2s)
and [`SNDBRD_DE3S`](#sndbrd_de3s). What every family gets is in
[the common method](common.md); this page says what differs.

On every one of these machines the sound board is PinMAME's board 1: board 0 is the DMD
controller (Data East DMD games and Whitestar, `s11.c` `MACHINE_INIT(s11)` for
`GEN_DEDMD*`, `se.c` `MACHINE_INIT(se)`/`(se3)`), which PinMAME flags
`SNDBRD_NOTSOUND`, so `sndbrd_exists(0)` is false and the tool sees a single board
(`board_mask`): the ids are single commands, not `board << 8 | byte`. The DMD CPU has no
CPU flags, so `shim_halt_game_cpus` halts it with the game CPU.

## <a name="sndbrd_de1s"></a>SNDBRD_DE1S

Data East alphanumeric sound board (YM2151 + MSM5205) · PinMAME interface `DE`
(`src/wpc/desound.c`, `de1sIntf`) · status ⚠️ · 38 sets, 15 games, 14 sound ROM ids,
1987-2025, Data East, Leon · e.g. Laser War, Time Machine (`tmac_a24`), Back to the Future
(`bttf_a28`), The Simpsons (`simp_a27`)

- **Hardware**: a 6809 at 2 MHz (`MACHINE_DRIVER_START(de1s)`, `CPU_AUDIO_CPU`), a
  YM2151 (`de1s_ym2151Int`, its IRQ on the 6809's IRQ) and an MSM5205 ADPCM chip fed from
  a banked voice ROM (`de1s_MSM5025_w`, `de1s_msmIrq`). No DAC. Used by the Data East
  alphanumeric generation (`GEN_DE`) and the 128x16 DMD games (`GEN_DEDMD16`, board 1 =
  `hw.soundBoard`), started in `s11.c` `MACHINE_INIT(s11)`.
- **Commands**: the game writes the byte through PIA 5 port B (`s11.c` `pia5b_w`, which
  calls `sndbrd_1_data_w`), then strobes CB2 (`pia5cb2_w`, `sndbrd_1_ctrl_w`). The board
  latches the byte (`de1s_data_w`) and a low strobe asserts the 6809's FIRQ
  (`de1s_ctrl_w`); the program reads the latch at `$2400`, which clears the FIRQ
  (`de1s_cmd_r`). `de1s_manCmd_w` does the same two steps (data, then `ctrl_w(0)`), so
  rom2altsound's one byte per `sndbrd_manCmd` is what the game sends for a one-byte
  command. Multi-byte commands: not determined from the code.
- **Sound list**: no sounds.dat section for any DE1S set: the generic sweep, single bytes
  `01`..`FF` (`sweep`, default branch).
- **Stop, boot and resets**: `DE` is in neither `BUILTIN_STOPS` nor a sounds.dat family
  section, so `stop_sends` falls back to a board reset after every sound:
  `shim_reset_audio_cpus` pulses the 6809's reset line, then the tool waits 4 s for
  silence (`QUIET_AFTER_RESET_SECS`). The pulse resets the CPU, not the YM2151 or the
  MSM5205 (`de1s_init` is not run again).
- **Volume**: no volume command is decoded (`volume::decode` has no `DE` case) and `DE`
  is not in `volume::full_scale`: the files are at whatever level the board plays,
  `reference_volume: "none: recorded at the game's own volume"`, not scaled, not
  AC-coupled (there is no DAC to couple).
- **Loops**: audio, and the sequencer state of the 6809 (8-bit, `MRA_RAM` at
  `$0000`-`$1FFF` in `de1s_readmem`: 8 KB of state). Never measured on this family (the
  survey runs without loop search).
- **DUCK / STOP / CHANNEL**: the defaults.
- **Measured** ([board support](../board-support.md)): `tmac_a24` 38 of 40, all from
  silence; `bttf_a28` 40 of 40 and `simp_a27` 37 of 40, but the files run to the 5 s cap
  and 34 to 38 of them do not start from silence; `simp_a27` ends with "still not silent
  after 3 waits".
- **Limits and what is missing**: on the later games the board keeps playing through the
  reset that serves as the stop. Board support's fix: find the stop command of the later
  DE sound programs. From the code: the tool never sends a stop command on this board,
  only the CPU reset, which leaves the YM2151's and MSM5205's own state alone; whether
  that is what keeps them playing is not known. A cheap test is `--stop` with the byte the
  game sends between sounds (its boot log shows it); `00` would be the first candidate,
  as on the BSMT board of the same maker, but this is untried.
- **In VPinball**: AltSound gets the byte `pia5b_w` writes; for `GEN_DE` and the DMD
  generations `snd_alt.cpp` `preprocess_commands` takes every byte but `00` and `FF` as an
  8-bit command, `00`/`FF` starting a 16-bit one. The pack's ids are single bytes
  `01`..`FF`, so they match the game's one-byte commands; `FF` and the 16-bit commands
  would not. Not tested in VPinball.

## <a name="sndbrd_de2s"></a>SNDBRD_DE2S

BSMT2000 sound board of Data East (DMD games, 1991-1994) and Sega/Stern Whitestar · PinMAME
interface `BSMT` (`src/wpc/desound.c`, `de2sIntf`) · status ✅ · 307 sets, 48 games, 59
sound ROM ids, 1991-2026, Stern, Sega · e.g. Batman (`btmn_106`), Guns N' Roses
(`gnr_300`), Jurassic Park (`jupk_513`), Apollo 13 (`apollo13`), X-Files (`xfiles`),
Monopoly (`monopole`)

- **Hardware**: a 6809 at 2 MHz with a fixed 489 Hz FIRQ (`MACHINE_DRIVER_START(de2as)`,
  `de2s_firq`) and a BSMT2000 (11 or 12 voices: `de2s_bsmt2000aaInt`, `aInt`, `bInt`,
  `tInt`), a TMS320C15 DSP with its program in mask ROM. The 6809 writes the BSMT's
  registers as 16-bit words (`de2s_bsmtcmdHi_w`, `de2s_bsmtcmdLo_w`), polls its ready line
  (`de2s_bsmtready_r`) and resets it through bit 7 of `$2000` (`de2s_bsmtreset_w`).
  PinMAME emulates the chip at a high level (HLE) or, in the fork rom2altsound builds, runs
  its real program (LLE) when it finds MAME's `bsmt2000.bin`; `src/bsmtfw.rs` stages the
  file and reports which ran (`bsmt2000.emulation`, see
  [The BSMT2000 program](../how-it-works.md#bsmt2000-the-chips-own-program)).
- **Commands**: the interface's `data_w` and `manCmd_w` are both `soundlatch_w`
  (`de2sIntf`, `SNDBRD_NODATASYNC`), read by the 6809 at `$2002` (`soundlatch_r`): the
  tool's byte is exactly the game's. Data East games write it through `s11.c` `pia5b_w`,
  Whitestar through `$3800` (`se.c`, `sndbrd_1_data_w`). Single bytes play sounds; `00` is
  the stop; `FC`..`FF` start two-byte commands (`BSMT_LAST`); on Whitestar `FE xx` sets
  the master volume and must be followed by `FD`, without which the board swallows every
  later command (`volume.rs`, `BSMT_END`). The game re-sends its `FE xx FD` every 0.5 s.
  On Data East the bytes `20`..`2F` set a music volume (`volume::decode_de_music`).
  Which of the two a board is: `ffi::is_data_east` (the game's generation is one of
  `GEN_DE`, `GEN_DEDMD16/32/64`), `Extractor::is_de_board`.
- **Sound list**: no sounds.dat section; the sweep is `01`..`FB` (`sweep`, `"BSMT"`
  branch). `FC`..`FF` are left out: `FF xx` plays the same sound as `xx` and `FC xx` starts
  a loop for every `xx` (probed on apollo13, gnr_300, xfiles; `BSMT_LAST`). On Data East
  the music volume bytes `21`..`2F` are swept too: they end as `no_sound` or a blip
  because the music volume is set back before each command, and the real sounds among them
  are kept (gnr_300 `2E`, a loop).
- **Stop, boot and resets**: stop `00` (`BUILTIN_STOPS`, "BSMT"). If the halt of the game
  CPU split a Whitestar `FE xx` from its `FD`, the tool sends the `FD` (`end_boot`). Before
  every command (`set_refresh`): on Data East the music volume (the game's last `20`..`2F`
  at boot, else `20`; `20` with `--volume reference`) **then `00`**, which does not reset
  the music volume but brought btmn_106's output back from a held DC level; on Whitestar
  the master volume `FE xx FD` (the reference, see below). A board reset is a pulse of the
  6809's reset line (`shim_reset_audio_cpus`), after which the volume is sent again.
- **Volume**: Whitestar: factory volume = the game's last `FE xx` at boot (apollo13 and
  monopole `FE 2C`, level 3/31; xfiles `FE 20`); recorded at the reference `FE 11 FD`
  (level 30/31, `--whitestar-volume`; `reference_master`), then scaled by the measured
  factory offset (apollo13 -32.56 dB, xfiles -9.18 dB). A game that sends no `FE xx`
  leaves the board at its power-on level: nothing is sent, nothing is scaled. The master
  volume check (`alt_volume`, 8 levels away) flags files that ignore the volume (xfiles
  `1F`). Data East: the master volume is a pot in the power box ("it was not done through
  the software", comment above `de2s_bsmt2000aaInt`), so the files are at full scale
  (`volume::full_scale`, "BSMT (Data East)"), offset 0. No AC coupling; the end trimming
  walks back over held DC steps (gnr_300 `67`, xfiles `69`; `held_dc_start`).
- **Loops**: audio (only the BSMT test tones `F0`..`F2`, 0.068 s, loop sample-exactly), then
  the 6809's sequencer state (8 KB of `MRA_RAM`, `$0000`-`$1FFF`, `de2s_readmem`); on
  these boards the audio follows every other state cycle, as the music also lies in the
  BSMT2000's own streams ([Loops](../how-it-works.md#loops)). Most music stays cut at
  `--max-secs`.
- **DUCK / STOP / CHANNEL**: the defaults. No chips pass and no ducking check: two takes of
  the same sound correlate only 0.3 to 0.99 on Whitestar, so a fit means nothing
  ([how it works](../how-it-works.md#ducking-stops-and-channels-dcs)).
- **Measured** ([how it works](../how-it-works.md#factory-results-for-our-roms), 0.2.1 at
  the reference volume): apollo13 251 tried / 182 sound / 178 written / 45 loops; xfiles
  251 / 170 / 169 / 40; gnr_300 251 / 161 / 161 / 40 (12 clipped files, `67` 4966
  samples); btmn_106 251 / 157 / 141 / 34. Loops: 3 test tones each from the audio, xfiles
  5 musics from the state, apollo13, gnr_300 and btmn_106 none. With the BSMT2000's own
  program Monopoly writes 208 files instead of 165. Survey
  ([board support](../board-support.md)): jupk_513 23 of 40, swtril43 25 of 40, gnr_300 21
  of 40, monopole 40 of 40.
- **Limits and what is missing**: BSMT music does not repeat sample-exactly; Data East
  ADPCM state carries over between commands (gnr_300 `67`); a few sounds ignore the master
  volume (apollo13 `5F`, xfiles `1E`, `1F`); the factory volume is the attract-mode one
  ([Limits](../how-it-works.md#limits)).
- **In VPinball**: AltSound gets the same latch byte. `snd_alt.cpp` takes Whitestar's
  `FC`..`FF` as the first byte of a two-byte command, `FE 10`..`2F` as the volume, and
  `0000`/`F0xx` as "stop music"; Data East bytes but `00`/`FF` are 8-bit commands. The
  pack's ids are the single bytes `01`..`FB`, so they match the game's one-byte commands.
  Not tested in VPinball.

## <a name="sndbrd_de3s"></a>SNDBRD_DE3S

Whitestar "CPU/Sound Board II" with an Atmel AT91 (ARM7) · PinMAME interface `AT91`
(`src/wpc/desound.c`, `de3sIntf`) · status ⚠️ · 156 sets, 7 games, 30 sound ROM ids,
2003-2008, Stern · e.g. The Lord of the Rings (`lotr`), Elvis (`elvis`), The Sopranos
(`sopranos`), NASCAR (`nascar`)

- **Hardware**: an AT91 at 40 MHz (`MACHINE_DRIVER_START(de3as)`, `CPU_AUDIO_CPU`) that
  emulates the BSMT2000 in software (with 16-bit ADPCM samples, desound.c's comment) and
  writes its mix through a Xilinx FPGA (`xilinx_w`) into a 24 kHz stereo stream
  (`at91_sh_update`, `WAVE_OUT_RATE`). Started by `se.c` `MACHINE_INIT(se3)`.
- **Commands**: `data_w` (`scmd_w`) and `manCmd_w` (`man3_w`, which calls `scmd_w`) are
  the same: the byte goes into a 4-entry queue that the AT91 reads through the Xilinx
  (`xilinx_r` → `scmd_r`). Two properties of that queue matter: `scmd_w` **ignores a byte
  equal to the last one written** (`if (data != sndcmdlast)`), and `scmd_r` keeps returning
  the last byte once the queue is empty. The game writes `$3800` (`se.c`,
  `sndbrd_1_data_w`). Commands are two bytes: sounds.dat lists them as `FC xx`, `FD xx` and
  `FE xx` (lotr 207 / 240 / 178 entries), its `FE` entries starting at `FE30`, above the
  Whitestar volume range `FE 10`..`2F`.
- **Sound list**: unlike every other family here, the DE3S games have sounds.dat
  sections (`lotr`, `ripleys`, `elvis`, `sopranos`, `nascar`, `gprix`; 155 of the 156 sets
  by the prefix rule), so their list is that section's, with names (`build_commands`);
  each two-byte entry goes out as two `sndbrd_manCmd` calls 4 frames apart
  (`game_cmd`). A set without a section would get `01`..`FB` (`sweep`, `"BSMT" | "AT91"`).
- **Stop, boot and resets**: `AT91` is not in `BUILTIN_STOPS` and there is no `at91:`
  sounds.dat section, so the stop is a board reset: `shim_reset_audio_cpus` pulses the
  AT91's reset line (`AT91` is not in `CTRL_RESET`), then 4 s of silence are awaited.
  The halt's `FD` completion and the Whitestar refresh apply (`end_boot`, `set_refresh`
  match `"BSMT" | "AT91"`).
- **Volume**: the code treats it as Whitestar: `FE 10`..`2F` from the game are decoded as
  its master volume (`volume::decode`), the reference is `FE 11 FD` (`reference_master`),
  re-sent before every command, and the volume check and factory offset run on it
  (`alt_volume`). That this board takes `FE xx FD` as Whitestar does is inferred from
  sounds.dat leaving `FE 10`..`2F` free; it was not measured. If the game sends no
  `FE xx` at boot, the board stays at its power-on level and is not scaled
  (`factory_master`).
- **Loops**: audio only. The AT91 has a 32-bit bus, so `shim_audio_cpu` (8-bit audio CPUs
  only) gives no sequencer state to read.
- **DUCK / STOP / CHANNEL**: the defaults; `CHANNEL` 0 for sounds.dat `Music:` names.
- **Measured** ([board support](../board-support.md)): lotr 39 of 40 (speech and
  effects), elvis 40 of 40 (songs and effects), distinct and named from sounds.dat, but
  24 (lotr) and 31 (elvis) files start over what was playing: neither the stop (the
  reset) nor a second reset silences the board within 10 s.
- **Limits and what is missing**: board support's fix: find how the AT91 board is
  stopped, its real stop or its idle floor. What the code shows, not verified: (1) the
  tool sends no stop command at all, only the reset; Whitestar's `00` was never tried here
  (`--stop 0x00` would); (2) after a reset the AT91 reads the queue again and gets the
  last byte written (`scmd_r`), which may restart a sound; (3) because `scmd_w` drops a
  repeated byte, an `FD xx` command sent right after the `FE 11 FD` refresh loses its
  `FD` and reaches the board as `xx` alone (only when the game sent an `FE xx` at boot,
  which turns the refresh on). lotr's first 40 commands are all `FC xx`, elvis's all
  `FD xx`; the survey does not record whether elvis's refresh ran, so its 40 of 40 does
  not tell whether (3) happened.
- **In VPinball**: the game's bytes reach AltSound unchanged (`se.c` writes the same
  queue), and `snd_alt.cpp` joins `FC`..`FF xx` into one 16-bit id, which is how sounds.dat
  and the pack name them (`0xFC01`). Not tested in VPinball.
