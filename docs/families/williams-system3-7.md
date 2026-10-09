# Williams System 3 to 7

The sound boards of Williams System 3, 4, 6 and 7 (1977-1984) and their late
conversions: `SNDBRD_S67S`, `SNDBRD_S3S`, `SNDBRD_S3DFS`, `SNDBRD_S3WCS` and
`SNDBRD_S7S_ND`. They are one PinMAME interface, `WMSS67` (`src/wpc/wmssnd.c`,
`s67sIntf`), told apart by a sub-type that changes how the command byte is read. What is
not said here is the [common method](common.md).

None of these families works today (⚠️ or ❌ in [board support](../board-support.md)),
for one reason, read below in `s67s_cmd_w`: **the board starts a sound on the change
from an idle value to a command**, and rom2altsound sends commands back to back without
the idle value in between. The fix is the first of the "cheapest fixes" of board
support (113 sets).

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

## What rom2altsound does today

- **Sending**: one byte per command through `sndbrd_manCmd` (`board_sends`, the default
  path), one every 4 frames; nothing in `src/extract.rs` knows the idle protocol.
- **Sound list**: no sounds.dat section except `frpwr_l2` (which the prefix rule gives to
  `frpwr_l2` and `frpwr_l2ff`): the default sweep `01`..`FF` (`sweep`). On sub-type 0
  only the low five bits reach the program, so the 255 bytes are 31 commands, each up to
  8 times, and command `00` (Firepower's "FIREPOWER/start") is only reached as `20`,
  `40`...
- **Stop, boot and resets**: none of `WMSS67` is in `BUILTIN_STOPS` and there is no
  sounds.dat `wmss67` section: **the stop is a board reset** after every sound
  (`shim_reset_audio_cpus`, the 6808's reset line), then 4 s of silence. The reset
  restarts the program but not the PIA's input lines, so CB1 stays as the last command
  left it.
- **What comes out**: a command plays only when the byte before it was idle-looking. The
  quick survey is consistent with that reading. bk_l4: 3 of 40 (its boot ends on the
  idle `7F`, so `01` plays; `1F` resets CB1 and `20` plays; in the retry pass `1F` again
  makes `21` play). thund_p1 (`S7S_ND`): 0 of 40, its boot ending on `26` (CB1 active)
  and no byte up to `28` being `7F`. phnix_l1 and wldcp_l1 0 of 40, disco_l1 1 (a 0.35 s
  file): their boots end on `7F`, so the first command should make an edge; why it plays
  nothing there was not determined from the code (the sub-type 2 bits: data bit 4 lands
  in bit 7 of the byte the program reads, and `01`..`0F` have it clear).
- **Volume**: no volume command known and no volume stage listed (`volume::full_scale`
  does not name `WMSS67`): `reference_volume` is "none: recorded at the game's own
  volume", not scaled. The DAC is **not** AC-coupled (`ac_couples_dac` covers only WPCS and
  `WMSS11*`): the WAVs keep the DC of PinMAME's unsigned DAC; levels are measured
  DC-blocked.
- **Loops**: audio, then sequencer state (the 6808's registers and its 128 bytes of RAM).
- **DUCK / STOP / CHANNEL**: defaults (the chips pass is for `WPCS` and `WMSS11*` only).

## What the fix is

From `s67s_cmd_w`: send the idle value **after** every command (as sounds.dat's
`frpwr_l2` does), and once before the first one, so that each command is a change from
idle:

- idle `1F` and commands `00`..`1E` on sub-type 0 (`SNDBRD_S67S`) and on the System 3
  sub-types, whose idle test (`& BF == BF`) is met by data `1F` (bits 0-4 set) with the
  game's usual bits 5-7 (`7F`, `FF`);
- idle `7F` and commands `00`..`7E` on `SNDBRD_S7S_ND`;
- the sweep then lists each distinct command once (the DIP bits are not the game's),
  with `00` included.

No inversion is needed on the tool's side: the games invert their solenoid byte before
`sndbrd_0_data_w`, and `manCmd_w` is the same `s67s_cmd_w`, so the bytes the tool sends
are already in the board's polarity. The extra bit of World Cup and Disco Fever comes
from `s67s_ctrl_w`, which `sndbrd_manCmd` never calls: their commands that need the other
value of that bit would also need a `sndbrd_ctrl_w` write. A real stop is still not
known: the sweep would keep the reset as its stop.

## <a name="sndbrd_s67s"></a>SNDBRD_S67S

Williams System 4, 6 and 7 sound board · interface `WMSS67` (sub-type 0) · ⚠️ · 105
sets, 38 games, 28 sound ROM ids, 1978-2022, Williams, Williams / Oliver · e.g. Gorgar
(`grgar_l1`), Firepower (`frpwr_l6`), Black Knight (`bk_l4`), Jungle Lord (`jngld_l2`)

- **Hardware, commands, list, stop, volume, loops, columns**: as above. The family also
  holds the System 4 shuffle alleys (`bowlgames.c`), three Zaccaria sets with an
  alternate Williams-type sound board (`stargoda`, `stargodaf`, `ewfa`, `zacgames.c`),
  and Big Strike / Triple Strike (`bstrk_l1`, `tstrk_l1`), whose machine (`s4_mS4`,
  chimes) has no sound CPU although `MACHINE_INIT(s4)` starts `SNDBRD_S67S` by default:
  nothing there can play.
- **Measured**: bk_l4 3 of 40 ([board support](../board-support.md)). No full run.
- **Limits**: the idle protocol (above). With it, sub-type 0 has 31 commands per game.
- **In VPinball**: no generation preprocessing in `snd_alt.cpp` for System 3 to 7: every
  byte the game writes, the idle bytes included, is an id of its own, with bits 5-7 as
  the game's solenoid lines leave them. A pack from today's sweep has an id for every
  byte, but its files are not the right sounds (most are silent or missing); the
  `frpwr_l2` section gives two-byte ids (`0x001F`) that AltSound never receives. Not
  tested.

## <a name="sndbrd_s3s"></a>SNDBRD_S3S

Williams System 3 sound board · interface `WMSS67` (sub-type 2) · ❌ · 3 sets, 3 games,
3 sound ROM ids, 1978, Williams · Contact (`cntct_l1`), Phoenix (`phnix_l1`), Pokerino
(`pkrno_l1`)

- As above (sub-type 2: data bits 0-3 and 4, a DIP switch in bit 6). Phoenix and
  Pokerino run `s4.c` (`GEN_S4`), Contact `GEN_S3`.
- **Measured**: phnix_l1 0 of 40, boot `FF FF 7F` ([board support](../board-support.md)).
- **Limits**: the idle protocol; why the first command after the idle boot is silent was
  not determined.
- **In VPinball**: as `SNDBRD_S67S`. Not tested.

## <a name="sndbrd_s3dfs"></a>SNDBRD_S3DFS

Disco Fever's System 3 sound board · interface `WMSS67` (sub-type 16|4|2) · ⚠️ · 1 set,
1978, Williams · Disco Fever (`disco_l1`)

- As above; bit 4 of the byte the program reads is data bit 6, bit 7 is bit 4 of the last
  control byte (`s67s_ctrl_w`, solenoids 1-8 through `s4_sol1_8_w`), which a manual
  command cannot set. PinMAME also defines a prototype variant, `SNDBRD_S3DFPS`
  (sub-type 4|2, without the control bit; `sndbrd.h`), which is not a family of the
  survey.
- **Measured**: disco_l1 1 of 40 (a 0.35 s file); boot `FF FF 7F`.
- **Limits**: the idle protocol, plus the control bit (above).
- **In VPinball**: as `SNDBRD_S67S`. Not tested.

## <a name="sndbrd_s3wcs"></a>SNDBRD_S3WCS

World Cup's System 3 sound board · interface `WMSS67` (sub-type 8|4|2) · ❌ · 1 set,
1978, Williams · World Cup (`wldcp_l1`)

- As above; bit 5 of the byte the program reads comes from bit 6 of the last control
  byte (`s67s_ctrl_w`), which a manual command cannot set.
- **Measured**: wldcp_l1 0 of 40; boot `FF FF 7F` (the full set's zip is complete).
- **Limits**: the idle protocol, plus the control bit.
- **In VPinball**: as `SNDBRD_S67S`. Not tested.

## <a name="sndbrd_s7s_nd"></a>SNDBRD_S7S_ND

The System 7 board of Thunderball (no sound DIPs, seven command bits) · interface
`WMSS67` (sub-type 1, `MACHINE_INIT(s7nd)` in `s7.c`) · ❌ · 3 sets, 1 game, 1 sound ROM
id, 1982, Williams · Thunderball (`thund_p1`, a prototype)

- As above, with seven bits: commands `00`..`7E`, idle `7F`.
- **Measured**: thund_p1 0 of 40; boot `7F 19 7F 26`: the commands sent between idle
  bytes ([board support](../board-support.md)).
- **Limits**: the idle protocol (idle `7F`); the default sweep `01`..`FF` also holds every
  command twice (bit 7 is dropped).
- **In VPinball**: as `SNDBRD_S67S`. Not tested.
