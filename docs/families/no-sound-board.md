# Machines without a sound board

The `SNDBRD_NONE` sets that are not Stern SAM (the "NONE (other)" row of
[board support](../board-support.md)). Stern SAM is also `SNDBRD_NONE` in PinMAME but is
read from its ROM image instead: see [Stern SAM](stern-sam.md#sndbrd_none_sam). The
common method is in [common.md](common.md); here it stops before the first command.

## <a name="sndbrd_none_other"></a>SNDBRD_NONE (other)

No sound board interface · no PinMAME interface · status — · 257 sets, 172 games, 17
sound ROM ids, 1974-2025 · makers include Bally, LTD, Juegos Populares, Gottlieb, Recel,
Splin, Sleic, MAC, Allied Leisure, Stern, Williams · e.g. the Bally -17 games
(`by35games.c`), LTD (`ltdgames.c`), Recel (`recelgames.c`), Juegos Populares
(`jpgames.c`), the bingo machines (`bingo.c`)

- **What these machines are**: sets whose machine init starts no sound board in PinMAME's
  sound board layer (`src/wpc/sndbrd.c`): `core_gameData->hw.soundBoard` is 0 and the
  CPU family's machine init picks none (`src/drivers.rs`, `Board::sound_boards` returns
  nothing, `Board::family` "SNDBRD_NONE"). From PinMAME's driver table
  (`--dump-table`): 100 of the 257 have no sound chip at all (chimes or bells, which
  PinMAME drives as solenoids or not at all), the others have sound on the CPU board or a
  sound CPU the driver runs by itself, without the board interface (AY-3-8910 on 53 sets,
  discrete components on 43, an AY-3-8910 and an MSM5205 on 12, YM3812 + MSM6295 on 10,
  a DAC and an HC55516 on 6...). The largest groups by driver file: `by35games.c` (30),
  `ltdgames.c` (26), `bingo.c` (25), `recelgames.c` (16), `jpgames.c` (15),
  `peypergames.c` (14), `stgames.c` (12), `mac.c` (12), `allied.c` (11), `gts1games.c`
  (11).
- **Commands**: there is no `sndbrdIntf`, so no `manCmd_w` to call and no
  `sndbrd_data_w` stream: the game's sound comes from its own CPU writes to the chips
  (or from solenoid outputs), with no command number anywhere.
- **What rom2altsound does**: the boot runs as usual, then `Extractor::end_boot` checks
  the boards: `board_mask()` (PinMAME's `sndbrd_exists(0)` / `(1)`) is 0, so the run ends
  with the error "no sound board on this machine" before any command is sent. (A machine
  with a board whose interface has no manual handler ends there too, with "has no manual
  command handler in PinMAME: nothing can be driven", `shim_board_has_mancmd`.) No file,
  no pack.
- **Sound list, stop, volume, loops, columns**: none.
- **Measured**: nothing to measure; 218 of these sets are complete in the full VPinMAME
  set ([board support](../board-support.md)).
- **Limits and what is missing**: supporting any of them would mean driving the game's
  own sound code, which has no command interface: for a machine with a sound CPU run by
  its driver, finding where the game CPU hands it work (a latch in the driver's memory
  map, as the shim hooks for Pinball 2000) and logging it; for chimes and tone outputs,
  recording the solenoid or line patterns, which are not sound commands. Nothing of this
  exists in rom2altsound.
- **In VPinball**: PinMAME's AltSound receives nothing from these machines (no
  `snd_cmd_log` calls), so there is no pack to play.
