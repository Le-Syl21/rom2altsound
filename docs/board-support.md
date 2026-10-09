# Sound board support

Which of PinMAME's sound board families rom2altsound gets sounds out of, measured on the
full VPinMAME ROM set (2804 zips, October 2026). The family of every game, and the counts
below, come from the PinMAME linked into rom2altsound (`rom2altsound roms --dump-table
table.json`, see [how it works](how-it-works.md#rom-verification)); the status column is a
quick survey, one ROM per family (two where the family spans two boards or the first one
was in doubt), chosen among the family's best-known games:

```
rom2altsound roms ~/roms-full --fix-names fixed   # correctly named zips (and system zips)
# + bsmt2000.zip in fixed/ for the BSMT boards
rom2altsound <one set per family> --roms fixed --jobs 6 \
    --max-secs 5 --limit 40 --no-chip-check --loop-max-secs 0 --no-html
```

that is: the factory boot (cold, then warm), then the first 40 commands of the sweep, each
recorded for at most 5 seconds, without loop search. "n of 40" is how many of those 40
commands gave a sound. It says whether a family works, not how well: loops, volume,
ducking and the full sweep are only verified for the families of the README's table.

How each family is driven (hardware, commands, stop, volume, loops, what is missing): [sound board families](families/README.md).

- ✅ sounds come out, distinct, each from silence;
- ⚠️ partial: few commands give a sound, or doubtful: most files do not start from
  silence (something keeps playing between commands) or are all the same;
- ❌ no sound from any of the 40 commands (or only identical clicks);
- — nothing to drive (no sound board in PinMAME's driver);
- ❔ not tried: no ROM of the family in the full set, or not run yet.

PinMAME 3.7 (the submodule) knows 2971 sets, 10 of them shared system ROM sets (`gts80s`,
`allied`...: no game, see [how it works](how-it-works.md#rom-verification)). Of the other
2961, 1939 (378 of 797 games) are in a ✅ family (Pinball 2000's 52 included, tried on
sets built outside the full set, see its row), 444 in a ⚠️ one, 315 in a ❌ one, 6 in a ❔
one (S11S, not run yet), 257 have no sound board. "Sets" counts every set (clones
and revisions included), "games" the sets without a parent, "sound ROM ids" the distinct
sound ROM sets (see [how it works](how-it-works.md#sound-rom-id)): a revision that kept its
sound ROMs shares its id, so one pack serves all of them. "Sets in the full set": the sets
of the family found complete in the 2804 zips (`rom2altsound roms`: 2796 OK, the other 8
are the system zips; 2796 of the 2961 sets are there).

| family (`SNDBRD_*`) | PinMAME board | sets | games | sound ROM ids | years | makers | sets in the full set | test ROM | status | notes / what is missing |
| family (`SNDBRD_*`) | PinMAME board | sets | games | sound ROM ids | years | makers | sets in the full set | test ROM | status | notes / what is missing |
|---|---|---|---|---|---|---|---|---|---|---|
| NONE (Stern SAM) | - | 406 | 27 | 0 | 2006-2024 | Stern, Stern/Destruk | 398 | acd_170h | ✅ | no sound board to drive: the sounds are read from the flash image (README, Stern SAM): 1036 files from acd_170h |
| DE2S | BSMT | 307 | 48 | 59 | 1991-2026 | Stern, Sega | 297 | jupk_513, swtril43 | ✅ | Data East BSMT (jupk_513: 23 of 40) and Sega/Stern Whitestar (swtril43: 25 of 40), both with the BSMT2000's own program; the earlier run had gnr_300 (21 of 40) and monopole (40 of 40) |
| WPCS | WPCS | 219 | 25 | 35 | 1990-2026 | Bally, Williams | 212 | tz_92 | ✅ | 37 of 40 (README: Twilight Zone, The Addams Family) |
| DCS | DCS | 191 | 19 | 39 | 1993-2026 | Williams, Bally | 187 | ij_l7, tom_13 | ✅ | first run of pre-WPC95 DCS: ij_l7 40 of 40, tom_13 39 of 40, all from silence; same board code as DCS95 |
| DE3S | AT91 | 156 | 7 | 30 | 2003-2008 | Stern | 156 | lotr, elvis | ✅ | fixed: the stop is now 00, which the games send at boot (the board reset used before left the AT91 rereading its last command): lotr 39 of 40, elvis 40 of 40, sopranos 40 of 40, nascar 40 of 40, all from silence, no board reset (24 and 31 files over what was playing before), named from sounds.dat |
| GTS80B | GTS80B | 148 | 32 | 31 | 1985-2021 | Gottlieb, Flipprojets | 142 | badgirls | ✅ | 28 of 40 (raven: 37 of 40) |
| S11XS+S11CS | WMSS11+WMSS11C | 109 | 22 | 27 | 1985-2026 | Williams, Bally | 95 | bk2k_l4 | ✅ | 40 of 40 (36 files, 4 blips; both boards swept); whirl_l3 40 of 40 earlier |
| S67S | WMSS67 | 105 | 38 | 28 | 1978-2022 | Williams, Williams / Oliver | 104 | bk_l4 | ✅ | fixed (3 of 40 before): each command now goes out between two idle bytes (`FF cmd FF`, `s67s_framed`), as the games send it, and the sweep is the five bits the board reads, 00..1E: bk_l4 30 of 31, grgar_l1 30 of 31, jngld_l2 30 of 31, frpwr_l2 30 of 31, all from silence; no stop known (board reset after each sound) |
| DCS95 | DCS | 82 | 16 | 30 | 1995-2019 | Bally, Williams | 82 | mm_10 | ✅ | 39 of 40; the full pipeline (loops, factory volume, ducking) is verified on afm_113b, see the README |
| DCSP2K | DCS | 52 | 2 | 6 | 1999-2025 | Midway, Midway / mypinballs | 0 | swep1_130, rfm_120 | ✅ | Pinball 2000. Not in the full VPinMAME set (its swep1/rfm zips hold only update files): tried on complete sets that `rom2altsound roms --fix-names` builds from the version zips and MAME's rfmpb/swe1pb zips. Full runs: swep1_130 683 of its 690 catalog tracks (26 loops, 24 exact from the track programs), rfm_120 1538 of 1557 (34 loops, all exact), none clipped, all from silence. The game's 16-bit DCS2 protocol, read in its code (see how it works); no ducking/stop/channel analysis; the packs do not play in VPinball (no sound command reaches AltSound) |
| GTS3 | GTS80B | 51 | 23 | 23 | 1989-2023 | Gottlieb, Gottlieb / Vifico | 47 | cueball | ✅ | 40 of 40 (Gottlieb System 3, GTS80B board code) |
| S11CS | WMSS11C | 50 | 10 | 15 | 1988-2026 | Bally, Williams | 47 | diner_l4 | ✅ | first run of the separate board alone: 30 of 40, all from silence (the same board as the second one of whirl_l3) |
| ZAC13136 | ZAC1370 | 45 | 5 | 20 | 1983-1985 | Zaccaria | 45 | tmachzac | ✅ | fixed as ZAC1370 (0 of 40 before): tmachzac 35 of 40, farfalla 34 of 40, all from silence, no board reset |
| BY51 | BY51 | 41 | 14 | 14 | 1979-2019 | Bally / Oliver, Bally | 40 | spaceinv | ✅ | 32 of 32 |
| BY32 | BY32 | 39 | 12 | 2 | 1978-2022 | Bally / Oliver, Bally | 39 | kiss | ✅ | no sound processor; 15 tones from the 32 commands on lostwrld (the first survey; kiss's count not recorded) |
| DE1S | DE | 38 | 15 | 14 | 1987-2025 | Data East, Leon | 36 | tmac_a24, bttf_a28, simp_a27 | ✅ | fixed: the stop is now 00 (what the games send after a sound) then the board reset: tmac_a24 38 of 40, bttf_a28 40 of 40, simp_a27 40 of 40, all from silence (34 to 38 of bttf's and simp's files over the last sound before) |
| BY61 | BYSNT | 34 | 10 | 12 | 1981-2019 | Bally, Bally / Oliver | 34 | flashgdn | ✅ | 26 of 40 (README: eballdlx) |
| GTS80SP | GTS80 | 26 | 11 | 11 | 1983-2022 | Gottlieb, Flipprojets | 26 | alienstr | ✅ | 38 of 40 |
| ZAC1370 | ZAC1370 | 25 | 3 | 8 | 1982-1987 | Zaccaria, Apple Time | 24 | socrking | ✅ | fixed: 0 of 40 before; each byte is now sent with its bit-7 strobe, low bits first, then with bit 7, then without (`7E FE 7E`, `zac_strobed`; the game sends `FE FE 7E`), the sweep FE..80 (commands 01..7F, read inverted), stop FF (command 00): socrking 25 of 40 (24 files, 1 blip), pinchamp 26 of 40, all from silence |
| BY45 | BY45 | 23 | 10 | 12 | 1983-2021 | Bally, Bally / Oliver | 23 | xsandos | ✅ | 39 of 40 (README: Cheap Squeak) |
| BY61B | BYSNT | 20 | 6 | 6 | 1981-2011 | Bally / Oliver, Bally | 20 | centaur | ✅ | 30 of 40 |
| ZAC11178 | ZAC1370 | 18 | 4 | 9 | 1985-1986 | Zaccaria | 18 | clown | ✅ | fixed as ZAC1370 (0 of 40 before): clown 40 of 40, poolcham 40 of 40, all from silence, no board reset |
| TAITO_SINTETIZADOR | TAITO | 18 | 14 | 15 | 1979-1982 | Taito | 18 | shock | ✅ | fixed (0 of 40 before): the stop is now 00, the games' idle value. The program arms CB1's rising edge and never lowers it itself (one read of the command at power-on on shock, none after: traced with `R2A_TRACE`), so a command that follows a non-zero byte made no edge. shock 35 of 40, football 36 of 40, obaoba 37 of 40, all from silence; many sounds run to the 5 s cap and 00 silences few of them (the board reset that follows does) |
| CAPCOMS | TMS320AV120 | 17 | 6 | 11 | 1995-2000 | Capcom, Illinois Pinball | 17 | bsv103 | ✅ | fixed (0 of 40 before): the board takes serial messages, so the sweep is `DA 04 07 0F nnnn` over the sample number (sounds.dat's Big Bang Bar format) and the stop `DA 02 03 01`: bsv103, pmv112, kpb105, ffv104 40 of 40 each, all from silence |
| PLAY4 | PLAY4 | 16 | 13 | 13 | 1984-1987 | Playmatic, JocMatic | 16 | madrace | ✅ | 30 of 40 |
| ST100B | ST100 | 16 | 7 | 0 | 1979-2022 | Stern, Stern / Quench | 16 | trident | ✅ | 40 of 40, all from silence since the stop is 00 (the empty tone mask; before, 38 of 40 started over a held tone: a board reset does nothing on this CPU-less board); 39 files are held tones cut at the 5 s cap |
| BYTCS | BYTCS | 12 | 5 | 5 | 1986-1987 | Bally | 11 | blackblt | ✅ | 40 of 40 (38 files, 2 blips; README: Turbo Cheap Squeak) |
| S9S | WMSS11 | 12 | 7 | 7 | 1983-1985 | Williams | 12 | sorcr_l2 | ✅ | 40 of 40 |
| ZAC11178_13181 | ZAC1370 | 12 | 2 | 6 | 1986-1987 | Zaccaria | 12 | spooky | ✅ | fixed as ZAC1370 (0 of 40 before): spooky 38 of 40, zankor 39 of 40, all from silence; the game's own framing `3F BF 3F` (bit 6 clear: the Z80 board's NMI) |
| ALVGS1 | OKI | 10 | 3 | 5 | 1992-1993 | Alvin G | 10 | agsoccer | ✅ | 33 of 40 (OKI) |
| ALVGS2 | BSMT | 10 | 5 | 6 | 1993-1994 | Alvin G | 9 | wrldtour | ✅ | 24 of 40 (BSMT2000, its own program) |
| TAITO_SINTEVOX | TAITO | 10 | 5 | 7 | 1981-1982 | Taito | 10 | titan | ✅ | 38 of 40 (titan, with the stop 00 since the Sintetizador fix: the same 38, 11 board resets instead of 45) |
| ST100 | ST100 | 10 | 4 | 0 | 1978-2022 | Stern, Stern / Idleman | 10 | dracula | ✅ | as ST100B: dracula 40 of 40, all from silence with the stop 00 (38 not from silence before) |
| BY56 | BY51 | 8 | 1 | 4 | 1980-2008 | Bally, Bally / Oliver | 8 | xenon | ✅ | 30 of 40 (README) |
| ZAC1125 | ZAC1125 | 8 | 4 | 0 | 1979-1980 | Zaccaria | 8 | firemntn | ✅ | first run: 38 of 40, all from silence (no sound ROM: the board's tones) |
| ZAC13181x3 | ZAC1370 | 8 | 2 | 2 | 1987-1987 | Zaccaria | 8 | strsphnx | ✅ | fixed as ZAC1370 (0 of 40 before): strsphnx 21 of 40 (20 files, 1 blip), nstrphnx the same, all from silence; the first 40 commands (FE..D7) all go to the third Z80 |
| TAITO_SINTETIZADORPP | TAITO | 8 | 5 | 5 | 1982-1985 | Taito | 8 | snake | ✅ | fixed as shock (0 of 40 before): snake 35 of 40, mrblack 36 of 40, polar 25 of 40, all from silence |
| PLAY1 | PLAY1 | 7 | 7 | 0 | 1978-1979 | Playmatic, Sonic (Spain) | 7 | spcgambl | ✅ | first run: 39 of 40; 15 of them not from silence (a tone held between commands) |
| GPMSU1 | GPSM | 7 | 7 | 7 | 1980-1985 | Game Plan | 7 | lizard | ✅ | each nibble now goes out between two idle 0F, as the game sends it, sweep 00..0E: lizard 11 of 15, attila 15 of 15, all from silence (before: 40 of 40, 37 over one tone at -14.9 LUFS) |
| NUOVA | NUOVA | 6 | 6 | 6 | 1986-1988 | Nuova Bell Games | 6 | f1gp | ✅ | first run: 15 of 40, all from silence |
| PLAY2 | PLAY2 | 6 | 5 | 0 | 1979-1980 | Playmatic, Sonic (Spain) | 6 | antar | ✅ | 40 of 40 (tones) |
| ZSU | ZSU | 6 | 6 | 6 | 1987-1988 | Playmatic, Maibesa | 6 | sklflite | ✅ | first run: 24 of 40, all from silence |
| GPMSU3 | GPSM3 | 6 | 3 | 2 | 1985-1985 | Game Plan | 6 | andromed | ✅ | fixed (0 of 40 before): a command is a byte sent as two nibbles, low then high, then the idle F, which runs it (read in andromed's sound program): andromed 39 of 40, cyclopes 39 of 40, all from silence |
| HANKIN | HNK | 5 | 5 | 5 | 1978-1981 | Hankin | 5 | fjholden | ✅ | 38 of 40 |
| MRGAME | MRGAME | 5 | 4 | 4 | 1988-1990 | Mr. Game (Italy) | 5 | dakar | ✅ | 26 of 40 |
| GRAND | GRAND | 4 | 1 | 1 | 1986-2021 | Grand Products Inc., Grand Products Inc. / Quench | 4 | bullseye | ✅ | 39 of 40 |
| BY45BP | BY45 | 4 | 2 | 2 | 1982-2006 | Bally, Bally / Oliver | 4 | babypac | ✅ | fixed (0 of 40 before): the video CPU sends a byte as two nibbles with its PIA's CB2 as the strobe (low nibble up, high nibble down, read in the sound program), which the tool now does with sndbrd_data_w/ctrl_w, after clearing the port 2 bit the video CPU clears: babypac 32 of 40, granny 28 of 40, all from silence |
| JEUTEL | JEUTEL | 3 | 3 | 2 | 1983-1984 | Jeutel | 3 | leking | ✅ | 40 of 40 |
| S11BS+S11JS | WMSS11+WMSS11J | 3 | 1 | 1 | 1989-1989 | Williams | 3 | jokrz_l6 | ✅ | 40 of 40 (38 files, 2 blips) on both boards |
| S3S | WMSS67 | 3 | 3 | 3 | 1978-1978 | Williams | 3 | phnix_l1 | ✅ | fixed as S67S (0 of 40 before): phnix_l1 15 of 31, the program's 15 numbered sounds (10..1E; 00..0F only set a flag, read in its code), all from silence; the flag-then-command sounds not swept |
| S7S_ND | WMSS67 | 3 | 1 | 1 | 1982-1982 | Williams | 3 | thund_p1 | ✅ | fixed as S67S (0 of 40 before): the seven bits, 00..7E, idle 7F: thund_p1 34 of 40, all from silence |
| GPSSU1 | GPS1 | 3 | 3 | 0 | 1979-1979 | Game Plan | 3 | startrip | ✅ | stop 0F (no tone), sweep 00..0E: startrip 4 of 15, the board's four tones, all from silence (before: 5 not from silence) |
| BARNI | BARNI | 2 | 2 | 2 | 1985-1985 | Barni | 2 | redbaron | ✅ | first run: 26 of 40, all from silence |
| PLAY3 | PLAY3 | 2 | 2 | 2 | 1982-1982 | Playmatic | 2 | cerberus | ✅ | 19 of 40 |
| TAITO_SINTEVOXPP | TAITO | 2 | 2 | 2 | 1982-1982 | Taito | 2 | gork | ✅ | 37 of 40 (gork; with the stop 00: the same 37, 28 board resets instead of 47) |
| BY61B2 | BYSNT | 1 | 1 | 1 | 1982-1982 | Bally | 1 | mysteria | ✅ | first run: 18 of 40, all from silence |
| BY61N | BYSNT | 1 | 1 | 1 | 1985-1985 | Bell Games | 1 | cosflash | ✅ | first run: 26 of 40, all from silence |
| JVH2 | JVH | 1 | 1 | 1 | 1988-1988 | Jac Van Ham (Royal) | 1 | formula1 | ✅ | first run: 27 of 40, all from silence |
| TECHNO | TECHNO | 1 | 1 | 1 | 1987-1987 | Tecnoplay | 1 | scram_tp | ✅ | 37 of 40 |
| GPSSU2 | GPS2 | 1 | 1 | 0 | 1979-1979 | Game Plan | 1 | sshooter | ✅ | stop 0F, sweep 00..0E: sshooter 15 of 15, all from silence (24 not from silence before) |
| GPSSU3 | GPS2 | 1 | 1 | 0 | 1979-1979 | Game Plan | 1 | coneyis | ✅ | as sshooter: coneyis 15 of 15, all from silence |
| GPSSU4 | GPS4 | 1 | 1 | 0 | 1982-1982 | Game Plan | 1 | suprnova | ✅ | stop 00 (wave and tone off), sweep 00..0E: suprnova 10 of 15, all from silence (6 not from silence before) |
| PLAYZ | PLAYZ | 1 | 1 | 1 | 1981-1981 | Playmatic | 1 | zira | ✅ | fixed (0 of 40 before): the board reads three lines (bits 4-6), so the sweep is 10..70, each between two idle 00: zira 4 of 7, all from silence |
| GTS80S | GTS80 | 37 | 19 | 20 | 1979-2008 | Gottlieb, Oliver | 37 | spidermn, buckrgrs | ⚠️ | first run, doubtful: spidermn 40 of 40 but 37 do not start from silence and 38 run to the 5 s cap (a sound left playing); buckrgrs 30 of 40, all at the same -13.8 LUFS. The game streams bytes at boot (4956 on spidermn): command protocol to look at (gts80s.c) |
| SPINB | SPINB | 27 | 16 | 14 | 1985-1996 | Inder (Spain), Spinball (Spain) | 26 | bushido, corsario | ⚠️ | Spinball's own boards fixed (bushido, mach2, jolypark, vrnwrld: 7 sets; 0 of 40 before): both sound CPUs poll the latch and take a byte only while its bit 7 is set, so each command is `8x`..`FF` followed by `00`, stop `8F`; the MSM6585 boards' step volume, which the games' boot steps down to 0, is set back to 122. bushido 31 of 40, mach2 40, jolypark 40, vrnwrld 40, all from silence. Inder's machines (20 sets, their own command handler, `INDER` in the logs) unchanged: lapbylap 30 of 40, corsario and atleta 40 of 40 but 39 not from silence (a sound playing on), brvteam no board |
| GTS80SS_VOTRAX | GTS80SS | 24 | 9 | 13 | 1981-2008 | Gottlieb, Flipprojets | 24 | blckhole | ⚠️ | 5 of 40 (Votrax speech board): most commands silent; to look at the board's command bits |
| ST300V | ST300 | 21 | 6 | 7 | 1980-2024 | Stern, Stern / Idleman | 20 | flight2k | ⚠️ | speech only: the sweep is now 40..7F, the S14001A's 64 words (the manual command is the speech path): flight2k 37 of 40, freefall 40 of 40, all from silence, 0.2 to 0.4 s each, some clipped (0 of 40 before). The effects are the game's own timer programming (as ST300), not extracted |
| GTS80SS | GTS80SS | 17 | 8 | 8 | 1982-2023 | Gottlieb, Flipprojets | 17 | hh | ⚠️ | first run: 5 of 40, as blckhole (same board) |
| BYSD | BYSD | 10 | 6 | 6 | 1986-1988 | Bally | 10 | specforc | ⚠️ | 12 of 40 (Sounds Deluxe): sounds come out, but 28 of the first 40 commands are silent; to check on a full sweep whether the board takes a two-byte command |
| ZAC1346 | ZAC1346 | 6 | 3 | 3 | 1980-1981 | Zaccaria | 6 | locomotn | ⚠️ | first run, doubtful: 40 of 40 but 39 not from silence and 40 run to the 5 s cap (7 distinct levels): no stop known |
| JVH | JVH | 3 | 3 | 2 | 1986-1987 | Jac Van Ham (Royal) | 3 | icemania | ⚠️ | 1 of 40 (a 0.2 s file): to look at |
| BY51N | BY51 | 2 | 1 | 1 | 1984-2018 | Bell Games, Bell Games / Quench | 2 | suprbowl | ⚠️ | first run: 6 of 32 (the BY51 board of Bell Games' conversion; spaceinv gives 32 of 32 on BY51) |
| GTS80SS_VOTRAX_OLD | GTS80SS | 1 | 0 | 1 | 1981-1981 | Gottlieb | 1 | marsp | ⚠️ | first run: 3 of 40, as blckhole (the Votrax board) |
| ROMSTAR | TMS320AV120 | 1 | 1 | 1 | 1994-1994 | Romstar | 1 | ghv101 | ⚠️ | 40 of 40 but doubtful: every file runs to the 5 s cap and none starts from silence (something keeps playing under every command) |
| ROWAMET | ROWAMET | 1 | 1 | 1 | ? | Rowamet | 1 | heavymtl | ⚠️ | first run: 5 of 40 |
| S3DFS | WMSS67 | 1 | 1 | 1 | 1978-1978 | Williams | 1 | disco_l1 | ⚠️ | with the S67S idle framing (1 of 40 before): 40 of 40, all from silence, but few distinct (the files repeat with the lowest low bit of the command); the control bit (s67s_ctrl_w) is never set |
| TABART | TABART | 1 | 1 | 1 | 1986-1986 | Christian Tabart (France) | 1 | hexagone | ⚠️ | first run (the full set's zip is complete), doubtful: 31 of 40, 22 not from silence, 5 distinct levels, then "still not silent after 3 waits" |
| TABART2 | TABART | 1 | 1 | 1 | 1984-1984 | Christian Tabart (France) | 1 | sahalove | ⚠️ | first run, doubtful: 40 of 40 but 39 not from silence and all alike (-17.0 LUFS): one tone that never stops |
| TABART3 | TABART | 1 | 1 | 1 | 1985-1985 | Christian Tabart (France) | 1 | grand8 | ⚠️ | first run: as sahalove |
| S3WCS | WMSS67 | 1 | 1 | 1 | 1978-1978 | Williams | 1 | wldcp_l1 | ⚠️ | with the S67S idle framing (0 of 40 before): 16 of 40, all from silence, but all alike (one 2.6 s sound); the control bit (s67s_ctrl_w) is never set |
| ST300 | ST300 | 76 | 17 | 0 | 1979-2026 | Stern, Stern / Idleman | 67 | meteor | ❌ | not a command board: no sound ROM, the game programs the MC6840 timers itself (boot: register/value pairs 01 00 06 07 04 05); rom2altsound's command sweep does not apply |
| ATARI1 | ATARI1 | 12 | 5 | 3 | 1976-2024 | Atari | 12 | atarians | ❌ | first run (the full set's zip is complete): 0 of 40; the game streams 00 bytes (14498 in the boot): discrete sound driven by lines, not by command numbers |
| ATARI2 | ATARI2 | 3 | 3 | 1 | 1979-1979 | Atari | 3 | superman | ❌ | 0 of 40: discrete sound (no sound ROM), the game sent no sound byte at boot; the board is driven by lines, not by command numbers |
| JOCTRONIC | JOCTRONIC | 3 | 3 | 3 | 1986-1986 | Joctronic | 3 | punkywil | ❌ | 0 of 40, and the boot plays nothing either: the sound program queues every command on its NMI (that part works) but its main loop waits on an IRQ counter, and the IRQ comes from the main CPU's CTC channel 0, which does not reach the sound CPU in PinMAME (read in punkywil's sound program); not fixable without inventing that interrupt |
| ASTRO | ASTRO | 2 | 1 | 0 | ? | Stern | 2 | sam_iii | ❌ | first run: 0 of 40; the game toggles 00/01 (20 bytes at boot): a line, not a command |
| TECNOPLAY | TECNOPLAY | 2 | 2 | 1 | 1987-1988 | Tecnoplay | 2 | xforce | ❌ | 0 of 40: the game writes only 00 to the board in attract (76923 bytes, its display clocking), and commands followed by 00 (which releases the TMS7000's IRQ3) play nothing either; the TMS7000 program not read (no disassembler at hand) |
| S11S | WMSS11 | 6 | 5 | 4 | 1985-1987 | Williams | 6 | - | ❔ | Williams' shuffle alleys (alcat_l7, tts_l2...): GEN_S11, the System 11 CPU board's own sound (wmssnd.c `s11sIntf`, WMSS11, the same board type as S11XS). Classed NONE until `drivers.rs` read GEN_S11 as 0x8000000 instead of gen.h's 0x80000000; not run yet |
| NONE (other) | - | 251 | 167 | 13 | 1974-2025 | Bally, LTD | 212 | - | — | no sound board in PinMAME's sound board interface (sndbrd.c): sound on the CPU board, chimes, or a sound CPU the driver runs by itself (LTD, Bally -17, Recel, Sleic, Juegos Populares...); rom2altsound has nothing to send commands to |
| ZAC1311 | - | 6 | 3 | 0 | 1978-1978 | Zaccaria | 6 | futurwld | — | no sound board interface: PinMAME has no manual command handler for it, nothing to drive |

## Notes

- **The family is PinMAME's, read from the library.** The sound board is the one the
  game's machine init starts: most pass on `core_gameData->hw.soundBoard`, set by the
  game's init function; WPC, System 3 to 11, Data East alphanumeric, Whitestar and
  Pinball 2000 pick it from the hardware generation instead, which `drivers.rs`
  (`Board::sound_boards`) mirrors. Two boards are listed when the machine runs both
  (System 11: the CPU board's and the separate one). The extractor's own report names
  the same board for every ROM of the survey (`boards` in `manifest.json`).
- **What a ❌ needs** is, in most cases, the command protocol: the byte (or bytes)
  the game sends for one sound is not what the sweep sends. The boot log of each ROM
  (`rom2altsound.log`, "game sent N sound byte(s)") shows what the game sends, which is
  the place to start.
- **A ⚠️ doubtful family** plays something under every command. The usual cause is a
  board with no known stop command: the tool resets the sound CPU between commands, and
  a board whose program starts a background tone at reset is never silent.
- **Pinball 2000** (DCSP2K) is the only family whose sets are not complete in the full
  set at hand: its swep1_*/rfm_* zips hold only a version's update files, the shared sound
  and Prism ROMs being in MAME's base zips (`rfmpb.zip`, `swe1pb.zip`), which
  `rom2altsound roms --fix-names` merges in (see
  [how it works](how-it-works.md#rom-verification)). It was run in full, not surveyed.

## Cheapest fixes

By sets gained for the work, from the boot logs above (nothing here is done yet):

1. **The Williams System 3 to 7 idle protocol** (S67S, S3S, S3DFS, S3WCS, S7S_ND: 113
   sets). The board starts a sound on the change from the idle value, and the games send
   7F, the command, 7F (boot FF FF 7F, or 7F 19 7F 26). Sending the idle byte before each
   command of the sweep (and the inverted value) is one change in the sweep for the
   WMSS67 board.
2. **Stopping the AT91 board** (DE3S, Whitestar 2003-2008: 156 sets). The sounds already
   come out, named from sounds.dat; only the stop between commands fails (neither the
   stop command nor a board reset silences it within 10 s). Finding the board's real stop
   (or its idle floor, if what plays on is only a level the silence detection misses)
   should make the family ✅.
3. **The later Data East alphanumeric games** (DE1S: bttf_a28, simp_a27; 38 sets in the
   family): the same symptom on the older DE board, the stop to find in their sound
   program.
4. **The Zaccaria inverted strobed byte** (ZAC1370, ZAC13136, ZAC11178, ZAC11178_13181,
   ZAC13181x3: 108 sets). The games send 00 FE FE 7E or 00 7F FF 7F 3F BF: one byte
   protocol, inverted and strobed, to send for every command; one change for five
   families.
5. **Taito's doubled commands** (TAITO_SINTETIZADOR, TAITO_SINTETIZADORPP: 26 sets): the
   games send each command twice, with and without bit 7 (98 18); the sweep sends one.
6. **A stop for the tone boards** (ST100, ST100B, GPMSU1, GPSSU1..4, ZAC1346, TABART,
   TABART2, TABART3: 48 sets): the files come out, but a tone sustains between commands;
   the idle byte of each board to send as the stop.
7. **The Gottlieb System 80 boards** (GTS80S, GTS80SS, GTS80SS_VOTRAX, _OLD: 79 sets): the
   command bits of the board (gts80s.c), as the game streams them at boot.

The rest needs more than a protocol: ST300/ST300V (97 sets, the game programs the timers
itself), the discrete Atari and Astro boards (driven by lines), Capcom, Spinball and
Tecnoplay (their own command paths), Baby Pac-Man (the board behind the video board).
