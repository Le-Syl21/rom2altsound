# PinMAME changes for AltSound on the "raw" families

Thirteen small patches against PinMAME (`445b3cda`, the `rom2altsound-0.5` submodule head). Each one
changes only what PinMAME logs as a sound command (`snd_cmd_log`, the bytes VPinball hands to
AltSound), never what the sound board receives, so the emulated audio is unchanged (patch 0012 is
the one exception: it changes what the Mr. Game ROM is told at its first power up, see below). They are
**not upstream**: no pull request has been opened. VPinball builds that include them are needed
before these families play (see `docs/vpx_playback.json`, which describes the builds that exist).

| Patch | Family | What PinMAME logs after it |
|---|---|---|
| 0001 `gp.c` | Game Plan MSU-3 | one byte per command, assembled from the nibbles the board shifts in |
| 0002 `by35.c` | Hankin | the sound nibble on the `CA2` strobe, not every write of the shared lines |
| 0003 `byvidpin.c` | Baby Pac-Man, Granny and the Gators (BY45BP) | the two nibbles assembled into one byte on the `CB2` fall |
| 0004 `tabart.c` | Tabart L'Hexagone (sub-type 0) | the switch the sound program sees close (BCD), and the chime lines |
| 0005 `play.c` | Playmatic Cerberus (PLAY3) | the pair of 3-bit values the board reads, as one byte |
| 0006 `zac.c` | Zaccaria 1311 (ZAC1311) | the set of the four tone lines (solenoids 21-24) that are on, as one byte, `00` = all off |
| 0007 `wmssnd.c` | Williams System 3 World Cup (S3WCS) and Disco Fever (S3DFS) | the id of the sound the board's program starts at each CB1 edge, from the data lines or the control line, following its flag and Sound Dip 2 (`0x10` `0x4050`..`0x405E` `0x50`..`0x5E` `0x80`; `0x0E`.. `0x40`..`0x4E` `0x800E`..) |
| 0008 `capcoms.c` | Capcom (CAPCOMS: Airborne, Big Bang Bar, Breakshot, Flipper Football, Kingpin, Pinball Magic) | the sample number of each sound, taken from the game's serial message `DA 04 07 vv nnnn` (`06` loops), one value per sound |
| 0009 `play.c` | Playmatic Zira (PLAYZ) | the command lines (bits 4-6 of the lamp port) each time they take a non-idle value, as `10` `20` .. `70` (the game's `sndbrd_ctrl_w` was never logged) |
| 0010 `spinb.c`, `inder.c` | Spinball (bushido, mach2, jolypark, vrnwrld, gunshot) and Inder (lapbylap to metalman) | the byte the game writes to the sound latch, once, when the latch changes to a value with bit 7 set (`00` releases it); Lap By Lap's NMI board: each write |
| 0011 `zac.c` | Tecnoplay Scramble (TECHNO) | each command once, as the first byte of the pair of writes the game sends (`28 A8` -> `28`, `FE BE` -> `FE`), and nothing for the idle pair `3F BF` |
| 0012 `mrgame.c` | Mr. Game (dakar, motrshow, macattck, wcup90), first start | not a log change: with no nvram file the ROM finds a bad CMOS checksum and loops in an error state unless the operator switch CPU SW1-1 is read "off" (hard reset) on its first read; the first read now answers "off", as an operator would, so the game starts instead of showing no lamp and no solenoid |
| 0013 `mrgame.c` | Mr. Game (MRGAME) | the 7 low bits the game writes to the sound latch while bit 7 is cleared, once per command, on the edge that fires the sound CPUs' NMI (`sndbrd_data_w` is never called on this board) |

All thirteen apply together (`git am`, no conflict) and the result builds as `libpinmame`.

## Measured in VPinball 10.8.1-6577 (Docker bench, table script driving the game)

- **Mr. Game** (`motrshow`, patches 0012 and 0013; real table "JP's Motor Show", Docker bench, coin, start and flippers
  by keystrokes, the table's own script otherwise): without patch 0012 no game of the family leaves its error loop on a
  fresh nvram (`dakar` and `motrshow` show no lamp and no solenoid, in PinMAME 3.6 as in master; with it the lamps and
  solenoids run). With both patches AltSound made 57 lookups in one game, 56 found (11 distinct ids: `15`, `55`,
  `14`, `71`, `56`, `48`, `0F`, `0E`, `72`, `16`, `08`, `03`) and one miss, `6F`, an id the pack has no sound for.
  Command ids seen in the PinMAME harness: `dakar` `72` `12` `08` `73`, `macattck` `6F` `13`.
- **Playmatic Cerberus**: 349 000 lookups and none found, then 74 of 74 non-idle lookups found.
- **Game Plan MSU-3** (Andromeda): `0C`, `07`, `08`, `09`, `01`, `06` found; `04` is a silent row.
- **Hankin**: `empsback`, `fjholden`, `shark`, `howzat` play and look up clean nibbles (9 to 11
  pack ids each); Orbit 1 (`orbit1`) is in its own entry below.
- **Orbit 1** (`orbit1`, Hankin patch 0002, shell of the FJ Holden table, switches 1 to 64 closed one at a time, twice, 330 s):
  52 lookups found, ids `0001` `0002` `0003` `0004` `0005` `0009` `000A` `000C` `000F`; 5 lookups of `0000` failed (no row by design); `0006` `0007` `0008` `000B` not reached.
- **Winter Sports** (`wsports`, `wsportsf`, patch 0006, shell of the House of Diamonds table, same sweep, 330 s): 328 and 432 lookups, all found, none failed, ids `0000` `0001` `0002` `0004` `0008`.

- **Baby Pac-Man** 15 of 46 ids and **Granny** 30 of 48 reached by a random walk; whole bytes.
- **Tabart L'Hexagone**: 27 of 27 scripted switches found, with the alias rows of
  `Aliases::SwitchRoutines`; the game never raises a chime line, so the chime rows are never looked up.

- **Zaccaria 1311** (`futurwld`, `hod`; Winter Sports in its own entry below): before the patch, 0
  lookups in 150 s of play (coin, start, plunger, flippers); after it, with the generated packs, 228
  lookups for Future World and 212 for House of Diamonds, all found, none failed, ids `0000` `0001` `0002` `0004` `0008` only (one
  line at a time). The pack's rows `0001`..`000F` are the tones and `0000` a silent row on the
  same channel; see `docs/families/zaccaria.md`.

- **World Cup and Disco Fever** (`wldcp_l1`, `disco_l1`, patch 0007; table scripts closing each of
  the switches 1 to 40 once, with Sound Dip 2 off and then on, set with `Controller.Dip(0)`): World
  Cup 160 lookups, 160 found (`005E` `005D` `005B` `0080`, and with the DIP on or the game's own
  prefix `405E` `405D` `405B` `4057`, `0010`), Disco Fever 57 lookups, 57 found (DIP off `004B`
  `000F` `800E`, on `000B` `000D` `000E` `000F` `800E`; an earlier run also `004D` `004E`), none
  missed in either; before, no row ever matched. Beyond the game sessions, the tool's sweep with the
  patched PinMAME logs, for each of the 16 and 24 sounds it records (DIP off), exactly that sound's
  pack id.

- **Capcom** (patch 0008; `pmv112`, `bbb109`, `bsv103`, `ffv104`, `abv106`, `kpb105`; table scripts closing
  coin (`Controller.Switch(1)`) and start (`7`), then every switch 11 to 88 once; a dummy pack with every
  id `0000`..`03FF`): with stock PinMAME the same `pmv112` run (128 switch closures) gave 0 lookups; with the patch, `pmv112` 64 lookups (33 ids, `0009`..`00E7`), `bbb109` 95 (56 ids, up
  to `027A`), `bsv103` 194 (41 ids, up to `00B4`), `ffv104` 109 (33 ids, up to `0168`), `abv106` 98 (46
  ids, up to `0212`), all found, none failed. Kingpin (`kpb105`) got 1 lookup (`0007`) from switches alone; the game does not get far (a headless run showed `DA 04 07 29 00 75`, logged as `0075`). The ids are the sample numbers
  the pack's rows are keyed on (`0000`..`03FF`). The cabinet switches are `1`-`8` (coin `1`-`4`, start
  `7`), not the `91`-`98` of the core's numbering, which the keyboard port overwrites.
- **Playmatic Zira** (`zira`, patch 0009; the table's own script plus a switch sweep, 330 s): before,
  0 lookups in 150 s (coin, start, flippers); after, 644 lookups, 301 found (`0030` 3, `0040` 291,
  `0060` 7) and 343 missed, all of them ids the pack has no row for (`0010` 23, `0020` 315, `0050` 5:
  the three values the generated pack leaves out as silent); `0070` was not reached.
- **Spinball and Inder** (patch 0010; table shells with `cGameName` set, a test pack with a row
  for every id `0000`..`01FF`, trough, three coins, Start, then the other switches; the
  emulation runs about ten times slower than real time in the container, so each press is
  held for seconds): before, 0 lookups in 200 s with coins pressed (`jolypark`, `metalman`,
  `lapbylap`). After: `jolypark` 132 of 132 found (`85` coin x3, `8C` attract music, `DE`,
  127 `C4` volume steps), `bushidoa` 3 of 3 (`85`), `metalman` 3 of 3 (`85`), `ind250cc` 7 of 7
  (`85` x3, `8F` x4), `lapbylap` 5 of 5 (`09`, `0A`, `0C` x3). No game started (Start never took,
  natively or in VPinball), so a game in play was not seen. Brave Team and Canasta '86 are not
  covered (the game's own code makes their sounds; see `docs/families/other-makers.md`).
- **Tecnoplay Scramble** (`scram_tp`, patch 0011; table script closing the switches 9 to 64 once, plus
  coin, start, plunger and flippers): before, every command was looked up as its two writes in two
  forms (`0028` `00A8` `0028` `00A8` for a command the game sends twice, `00FE` `00BE`) plus the idle
  pair `003F` `00BF`: 43 lookups, 34 found, 9 missed (the idle ids, which have no row). After: 20
  lookups, 20 found, none missed, ids `0028` `002D` `00FE` `0027` `001D` `0036` `0038` `0031` `002C`
  `0029` (a command the game sends twice is two lookups, as the board takes it twice). The pack
  needed no change: it already has a row for each byte, and the first byte of a pair has the row of
  the sound the board plays (`FE` is the TMS7000 sample).

Not checked: the audio itself (the bench has no listening path), the ids no game session reached.
