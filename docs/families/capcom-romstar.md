# Capcom and Romstar sound boards

`SNDBRD_CAPCOMS` (Capcom's MPEG sound board) and `SNDBRD_ROMSTAR` (Romstar's Goofy Hoops,
QSound). Both are sub-types of one PinMAME interface, `"TMS320AV120"`
(`src/wpc/capcoms.c`, `capcomsIntf`), whose manual handler does two different things by
sub-type. rom2altsound drives them with the [common method](common.md): the interface is
named nowhere in `sweep`, `BUILTIN_STOPS` or `volume::full_scale`, so a sounds.dat section
if the game has one, else the raw sweep `01`..`FF`; a board reset as the stop
(`shim_reset_audio_cpus`); no volume command; default pack columns.

## <a name="sndbrd_capcoms"></a>SNDBRD_CAPCOMS

Capcom sound board (Intel 87C52 + one or two TMS320AV120 MPEG decoders) · PinMAME
interface `TMS320AV120` (`src/wpc/capcoms.c`) · status ❌ · 17 sets, 6 games, 11 sound ROM
ids, 1995-2000, Capcom, Illinois Pinball · e.g. Breakshot (`bsv103`), Pinball Magic
(`pmv112`), Flipper Football (`ffv104`), Big Bang Bar (`bbb109`)

- **Hardware**: an 87C52 (emulated as an `I8752` at 12 MHz, `MACHINE_DRIVER_START(capcoms)`,
  `CPU_AUDIO_CPU`) with 32 KB of external RAM, one TMS320AV120 MPEG-1 layer 2 decoder on
  Breakshot (`capcom1s`), two on the others (`capcom2s`), an X9241 digital volume pot read
  from the 87C52's serial lines. The file header lists PinMAME's own workarounds: the BOF
  line timing is not emulated, the ROM is patched past its start-up tests
  (`TEST_BYPASS`), resets of the board misbehave on Kingpin.
- **Commands**: the game's 68306 talks to the board over a serial line: its DUART
  transmitter calls `send_data_to_8752` directly (`src/cpu/m68000/m68kmame.c`), which
  latches the byte and raises the 87C52's serial RX interrupt. **Not through
  `sndbrd_data_w`**: `capcomsIntf` has no data handler, so the game's bytes are never
  logged ("the game sent no sound byte at boot", [board support](../board-support.md)).
  The manual handler `capcoms_sndCmd_w` (sub-type 0) calls the same
  `send_data_to_8752`: one call is one serial byte. A command is several bytes: the
  sounds.dat sections of Kingpin and Big Bang Bar write them as `DA 04 07 vv nnnn`, with
  the comments of `bbb109:` reading `DA` "command", `04` unknown, `07` "don't loop?" /
  `06` "loop", `vv` "volume?", `nnnn` the sample number; `DA 02 03 01` is listed as
  "Stop sound?".
- **Sound list**: `kpb105`, `bbb109` and its clone `bbb108` have a sounds.dat section
  (`game_entries`, prefix rule: 5 entries for Kingpin, 21 for Big Bang Bar): for these
  three sets the tool plays those multi-byte commands, one byte every 4 frames, with their
  names. Every other set gets the raw sweep `01`..`FF`, single bytes, which are not
  commands of this protocol.
- **Stop, boot and resets**: board reset after every sound (the 87C52's reset line), 4 s
  wait. The `DA 02 03 01` entry of sounds.dat is not used as the stop: `stop_sends` only
  looks for "sound off"/"reset sound" names in a section named after the interface.
- **Volume**: the X9241 pot is the board's volume, set by its own program from what the
  game sends (`X9241_DELAY_COMMAND`); no master volume command is decoded by rom2altsound.
- **Loops**: audio only: the 87C52's read map has no `MRA_RAM` range (external RAM goes
  through `ram_r`, `capcoms_readmem`), so there is no sequencer state to read.
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured**: `bsv103`: 40 identical 0.2 s clicks, the same peak, after 121 board
  resets, no sound; `pmv112` 0 of 40 ([board support](../board-support.md)). The sets
  with a sounds.dat section were not run.
- **Limits and what is missing**: the single-byte sweep does not speak the board's
  serial protocol. The cheapest fix from what is known: build the commands as sounds.dat's
  `DA 04 07 vv nnnn` and sweep the sample number `nnnn` (with `DA 04 06 ...` for loops),
  and try `DA 02 03 01` as the stop; whether the other games use the same format is not
  known. PinMAME's own notes (start-up tests bypassed, reset problems) may still limit the
  result.
- **In VPinball**: **the pack cannot play**: the game's bytes go from the 68306's DUART
  straight to `send_data_to_8752`, never through `sndbrd_data_w`, so `snd_cmd_log` (and
  AltSound) never sees them.

## <a name="sndbrd_romstar"></a>SNDBRD_ROMSTAR

Romstar Goofy Hoops (QSound, no sound CPU) · PinMAME interface `TMS320AV120`
(`src/wpc/capcoms.c`, sub-type 1) · status ⚠️ · 1 set, 1 game, 1 sound ROM id, 1994,
Romstar · e.g. Goofy Hoops (`ghv101`)

- **Hardware**: a 68306 game CPU only (no audio CPU) and a Capcom QSound chip, which the
  game's code drives directly (`src/wpc/capcom.c`, `MACHINE_INIT(romstar)` and the QSound
  handlers around `qsound_cmd_w`).
- **Commands**: the game writes the QSound registers itself; nothing goes through
  `sndbrd_data_w`. `sndbrd_0_init` is called only "needed for sound commander to work"
  (comment in `MACHINE_INIT(romstar)`). The manual handler (`capcoms_sndCmd_w`, sub-type
  1, commented "for testing Goofy Hoops' Q-Sound chip") does not send a command: it
  programs a QSound voice itself, start address `(data & 1) ? 0x8000 : 0` in bank
  `(data & 0x7F) >> 1`, a fixed pitch, length `0x7FFF`, no loop, pan from bits 0 and 7, and
  starts it.
- **Sound list**: raw sweep `01`..`FF`: each byte plays a fixed 32 KB window of the QSound
  sample ROM at a fixed pitch, not one of the game's sounds.
- **Stop, boot and resets**: the stop is a board reset, but there is no audio CPU to
  reset (`shim_reset_audio_cpus` resets nothing) and the QSound voice is not stopped.
- **Volume**: the voice volume the handler sets (`0x2000`); no volume command.
- **Loops**: audio only (no sound CPU).
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured**: `ghv101`, 40 of 40 but doubtful: every file runs to the 5 s cap and none
  starts from silence ([board support](../board-support.md)).
- **Limits and what is missing**: what comes out are slices of the sample ROM played by
  PinMAME's test code, not the game's sounds. Getting the game's sounds would need its
  own QSound programming, read in the game's code; nothing in PinMAME exposes it as
  commands.
- **In VPinball**: **the pack cannot play**: the game sends no sound command at all.
