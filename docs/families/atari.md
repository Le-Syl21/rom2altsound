# Atari sound boards

The two Atari discrete sound boards of `src/wpc/atarisnd.c`: `SNDBRD_ATARI1` (Generation
1) and `SNDBRD_ATARI2` (Generation 2). Everything not said here is the
[common method](common.md). Neither has a sound CPU or a command set: the game's CPU
builds every sound by writing the board's tone registers, one step at a time. The sounds
are asked for from the game's own sound layer instead
([game-driven boards](common.md#game-driven-boards), `src/gamesound.rs`, `atari`).

## <a name="sndbrd_atari1"></a>SNDBRD_ATARI1

Atari Generation 1 sound · PinMAME interface `ATARI1` (`src/wpc/atarisnd.c`) · ✅
(game-driven) · 12 sets, 5 games, 3 sound ROM ids, 1976-2024, Atari · e.g. The Atarians
(`atarians`), Time 2000 (`time2000`), Airborne Avenger (`aavenger`), Middle Earth
(`midearth`), Space Riders (`spcrider`)

- **Hardware**: no sound CPU. A PROM holds 16 waveforms of 32 steps; PinMAME plays the
  selected one as a looping mixer sample (`atari1s_custInt`, `playSound1`).
- **Commands**: none. The game writes latches (`src/wpc/atari.c`): `latch1080_w` sends the
  waveform and an enable bit to `sndbrd_0_ctrl_w`, `latch1084_w`/`latch1088_w` the
  frequency and volume nibbles to `sndbrd_0_data_w`; `soundg1_w` (`3000`) and `audiog1_w`
  (`6000`) set the enable. These latch writes are the `00` bytes the board log shows
  (atarians: 14498 in the boot); the survey's sweep set a frequency and volume with no
  enable (0 of 40 before).
- **The game's sound layer**: a routine the main loop calls steps the sound one table
  entry per call and writes the latches; what the rest of the game sets to ask for a
  sound is a RAM byte per sound. Three layouts among the five programs, each read in the
  program:
  - The Atarians, Time 2000, Airborne Avenger: one down-counter per sound (Atarians `$C5`,
    `$A5`, `$A4`, `$A3`): the routine (`785D`: `LDAA c; BEQ; LDX #table; JSR index; LDAB
    wave; DEC c`, one block per counter) plays the highest one that is not zero, one step
    of its table per call, and turns the tone off when all are zero. The game starts a
    sound by storing its length (`LDAA #1F; STAA $A5`). Sound list: every length the
    program stores in each counter. Id: **the counter's address and the length**
    (`0xA51F`).
  - Space Riders: a table of 13 slots, a step byte and a pending count each (`$8D`..`$A6`,
    `7AC4`: `LDAA 0,X; BNE; LDAA 1,X; BEQ; DEC 1,X`); the game asks for slot `n` by adding
    to its pending count. Id: **the slot number**.
  - Middle Earth: eight sound descriptors (`LDX #descriptor; BSR 7980`), each pointing at
    its own step and pending bytes in RAM. Id: **the descriptor's address** (`0x7D35`).
- **Request**: the counter's length, or a pending count of 1. **Stop**: every counter, slot
  or step and pending byte back to 0, as the game leaves them when nothing plays.
- **Measured** (every sound of the list): atarians 6 of 6, time2000 7 of 7, aavenger 10 of
  10, midearth 8 of 8, spcrider 13 of 13, all from silence; short sounds (0.1 to 0.4 s,
  peaks at -26 to -16 dBFS).
- **Limits**: `mideartp` (a prototype, its `c.e0` a bad dump in PinMAME) has no routine of
  these kinds: no sound layer, the run stops with an error. A sound the game asks for
  several times in a row is recorded once.
- **In VPinball**: **the pack cannot play**: no command reaches AltSound for these sounds
  (the latch writes are what PinMAME logs, and libaltsound has no case for generation 0,
  so it pairs them: [In VPinball](common.md#in-vpinball)). Measured on `aavenger`: the
  game sent `02 02 02...`, AltSound looked up `0202`. The pack's ids are the game's own.

## <a name="sndbrd_atari2"></a>SNDBRD_ATARI2

Atari Generation 2 sound · PinMAME interface `ATARI2` (`src/wpc/atarisnd.c`) · ✅
(game-driven) · 3 sets, 3 games, 1 sound ROM id, 1979, Atari · Superman (`superman`),
Hercules (`hercules`), Road Runner (`roadrunr`, prototype)

- **Hardware**: no sound CPU; a PROM waveform channel and a noise channel
  (`atari2s_custInt`, `playSound`).
- **Commands**: none. The game writes `sound0_w` (`1800`) → `sndbrd_ctrl_w` (noise on,
  wave on, octave, waveform) and `sound1_w` (`1820`) → `sndbrd_data_w` (frequency divider,
  amplitude) (`src/wpc/atari.c`), one step at a time. The survey's sweep reached only the
  frequency and amplitude (superman: 0 of 40 before).
- **The game's sound layer**: the system program (the M and J ROMs, shared by Superman and
  Hercules; Road Runner's differs) keeps one pending count per sound number (`$2F`..`$42`),
  and a driver in the interrupt plays the pending sounds from a table of 6-byte
  descriptors in the game ROM (`A8B7`: the pitch table, the control byte for `1800`, two
  lengths, repeat and flags). The game's rule code asks for sound `n` through its sound
  instruction (entry `10` of the system's jump table, `3214`: `INC count[n]`, the high bits
  of the operand a repeat count). Sound list: the descriptors in use, up to the driver's
  count (`CMPA #20`): the table is followed by other data. Id: **the sound number**.
- **Request**: a pending count of 1. **Stop**: every count to 0, the driver's current sound
  to none (`$46` = `FF`), and `00` to `1800`, as the driver does at the end of a pass.
- **Measured**: superman 18 of 18, hercules 20 of 20, roadrunr 16 of 16, all from silence
  (0.07 to 0.9 s, peaks at -18 to -8 dBFS).
- **In VPinball**: **the pack cannot play**: the sound register writes are what PinMAME
  logs (the game sent no `sndbrd_data_w` byte at boot on superman; generation 0 is paired
  by libaltsound: [In VPinball](common.md#in-vpinball)). Measured on `spcrider` (a
  generation 1 machine with this sound ROM): the game sent `FE FE FE...`, AltSound looked
  up `FEFE`. The pack's ids are the game's own.
