# Early Stern sound boards

The Stern boards of `src/wpc/stsnd.c`, on Bally-style CPU boards (`src/wpc/by35.c`,
`src/wpc/stgames.c`): `SNDBRD_ST100`, `SNDBRD_ST100B` (discrete tones), `SNDBRD_ST300`,
`SNDBRD_ST300V` (programmable timers, the V with VS-1000 speech) and `SNDBRD_ASTRO`.
Everything not said here is the [common method](common.md).

None of these boards has a CPU: PinMAME emulates them as custom sound generators
(`st100_sh_start`, `st100b_sh_start`, `st300_sh_start`). So a board reset
(`shim_reset_audio_cpus`, the only stop they get: none is in `BUILTIN_STOPS`) resets
nothing, and the sound CPU state method has nothing to read: loops can only come from the
audio. No master volume is decoded (`none: recorded at the game's own volume`), and the
pack has the default columns. None has a sound ROM except the ST300V's speech ROM.

## <a name="sndbrd_st100"></a>SNDBRD_ST100

Stern SB-100 · PinMAME interface `ST100` (`src/wpc/stsnd.c`) · ⚠️ · 10 sets, 4 games, no
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
- **Stop, boot and resets**: no stop: the reset does nothing, so a tone left on sustains
  into the next command.
- **Measured**: dracula 40 of 40, 38 not from silence (board-support).
- **Limits**: "the tones sustain, no stop known"; the fix listed is the board's idle byte
  as the stop (board-support, cheapest fix 6). From `sts_data_w`, `00` switches off the
  tones of bits 0 to 4 (not bit 5); not tried (`--stop 0x00`).
- **In VPinball**: AltSound receives the mask bytes the game writes at `A0`, one per
  change; the pack's ids are masks too, but a tone is a state, not a sound with an end.
  Not tested.

## <a name="sndbrd_st100b"></a>SNDBRD_ST100B

Stern SB-100 without chimes · interface `ST100`, sub-type 1 (`src/wpc/stsnd.c`) · ⚠️ ·
16 sets, 7 games, no sound ROM, 1979-2022, Stern, Monroe Bowling · Trident (`trident`),
Hot Hand (`hothand`), Magic (`magic`), Cosmic Princess (`princess`)

- **Hardware**: the ST100's tones without the chimes (`st100b_sh_start`); the game only
  writes `A0` (`by35.c` `MACHINE_INIT(by35)`).
- **Everything else**: as [SNDBRD_ST100](#sndbrd_st100).
- **Measured**: trident 40 of 40, 38 not from silence: the tones sustain (board-support).

## <a name="sndbrd_st300"></a>SNDBRD_ST300

Stern SB-300 · PinMAME interface `ST300` (`src/wpc/stsnd.c`) · ❌ · 76 sets, 17 games, no
sound ROM, 1979-2026, Stern · Meteor (`meteor`), Galaxy (`galaxy`), Seawitch
(`seawitch`), Nine Ball (`nineball`)

- **Hardware**: three programmable timers (an MC6840-like timer set), a noise generator
  and a volume envelope, emulated in C (`st300_sh_start`, `st300_data_w`,
  `st300_pulse`).
- **Commands**: not a command board. The game writes the timer registers at `A0`-`A7`
  (`by35.c` `snd300_w`: the value is stored in `snddatst300.ax[offset]` and the
  register number goes to `sndbrd_0_data_w`) and a control byte at `C0` (`snd300_wex` →
  `sndbrd_0_ctrl_w`). `manCmd_w` is `st300_man_w`, which selects the speech path of
  `st300_ctrl_w` (`voiceSw = 1`): bytes `40`..`7F` go to the S14001A speech chip, `80` and
  above set its speed and volume. On ST300 there is no speech chip.
- **Sound list**: the generic `01`..`FF`. The survey's first 40 (`01`..`28`) reach
  neither path.
- **Measured**: meteor 0 of 40; boot: register/value pairs `01 00 06 07 04 05`
  (board-support).
- **Limits**: the game programs the timers itself; the command sweep does not apply
  (board-support). Not determined whether sending `40`..`7F` to a machine without the
  S14001A is harmless.
- **In VPinball**: AltSound receives timer register numbers, not sounds. Not tested.

## <a name="sndbrd_st300v"></a>SNDBRD_ST300V

Stern SB-300 with the VS-1000 speech board · interface `ST300`, sub-type 1
(`src/wpc/stsnd.c`) · ❌ · 21 sets, 6 games, 7 sound ROM ids, 1980-2024, Stern · Flight
2000 (`flight2k`), Free Fall (`freefall`), Split Second (`splitsec`), Orbitor 1
(`orbitor1`)

- **Hardware**: the ST300's timers plus an S14001A speech chip with its ROM
  (`MACHINE_DRIVER_START(st300v)`).
- **Commands**: the effects as ST300. The game's speech goes through `by35.c`
  `pia1ca2_w`: `sndbrd_0_diag(1)` (`st300_switch_w`, the speech path) then
  `sndbrd_0_ctrl_w` with the word; a write `40 | word` starts word `word` (`S14001A_reg_0_w`),
  `80 | ...` sets speed and volume. `st300_man_w` takes the same path.
- **Sound list**: the generic `01`..`FF`. From the code, `40`..`7F` should play the 64
  speech words; the survey (`01`..`28`) never reached them.
- **Measured**: flight2k 0 of 40 (board-support).
- **Limits**: as ST300 for the effects. The speech words look reachable with a full sweep
  or `--only 0x40,...`; not tried.
- **In VPinball**: the speech goes through `sndbrd_ctrl_w`, which AltSound does not
  receive (`snd_cmd_log` is called from `sndbrd_data_w` only). Not tested.

## <a name="sndbrd_astro"></a>SNDBRD_ASTRO

Stern Astro board · PinMAME interface `ASTRO` (`src/wpc/stsnd.c`) · ❌ · 2 sets, 1 game,
no sound ROM, date unknown, Stern · S.A.M. III Board Tester (`sam_iii`)

- **Hardware**: the ST300 generator ("can switch between SB-100 and SB-300", `stsnd.c`
  header).
- **Commands**: the game writes the ST300 registers (`by35.c` `snd300_w`). `manCmd_w` is
  `st300_data_w`: the byte is taken as a register number, whose value the tool cannot
  set.
- **Measured**: sam_iii 0 of 40; the game toggles `00`/`01` (20 bytes at boot): a line,
  not a command (board-support).
- **Limits**: as ST300.
- **In VPinball**: as ST300. Not tested.
