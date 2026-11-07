# PinMAME changes for AltSound on the "raw" families

Seven small patches against PinMAME (`445b3cda`, the `rom2altsound-0.5` submodule head). Each one
changes only what PinMAME logs as a sound command (`snd_cmd_log`, the bytes VPinball hands to
AltSound), never what the sound board receives, so the emulated audio is unchanged. They are
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

All seven apply together (`git am`, no conflict) and the result builds as `libpinmame`.

## Measured in VPinball 10.8.1-6577 (Docker bench, table script driving the game)

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

Not checked: the audio itself (the bench has no listening path), the ids no game session reached.
