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
  `FF 00 FF 1F`. On World Cup and Disco Fever, `80` in a command is the control line
  instead (`S3_CTRL`): `s67s_ctrl_w` with `00`, then `FF` (the line on, then off: the game
  writes its solenoids 1-8 there inverted, `s4_sol1_8_w`), the data lines idle; data bit 7
  never reaches these boards, so `80` repeats no command.
- **Sound list** (`sweep`, `"WMSS67"`): only the bits the board reads, each value once:
  `00`..`1E` on sub-type 0 (31 commands; bits 5-7 are the board's DIP switches) and on
  `S3S`; `00`..`7E` on `S7S_ND`; on World Cup and Disco Fever, each sound their program
  can play, once (below: 16 and 24). The ids are the bytes swept: the game's own
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
decoder, table at `7FE7`): a two-byte sequence the sweep does not send. World Cup's and
Disco Fever's programs are read below (their sections); both use the bit their board
takes from the control port (`s67s_ctrl_w`, which `sndbrd_manCmd` never calls), and both
read bit 6, the board's Sound Dip 2 (s4.h, DIP bank 0 `02`, on in PinMAME by default:
bit 6 low), to choose between two paths. **The tool turns Sound Dip 2 off on these two
boards** (`PinmameSetDIP`, after the boot): with it on, one path is never taken (World
Cup's four direct routines, Disco Fever's 15 numbered sounds); with it off, both are. The
packs hold the sounds the board can play under either setting.

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

Disco Fever's System 3 sound board · interface `WMSS67` (sub-type 16|4|2) · ✅ · 1 set,
1978, Williams · Disco Fever (`disco_l1`)

- As above; bits 0-3 of the byte the program reads are the low data nibble, bit 4 is data
  bit 6, bit 6 the board's Sound Dip 2, bit 7 bit 4 of the last control byte
  (`s67s_ctrl_w`: solenoid 5, through `s4_sol1_8_w`). PinMAME also defines a prototype
  variant, `SNDBRD_S3DFPS` (sub-type 4|2, without the control bit; `sndbrd.h`), which is
  not a family of the survey.
- **The program** (`483_s0_disco_fever.716`, interrupt handler `7F45`, read with a 6800
  disassembler): it inverts the byte and ignores the idle one; the control line alone
  (bit 7 low) sets a flag (`$09` = 5) and waits. Without the flag, data bit 6 high (and
  Sound Dip 2 off) plays one of 15 numbered sounds, the inverted nibble minus one (`7E6A`
  with the pair of the table at `7FD0`): `40`..`4E`. Otherwise a priority decoder: the
  lowest low bit of the inverted byte (bits 0-4) picks a routine of the table at `7FE0`,
  the flag adding 5: `0E`, `0D`, `0B`, `07` and `0F` (data bit 6 low), then, after the
  control line, `80 0E`, `80 0D`, `80 0B`, `80 07` (the flag and `0F` would take entry 9,
  the word `0000`: not sent). The other commands of `00`..`0F` repeat these.
- **Sound list** (`s3dfs_sounds`): those 24, each once; `80` is the control line pulse
  (above), the ids are the bytes sent (`0x800E`).
- **Measured** (survey settings): disco_l1 24 of 24, all from silence, all different (15
  numbered sounds of 0.9 to 3.3 s, 5 priority sounds of 0.2 to 2.7 s, 4 after the control
  line of 2.0 to 4.8 s). Before: 40 of 40 but few distinct (the sweep `00`..`27` repeats
  the priority decoder: `00`, `02`, `04`... alike), 1 of 40 before the idle framing.
- **Limits**: no stop (a board reset after each sound). The numbered sounds need Sound
  Dip 2 off, the tool's setting (PinMAME's default has it on).
- **In VPinball**: **the pack does not play as written, in any VPinball so far**: `s4.c`
  logs the inverted solenoid byte, whose bits 5 and 7 follow other lines (it matches the
  pack's ids only while they are clear), the control line is not logged at all
  (`sndbrd_ctrl_w`), and up to 10.8.1-5436 libaltsound has no case for `GEN_S3` and joins
  the bytes two by two ([In VPinball](common.md#in-vpinball)). Measured on `disco_l1`: the
  game sent `5F 5F 7F 5F 7F`, AltSound looked up `5F5F 7F5F 7F5F`; with one byte per
  command ([vpinball/libaltsound#20](https://github.com/vpinball/libaltsound/pull/20), in
  VPinball master from 3abe805) it looks up `005F 005F 007F`.

## <a name="sndbrd_s3wcs"></a>SNDBRD_S3WCS

World Cup's System 3 sound board · interface `WMSS67` (sub-type 8|4|2) · ✅ · 1 set,
1978, Williams · World Cup (`wldcp_l1`)

- As above; bits 0-3 of the byte the program reads are the low data nibble, bit 4 data
  bit 6, bit 5 bit 6 of the last control byte (`s67s_ctrl_w`: solenoid 7), bit 6 the
  board's Sound Dip 2, bit 7 data bit 4.
- **The program** (`481_s0_world_cup.716`, interrupt handler `7F15`): with bit 7 low (data
  bit 4 low) it only stores the byte as a prefix flag (`$0B`; a second one clears it, the
  board reset too). Otherwise, inverted: data bit 6 low plays one sound (`7EF5`, whatever
  the nibble); the control line plays another (`7EA0` with `B9`); else the inverted
  nibble minus one picks, after a prefix (or with Sound Dip 2 on), a pitch in the table at
  `7FDA` for the sound at `7EA0` (15 entries, ten different), without one a routine of the
  table at `7FB1`, of which only the first eight are code (four different: `7E89` five
  times, `7E7B`, `7E76`, `7E9B`; the other seven words jump into nothing: nibbles `6`..`0`
  without a prefix are not sent).
- **Sound list** (`s3wcs_sounds`): `10`; `5E 5D 5B 57` (the four routines); the prefix
  `40`, then `5E 5D 5C 5B 57 56 55 54 53 50` (the ten pitches; ids `0x405E`...); `80`
  (the control line): 16 sounds.
- **Measured** (survey settings): wldcp_l1 16 of 16, all from silence, all different (0.1
  to 3.5 s). Before: 16 of 40, all alike (`10`..`1F`: the data bit 6 sound, 2.6 s; the
  sweep stopped at `27` and never reached `50`..`5E`); 0 of 40 before the idle framing.
- **Limits**: no stop (a board reset after each sound). The four direct routines need
  Sound Dip 2 off, the tool's setting (with it on, `5E` plays as `40 5E`).
- **In VPinball**: **the pack does not play as written, in any VPinball so far**: `s4.c`
  logs the inverted solenoid byte, whose bits 5 and 7 follow other lines (it matches the
  pack's ids only while they are clear), the prefix and the command come as two commands,
  the control line is not logged (`sndbrd_ctrl_w`), and up to 10.8.1-5436 libaltsound has
  no case for `GEN_S3` and joins the bytes two by two ([In VPinball](common.md#in-vpinball)).
  Measured on `wldcp_l1`: the game sent `5F 5F 7F 5F 7F`, AltSound looked up `5F5F 7F5F
  7F5F`; with one byte per command
  ([vpinball/libaltsound#20](https://github.com/vpinball/libaltsound/pull/20), in VPinball
  master from 3abe805) it looks up `005F 005F 007F`.

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
