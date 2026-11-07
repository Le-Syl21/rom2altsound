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
  `FF 00 FF 1F`. Since 0.3 the sweep follows the section (as on every board but DCS):
  a section command and a swept byte are the same command when they differ only by the
  trailing idle byte (`Extractor::list_key`: `001F` is `00`), so Firepower plays each of
  its 31 commands once, under the section's ids and names. On World Cup and Disco Fever, `80` in a command is the control line
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
read bit 6, the board's Sound Dip 2 (s4.h, DIP bank 0 `02`, on in PinMAME's own default:
bit 6 low; VPinball's controller loads it off, measured below), to choose between two paths. **The tool turns Sound Dip 2 off on these two
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
- **In VPinball**: **the pack plays from VPinball master 3abe805 on, with the rows for the solenoid lines (below)**: the logged bytes keep bits 5-7 as the solenoid lines leave them (bk_l4 `2C` for `0C`), the pack's ids are the command bits; and up to 10.8.1-5436 libaltsound has no case for
  `GEN_S6 / GEN_S7` and joins the bytes two by two ([In VPinball](common.md#in-vpinball)).
  Measured on `algar_l1`: the game sent `5F 5F 7F 5F 5F 7F 77 7F`, AltSound looked up
  `5F5F 7F5F 5F7F 777F`; with one byte per command
  ([vpinball/libaltsound#20](https://github.com/vpinball/libaltsound/pull/20), in VPinball master from 3abe805) it
  looks up `005F 005F 007F 005F`. Every byte the game writes, the idle bytes
  included, is logged with bits 5-7 as the solenoid lines leave them; the `frpwr_l2`
  section gives two-byte ids (`0x001F`) that AltSound would not look up either way.
  The pack now also carries the rows `0x20|xx`..`0xE0|xx` for every five-bit row (same
  file; `solenoid_line_aliases`), so a command logged with any bits 5-7 finds its sound
  (System 3 to 7 boards of sub-type 0 and 2). Played in VPinball 10.8.1-6577: Black Knight
  (`bk_l4`, ids `0x23`..`0x3B`) and Contact (`cntct_l1`, `0x77`..`0x7E`) find their sounds.

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
- **In VPinball**: **the pack plays from VPinball master 3abe805 on, with the rows for the solenoid lines (below)**: the logged bytes keep the solenoid bits 5-7 (`s4.c` `s4_sol9_16_w`), the pack's ids are the command bits; and up to 10.8.1-5436 libaltsound has no case for
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
  Dip 2 off, the tool's setting (PinMAME's own default has it on, VPinball loads it off).
- **In VPinball**: **the pack does not play in any VPinball build so far; with the PinMAME
  patch [0007](../pinmame-patches/README.md) (not upstream) it plays**. `s4.c` logs the inverted
  solenoid byte (bits 5 and 7 follow other lines, bit 4 is unused by the board), the control
  line is not logged at all (`sndbrd_ctrl_w`), and up to 10.8.1-5436 libaltsound has no case
  for `GEN_S3` and joins the bytes two by two ([In VPinball](common.md#in-vpinball)); from
  master 3abe805 it takes one byte, which is still not the pack's id. Measured on `disco_l1`
  before the patch: the game sent `5F 5F 7F 5F 7F`, AltSound looked up `5F5F 7F5F 7F5F`.
  What the game writes: the data byte `7F` (idle) most of the time, `7E 7D 7B 77` for its
  sounds (nibble, data bit 6 high), and solenoid 5 (`EF` on the control line) pulsed *before*
  some of them: the flag, then the command, as the pack's `800E`..`8007` rows.
  The patch (shared with World Cup, one function in `wmssnd.c`) follows the program: at each
  rising edge of CB1, from either port, it logs the id of the sound the program plays. The
  control line alone sets the flag and logs nothing; then `0x40 | nibble` for a numbered sound
  (data bit 6 high, Sound Dip 2 off, no flag), else the priority decoder's `0E 0D 0B 07` or
  `0F`, plus `0x8000` after the control line (nothing for `0F` with it: the table entry is the
  word `0000`). **The ids follow the DIP**: with Sound Dip 2 on, the numbered sounds do not
  exist and the same bytes log the priority rows. They are exactly the pack's ids, no alias
  rows. (`Aliases::SolenoidLines` used to add 31 rows for `0x2E`, `0x6E`... to this pack by
  mistake, see the CHANGELOG; they are gone.)
  Played in VPinball 10.8.1-6577 with the patched libpinmame (Docker bench, a table script
  closing each of the switches 1 to 40 once, 0.5 s each, 3 s apart; the DIP read with
  `Controller.Dip(0)` is 0 as VPinball loads it, Sound Dip 2 off, then set to 2, on):
  57 lookups, 57 found, none missed (Sound Dip 2 off: `004B`, `000F`, `800E` x2; on: `000B` x24,
  `000D` x20, `000E` x5, `000F`, `800E` x3). A first run, with the patch tracking the flag
  itself instead of reading it from the program's RAM, gave 67 of 67 and also `004D` x10,
  `004E` x2. A switch that sounds
  `000B` repeats it 5 to 8 times, 0.1 to 0.25 s apart: the game's own rhythm (one CB1 edge
  each), logged as it is. Not reached by the game session:
  the other numbered ids (`0x40`..`0x4A` but `4B`, `4D`, `4E`), `0007`, `800D`, `800B`, `8007`
  (seen once in an earlier, unpatched-pack session); the audio itself is not listened to.

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
- **In VPinball**: **the pack does not play in any VPinball build so far; with the PinMAME
  patch [0007](../pinmame-patches/README.md) (not upstream) it plays**. Unpatched, `s4.c`
  logs the inverted solenoid byte (bits 5 and 7 follow other lines) and the control line,
  `sndbrd_ctrl_w`, not at all (the game raises solenoid 7 with no data write: switches 28
  and 35 of the table); and up to 10.8.1-5436 libaltsound has no case for `GEN_S3` and joins
  the bytes two by two ([In VPinball](common.md#in-vpinball)). Measured on `wldcp_l1`
  before the patch: the game wrote `7F`, `FF` and `5F` (idle for the board) on the data
  lines and its sounds as `5E 5D 5B`; AltSound looked up `007F`, `00FF`, `005F`, never a row.
  The patch logs, at each rising edge of CB1 from either port, the id of the sound the
  program plays (the program's prefix flag is followed too): a prefix (data bit 4 low) logs
  nothing and toggles the flag; data bit 6 low `10`; the control line `80`; with the flag set
  or Sound Dip 2 on, a pitch `0x405x` (the nibbles sharing an entry of the pitch table, `8 9 A
  C`, `1 5` and `2 4`, as one row: `C`, `5`, `4`); else the routine `5E 5D 5B 57` (`8 9 A C`
  play `5E`'s; the nibbles `0`..`6` jump into nothing and log nothing). These are the pack's own ids, no alias
  rows. **The ids follow the DIP**, which answers what plays for the game's `5E 5D 5B`: with
  Sound Dip 2 off (as VPinball loads it, `Controller.Dip(0)` = 0, also with no saved configuration) the four direct routines,
  the pack's `5E 5D 5B` rows; with it on (PinMAME's own default in `s4.h`, `Controller.Dip(0)
  = 2`) the program turns the same bytes into pitches, the rows `405E`, `405D`, `405B`. The pack
  holds both sets and the log picks the right one, so neither needs to be copied over the other.
  Played in VPinball 10.8.1-6577 with the patched libpinmame (Docker bench, a table script
  closing each of the switches 1 to 40 once, 0.5 s each, 3 s apart, with Sound Dip 2 off and
  then on): 160 lookups, 160 found, none missed. Off: `005E` x16, `005D` x37, `005B` x2, `0080`
  x10 (+1 at boot), and the game's tune `405E` x8, `405D`, `405B`, `4057` (the game sets the
  program's prefix flag itself); on: `405E` x28, `405D` x15, `405B` x4, `4057` x3, `0010` x3,
  `0080` x10. A first run, with the patch tracking the flag itself, gave 182 of 182. The control line comes as 4 pulses 0.1 s apart (switches 28, 35: the
  game's own rhythm, one CB1 edge each; the program restarts its sound on every edge, and in
  AltSound's log each new play finds channel 0 free again), so each is logged: a dedup by id
  would also merge the tune's eight equal notes. Not reached by the game session: `57`, the pitches
  `405C`, `4056`..`4050`; the audio itself is not listened to.

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
