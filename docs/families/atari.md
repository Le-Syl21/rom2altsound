# Atari sound boards

The two Atari discrete sound boards of `src/wpc/atarisnd.c`: `SNDBRD_ATARI1` (Generation
1) and `SNDBRD_ATARI2` (Generation 2). Everything not said here is the
[common method](common.md). Neither has a sound CPU or a command set: the game's CPU
builds every sound by writing the board's tone registers, so the command sweep does not
apply.

## <a name="sndbrd_atari1"></a>SNDBRD_ATARI1

Atari Generation 1 sound · PinMAME interface `ATARI1` (`src/wpc/atarisnd.c`) · ❌ · 12
sets, 5 games, 3 sound ROM ids, 1976-2024, Atari · e.g. The Atarians (`atarians`), Time
2000 (`time2000`), Airborne Avenger (`aavenger`), Middle Earth (`midearth`)

- **Hardware**: no sound CPU. A PROM holds 16 waveforms of 32 steps; PinMAME plays the
  selected one as a looping mixer sample (`atari1s_custInt`, `playSound1`).
- **Commands**: the game writes latches (`src/wpc/atari.c`): `latch1080_w` sends the
  waveform and an enable bit to `sndbrd_0_ctrl_w`, `latch1084_w`/`latch1088_w` the
  frequency and volume nibbles to `sndbrd_0_data_w`; `soundg1_w`/`audiog1_w` set the
  enable. `manCmd_w` is the data handler (`atari_data1_w`): a byte sets frequency (low
  nibble) and volume (high nibble) of a tone that only plays if the enable, set through
  the control path, is on.
- **Sound list**: the generic `01`..`FF`; each byte is a frequency/volume pair, not a
  sound.
- **Stop, boot and resets**: no stop known: a board reset after every command, which does
  nothing (no audio CPU, `shim_reset_audio_cpus`).
- **Volume**: none decoded: `none: recorded at the game's own volume`.
- **Loops**: audio method only (no audio CPU).
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured**: atarians 0 of 40; the game streams `00` bytes (14498 in the boot)
  (board-support).
- **Limits**: discrete sound driven by lines, not by command numbers (board-support).
  From the code, a sound is a sequence of register writes the game makes over time; there
  is no command to record. rom2altsound cannot support it with a sweep.
- **In VPinball**: **the pack does not play as written**: libaltsound has no case for
  generation 0 (none) and joins the bytes two by two ([In
  VPinball](common.md#in-vpinball)). Measured on `aavenger`: the game sent `02 02 02...`,
  AltSound looked up `0202`; with one byte per command
  ([vpinball/libaltsound#20](https://github.com/vpinball/libaltsound/pull/20), draft) it
  would look up `0002`. These bytes are the frequency/volume writes, not sound commands: a
  pack keyed by command could not be triggered either way.

## <a name="sndbrd_atari2"></a>SNDBRD_ATARI2

Atari Generation 2 sound · PinMAME interface `ATARI2` (`src/wpc/atarisnd.c`) · ❌ · 3
sets, 3 games, 1 sound ROM id, 1979, Atari · Superman (`superman`), Hercules
(`hercules`), Road Runner (`roadrunr`, prototype)

- **Hardware**: no sound CPU; a PROM waveform channel and a noise channel
  (`atari2s_custInt`, `playSound`).
- **Commands**: the game writes `sound0_w` → `sndbrd_ctrl_w` (noise on, wave on, octave,
  waveform) and `sound1_w` → `sndbrd_data_w` (frequency divider, amplitude)
  (`src/wpc/atari.c`). `manCmd_w` is `atari_data_w`: frequency and amplitude only; the
  wave and noise enables are on the control path.
- **Sound list, stop, volume, loops, columns**: as ATARI1 (generic sweep, a reset that
  does nothing, no volume decoded, audio loops only, defaults).
- **Measured**: superman 0 of 40; the game sent no sound byte at boot (board-support).
- **Limits**: as ATARI1: driven by lines, not by command numbers.
- **In VPinball**: **the pack does not play as written**: libaltsound has no case for
  generation 0 (none) and joins the bytes two by two ([In
  VPinball](common.md#in-vpinball)). Measured on `spcrider`: the game sent `FE FE FE...`,
  AltSound looked up `FEFE`; with one byte per command
  ([vpinball/libaltsound#20](https://github.com/vpinball/libaltsound/pull/20), draft) it
  would look up `00FE`. As ATARI1, these are tone writes, not commands.
