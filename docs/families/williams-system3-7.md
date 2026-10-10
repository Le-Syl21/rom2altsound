# Williams System 3 to 7

The sound boards of Williams System 3, 4, 6 and 7 (1977-1984) and their late
conversions: `SNDBRD_S67S`, `SNDBRD_S3S`, `SNDBRD_S3DFS`, `SNDBRD_S3WCS` and
`SNDBRD_S7S_ND`. They are one PinMAME interface, `WMSS67` (`src/wpc/wmssnd.c`,
`s67sIntf`), told apart by a sub-type that changes how the command byte is read. What is
not said here is the [common method](common.md).

**The board starts a sound on the change from an idle value to a command** (read below
in `s67s_cmd_w`). Until 0.2.3 rom2altsound sent its commands back to back, without the
idle value in between, and none of these families worked; each command now goes out
between two idle bytes, as the games send it.

## The board and its protocol

- **Hardware** (`MACHINE_DRIVER_START(wmssnd_s67s)`): a 6808 at 3.58 MHz / 4, 128 bytes
  of `MRA_RAM` (`0000`-`007F`), one PIA (`s67s_pia`) whose port A feeds an 8-bit DAC
  (`DAC_0_data_w`, mixing level 40) and whose CA2/CB2 drive an HC55516 CVSD
  (`s67s_hc55516Int`; the source notes the first speech boards used an MC3417). Port B
  reads the command (`snd_r`), CB1 is the "sound input != 1F" line.
- **The game side**: the sound lines are solenoid outputs. System 4 and 3
  (`src/wpc/s4.c`, `s4_sol9_16_w`) and System 6 (`s6.c`, `s6_sol9_16_w`) write
  `sndbrd_0_data_w(0, ~data)`, the inverted solenoid byte; System 7 (`s7.c`, `pia0a_w`)
  writes its PIA byte as is (`s7S6`: `pia1b_w`, inverted). With no solenoid on, the board
  sees all ones: the idle value. System 3/4 also send solenoids 1-8 to the board's control
  handler (`s4_sol1_8_w` → `sndbrd_0_ctrl_w`, `s67s_ctrl_w`), which only the World Cup
  and Disco Fever sub-types use.
- **`s67s_cmd_w`** (also the board's `manCmd_w`: the interface lists it for both) builds
  the byte the 6808 reads and sets CB1:
  - sub-type 0 (`SNDBRD_S67S`): `sndCmd = (data & 1F) | dip << 5`, CB1 = `(sndCmd & 1F) !=
    1F`. Five bits: 32 values, `1F` is idle; bits 5-7 come from the board's DIP switches,
    not from the game.
  - sub-type 1 (`SNDBRD_S7S_ND`, "won't use sound dips, but transmit 7 sound bits",
    `MACHINE_INIT(s7nd)`): `sndCmd = data & 7F`, idle when `& 7F == 7F`. Seven bits.
  - sub-type 2 and up (`SNDBRD_S3S` = 2, `S3WCS` = 8|4|2, `S3DFS` = 16|4|2): `sndCmd =
    30 | (data & 0F) | (data & 10) << 3 | dip`, then bit 4 from data bit 6 (`& 4`), bit 5
    (`& 8`, World Cup) or bit 7 (`& 16`, Disco Fever) from the last control byte
    (`sndBitsA`, set by `s67s_ctrl_w`); idle when `(sndCmd & BF) == BF`.
- **CB1 is an edge input** of the PIA: the board's interrupt (`s67s_piaIrq`) fires when
  CB1 goes from idle to active. A second command written while CB1 is already active
  changes the byte but makes no edge: the program never sees it. That is why the games
  always go back to idle between two commands: the boot logs show `FF FF 7F` (phnix_l1,
  wldcp_l1, disco_l1) and `7F 19 7F 26` (thund_p1), and bk_l4 sends `7F`, the command
  (`2C`), `7F` ([board support](../board-support.md)). PinMAME's own sounds.dat does the
  same: its only section for these boards, `frpwr_l2` (Firepower, 31 entries), writes
  every command as two bytes, the command then `1F` (`001f`, `011f`, ...).

## What rom2altsound does

- **Sending** (`s67s_framed`, in `board_sends`, so the sweep, sounds.dat's commands,
  `--only` and the stop all go through it): every byte the board does not read as idle
  (`s67s_idle`: low five bits `1F` on System 4-7, seven bits `7F` on `S7S_ND`, bits
  0-4 and 6 on World Cup and Disco Fever) goes out as `FF`, the byte, `FF`, one send every
  4 frames: CB1 makes its edge on the command and drops on the idle byte that follows. An
  idle byte is sent as is, so Firepower's sounds.dat commands (`00 1F`) become
  `FF 00 FF 1F`.
- **Sound list** (`sweep`, `"WMSS67"`): only the bits the board reads, each value once:
  `00`..`1E` on sub-type 0 (31 commands; bits 5-7 are the board's DIP switches) and on
  `S3S`; `00`..`7E` on `S7S_ND`; `00`..`1F` and `40`..`5E` on World Cup and Disco Fever
  (data bit 6 reaches the program there). The ids are the bytes swept: the game's own
  bytes carry other solenoid lines in bits 5-7 (bk_l4 sends `2C` for command `0C`), so a
  pack's ids are the command bits, not what VPinball's AltSound receives.
- **Stop, boot and resets**: no stop known: **the stop is a board reset** after every
  sound (`shim_reset_audio_cpus`, the 6808's reset line), then 4 s of silence (34 resets
  in the survey run of bk_l4: the long sounds and the backgrounds do not end by
  themselves). The reset restarts the program but not the PIA's input lines; the idle
  byte after each command leaves CB1 down.
- **Volume**: no volume command known and no volume stage listed (`volume::full_scale`
  does not name `WMSS67`): `reference_volume` is "none: recorded at the game's own
  volume", not scaled. The DAC is **not** AC-coupled (`ac_couples_dac` covers only WPCS and
  `WMSS11*`): the WAVs keep the DC of PinMAME's unsigned DAC; levels are measured
  DC-blocked.
- **Loops**: audio, then sequencer state (the 6808's registers and its 128 bytes of RAM).
- **DUCK / STOP / CHANNEL**: defaults (the chips pass is for `WPCS` and `WMSS11*` only).

## System 3's programs

Phoenix's program (`485_s0_phoenix.716`, 512 bytes mirrored; the interrupt handler at
`7F45`), read with a 6800 disassembler: it inverts the byte, ignores it when nothing but
the idle bits is low, and, when data bit 4 (bit 7 of the byte it reads) is low, only sets
a flag and waits for the next command. With bit 4 high and no flag, the low nibble,
inverted, minus one, selects one of 15 numbered sounds (`7E4B`): the commands `10`..`1E`.
After the flag, the next command plays the sound of the lowest low bit (a priority
decoder, table at `7FE7`): a two-byte sequence the sweep does not send. World Cup and
Disco Fever have other programs, not read; their extra bit from the control byte
(`s67s_ctrl_w`, which `sndbrd_manCmd` never calls) is not set either.

## <a name="sndbrd_s67s"></a>SNDBRD_S67S

Williams System 4, 6 and 7 sound board · interface `WMSS67` (sub-type 0) · ✅ · 105
sets, 38 games, 28 sound ROM ids, 1978-2022, Williams, Williams / Oliver · e.g. Gorgar
(`grgar_l1`), Firepower (`frpwr_l6`), Black Knight (`bk_l4`), Jungle Lord (`jngld_l2`)

- **Hardware, commands, list, stop, volume, loops, columns**: as above. The family also
  holds the System 4 shuffle alleys (`bowlgames.c`), three Zaccaria sets with an
  alternate Williams-type sound board (`stargoda`, `stargodaf`, `ewfa`, `zacgames.c`),
  and Big Strike / Triple Strike (`bstrk_l1`, `tstrk_l1`), whose machine (`s4_mS4`,
  chimes) has no sound CPU although `MACHINE_INIT(s4)` starts `SNDBRD_S67S` by default:
  nothing there can play.
- **Measured** (survey settings, [board support](../board-support.md)): bk_l4 30 of 31,
  grgar_l1 30 of 31, jngld_l2 30 of 31, frpwr_l2 30 of 31 (its sounds.dat), all from
  silence (bk_l4 3 of 40 and grgar_l1 3 of 40 before the idle framing). No full run.
- **Limits**: no stop (a board reset after each sound); the ids are the command bits
  (above).
- **In VPinball**: **the pack does not play as written, in any VPinball so far**: the logged bytes keep bits 5-7 as the solenoid lines leave them (bk_l4 `2C` for `0C`), the pack's ids are the command bits; and up to 10.8.1-5436 libaltsound has no case for
  `GEN_S6 / GEN_S7` and joins the bytes two by two ([In VPinball](common.md#in-vpinball)).
  Measured on `algar_l1`: the game sent `5F 5F 7F 5F 5F 7F 77 7F`, AltSound looked up
  `5F5F 7F5F 5F7F 777F`; with one byte per command
  ([vpinball/libaltsound#20](https://github.com/vpinball/libaltsound/pull/20), in VPinball master from 3abe805) it
  looks up `005F 005F 007F 005F`. Every byte the game writes, the idle bytes
  included, is logged with bits 5-7 as the solenoid lines leave them; the `frpwr_l2`
  section gives two-byte ids (`0x001F`) that AltSound would not look up either way.

## <a name="sndbrd_s3s"></a>SNDBRD_S3S

Williams System 3 sound board · interface `WMSS67` (sub-type 2) · ✅ · 3 sets, 3 games,
3 sound ROM ids, 1978, Williams · Contact (`cntct_l1`), Phoenix (`phnix_l1`), Pokerino
(`pkrno_l1`)

- As above (sub-type 2: data bits 0-3 and 4, a DIP switch in bit 6). Phoenix and
  Pokerino run `s4.c` (`GEN_S4`), Contact `GEN_S3`.
- **Measured**: phnix_l1 15 of 31, all from silence: the 15 numbered sounds, `10`..`1E`
  (0 of 40 before, with no idle byte between the commands; `00`..`0F` have data bit 4
  low, which only sets the program's flag, see [System 3's programs](#system-3s-programs)).
- **Limits**: the priority-coded sounds (the flag, then a command) are not swept.
- **In VPinball**: **the pack does not play as written, in any VPinball so far**: the logged bytes keep the solenoid bits 5-7 (`s4.c` `s4_sol9_16_w`), the pack's ids are the command bits; and up to 10.8.1-5436 libaltsound has no case for
  `GEN_S3` and joins the bytes two by two ([In VPinball](common.md#in-vpinball)). Measured
  on `cntct_l1`: the game sent `5F 5F 7F 5F 5F 7F 6F 7F`, AltSound looked up `5F5F 7F5F
  5F7F 6F7F`; with one byte per command
  ([vpinball/libaltsound#20](https://github.com/vpinball/libaltsound/pull/20), in VPinball master from 3abe805) it
  looks up `005F 005F 007F`.

## <a name="sndbrd_s3dfs"></a>SNDBRD_S3DFS

Disco Fever's System 3 sound board · interface `WMSS67` (sub-type 16|4|2) · ⚠️ · 1 set,
1978, Williams · Disco Fever (`disco_l1`)

- As above; bit 4 of the byte the program reads is data bit 6, bit 7 is bit 4 of the last
  control byte (`s67s_ctrl_w`, solenoids 1-8 through `s4_sol1_8_w`), which a manual
  command cannot set. PinMAME also defines a prototype variant, `SNDBRD_S3DFPS`
  (sub-type 4|2, without the control bit; `sndbrd.h`), which is not a family of the
  survey.
- **Measured**: disco_l1 40 of 40, all from silence, but few distinct: the files repeat
  with the lowest low bit of the command (`00`, `02`, `04`... 0.13 s; `01`, `05`, `09`...
  0.35 s; `0F` and `1F` 2.6 s), as a priority decoder would (1 of 40 before).
- **Limits**: the control bit (above); the program is not read.
- **In VPinball**: **the pack does not play as written, in any VPinball so far**: `s4.c` logs the inverted solenoid byte, whose bits 5 and 7 follow other lines: it matches the pack's ids only while they are clear; and up to 10.8.1-5436 libaltsound has no case for
  `GEN_S3` and joins the bytes two by two ([In VPinball](common.md#in-vpinball)). Measured
  on `disco_l1`: the game sent `5F 5F 7F 5F 7F`, AltSound looked up `5F5F 7F5F 7F5F`; with
  one byte per command
  ([vpinball/libaltsound#20](https://github.com/vpinball/libaltsound/pull/20), in VPinball master from 3abe805) it
  looks up `005F 005F 007F`.

## <a name="sndbrd_s3wcs"></a>SNDBRD_S3WCS

World Cup's System 3 sound board · interface `WMSS67` (sub-type 8|4|2) · ⚠️ · 1 set,
1978, Williams · World Cup (`wldcp_l1`)

- As above; bit 5 of the byte the program reads comes from bit 6 of the last control
  byte (`s67s_ctrl_w`), which a manual command cannot set.
- **Measured**: wldcp_l1 16 of 40, all from silence, but all 16 alike (`10`..`1F`, 2.6 s,
  the same level): one sound (0 of 40 before).
- **Limits**: the control bit (bit 5 of the byte the program reads, from `s67s_ctrl_w`),
  which the tool never sets; the program is not read.
- **In VPinball**: **the pack does not play as written, in any VPinball so far**: `s4.c` logs the inverted solenoid byte, whose bits 5 and 7 follow other lines: it matches the pack's ids only while they are clear; and up to 10.8.1-5436 libaltsound has no case for
  `GEN_S3` and joins the bytes two by two ([In VPinball](common.md#in-vpinball)). Measured
  on `wldcp_l1`: the game sent `5F 5F 7F 5F 7F`, AltSound looked up `5F5F 7F5F 7F5F`; with
  one byte per command
  ([vpinball/libaltsound#20](https://github.com/vpinball/libaltsound/pull/20), in VPinball master from 3abe805) it
  looks up `005F 005F 007F`.

## <a name="sndbrd_s7s_nd"></a>SNDBRD_S7S_ND

The System 7 board of Thunderball (no sound DIPs, seven command bits) · interface
`WMSS67` (sub-type 1, `MACHINE_INIT(s7nd)` in `s7.c`) · ✅ · 3 sets, 1 game, 1 sound ROM
id, 1982, Williams · Thunderball (`thund_p1`, a prototype)

- As above, with seven bits: commands `00`..`7E`, idle `7F`.
- **Measured**: thund_p1 34 of 40, all from silence (0 of 40 before; boot `7F 19 7F 26`,
  the commands between idle bytes, [board support](../board-support.md)).
- **Limits**: no stop (a board reset after each sound).
- **In VPinball**: **the pack plays from VPinball master 3abe805 on**, not in 10.8.1-5436 and older:
  their libaltsound has no case for `GEN_S7` and joins the bytes two by two ([In VPinball](common.md#in-vpinball)). Measured
  on `thund_p1`: the game sent `7F 19 7F 7F 26 7F`, AltSound looked up `7F19 7F7F 267F`;
  with one byte per command
  ([vpinball/libaltsound#20](https://github.com/vpinball/libaltsound/pull/20), in VPinball master from 3abe805) it
  looks up `007F 0019 007F 007F 0026`.
