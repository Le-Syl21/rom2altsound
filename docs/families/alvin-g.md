# Alvin G. and Co.

The two sound boards of Alvin G. (1992-1994): [`SNDBRD_ALVGS1`](#sndbrd_alvgs1) (YM3812 +
OKI MSM6295) and [`SNDBRD_ALVGS2`](#sndbrd_alvgs2) (BSMT2000), both in
`src/wpc/alvgs.c`. What every family gets is in [the common method](common.md); this page
says what differs.

On both, the sound board is PinMAME's board 0 and the DMD board 1 (`alvg.c`
`MACHINE_INIT`: `sndbrd_0_init(core_gameData->hw.soundBoard, ...)`,
`sndbrd_1_init(core_gameData->hw.display, ...)`). The game sends a command the same way to
both: the byte on VIA 1 port A (`alvg.c` `xvia_1_a_w`, `sndbrd_0_data_w`), then a rising
edge of port B bit 1, the sound clock (`xvia_1_b_w`, `sndbrd_0_ctrl_w(0,0)`). Both
interfaces' `manCmd_w` is `alvg_sndCmd_w`, which does the same two calls, so the tool's
byte is the game's.

## <a name="sndbrd_alvgs1"></a>SNDBRD_ALVGS1

Alvin G. first sound board (YM3812 + MSM6295) · PinMAME interface `OKI`
(`src/wpc/alvgs.c`, `alvgs1Intf`) · status ✅ · 10 sets, 3 games, 5 sound ROM ids,
1992-1993, Alvin G · e.g. A.G. Soccer-Ball (`agsoccer`), U.S.A. Football (`usafootb`),
Punchy The Clown (`punchy`)

- **Hardware**: a 6809 at 2 MHz (`MACHINE_DRIVER_START(alvg_s1)`, `CPU_AUDIO_CPU`; the
  schematic's 8 MHz divided by 4, `ALVGS1_SNDCPU_FREQ`), a YM3812 (FM) and an OKI MSM6295
  (ADPCM samples), both PinMAME sound chips. The command arrives through a 6522 VIA: the
  latch is read on its port A (`xvia_2_a_r`, `soundlatch_r`), the strobe is simulated as a
  CA2 transition (`alvgs1_ctrl_w`).
- **Commands**: see above; `alvgs1_data_w` is `soundlatch_w`. rom2altsound sends one byte
  per `sndbrd_manCmd`. Stop, idle and volume commands: not determined from the code.
- **Sound list**: no sounds.dat section: the generic sweep `01`..`FF`.
- **Stop, boot and resets**: `OKI` has no built-in stop: a board reset after every sound
  (`shim_reset_audio_cpus`, the 6809's reset line), 4 s of silence awaited.
- **Volume**: no volume command decoded, `OKI` not in `volume::full_scale`:
  `reference_volume: "none: recorded at the game's own volume"`, not scaled, no AC
  coupling.
- **Loops**: audio, and the 6809's sequencer state (`MRA_RAM` `$3000`-`$3FFF` in
  `alvgs1_readmem`, 4 KB). Not measured on this family.
- **DUCK / STOP / CHANNEL**: the defaults.
- **Measured** ([board support](../board-support.md)): agsoccer, 33 of the first 40
  commands give a sound.
- **Limits and what is missing**: only the quick survey; loops, volume and the full sweep
  are not verified.
- **In VPinball**: **the pack plays from VPinball master 3abe805 on**, not in 10.8.1-5436 and older:
  their libaltsound has no case for `GEN_ALVG` and joins the bytes two by two ([In VPinball](common.md#in-vpinball)).
  Measured on `agsocc07`: the game sent `00 00 49 74 43 74`, AltSound looked up `0000 4974
  4374`; with one byte per command
  ([vpinball/libaltsound#20](https://github.com/vpinball/libaltsound/pull/20), in VPinball master from 3abe805) it
  looks up `0000 0000 0049 0074 0043`.

## <a name="sndbrd_alvgs2"></a>SNDBRD_ALVGS2

Alvin G. second sound board (BSMT2000) · PinMAME interface `BSMT` (`src/wpc/alvgs.c`,
`alvgs2Intf`) · status ✅ · 10 sets, 5 games, 6 sound ROM ids, 1993-1994, Alvin G · e.g.
Al's Garage Band Goes On a World Tour (`wrldtour`), Mystery Castle (`mystcast`), Pistol
Poker (`pstlpkr`), Dinosaur Eggs (`dinoeggs`)

- **Hardware**: a 6809 (`MACHINE_DRIVER_START(alvg_s2)`, `CPU_AUDIO_CPU`, periodic FIRQ)
  writing a BSMT2000's registers (`bsmt_write`) and polling its ready lines
  (`bsmtready_r`; the polarity under the LLE is "inferred", says the source). The command
  latch is at `$0800` (`soundlatch_r`), the strobe pulses the 6809's IRQ
  (`alvgs_ctrl_w`). The BSMT2000 runs its own program when `bsmt2000.bin` is found, as on
  Data East and Whitestar (`src/bsmtfw.rs`; `shim_has_bsmt2000` is true for any machine
  with the chip; see
  [The BSMT2000 program](../how-it-works.md#bsmt2000-the-chips-own-program)).
- **Commands**: see above; `alvgs_data_w` is `soundlatch_w`. Because the interface's type
  string is `BSMT` and the game is not of a Data East generation, `src/extract.rs` drives
  it exactly as a Sega/Stern Whitestar board: the sweep `01`..`FB` (`sweep`, `BSMT_LAST`),
  `FE 10`..`2F` from the game decoded as a master volume (`volume::decode`), the
  `FE xx`/`FD` completion after the halt (`end_boot`), and `FE 11 FD` as the reference
  volume (`reference_master`). Whether Alvin G.'s sound program has such an `FE xx FD`
  volume command, and whether `FC`..`FF` start two-byte commands on it, is not known; see
  Limits.
- **Sound list**: no sounds.dat section: bytes `01`..`FB`.
- **Stop, boot and resets**: stop `00` (`BUILTIN_STOPS`, "BSMT"), the BSMT boards' stop,
  not checked against this board's program; if it does not bring silence within 10 s
  the board is reset (`shim_reset_audio_cpus`).
- **Volume**: in factory mode a Whitestar-style master volume is used only if the game
  sent an `FE 10`..`2F` at boot (`factory_master`, `our_master`); otherwise nothing is
  sent and the files are at the board's power-on level, not scaled. With `--volume
  reference`, `FE 11 FD` is sent whatever the game does (`reference_master`), and
  re-sent before every command (`set_refresh`). No AC coupling.
- **Loops**: audio, and the 6809's sequencer state (`MRA_RAM` `$2000`-`$3FFF` in
  `alvgs_readmem`, 8 KB). Not measured on this family.
- **DUCK / STOP / CHANNEL**: the defaults.
- **Measured** ([board support](../board-support.md)): wrldtour, 24 of the first 40
  commands give a sound, with the BSMT2000's own program.
- **Limits and what is missing**: only the quick survey. The Whitestar conventions the
  code applies to this board (`FE xx FD`, the stop `00`, `FC`..`FF` left out of the
  sweep) come from the Sega/Stern program, not from Alvin G.'s; with `--volume reference`
  the tool sends `FE 11 FD` to a program that may read it as sounds.
- **In VPinball**: **the pack plays from VPinball master 3abe805 on**, not in 10.8.1-5436 and older:
  their libaltsound has no case for `GEN_ALVG` and joins the bytes two by two ([In VPinball](common.md#in-vpinball)).
  Measured on `wrldtou3`: the game sent `00 00 15 15 61 E0`, AltSound looked up `0000 1515
  61E0`; with one byte per command
  ([vpinball/libaltsound#20](https://github.com/vpinball/libaltsound/pull/20), in VPinball master from 3abe805) it
  looks up `0000 0000 0015 0015 0061 00E0`.
