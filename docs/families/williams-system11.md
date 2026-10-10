# Williams System 9 and System 11

The sound boards of Williams System 9 (1984-1985) and System 11 (1986-1991), and of the
few later machines that kept the System 11C board: `SNDBRD_S9S`,
`SNDBRD_S11XS+SNDBRD_S11CS`, `SNDBRD_S11CS` and `SNDBRD_S11BS+SNDBRD_S11JS`. They share
PinMAME's `src/wpc/wmssnd.c` and are driven the same way: one byte per sound, through
a latch and a strobe, with a stop command of their own. What is not said here is the
[common method](common.md).

What is specific to these boards, in short:

- the stop is a command (`BUILTIN_STOPS` in `src/extract.rs`: `00` on `WMSS11` and
  `WMSS11J`, `20` on `WMSS11C`), so a reset is only the fallback;
- their 8-bit DACs are AC-coupled once the game has booted (`ac_couples_dac`, every
  interface starting with `WMSS11`), as the real boards' outputs are;
- where the machine has both a YM2151 and a CVSD, the **chips pass** measures each
  sound's chip, its ducking and its stops (`chip_commands`; see
  [how it works](../how-it-works.md#ducking-stops-and-channels-wpcs-and-system-11));
- no volume stage: full scale, not scaled (`volume::full_scale`).

## The System 11 game side

All System 9 and 11 games run `src/wpc/s11.c`, whose machine init picks the boards from
the hardware generation (`MACHINE_INIT(s11)`, the `switch (core_gameData->gen)`;
mirrored in `src/drivers.rs`, `Board::sound_boards`):

| generation | board 0 (CPU board sound) | board 1 (separate board) | family |
|---|---|---|---|
| `GEN_S9` | `SNDBRD_S9S` | - | `SNDBRD_S9S` |
| `GEN_S11` | `SNDBRD_S11S` | - | (see [SNDBRD_S11S](#sndbrd_s11s)) |
| `GEN_S11X` (= `S11A`, `S11B`) | `SNDBRD_S11XS` | `SNDBRD_S11CS` | `SNDBRD_S11XS+SNDBRD_S11CS` |
| `GEN_S11B2` | `SNDBRD_S11BS` | `SNDBRD_S11JS` | `SNDBRD_S11BS+SNDBRD_S11JS` |
| `GEN_S11C` | - | `SNDBRD_S11CS` | `SNDBRD_S11CS` |

How the game CPU (a 6808) writes a command:

- **Board 0** (`WMSS11` interface: `SNDBRD_S9S`, `S11S`, `S11XS`, `S11BS`): PIA 0 port A
  is `sndbrd_0_data_w` (the board's `soundlatch_w`), its CA2 `pia0ca2_w` →
  `sndbrd_0_ctrl_w`, which is the sound PIA's CA1 (the "Sound H.S." input). The byte is
  latched, then strobed.
- **Board 1** (`WMSS11C`, `WMSS11J`): `pia5b_w` writes the byte with `sndbrd_1_data_w`
  (the board's `cslatch2_w` / `jlatch2_w`), `pia5cb2_w` strobes it with
  `sndbrd_1_ctrl_w` (the sound PIA's CB1 on 11C). On games flagged `S11_SNDOVERLAY`
  (Whirlwind, `s11games.c`) `pia5cb2_w` does not strobe the bytes `00`..`1F`: they drive
  the solenoid overlay board instead (`extSol`), so the board never receives them.

PinMAME's manual command does the same in one call: `s11s_manCmd_w` latches the byte and
pulses CA1 (1 then 0); `s11cs_manCmd_w` latches it and pulses CB1; `s11js_manCmd_w`
latches it and raises the IRQ (`s11js_ctrl_w(0,0)`). rom2altsound sends each byte through
`sndbrd_manCmd` (`Send::Byte`), one every 4 frames; on two-board machines the id is
`board << 8 | byte` (`0x0105`: byte `05` on board 1), as in sounds.dat.

**Sound list.** One sounds.dat section covers this generation: `esha_la3` (Earthshaker,
155 entries, two-byte entries `bb cc` = board, command), which the prefix rule
(`soundsdat::game_entries`) applies to the 9 Earthshaker sets (their parent is
`esha_la3`). Every other game is swept raw: `01`..`FF` per board (`sweep`, the default
range), so 510 commands on a two-board machine. `00` is the stop and is not swept.

**Twins.** Several commands can map to one sound (whirl_l3 `0x0001` = `0x0004`); they are
marked as twins but kept ([how it works](../how-it-works.md#limits)). Repeated sounds are
not sample-identical on 11C (the YM2151 keeps running state), so twins there are rare.

**In VPinball (all System 11 families).** AltSound receives the bytes the game writes
through `sndbrd_data_w` (`snd_cmd_log`). For `GEN_S11`, `S11X`, `S11B2`, `S11C` and
`GEN_WPCALPHA_1`, libaltsound (as `snd_alt.cpp`) makes every byte an 8-bit id and skips a
byte equal to the one before; `GEN_S9` has no case and its bytes are paired (see
SNDBRD_S9S). The board number plays no part: VPinball's AltSound plugin calls
`AltSoundProcessCommand(cmd, 0)` and PinMAME's `alt_sound_handle` does not use it either,
so on a two-board machine a board 1 command `xx` is looked up as `0x00xx`, the id of board
0's byte `xx` (measured on bk2k_l4, bcats_l2, jokrz_l3, [In
VPinball](common.md#in-vpinball)). rom2altsound keeps the `0x01xx` ids and adds a row
`0x00xx` for each board 1 row where board 0 has none
(`altsound::system11_board1_aliases`); a board-aware lookup is proposed in
[vpinball/libaltsound#21](https://github.com/vpinball/libaltsound/pull/21) (draft).
One-board machines' ids are the game's bytes. On `S11_SNDOVERLAY` (Whirlwind) the
overlay's solenoid bytes `00`..`1F` are logged as sound commands too, since `pia5b_w`
calls `sndbrd_1_data_w` before the strobe is withheld (whirl_l3: 37792 board 1 bytes in 40
s of attract mode, looked up as `001F` and `000F`);
[vpinball/pinmame#723](https://github.com/vpinball/pinmame/pull/723) (draft) logs only the
sound commands. No pack has been played in VPinball itself.

## <a name="sndbrd_s9s"></a>SNDBRD_S9S

Williams System 9 sound (CPU board) · PinMAME interface `WMSS11` (`src/wpc/wmssnd.c`,
`s11sIntf`, machine `wmssnd_s9s`) · ✅ · 12 sets, 7 games, 7 sound ROM ids, 1983-1985,
Williams · e.g. Sorcerer (`sorcr_l2`), Space Shuttle (`sshtl_l7`), Comet (`comet_l5`),
Rat Race (`ratrc_l1`, a System 7 machine with the System 9 board, `MACHINE_INIT(rr)`)

- **Hardware**: a 6808 at 1 MHz (`wmssnd_s9s`, 4 KB `MRA_RAM` at `0000`-`0FFF`), a PIA
  whose port B feeds an 8-bit DAC (`DAC_0_data_w`, mixing level 20) and whose CA2/CB2
  clock an HC55536 CVSD (`s9s_hc55516Int`). No YM2151. `s11s_init` applies the per-game
  mixing levels of `hw.gameSpecific2`.
  Pennant Fever (`pfevr_l2`) runs other hardware (`wmssnd_s9ps`: a 6808 with 256 bytes
  of RAM and a DAC only), but its machine init (`MACHINE_INIT(s9pf)`) starts the
  `SNDBRD_S9S` interface all the same; how well the System 9 handlers drive that board
  was not determined, and it was not tried.
- **Commands**: as board 0 above; Rat Race writes its byte with `pia0a_w` (`s7.c`) and
  pulses the PIA's CA1 itself. Sent: single bytes, `sndbrd_manCmd`.
- **Sound list**: no sounds.dat section: `01`..`FF`.
- **Stop, boot and resets**: stop `00` (`BUILTIN_STOPS`, `WMSS11`). Reset fallback: the
  audio CPU's reset line (`shim_reset_audio_cpus`), then 4 s of silence.
- **Volume**: no volume stage (`volume::full_scale`): recorded at full scale, the
  reference, not scaled. DAC AC-coupled after the boot (`shim_dac_ac_couple`).
- **Loops**: audio, then sequencer state (the 6808 and its 4 KB of RAM).
- **DUCK / STOP / CHANNEL**: defaults. The chips pass needs a YM2151 in the mix
  (`chip_commands`: `has(FM_CHIP)`), which this board does not have.
- **Measured**: sorcr_l2 40 of 40 in the quick survey ([board support](../board-support.md)).
  No full run is recorded.
- **Limits**: none known beyond the common ones; only one ROM tried.
- **In VPinball**: **the pack does not play as written**: libaltsound has no case for
  `GEN_S9` and joins the bytes two by two ([In VPinball](common.md#in-vpinball)). Measured
  on `comet_l4`: the game sent `00 00 13 CD CD CD 71 2F`, AltSound looked up `0000 13CD
  CDCD 712F`; with one byte per command
  ([vpinball/libaltsound#20](https://github.com/vpinball/libaltsound/pull/20), draft) it
  would look up `0000 0000 0013 00CD`. `GEN_S9` is not in libaltsound's System 11 case.

## <a name="sndbrd_s11s"></a>SNDBRD_S11S

The System 11 CPU board's own sound, alone (`GEN_S11`: no separate board) · interface
`WMSS11` (`wmssnd_s11s`: the System 9 board with ROM banking, `s11s_bankSelect`) · ✅ · 6
sets in PinMAME 3.7, all Williams shuffle alleys of 1985-1986 (`bowlgames.c`: Alley Cats
`alcat_l7`, Tic-Tac-Strike `tts_l1`, `tts_l2`, Gold Mine `gmine_l2`, Top Dawg `tdawg_l1`,
Shuffle Inn `shfin_l1`).

PinMAME starts `SNDBRD_S11S` for them (`MACHINE_INIT(s11)`, case `GEN_S11`; their
machine `s9_mS11S` imports `s11_s11S`). Until `drivers.rs` read `GEN_S11` as gen.h's
`0x80000000`, rom2altsound's own table put them under `SNDBRD_NONE`. At extraction the
board is read from the running machine (`board_typestr`), so they are driven as
`SNDBRD_S9S` is: `WMSS11`, stop `00`, AC-coupled DAC, sweep `01`..`FF`.

- **Measured** (survey settings, [board support](../board-support.md)): alcat_l7 32 of 40,
  gmine_l2 40 of 40 (27 files, 13 blips), tdawg_l1 40 of 40 (23 files, 17 blips),
  shfin_l1 40 of 40 (30 files, 10 blips), all from silence.
- **Tic-Tac-Strike** (`tts_l1`, `tts_l2`): 0 of 255 (the full sweep), no sound at boot
  either: PinMAME lists both sound ROMs (`tts_u21.256`, `tts_u22.256`) as `NO_DUMP`, so
  the board has no program to run.

## <a name="sndbrd_s11xs_s11cs"></a>SNDBRD_S11XS+SNDBRD_S11CS

System 11, 11A and 11B: the CPU board's sound (board 0) and the separate System 11C board
(board 1) · interfaces `WMSS11` + `WMSS11C` (`wmssnd.c`, `wmssnd_s11xs` and
`wmssnd_s11cs`) · ✅ · 109 sets, 22 games, 27 sound ROM ids, 1985-2026, Williams, Bally ·
e.g. High Speed (`hs_l4`), Pin-Bot (`pb_l5`), Black Knight 2000 (`bk2k_l4`), Whirlwind
(`whirl_l3`)

- **Hardware**: board 0 as `SNDBRD_S9S` with ROM banking (6808, `wmssnd_s11xs`: second
  DAC and second CVSD, PIA variant 1 writes `DAC_1` and `hc55516_1`). Board 1
  (`wmssnd_s11cs`): a 6809 at 2 MHz, 8 KB `MRA_RAM` (`0000`-`1FFF`), a YM2151 (its timer
  drives the 6809's FIRQ through the PIA's CA1, `s11cs_ym2151IRQ`), an 8-bit DAC on the
  PIA's port A and an HC55536 CVSD; mixing levels DAC 100, CVSD 100, YM2151 10.
- **Commands**: see "The System 11 game side". Two boards: commands are (board, byte)
  pairs, ids `0x00xx` and `0x01xx`. Whirlwind is an `S11_SNDOVERLAY` game: its board 1
  never gets `00`..`1F` from the game.
- **Sound list**: `esha_la3` for the 9 Earthshaker sets (155 commands), else
  `01`..`FF` on each board (510 commands).
- **Stop, boot and resets**: `00` on board 0, `20` on board 1, both sent after every
  sound (`stop_sends` adds each board's). Measured on whirl_l3 (`BUILTIN_STOPS` comment):
  `00` cuts a looping sound of board 0 within 0.6 s; board 1 ignores `00` (and on overlay
  games never receives it), and a sweep of all 256 bytes as the stop after the looping
  `0x22` found `20`, `93`, `94`, `98` and `9E` silent within 0.5 s, `20` stopping all 22
  looping commands of the board. Reset fallback: both audio CPUs' reset lines at once.
- **Volume**: no volume stage on either board (wmssnd.c writes the DACs, the CVSD and the
  YM2151 directly; `volume::full_scale`): full scale, factory offset 0. Both DACs
  AC-coupled after the boot. Before AC coupling 187 of whirl_l3's 189 files started more
  than 256 LSB from 0 (up to 10251); with it none (157 LSB at most), same files, no
  clipped file ([how it works](../how-it-works.md#silence)).
- **Loops**: audio, then sequencer state over both CPUs (4 + 8 KB of RAM). whirl_l3: 22
  musics, 14 loops from the state within the default 240 s, 17 with
  `--loop-max-secs 600`, periods 20.6 to 268 s, seam error median 1 / worst 11 LSB;
  `0121` and `0122` repeat after 267.8 s, eight 33.5 s cycles counted by two bytes
  ([how it works](../how-it-works.md#loops)).
- **DUCK / STOP / CHANNEL**: measured by the chips pass (YM2151 and CVSD present). On
  whirl_l3, over music `0166`: 53 of 167 sounds on a voice chip (`CHANNEL` 1), 44 on the
  music chip, none lowers the music, 11 sounds of the music board end it (they go on the
  music channel, `CHANNEL` 0). Without sounds.dat names every voice-chip sound is a
  callout. 1337 s of emulation for the pass.
- **Measured**: whirl_l3 (0.2.1, [how it works](../how-it-works.md#factory-results-for-our-roms)):
  510 tried, 408 with sound, 189 written, 219 blips (DAC steps of 1-2 ms), 22 loops, 102
  retried (0 recovered), no clipped file, -20.4 LUFS all, -17.8 median file, -2.3 dBTP,
  229.6 s wall. Quick survey: bk2k_l4 40 of 40 (36 files, 4 blips, both boards swept),
  whirl_l3 40 of 40 ([board support](../board-support.md)).
- **Limits**: the ids of board 1 do not match what AltSound looks up (see above).
- **In VPinball**: board 0's ids are the game's bytes (`GEN_S11X`, 8-bit ids); board 1's
  commands are looked up as `0x00xx`, board 0's ids (bcats_l2: board 1's `00 55 AA FF`
  handshake as `0000 0055 00AA 00FF`; bk2k_l4: board 1's `98` as `0098`). **The pack has a
  row `0x00xx` for each board 1 row `0x01xx` where board 0 has none**
  (`altsound::system11_board1_aliases`; bk2k_l4's `98` then found its file); where both
  boards have a sound for a byte, board 0's plays. A board-aware lookup is proposed in
  [vpinball/libaltsound#21](https://github.com/vpinball/libaltsound/pull/21) (draft).
  Whirlwind's overlay bytes are looked up too (whirl_l3: `001F` 25 times and `000F` in 40
  s of attract mode); [vpinball/pinmame#723](https://github.com/vpinball/pinmame/pull/723)
  (draft) stops logging them.

## <a name="sndbrd_s11cs"></a>SNDBRD_S11CS

The System 11C board alone · interface `WMSS11C` (`wmssnd.c`, `s11csIntf`,
`wmssnd_s11cs`) · ✅ · 50 sets, 10 games, 15 sound ROM ids, 1988-2026, Bally, Williams ·
e.g. Diner (`diner_l4`), Pool Sharks (`pool_l7`), Bugs Bunny's Birthday Ball (`bbnny_l2`)

- **Hardware**: as board 1 of `SNDBRD_S11XS+SNDBRD_S11CS`. The family also holds games
  of other generations that run the same board: System 11C (`GEN_S11C`, 43 sets),
  Bally's 6803 games Atlantis and Truck Stop (`by6803games.c`, 4 sets) and three WPC
  alphanumeric prototypes with System 11 sound (`GEN_WPCALPHA_1`: Dr. Dude `dd_p06`,
  `dd_p7`, Funhouse `fh_pa1`), as `wpc.c` picks it for `GEN_WPCALPHA_1`.
- **Commands**: on System 11C board 1 only (`sndbrd_1_init`), so the machine has one
  board at index 1: `target` sends everything there, the ids are single bytes. Pool
  Sharks (`S11_SNDDELAY`) drops the first 7 bytes it receives (`s11cs_init`,
  `cslatch2_w`), which the game's boot uses up.
- **Sound list**: `01`..`FF`. `fh_pa1` takes the `fh_l9` section of sounds.dat by the
  parent rule, which lists the WPCS commands of the production Funhouse, not this
  board's: not checked.
- **Stop, boot and resets**: `20` (`BUILTIN_STOPS`, `WMSS11C`), reset fallback.
- **Volume**: full scale, not scaled; DAC AC-coupled.
- **Loops**: audio, then sequencer state (8 KB).
- **DUCK / STOP / CHANNEL**: measured by the chips pass (YM2151 and CVSD on the board).
- **Measured**: diner_l4 30 of 40, all from silence, in the quick survey (the first run
  of this board alone; [board support](../board-support.md)).
- **Limits**: only diner_l4 tried; the Bally 6803 and WPC prototype sets were not.
- **In VPinball**: on `GEN_S11C` and `GEN_WPCALPHA_1` machines, **plays as written**
  (8-bit ids, a byte equal to the one before is skipped). The Bally/Midway 6803 machines
  with this board (atlantis, trucksp3, `GEN_BY6803A`) do not: libaltsound pairs their
  bytes (atlantis: `64 7E 90 90` looked up as `647E 9090`); with
  [vpinball/pinmame#722](https://github.com/vpinball/pinmame/pull/722) and
  [vpinball/libaltsound#20](https://github.com/vpinball/libaltsound/pull/20), `0064 007E
  0090`. Not played in VPinball itself.

## <a name="sndbrd_s11bs_s11js"></a>SNDBRD_S11BS+SNDBRD_S11JS

System 11B with the Jokerz! board · interfaces `WMSS11` + `WMSS11J` (`wmssnd.c`,
`wmssnd_s11b2s` and `s11jsIntf` / `wmssnd_s11js`) · ✅ · 3 sets, 1 game, 1 sound ROM id,
1989, Williams · Jokerz! (`jokrz_l6`)

- **Hardware**: board 0 (`wmssnd_s11b2s`): the 6808 board with a DAC (#1, "unused except
  for Jokerz boot sound?" in the source) and the HC55536. Board 1 (`wmssnd_s11js`): a
  6809 at 2 MHz, 8 KB `MRA_RAM`, a YM2151 in stereo (left/right, `s11js_ym2151Int`), no
  DAC or CVSD of its own (its DAC write `s11js_dac_w` only logs).
- **Commands**: board 1 takes the byte in `jlatch2_w`, and the game's strobe
  (`s11js_ctrl_w`) raises the 6809's IRQ, cleared when it reads the port. The first 10
  bytes the board receives are dropped (`s11js_init`: `ignore = 10`). `s11js_manCmd_w`
  latches and raises the IRQ (the source comments "the manual commands are not passed
  through, why???"); the quick survey got sounds from both boards anyway. Ids
  `0x00xx`/`0x01xx`.
- **Sound list**: `01`..`FF` on each board.
- **Stop, boot and resets**: `00` on both (`BUILTIN_STOPS`: `WMSS11J` is an unmeasured
  guess; the reset fallback covers a wrong one).
- **Volume**: full scale, not scaled; DAC AC-coupled.
- **Loops**: audio, then sequencer state.
- **DUCK / STOP / CHANNEL**: the chips pass runs (the mix has a YM2151 and a CVSD);
  results not recorded for this game.
- **Measured**: jokrz_l6 40 of 40 (38 files, 2 blips) on both boards, quick survey
  ([board support](../board-support.md)).
- **Limits**: the stop of board 1 is not measured.
- **In VPinball**: as SNDBRD_S11XS+SNDBRD_S11CS (`GEN_S11B2`; jokrz_l3: board 1's
  handshake looked up as `0000 0055`): board 1 rows get their `0x00xx` alias where board 0
  has none.
