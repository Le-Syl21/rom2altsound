# Early Stern sound boards

The Stern boards of `src/wpc/stsnd.c`, on Bally-style CPU boards (`src/wpc/by35.c`,
`src/wpc/stgames.c`): `SNDBRD_ST100`, `SNDBRD_ST100B` (discrete tones), `SNDBRD_ST300`,
`SNDBRD_ST300V` (programmable timers, the V with VS-1000 speech) and `SNDBRD_ASTRO`.
Everything not said here is the [common method](common.md).

None of these boards has a CPU: PinMAME emulates them as custom sound generators
(`st100_sh_start`, `st100b_sh_start`, `st300_sh_start`). So a board reset
(`shim_reset_audio_cpus`, the stop of the boards with none in `BUILTIN_STOPS`: all but
`ST100`) resets nothing, and the sound CPU state method has nothing to read: loops can only come from the
audio. No master volume is decoded (`none: recorded at the game's own volume`), and the
pack has the default columns. None has a sound ROM except the ST300V's speech ROM.

## <a name="sndbrd_st100"></a>SNDBRD_ST100

Stern SB-100 · PinMAME interface `ST100` (`src/wpc/stsnd.c`) · ✅ · 10 sets, 4 games, no
sound ROM, 1978-2022, Stern · Dracula (`dracula`), Lectronamo (`lectrono`), Wild Fyre
(`wildfyre`), Nugent (`nugent`)

- **Hardware**: discrete tones and electronic chimes, emulated with mixer samples; DIP 23
  picks tones or chimes (`stsnd.c` header).
- **Commands**: the game writes address `A0` for the tones (`by35.c` `stern100_snd_w` →
  `sndbrd_0_data_w`) and `C0` for the chimes (`stern100_chm_w` → `sndbrd_0_ctrl_w`).
  The data byte is a bit mask: bits 0 to 5 each switch one tone on or off
  (`sts_data_w`); the switching off of bit 5 is commented out. `manCmd_w` is
  `sts_data_w` itself, so a swept byte turns on the tones of its bits.
- **Sound list**: the generic `01`..`FF`, i.e. tone combinations, many alike.
- **Stop, boot and resets**: `00` (`BUILTIN_STOPS`), the empty mask, which switches the
  tones off (`sts_data_w`; the switching off of bit 5 is commented out there, yet the
  commands `20`..`28` were followed by silent starts too). Before, the stop was a board
  reset, which does nothing on this board: the tone left on sustained into the next
  command.
- **Measured** (survey settings, board-support): dracula 40 of 40, all from silence (38
  not from silence before the stop `00`). 39 of the 40 files run to the 5 s cap: a tone
  is held as long as its bit is set, so each file is one held tone combination.
- **Limits**: the files are held tones, cut at `--max-secs` or at their loop (a full run
  with the loop search is not measured).
- **In VPinball**: **the pack plays from VPinball master 3abe805 on**, not in 10.8.1-5436 and older:
  their libaltsound has no case for `GEN_STMPU100` and joins the bytes two by two ([In VPinball](common.md#in-vpinball)).
  Measured on `dracula`: the game sent `00 00 00...`, AltSound looked up `0000`; with one
  byte per command
  ([vpinball/libaltsound#20](https://github.com/vpinball/libaltsound/pull/20), in VPinball master from 3abe805) it
  looks up `0000`. The bytes are the mask bytes the game writes at `A0`; a tone is a
  state, not a sound with an end.

## <a name="sndbrd_st100b"></a>SNDBRD_ST100B

Stern SB-100 without chimes · interface `ST100`, sub-type 1 (`src/wpc/stsnd.c`) · ✅ ·
16 sets, 7 games, no sound ROM, 1979-2022, Stern, Monroe Bowling · Trident (`trident`),
Hot Hand (`hothand`), Magic (`magic`), Cosmic Princess (`princess`)

- **Hardware**: the ST100's tones without the chimes (`st100b_sh_start`); the game only
  writes `A0` (`by35.c` `MACHINE_INIT(by35)`).
- **Everything else**: as [SNDBRD_ST100](#sndbrd_st100).
- **Measured**: trident 40 of 40, all from silence with the stop `00` (38 not from silence
  before: the tones sustained), 39 held to the 5 s cap (board-support).

## <a name="sndbrd_st300"></a>SNDBRD_ST300

Stern SB-300 · PinMAME interface `ST300` (`src/wpc/stsnd.c`) · ✅ (game-driven) · 76 sets,
17 games, no sound ROM, 1979-2026, Stern · Meteor (`meteor`), Galaxy (`galaxy`), Seawitch
(`seawitch`), Nine Ball (`nineball`)

- **Hardware**: three programmable timers (an MC6840-like timer set), a noise generator
  and a volume envelope, emulated in C (`st300_sh_start`, `st300_data_w`,
  `st300_pulse`).
- **Commands**: none. The game writes the timer registers at `A0`-`A7` (`by35.c`
  `snd300_w`: the value is stored in `snddatst300.ax[offset]` and the register number
  goes to `sndbrd_0_data_w`) and a control byte at `C0` (`snd300_wex` →
  `sndbrd_0_ctrl_w`). `manCmd_w` is `st300_man_w`, the speech path of the ST300V. The
  survey's sweep (`01`..`28`) reached neither (meteor: 0 of 40 before).
- **The game's sound layer** (`src/gamesound.rs`, `st300`; the method:
  [game-driven boards](common.md#game-driven-boards)). Every program of the family has,
  in its interrupt handler, an interpreter of sound scripts: a 16-bit pointer in RAM to
  the next script byte (`$6D` on Meteor, Galaxy and Ali, `$74` after) and a delay byte
  (`$4C`, then `$53`). When the delay has run out the handler reads the script: register
  writes (`A2 hh ll`...), delays, counted loops, jumps, pitch steps, control bits, and an
  op that silences the board (`A1`/`A0` reset, `C0` 0) and ends the script. The game
  starts a sound by storing the script's address in the pointer and clearing the delay
  (Meteor `5808`: `JSR 5373` reads the word after the call, `STX $6D`, `CLR $4C`); its
  thread code asks for one with the thread instructions `57` and `58` followed by the
  script's address (`58 5883`), and some sounds come from tables or a direct `LDX #script`.
  The script format changed over the years: six interpreters among the family's 15
  programs (A: Meteor, Galaxy, Ali; A': Big Game; B': Seawitch; C: Nine Ball; B: Cheetah,
  Star Gazer, Quicksilver; E: Viper, Dragonfist, Iron Maiden, Lazer Lord, Cue; F:
  Gamatron, Flight 2000's system). Each is told by its code (its entry, and for B and F
  the table of handlers of their opcodes `08` up), and each opcode's length was read in
  its handler.
- **Sound list**: every address the program refers to (thread instruction `57`/`58`,
  `LDX #` before `STX pointer`, table entries read before `STX pointer`) where a script of
  that interpreter starts, sets a pitch somewhere, and starts as a sound does (a register,
  timer or control op; a loop counter when the address is not inside another script).
  The rest are coincidences of bytes, the inside of another script, or the self-test's
  `C0` level steps (`05 xx 01`, cumulative, no sound of their own). Id: **the script's
  address** (`0x5883`).
- **Request**: the delay byte to 0 and the pointer to the script, in one frame. **Stop**:
  the pointer on the game's own silencing op (the end of one of its scripts: `08` on most
  interpreters, `00` on B and F), which the interrupt executes as the game's scripts end.
- **Measured** (`--max-secs 5`, every sound of the list, no loop search): meteor 16 of 16,
  galaxy 17 of 17, ali 27 of 28, biggame 21 of 22, seawitch 22 of 22, nineball 16 of 16,
  cheetah 29 of 31, stargzr 25 of 27, quicksil 27 of 28, viper 17 of 17, dragfist 19 of
  20, ironmaid 18 of 18, lazrlord 23 of 23, cue 14 of 14, gamatron 17 of 18: 308 sounds
  of 317 on the 15 programs, every file from silence. A few sounds hold a tone until
  stopped (3 of Meteor's). The MOD sets (2010s) whose code moved, Dragonfist MOD 14
  (19 of 19) and Nine Ball's ball-handling MOD (16 of 16) included: the layer is found on
  all 67 sets of the family in the full set.
- **Limits**: the 9 that play nothing are either bytes that only look like a thread
  instruction and a script (Cheetah's `580D` and `5858`, words of a table of routine
  addresses) or scripts that need what the game sets during play (Star Gazer's `599C`,
  `59B0`). A sound the game asks for again and again (a spinner) is recorded once.
- **In VPinball**: **the pack cannot play**: no command reaches AltSound for these sounds
  (the game's register writes are what PinMAME logs: `GEN_STMPU200` has no case in
  libaltsound and the bytes are paired, see [In VPinball](common.md#in-vpinball)). Measured
  on `ali`: the game sent `01 00 01 00 06 07 04 05`, AltSound looked up `0100 0100 0607
  0405`. These are timer register numbers; the pack's ids are script addresses.

## <a name="sndbrd_st300v"></a>SNDBRD_ST300V

Stern SB-300 with the VS-1000 speech board · interface `ST300`, sub-type 1
(`src/wpc/stsnd.c`) · ⚠️ · 21 sets, 6 games, 7 sound ROM ids, 1980-2024, Stern · Flight
2000 (`flight2k`), Free Fall (`freefall`), Split Second (`splitsec`), Orbitor 1
(`orbitor1`)

- **Hardware**: the ST300's timers plus an S14001A speech chip with its ROM
  (`MACHINE_DRIVER_START(st300v)`).
- **Commands**: the effects as ST300. The game's speech goes through `by35.c`
  `pia1ca2_w`: `sndbrd_0_diag(1)` (`st300_switch_w`, the speech path) then
  `sndbrd_0_ctrl_w` with the word; a write `40 | word` starts word `word` (`S14001A_reg_0_w`),
  `80 | ...` sets speed and volume. `st300_man_w` takes the same path.
- **Sound list**: `40`..`7F`, the S14001A's 64 words (`sweep`, `"ST300"` with sub-type 1,
  `ST300V_SUBTYPE`); the generic `01`..`FF` before, whose first 40 commands (`01`..`28`)
  never reached them. The speech plays at the chip's power-on rate (34722 Hz, "what is
  set by all Stern machines as first clock", `s14001a_sh_start`) and the mixer's
  default volume: the game's own speed and volume byte (`80` and up, sent through
  `sndbrd_ctrl_w`, which the boot log does not show) is not known.
- **Measured** (survey settings, board-support): flight2k 37 of 40, freefall 40 of 40,
  all from silence: the speech words, 0.2 to 0.4 s each (12 and 10 of them clipped,
  peaks at 0 dBFS; 0 of 40 before).
- **Limits**: only the speech is extracted. The effects, most of the game's sounds, are
  the game's own scripts as on ST300 (interpreter F, Gamatron's, read by
  [ST300's layer](#sndbrd_st300)), but the speech sweep halts the game CPU, which plays
  them: not done. The stop is still a board reset, which does nothing on a board without
  a CPU (the words end by themselves).
- **In VPinball**: **the pack does not play, in any VPinball so far**: its speech words go
  through `sndbrd_ctrl_w`, which is not logged, and up to 10.8.1-5436 libaltsound has no
  case for `GEN_STMPU200` and joins the bytes two by two ([In VPinball](common.md#in-vpinball)).
  Measured on `catacomb`: the game sent `01 00 06 07 04 05 02 03... (46018 in 45 s)`,
  AltSound looked up `0100 0607 0405 0203`; with one byte per command
  ([vpinball/libaltsound#20](https://github.com/vpinball/libaltsound/pull/20), in VPinball master from 3abe805) it
  looks up `0001 0000 0006 0007`. The speech goes through `sndbrd_ctrl_w`, which
  AltSound does not receive (`snd_cmd_log` is called from `sndbrd_data_w` only).

## <a name="sndbrd_astro"></a>SNDBRD_ASTRO

Stern Astro board · PinMAME interface `ASTRO` (`src/wpc/stsnd.c`) · ✅ (game-driven) · 2
sets, 1 game, no sound ROM, date unknown, Stern · S.A.M. III Board Tester (`sam_iii`)

- **Hardware**: the ST300 generator ("can switch between SB-100 and SB-300", `stsnd.c`
  header).
- **Commands**: none: the program writes the ST300 registers (`by35.c` `snd300_w`), the
  `00`/`01` the board log shows (20 bytes at boot) are register numbers. `manCmd_w` is
  `st300_data_w`: the byte is taken as a register number (sam_iii: 0 of 40 before).
- **The game's sound layer**: the tester's program has Meteor's interpreter (A, entry
  `1AA8`, pointer `$23`, delay `$2A`) and no thread engine: its sound tests load a script
  and call the routine that silences the board and stores the pointer (`1505`: `LDX
  #55B7; BRA 1512`). Six scripts (`LDX #` before a `BRA`/`JMP` to that routine). Id: the
  script's address. Request and stop as [ST300](#sndbrd_st300).
- **Measured**: sam_iii 6 of 6, from silence; 4 are held test tones (5 s cap).
- **Limits**: `sam_iv` (the external S.A.M. IV tester) boots its own program at `3000`,
  which has no sound code; it only carries the S.A.M. III's (not run): no sound layer, the
  run stops with an error.
- **In VPinball**: **the pack cannot play**: no command reaches AltSound (`GEN_ASTRO` has no
  case in libaltsound and the register writes are paired: measured on `sam_iii`, the game
  sent `00 01 00 01 00 01`, AltSound looked up `0001 0001 0100`).
