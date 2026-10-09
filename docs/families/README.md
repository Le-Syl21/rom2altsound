# Sound board families

How rom2altsound gets the sounds out of each of PinMAME's sound board families
(`SNDBRD_*`): the hardware, how the game talks to the board, how the list of sounds is
found, the boot, the volume, the loops, the pack's columns, what was measured and what
is missing. Every statement is taken from rom2altsound's code and PinMAME's source (the
`vendor/pinmame` submodule), with the function it comes from.

- [The common method](common.md): what every family goes through unless its section says
  otherwise (boot, sending commands, the sweep, stops and resets, volume, loops, the
  pack's columns, VPinball). Read it first: the family sections only describe what
  differs.
- One document per maker or board line, one section per family (each family has its own
  anchor, `#sndbrd_<name>` in lowercase); [families.json](families.json) maps every
  family to its section.

Status, set and game counts are those of [board support](../board-support.md) (PinMAME
3.7, the full VPinMAME set, October 2026): ✅ sounds come out, ⚠️ partial or doubtful,
❌ no sound, — nothing to drive. "Sets" counts every set (clones and revisions
included), "games" the sets without a parent; the examples are games of the family with
the most sets in PinMAME (the survey's test ROM first).

| family | status | sets | games | examples |
|---|---|---|---|---|
| [`SNDBRD_DCS`](dcs.md#sndbrd_dcs) | ✅ | 191 | 19 | Indiana Jones: The Pinball Adventure; Star Trek: The Next Generation; Demolition Man |
| [`SNDBRD_DCS95`](dcs.md#sndbrd_dcs95) | ✅ | 82 | 16 | Medieval Madness; Safe Cracker; Attack From Mars |
| [`SNDBRD_DCSP2K`](dcs.md#sndbrd_dcsp2k) | ✅ | 52 | 2 | Pinball 2000: Star Wars Episode I; Pinball 2000: Revenge From Mars |
| [`SNDBRD_WPCS`](wpcs.md#sndbrd_wpcs) | ✅ | 219 | 25 | Twilight Zone; Addams Family, The; Terminator 2: Judgment Day |
| [`SNDBRD_S11XS+SNDBRD_S11CS`](williams-system11.md#sndbrd_s11xs_s11cs) | ✅ | 109 | 22 | Black Knight 2000; Pin-Bot; Earthshaker |
| [`SNDBRD_S11CS`](williams-system11.md#sndbrd_s11cs) | ✅ | 50 | 10 | Diner; Rollergames; Dr. Dude |
| [`SNDBRD_S9S`](williams-system11.md#sndbrd_s9s) | ✅ | 12 | 7 | Sorcerer; Pennant Fever Baseball; Space Shuttle |
| [`SNDBRD_S11BS+SNDBRD_S11JS`](williams-system11.md#sndbrd_s11bs_s11js) | ✅ | 3 | 1 | Jokerz |
| [`SNDBRD_S11S`](williams-system11.md#sndbrd_s11s) | ✅ | 6 | 5 | Alley Cats; Gold Mine; Top Dawg |
| [`SNDBRD_S67S`](williams-system3-7.md#sndbrd_s67s) | ✅ | 105 | 38 | Black Knight; Firepower; Alien Poker |
| [`SNDBRD_S3DFS`](williams-system3-7.md#sndbrd_s3dfs) | ⚠️ | 1 | 1 | Disco Fever |
| [`SNDBRD_S3S`](williams-system3-7.md#sndbrd_s3s) | ✅ | 3 | 3 | Phoenix; Contact; Pokerino |
| [`SNDBRD_S7S_ND`](williams-system3-7.md#sndbrd_s7s_nd) | ✅ | 3 | 1 | Thunderball |
| [`SNDBRD_S3WCS`](williams-system3-7.md#sndbrd_s3wcs) | ⚠️ | 1 | 1 | World Cup |
| [`SNDBRD_DE2S`](data-east-sega-stern.md#sndbrd_de2s) | ✅ | 307 | 48 | Jurassic Park; Playboy; Batman Forever |
| [`SNDBRD_DE3S`](data-east-sega-stern.md#sndbrd_de3s) | ✅ | 156 | 7 | Lord of the Rings, The; Grand Prix; Ripley's Believe It or Not! |
| [`SNDBRD_DE1S`](data-east-sega-stern.md#sndbrd_de1s) | ✅ | 38 | 15 | Time Machine; Teenage Mutant Ninja Turtles; Back to the Future |
| [`SNDBRD_ALVGS1`](alvin-g.md#sndbrd_alvgs1) | ✅ | 10 | 3 | A.G. Soccer-Ball; U.S.A. Football; Punchy The Clown |
| [`SNDBRD_ALVGS2`](alvin-g.md#sndbrd_alvgs2) | ✅ | 10 | 5 | Al's Garage Band Goes On a World Tour; Mystery Castle; Pistol Poker |
| [`NONE (Stern SAM)`](stern-sam.md#sndbrd_none_sam) | ✅ | 406 | 27 | AC/DC Limited Edition; World Poker Tour; Spider-Man |
| [`SNDBRD_BY51`](bally.md#sndbrd_by51) | ✅ | 41 | 14 | Space Invaders; Silverball Mania; Speakeasy |
| [`SNDBRD_BY32`](bally.md#sndbrd_by32) | ✅ | 39 | 12 | Kiss; Playboy; Six Million Dollar Man, The |
| [`SNDBRD_BY61`](bally.md#sndbrd_by61) | ✅ | 34 | 10 | Flash Gordon; Eight Ball Deluxe; Vector |
| [`SNDBRD_BY45`](bally.md#sndbrd_by45) | ✅ | 23 | 10 | X's & O's; Spy Hunter; Kings of Steel |
| [`SNDBRD_BY61B`](bally.md#sndbrd_by61b) | ✅ | 20 | 6 | Centaur; Embryon; Spectrum |
| [`SNDBRD_BYTCS`](bally.md#sndbrd_bytcs) | ✅ | 12 | 5 | Black Belt; MotorDome; Strange Science |
| [`SNDBRD_BY56`](bally.md#sndbrd_by56) | ✅ | 8 | 1 | Xenon |
| [`SNDBRD_BY61B2`](bally.md#sndbrd_by61b2) | ✅ | 1 | 1 | Mysterian |
| [`SNDBRD_BY61N`](bally.md#sndbrd_by61n) | ✅ | 1 | 1 | Cosmic Flash |
| [`SNDBRD_BYSD`](bally.md#sndbrd_bysd) | ⚠️ | 10 | 6 | Special Force; Party Animal; Heavy Metal Meltdown |
| [`SNDBRD_BY51N`](bally.md#sndbrd_by51n) | ⚠️ | 2 | 1 | Super Bowl |
| [`SNDBRD_BY45BP`](bally.md#sndbrd_by45bp) | ✅ | 4 | 2 | Baby Pac-Man; Granny and the Gators |
| [`SNDBRD_ATARI1`](atari.md#sndbrd_atari1) | ❌ | 12 | 5 | Atarians, The; Middle Earth; Time 2000 |
| [`SNDBRD_ATARI2`](atari.md#sndbrd_atari2) | ❌ | 3 | 3 | Superman; Hercules; Road Runner |
| [`SNDBRD_ST100B`](stern-early.md#sndbrd_st100b) | ✅ | 16 | 7 | Trident; Magic; Cosmic Princess |
| [`SNDBRD_ST100`](stern-early.md#sndbrd_st100) | ✅ | 10 | 4 | Dracula; Wild Fyre; Lectronamo |
| [`SNDBRD_ST300`](stern-early.md#sndbrd_st300) | ❌ | 76 | 17 | Meteor; Dragonfist; Seawitch |
| [`SNDBRD_ST300V`](stern-early.md#sndbrd_st300v) | ⚠️ | 21 | 6 | Flight 2000; Lightning; Orbitor 1 |
| [`SNDBRD_ASTRO`](stern-early.md#sndbrd_astro) | ❌ | 2 | 1 | S.A.M. III Board Tester |
| [`SNDBRD_GTS80B`](gottlieb.md#sndbrd_gts80b) | ✅ | 148 | 32 | Bad Girls; Monte Carlo; Spring Break |
| [`SNDBRD_GTS3`](gottlieb.md#sndbrd_gts3) | ✅ | 51 | 23 | Cue Ball Wizard; Super Mario Bros.; Stargate |
| [`SNDBRD_GTS80SP`](gottlieb.md#sndbrd_gts80sp) | ✅ | 26 | 11 | Alien Star; Chicago Cubs Triple Play; Tag-Team Pinball |
| [`SNDBRD_GTS80S`](gottlieb.md#sndbrd_gts80s) | ✅ | 37 | 19 | Amazing Spider-Man, The; James Bond; Volcano |
| [`SNDBRD_GTS80SS_VOTRAX`](gottlieb.md#sndbrd_gts80ss_votrax) | ✅ | 24 | 9 | Black Hole; Mars - God of War; Caveman |
| [`SNDBRD_GTS80SS`](gottlieb.md#sndbrd_gts80ss) | ✅ | 17 | 8 | Haunted House; Spirit; Royal Flush Deluxe |
| [`SNDBRD_GTS80SS_VOTRAX_OLD`](gottlieb.md#sndbrd_gts80ss_votrax_old) | ✅ | 1 | 0 | Mars - God of War |
| [`SNDBRD_ZAC1125`](zaccaria.md#sndbrd_zac1125) | ✅ | 8 | 4 | Fire Mountain; Shooting the Rapids; Hot Wheels |
| [`SNDBRD_ZAC1346`](zaccaria.md#sndbrd_zac1346) | ⚠️ | 6 | 3 | Locomotion; Space Shuttle; Earth, Wind & Fire |
| [`SNDBRD_ZAC13136`](zaccaria.md#sndbrd_zac13136) | ✅ | 45 | 5 | Time Machine; Farfalla; Devil Riders |
| [`SNDBRD_ZAC1370`](zaccaria.md#sndbrd_zac1370) | ✅ | 25 | 3 | Soccer Kings; Pinball Champ; Thunder Man |
| [`SNDBRD_ZAC11178`](zaccaria.md#sndbrd_zac11178) | ✅ | 18 | 4 | Clown; Black Belt; Pool Champion |
| [`SNDBRD_ZAC11178_13181`](zaccaria.md#sndbrd_zac11178_13181) | ✅ | 12 | 2 | Spooky; Zankor |
| [`SNDBRD_ZAC13181x3`](zaccaria.md#sndbrd_zac13181x3) | ✅ | 8 | 2 | Star's Phoenix; New Star's Phoenix |
| [`SNDBRD_ZAC1311`](zaccaria.md#sndbrd_zac1311) | — | 6 | 3 | Future World; Winter Sports; House of Diamonds |
| [`SNDBRD_GPMSU1`](game-plan.md#sndbrd_gpmsu1) | ✅ | 7 | 7 | Lizard; Global Warfare; Mike Bossy |
| [`SNDBRD_GPSSU1`](game-plan.md#sndbrd_gpssu1) | ✅ | 3 | 3 | Star Trip; Family Fun!; Vegas |
| [`SNDBRD_GPSSU2`](game-plan.md#sndbrd_gpssu2) | ✅ | 1 | 1 | Sharpshooter |
| [`SNDBRD_GPSSU3`](game-plan.md#sndbrd_gpssu3) | ✅ | 1 | 1 | Coney Island! |
| [`SNDBRD_GPSSU4`](game-plan.md#sndbrd_gpssu4) | ✅ | 1 | 1 | Super Nova |
| [`SNDBRD_GPMSU3`](game-plan.md#sndbrd_gpmsu3) | ✅ | 6 | 3 | Andromeda; Lady Sharpshooter; Cyclopes |
| [`SNDBRD_TAITO_SINTEVOX`](taito.md#sndbrd_taito_sintevox) | ✅ | 10 | 5 | Titan; Cavaleiro Negro; Lady Luck |
| [`SNDBRD_TAITO_SINTEVOXPP`](taito.md#sndbrd_taito_sintevoxpp) | ✅ | 2 | 2 | Gork; Fire Action Deluxe |
| [`SNDBRD_TAITO_SINTETIZADOR`](taito.md#sndbrd_taito_sintetizador) | ✅ | 18 | 14 | Shock; Oba-Oba; Gemini 2000 |
| [`SNDBRD_TAITO_SINTETIZADORPP`](taito.md#sndbrd_taito_sintetizadorpp) | ✅ | 8 | 5 | Snake Machine; Mr. Black; Space Shuttle |
| [`SNDBRD_PLAY4`](playmatic.md#sndbrd_play4) | ✅ | 16 | 13 | Mad Race; Meg-Aaton; Raid, The |
| [`SNDBRD_PLAY1`](playmatic.md#sndbrd_play1) | ✅ | 7 | 7 | Space Gambler; Big Town; Last Lap |
| [`SNDBRD_PLAY2`](playmatic.md#sndbrd_play2) | ✅ | 6 | 5 | Antar; Storm; Evil Fight |
| [`SNDBRD_ZSU`](playmatic.md#sndbrd_zsu) | ✅ | 6 | 6 | Skill Flight; Cobra; Phantom Ship |
| [`SNDBRD_PLAY3`](playmatic.md#sndbrd_play3) | ✅ | 2 | 2 | Cerberus; Spain 82 |
| [`SNDBRD_PLAYZ`](playmatic.md#sndbrd_playz) | ✅ | 1 | 1 | Zira |
| [`SNDBRD_ROMSTAR`](capcom-romstar.md#sndbrd_romstar) | ⚠️ | 1 | 1 | Goofy Hoops |
| [`SNDBRD_CAPCOMS`](capcom-romstar.md#sndbrd_capcoms) | ✅ | 17 | 6 | Breakshot; Airborne; Flipper Football |
| [`SNDBRD_NUOVA`](other-makers.md#sndbrd_nuova) | ✅ | 6 | 6 | F1 Grand Prix; Skill Flight; Cobra |
| [`SNDBRD_HANKIN`](other-makers.md#sndbrd_hankin) | ✅ | 5 | 5 | FJ Holden; Orbit 1; Howzat |
| [`SNDBRD_MRGAME`](other-makers.md#sndbrd_mrgame) | ✅ | 5 | 4 | Dakar; Motor Show; Mac Attack |
| [`SNDBRD_GRAND`](other-makers.md#sndbrd_grand) | ✅ | 4 | 1 | 301/Bullseye |
| [`SNDBRD_JEUTEL`](other-makers.md#sndbrd_jeutel) | ✅ | 3 | 3 | Le King; Olympic Games; Papillon |
| [`SNDBRD_BARNI`](other-makers.md#sndbrd_barni) | ✅ | 2 | 2 | Red Baron; Champion |
| [`SNDBRD_JVH2`](other-makers.md#sndbrd_jvh2) | ✅ | 1 | 1 | Formula 1 |
| [`SNDBRD_TECHNO`](other-makers.md#sndbrd_techno) | ✅ | 1 | 1 | Scramble |
| [`SNDBRD_JVH`](other-makers.md#sndbrd_jvh) | ⚠️ | 3 | 3 | Ice Mania; Escape; Movie Masters |
| [`SNDBRD_ROWAMET`](other-makers.md#sndbrd_rowamet) | ⚠️ | 1 | 1 | Heavy Metal |
| [`SNDBRD_TABART`](other-makers.md#sndbrd_tabart) | ⚠️ | 1 | 1 | L'Hexagone |
| [`SNDBRD_TABART2`](other-makers.md#sndbrd_tabart2) | ⚠️ | 1 | 1 | Sahara Love |
| [`SNDBRD_TABART3`](other-makers.md#sndbrd_tabart3) | ⚠️ | 1 | 1 | Grand 8, Le |
| [`SNDBRD_SPINB`](other-makers.md#sndbrd_spinb) | ⚠️ | 27 | 16 | Bushido; Brave Team; Canasta '86' |
| [`SNDBRD_JOCTRONIC`](other-makers.md#sndbrd_joctronic) | ❌ | 3 | 3 | Punky Willy; Walkyria; Pin Ball |
| [`SNDBRD_TECNOPLAY`](other-makers.md#sndbrd_tecnoplay) | ❌ | 2 | 2 | X Force; Space Team |
| [`NONE (other)`](no-sound-board.md#sndbrd_none_other) | — | 257 | 172 | Mata Hari; Golden Game; Michigan |


## <a name="sndbrd_none"></a>SNDBRD_NONE

PinMAME gives no sound board to 663 sets. Two rows of the survey share that constant:
the 406 Stern SAM sets (driver `src/wpc/sam.c`), whose sound is mixed by the main CPU
and which rom2altsound reads from the ROM image ([Stern SAM](stern-sam.md#sndbrd_none_sam)),
and the 257 others, which have nothing rom2altsound could send a command to
([no sound board](no-sound-board.md#sndbrd_none_other)). In `families.json` they are
`SNDBRD_NONE (Stern SAM)` and `SNDBRD_NONE (other)`; a set's driver source tells them
apart.
