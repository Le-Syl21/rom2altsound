# PinMAME changes for AltSound on the "raw" families

Six small patches against PinMAME (`445b3cda`, the `rom2altsound-0.5` submodule head). Each one
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

All six apply together (`git am`, no conflict) and the result builds as `libpinmame`.

## Measured in VPinball 10.8.1-6577 (Docker bench, table script driving the game)

- **Playmatic Cerberus**: 349 000 lookups and none found, then 74 of 74 non-idle lookups found.
- **Game Plan MSU-3** (Andromeda): `0C`, `07`, `08`, `09`, `01`, `06` found; `04` is a silent row.
- **Hankin**: `empsback`, `fjholden`, `shark`, `howzat` play and look up clean nibbles (9 to 11
  pack ids each); Orbit 1 has no table.
- **Baby Pac-Man** 15 of 46 ids and **Granny** 30 of 48 reached by a random walk; whole bytes.
- **Tabart L'Hexagone**: 27 of 27 scripted switches found, with the alias rows of
  `Aliases::SwitchRoutines`; the game never raises a chime line, so the chime rows are never looked up.

- **Zaccaria 1311** (`futurwld`, `hod`; Winter Sports has no table here): before the patch, 0
  lookups in 150 s of play (coin, start, plunger, flippers); after it, with the generated packs, 228
  lookups for Future World and 212 for House of Diamonds, all found, none failed, ids `0000` `0001` `0002` `0004` `0008` only (one
  line at a time). The pack's rows `0001`..`000F` are the tones and `0000` a silent row on the
  same channel; see `docs/families/zaccaria.md`.

Not checked: the audio itself (the bench has no listening path), the ids no game session reached.
