# Sound board support

Which of PinMAME's sound board families rom2altsound gets sounds out of, measured on the
ROMs at hand (76 zips, October 2026). The family of every game, and the counts below, come
from the PinMAME linked into rom2altsound (`rom2altsound roms --dump-table table.json`,
see [how it works](how-it-works.md#rom-verification)); the status column is a quick
survey, one ROM per family:

```
rom2altsound roms ~/roms --fix-names fixed        # correctly named, standalone zips
rom2altsound <one set per family> --roms fixed --jobs 6 \
    --max-secs 5 --limit 40 --no-chip-check --loop-max-secs 0 --no-html
```

that is: the factory boot (cold, then warm), then the first 40 commands of the sweep, each
recorded for at most 5 seconds, without loop search. "n of 40" is how many of those 40
commands gave a sound. It says whether a family works, not how well: loops, volume,
ducking and the full sweep are only verified for the families of the README's table.

- ✅ sounds come out, distinct, each from silence;
- ⚠️ partial: few commands give a sound, or doubtful: most files do not start from
  silence (something keeps playing between commands) or are all the same;
- ❌ no sound from any of the 40 commands (no crash in this survey: every ROM booted);
- — nothing to drive (no sound board in PinMAME's driver);
- ❔ not tried: no ROM at hand (or the zip at hand is incomplete).

PinMAME 3.7 (the submodule) knows 2961 sets: 1653 of them (335 of 797 games) are in a ✅
family, 168 in a ⚠️ one, 270 in a ❌ one, 613 in an untried one, 257 have no sound board.
"Sets" counts every set (clones and revisions included), "games" the sets without a
parent, "sound ROM ids" the distinct sound ROM sets (see
[how it works](how-it-works.md#sound-rom-id)): a revision that kept its sound ROMs shares
its id, so one pack serves all of them.

| family (`SNDBRD_*`) | PinMAME board | sets | games | sound ROM ids | years | makers | test ROM here | status | notes / what is missing |
|---|---|---|---|---|---|---|---|---|---|
| NONE (Stern SAM) | - | 406 | 27 | 0 | 2006-2024 | Stern, Stern/Destruk | acd_168h | ✅ | no sound board to drive: the sounds are read from the flash image (README, Stern SAM) |
| DE2S | BSMT | 307 | 48 | 59 | 1991-2026 | Stern, Sega | gnr_300, monopole | ✅ | Data East BSMT (gnr_300: 21 of 40) and Sega/Stern Whitestar (monopole: 40 of 40), both with the BSMT2000's own program; good ROMs also at hand: hook_408, trek_201, rctycn, xfiles |
| WPCS | WPCS | 219 | 25 | 35 | 1990-2026 | Bally, Williams | tz_94h | ✅ | 38 of 40 (README: Twilight Zone, The Addams Family) |
| GTS80B | GTS80B | 148 | 32 | 31 | 1985-2021 | Gottlieb, Flipprojets | raven | ✅ | 37 of 40 |
| S11XS+S11CS | WMSS11+WMSS11C | 109 | 22 | 27 | 1985-2026 | Williams, Bally | whirl_l3 | ✅ | 40 of 40 (both boards swept); good ROMs also at hand: pb_l5, hs_l4, taxi_l4 |
| DCS95 | DCS | 82 | 16 | 30 | 1995-2019 | Bally, Williams | afm_113b | ✅ | 37 of 40; the full pipeline (loops, factory volume, ducking) is verified on it, see the README |
| GTS3 | GTS80B | 51 | 23 | 23 | 1989-2023 | Gottlieb, Gottlieb / Vifico | sfight2 | ✅ | 30 of 40 (Gottlieb System 3, GTS80B board code) |
| BY51 | BY51 | 41 | 14 | 14 | 1979-2019 | Bally / Oliver, Bally | vikingb | ✅ | 30 of 32 (README) |
| BY32 | BY32 | 39 | 12 | 2 | 1978-2022 | Bally / Oliver, Bally | lostwrld | ✅ | 15 tones from the 32 commands (no sound processor) |
| DE1S | DE | 38 | 15 | 14 | 1987-2025 | Data East, Leon | tmac_a24 | ✅ | 38 of 40 (Data East alphanumeric and 128x16, the board before the BSMT) |
| BY61 | BYSNT | 34 | 10 | 12 | 1981-2019 | Bally, Bally / Oliver | eballdlx | ✅ | 33 of 40 (README) |
| GTS80SP | GTS80 | 26 | 11 | 11 | 1983-2022 | Gottlieb, Flipprojets | bountyh | ✅ | 38 of 40 |
| BY45 | BY45 | 23 | 10 | 12 | 1983-2021 | Bally, Bally / Oliver | spyhuntr | ✅ | 38 of 40 (README: Cheap Squeak) |
| BY61B | BYSNT | 20 | 6 | 6 | 1981-2011 | Bally / Oliver, Bally | fathom | ✅ | 32 of 40, first run of this variant |
| PLAY4 | PLAY4 | 16 | 13 | 13 | 1984-1987 | Playmatic, JocMatic | madrace | ✅ | 30 of 40 |
| BYTCS | BYTCS | 12 | 5 | 5 | 1986-1987 | Bally | cityslck | ✅ | 35 of 40 (README: Turbo Cheap Squeak) |
| S9S | WMSS11 | 12 | 7 | 7 | 1983-1985 | Williams | sshtl_l7 | ✅ | 39 of 40 |
| ALVGS1 | OKI | 10 | 3 | 5 | 1992-1993 | Alvin G | agsoccer | ✅ | 33 of 40 (OKI) |
| ALVGS2 | BSMT | 10 | 5 | 6 | 1993-1994 | Alvin G | wrldtour | ✅ | 24 of 40 (BSMT2000, its own program) |
| TAITO_SINTEVOX | TAITO | 10 | 5 | 7 | 1981-1982 | Taito | titan | ✅ | 38 of 40 |
| BY56 | BY51 | 8 | 1 | 4 | 1980-2008 | Bally, Bally / Oliver | xenon | ✅ | 30 of 40 (README) |
| PLAY2 | PLAY2 | 6 | 5 | 0 | 1979-1980 | Playmatic, Sonic (Spain) | antar | ✅ | 40 of 40 (tones) |
| HANKIN | HNK | 5 | 5 | 5 | 1978-1981 | Hankin | fjholden | ✅ | 38 of 40 |
| MRGAME | MRGAME | 5 | 4 | 4 | 1988-1990 | Mr. Game (Italy) | dakar | ✅ | 26 of 40 |
| GRAND | GRAND | 4 | 1 | 1 | 1986-2021 | Grand Products Inc., Grand Products Inc. / Quench | bullseye | ✅ | 39 of 40 |
| JEUTEL | JEUTEL | 3 | 3 | 2 | 1983-1984 | Jeutel | leking | ✅ | 40 of 40 |
| S11BS+S11JS | WMSS11+WMSS11J | 3 | 1 | 1 | 1989-1989 | Williams | jokrz_l6 | ✅ | 40 of 40 on both boards |
| PLAY3 | PLAY3 | 2 | 2 | 2 | 1982-1982 | Playmatic | cerberus | ✅ | 19 of 40 |
| TAITO_SINTEVOXPP | TAITO | 2 | 2 | 2 | 1982-1982 | Taito | gork | ✅ | 37 of 40 |
| GPSSU4 | GPS4 | 1 | 1 | 0 | 1982-1982 | Game Plan | suprnova | ✅ | 26 of 40 (9 not from silence) |
| TECHNO | TECHNO | 1 | 1 | 1 | 1987-1987 | Tecnoplay | scram_tp | ✅ | 37 of 40 |
| S67S | WMSS67 | 105 | 38 | 28 | 1978-2022 | Williams, Williams / Oliver | bk_l4 | ⚠️ | 3 of 40. The board's lines are active low with 1F as the idle value, and it starts a sound on the change from idle (wmssnd.c s67s_cmd_w): the game sends 7F, the command (2C), 7F again. A sweep of single bytes only starts the commands that follow an idle-looking value. To do: send the idle value before each command (and invert the sweep) |
| GTS80SS_VOTRAX | GTS80SS | 24 | 9 | 13 | 1981-2008 | Gottlieb, Flipprojets | blckhole | ⚠️ | 5 of 40 (Votrax speech board): most commands silent; to look at the board's command bits |
| ST100B | ST100 | 16 | 7 | 0 | 1979-2022 | Stern, Stern / Quench | tridenta | ⚠️ | 40 of 40 but 38 do not start from silence: the tones sustain, no stop known |
| BYSD | BYSD | 10 | 6 | 6 | 1986-1988 | Bally | specforc | ⚠️ | 12 of 40 (Sounds Deluxe): sounds come out, but 28 of the first 40 commands are silent; to check on a full sweep whether the board takes a two-byte command |
| GPMSU1 | GPSM | 7 | 7 | 7 | 1980-1985 | Game Plan | lizard | ⚠️ | 40 of 40 but doubtful: 37 do not start from silence and all are at -14.9 LUFS (one tone that never stops) |
| JVH | JVH | 3 | 3 | 2 | 1986-1987 | Jac Van Ham (Royal) | icemania | ⚠️ | 1 of 40: to look at |
| ROMSTAR | TMS320AV120 | 1 | 1 | 1 | 1994-1994 | Romstar | ghv101 | ⚠️ | 40 of 40 but doubtful: every file runs to the 5 s cap and none starts from silence (something keeps playing under every command) |
| GPSSU2 | GPS2 | 1 | 1 | 0 | 1979-1979 | Game Plan | sshooter | ⚠️ | 38 of 40, 24 of them not from silence: a tone left on between commands (no stop known) |
| GPSSU3 | GPS2 | 1 | 1 | 0 | 1979-1979 | Game Plan | coneyis | ⚠️ | same board program as sshooter, same result |
| ST300 | ST300 | 76 | 17 | 0 | 1979-2026 | Stern, Stern / Idleman | meteor | ❌ | not a command board: no sound ROM, the game programs the MC6840 timers itself (boot: register/value pairs 01 00 06 07 04 05); rom2altsound's command sweep does not apply |
| ZAC13136 | ZAC1370 | 45 | 5 | 20 | 1983-1985 | Zaccaria | tmachzac | ❌ | 0 of 40, as ZAC1370 (boot 00 FF FF 7F) |
| SPINB | SPINB | 27 | 16 | 14 | 1985-1996 | Inder (Spain), Spinball (Spain) | bushido | ❌ | 0 of 40: the game sent no sound byte at boot; two MSM5205 boards fed by their own CPUs, to look at |
| ZAC1370 | ZAC1370 | 25 | 3 | 8 | 1982-1987 | Zaccaria, Apple Time | socrking | ❌ | 0 of 40; boot bytes 00 FE FE 7E: the board likely takes an inverted, strobed byte |
| ST300V | ST300 | 21 | 6 | 7 | 1980-2024 | Stern, Stern / Idleman | flight2k | ❌ | as ST300, plus the S14001A speech chip |
| TAITO_SINTETIZADOR | TAITO | 18 | 14 | 15 | 1979-1982 | Taito | shock | ❌ | 0 of 40; the game sends each command twice, with and without bit 7 (11 91, 98 18): one byte per command does not start a sound |
| ZAC11178 | ZAC1370 | 18 | 4 | 9 | 1985-1986 | Zaccaria | clown | ❌ | 0 of 40, as ZAC1370 |
| CAPCOMS | TMS320AV120 | 17 | 6 | 11 | 1995-2000 | Capcom, Illinois Pinball | pmv112 | ❌ | 0 of 40: the game sent no sound byte at boot; the Capcom board takes its commands some other way (to look at in capcoms.c) |
| TAITO_SINTETIZADORPP | TAITO | 8 | 5 | 5 | 1982-1985 | Taito | snake | ❌ | 0 of 40, as shock |
| GPMSU3 | GPSM3 | 6 | 3 | 2 | 1985-1985 | Game Plan | andromed | ❌ | 0 of 40; the game sends nibbles at boot (0F 0C 00 0F): command protocol to look at |
| BY45BP | BY45 | 4 | 2 | 2 | 1982-2006 | Bally, Bally / Oliver | babypac | ❌ | 0 of 40. Baby Pac-Man's Cheap Squeak sits behind the video board: the game sent 00/0F at boot, our commands never start a sound. To look at: which CPU feeds the board |
| ATARI2 | ATARI2 | 3 | 3 | 1 | 1979-1979 | Atari | superman | ❌ | 0 of 40: discrete sound (no sound ROM), the game sent no sound byte at boot; the board is driven by lines, not by command numbers |
| TECNOPLAY | TECNOPLAY | 2 | 2 | 1 | 1987-1988 | Tecnoplay | xforce | ❌ | 0 of 40; the game streams sound bytes all the time (76923 bytes in the 15 s boot): to look at |
| NONE (other) | - | 257 | 172 | 17 | 1974-2025 | Bally, LTD | - | — | no sound board in PinMAME's sound board interface (sndbrd.c): sound on the CPU board, chimes, or a sound CPU the driver runs by itself (LTD, Bally -17, Recel, Sleic, Juegos Populares...); rom2altsound has nothing to send commands to |
| DCS | DCS | 191 | 19 | 39 | 1993-2026 | Williams, Bally | - | ❔ | pre-WPC95 DCS (Indiana Jones .. Jack*Bot): same board code as DCS95 but no ROM at hand |
| DE3S | AT91 | 156 | 7 | 30 | 2003-2008 | Stern | - | ❔ | Stern Whitestar with the AT91 sound board (2003-2008, 156 sets): no ROM at hand |
| DCSP2K | DCS | 52 | 2 | 6 | 1999-2025 | Midway, Midway / mypinballs | rfm_260 (incomplete) | ❔ | Pinball 2000: our rfm_260 zip is a split mod without its parent's sound ROMs (rfm_u109/u110), so it cannot run |
| S11CS | WMSS11C | 50 | 10 | 15 | 1988-2026 | Bally, Williams | - | ❔ | System 11C games with only the separate board (WPC alpha 1 too): no ROM at hand; the same board as whirl_l3's second one |
| GTS80S | GTS80 | 37 | 19 | 20 | 1979-2008 | Gottlieb, Oliver | - | ❔ | no ROM at hand |
| GTS80SS | GTS80SS | 17 | 8 | 8 | 1982-2023 | Gottlieb, Flipprojets | - | ❔ | no ROM at hand |
| ATARI1 | ATARI1 | 12 | 5 | 3 | 1976-2024 | Atari | atarians (incomplete) | ❔ | our zip lacks the sound PROM 07028-01.bin |
| ZAC11178_13181 | ZAC1370 | 12 | 2 | 6 | 1986-1987 | Zaccaria | - | ❔ | same family as clown (❌ there); no ROM at hand |
| ST100 | ST100 | 10 | 4 | 0 | 1978-2022 | Stern, Stern / Idleman | - | ❔ | no sound ROM (tones); no ROM at hand |
| ZAC1125 | ZAC1125 | 8 | 4 | 0 | 1979-1980 | Zaccaria | - | ❔ | no sound ROM; no ROM at hand |
| ZAC13181x3 | ZAC1370 | 8 | 2 | 2 | 1987-1987 | Zaccaria | - | ❔ | no ROM at hand |
| PLAY1 | PLAY1 | 7 | 7 | 0 | 1978-1979 | Playmatic, Sonic (Spain) | - | ❔ | no ROM at hand |
| NUOVA | NUOVA | 6 | 6 | 6 | 1986-1988 | Nuova Bell Games | - | ❔ | no ROM at hand |
| ZSU | ZSU | 6 | 6 | 6 | 1987-1988 | Playmatic, Maibesa | - | ❔ | no ROM at hand |
| ZAC1311 | - | 6 | 3 | 0 | 1978-1978 | Zaccaria | - | ❔ | no sound ROM; no ROM at hand |
| ZAC1346 | ZAC1346 | 6 | 3 | 3 | 1980-1981 | Zaccaria | - | ❔ | no ROM at hand |
| GPSSU1 | GPS1 | 3 | 3 | 0 | 1979-1979 | Game Plan | - | ❔ | no sound ROM (discrete); no ROM at hand |
| JOCTRONIC | JOCTRONIC | 3 | 3 | 3 | 1986-1986 | Joctronic | - | ❔ | no ROM at hand |
| S3S | WMSS67 | 3 | 3 | 3 | 1978-1978 | Williams | - | ❔ | no ROM at hand (disco_l1, phnix_l1, wldcp_l1 here lack their sound ROM) |
| S7S_ND | WMSS67 | 3 | 1 | 1 | 1982-1982 | Williams | - | ❔ | no ROM at hand |
| BARNI | BARNI | 2 | 2 | 2 | 1985-1985 | Barni | - | ❔ | no ROM at hand |
| BY51N | BY51 | 2 | 1 | 1 | 1984-2018 | Bell Games, Bell Games / Quench | - | ❔ | no ROM at hand |
| ASTRO | ASTRO | 2 | 1 | 0 | ? | Stern | - | ❔ | no ROM at hand |
| BY61B2 | BYSNT | 1 | 1 | 1 | 1982-1982 | Bally | - | ❔ | no ROM at hand |
| BY61N | BYSNT | 1 | 1 | 1 | 1985-1985 | Bell Games | - | ❔ | no ROM at hand |
| GTS80SS_VOTRAX_OLD | GTS80SS | 1 | 0 | 1 | 1981-1981 | Gottlieb | - | ❔ | no ROM at hand |
| JVH2 | JVH | 1 | 1 | 1 | 1988-1988 | Jac Van Ham (Royal) | - | ❔ | no ROM at hand |
| PLAYZ | PLAYZ | 1 | 1 | 1 | 1981-1981 | Playmatic | - | ❔ | no ROM at hand |
| ROWAMET | ROWAMET | 1 | 1 | 1 | ? | Rowamet | - | ❔ | no ROM at hand |
| TABART2 | TABART | 1 | 1 | 1 | 1984-1984 | Christian Tabart (France) | - | ❔ | no ROM at hand |
| TABART3 | TABART | 1 | 1 | 1 | 1985-1985 | Christian Tabart (France) | - | ❔ | no ROM at hand |
| TABART | TABART | 1 | 1 | 1 | 1986-1986 | Christian Tabart (France) | hexagone (incomplete) | ❔ | our zip lacks A1752CF.bin and holds an unknown u5_cf.bin |
| S3WCS | WMSS67 | 1 | 1 | 1 | 1978-1978 | Williams | wldcp_l1 (incomplete) | ❔ | our zip lacks 481_s0_world_cup.716 (it holds Flash's sound1.716 instead) |
| S3DFS | WMSS67 | 1 | 1 | 1 | 1978-1978 | Williams | disco_l1 (incomplete) | ❔ | our zip lacks 483_s0_disco_fever.716 (it holds Flash's sound1.716 instead) |

## Notes

- **The family is PinMAME's, read from the library.** The sound board is the one the
  game's machine init starts: most pass on `core_gameData->hw.soundBoard`, set by the
  game's init function; WPC, System 3 to 11, Data East alphanumeric, Whitestar and
  Pinball 2000 pick it from the hardware generation instead, which `drivers.rs`
  (`Board::sound_boards`) mirrors. Two boards are listed when the machine runs both
  (System 11: the CPU board's and the separate one). The extractor's own report names
  the same board for every ROM of the survey (`boards` in `manifest.json`).
- **What a ❌ needs** is, in every case seen, the command protocol: the byte (or bytes)
  the game sends for one sound is not what the sweep sends. The boot log of each ROM
  (`rom2altsound.log`, "game sent N sound byte(s)") shows what the game sends, which is
  the place to start.
- **A ⚠️ doubtful family** plays something under every command. The usual cause is a
  board with no known stop command: the tool resets the sound CPU between commands, and
  a board whose program starts a background tone at reset is never silent.
- **Not tried** families wait for ROMs: the full VPinMAME set will give one per family.
