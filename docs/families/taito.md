# Taito (Brazil) sound boards

The four Taito families: `SNDBRD_TAITO_SINTETIZADOR`, `SNDBRD_TAITO_SINTETIZADORPP`,
`SNDBRD_TAITO_SINTEVOX` and `SNDBRD_TAITO_SINTEVOXPP`. They are one PinMAME board
interface, `"TAITO"` (`src/wpc/taitos.c`, `taitoIntf`), whose sub-type only adds chips, so
rom2altsound drives all four identically, with the [common method](common.md). What
differs between them is the sound program in each game's ROMs.

Shared by the four:

- **Board interface**: `taitoIntf` = `{"TAITO", taitos_init, NULL, taitos_diag,
  taitos_data_w, taitos_data_w, ...}`: the manual command handler is the board's own data
  handler. `taitos_data_w` puts the byte **inverted** on the 6821 PIA's port B
  (`pia_b = data ^ 0xff`) and sets the CB1 input to 1 when the byte is not 0, to 0 when it
  is: the byte is a level on the lines, CB1 a strobe that follows "not zero".
- **What the game sends** (`src/wpc/taito.c`, `taito_sndCmd_w`, called from the output
  multiplexer): the command is assembled from two nibbles of the lamp/solenoid outputs
  (offset 3 gives bits 5-8, offset 2 bits 1-4), each XORed with DIP bank 1, and written
  with `sndbrd_0_data_w` **only when the byte changes** (`oldsndCmd`). A comment in
  `taito_silenceSavedSndCmd` gives the idle value: `0` on the 1980-85 boards, `8` in the
  saved upper nibble on the 1979 board, "where bit 7 is an enable the firmware ORs
  commands into".
- **Sound CPU**: a 6802 (`MACHINE_DRIVER_START(taitos_sintetizador)`, 600 kHz, the
  `_nmi` variants 660 kHz with a timed NMI), RAM `0000`-`007F` (`MRA_RAM` in
  `taitos_readmem`), so the sequencer state can be read.
- **Output**: the sound is the PIA's port A, written to the DAC through
  `DAC_DC_offset_correction_data_16_w` (`pia0a_w`): PinMAME already AC-couples this DAC
  (10 Hz high-pass), so the files carry no held DC level although rom2altsound does not
  switch it itself (`ac_couples_dac` is only WPCS and System 11).
- **The edge**: the programs arm CB1's rising edge (shock writes CRB `07` at reset,
  `7808`-`7816`) and read port B in the interrupt. The CB1 input only falls on a `00`
  byte, or when the program pulses CA2 (`pia0ca2_w`, "reset sound command"). The
  Sintetizador programs never pulse it: traced on shock (`R2A_TRACE=0:8400-8403`, see
  [common](common.md)), the program read the command once at power-on (`11`), and none
  of the game's next bytes (`91 11 91 11`, all non-zero) made an edge. A command sent
  after a non-zero byte is never read.
- **rom2altsound**: no sounds.dat section for any Taito game, so the raw sweep `01`..`FF`
  (`sweep`, default range), one byte per `sndbrd_manCmd` every 4 frames. **The stop is
  `00`** (`BUILTIN_STOPS`), the games' idle value: it lowers CB1, so the next command
  makes its edge (a board reset leaves the PIA as it was, CB1 included). `00` silences
  few sounds by itself: when the board is still playing after 10 s, the tool resets it
  (`shim_reset_audio_cpus`, the 6802's reset line), then waits 4 s of silence. Until
  0.2.3 the stop was that reset alone, and CB1 stayed high from the game's last byte. No volume command is known
  (`volume::none_reason`: "no known volume command for this board family"): recorded at
  the board's level, not scaled. Loops: audio and sequencer state. Pack columns: the
  defaults.
- **In VPinball**: AltSound gets every changed byte the game writes (`taito_sndCmd_w` →
  `sndbrd_0_data_w` → `snd_cmd_log`), with the generation 0 (none) set on every Taito
  machine; up to 10.8.1-5436 libaltsound has no case for it and joins the bytes two by two ([In
  VPinball](common.md#in-vpinball)), so **the packs do not play as written there** (cavnegro:
  `00 39 00 3A` looked up as `0039 003A` only because each pair started on the `00`). With
  one byte per command
  ([vpinball/libaltsound#20](https://github.com/vpinball/libaltsound/pull/20), in VPinball master from 3abe805) the
  game's one-byte commands are the pack's ids, and **the packs play from VPinball master
  3abe805 on**; where it writes two (the 1979 boards, below), AltSound sees both.

## <a name="sndbrd_taito_sintetizador"></a>SNDBRD_TAITO_SINTETIZADOR

Taito Sintetizador (6802 + DAC) · PinMAME interface `TAITO` (`src/wpc/taitos.c`) ·
status ✅ · 18 sets, 14 games, 15 sound ROM ids, 1979-1982, Taito · e.g. Shock (`shock`),
Football (`football`), Oba-Oba (`obaoba`), Gemini 2000 (`gemini`)

- **Hardware**: 6802, one DAC on the PIA's port A (`TAITO_dacInt`, volume 25). Some games
  run the `taitos_sintetizador_nmi` machine (a timed NMI at about 7.5 Hz).
- **Commands**: as above. The boot log of `shock` shows the game writing `98 18` (warm
  boot) or `11 91` (cold boot) every 3 s, with and without bit 7, which matches the "bit 7
  is an enable" comment of `taito.c`. Both bytes are non-zero, so CB1 stays high and the
  program reads neither (the boot plays no sound). One byte after a `00` is a command.
- **Sound list**: raw sweep `01`..`FF`, one byte per command.
- **Stop, boot and resets**: `00`, then a board reset when the board still plays.
- **Volume**: the board's only level; DAC DC-corrected by PinMAME.
- **Loops**: audio, sequencer state (6802 RAM).
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured** (survey settings, [board support](../board-support.md)): shock 35 of 40,
  football 36 of 40, obaoba 37 of 40, all from silence (0 of 40 before the stop `00`).
  Many sounds run to the 5 s cap (football: 31 of 36): long or held sounds, which `00`
  does not stop; the board reset after it does.
- **Limits and what is missing**: what the bit-7 byte does (`98` before `18`) is not
  known; a sound with bit 7 set is swept as its own command. No full run.
- **In VPinball**: **the pack does not play as written, in any VPinball so far**: the game sends a command with and without bit 7 (`98 18`), both pack rows, so each command looks up two ids; and up to 10.8.1-5436 libaltsound has no case for
  generation 0 (none) and joins the bytes two by two ([In
  VPinball](common.md#in-vpinball)); `cosmic` sent no sound command in 45 s of attract
  mode with a coin and start, so nothing was measured. AltSound sees every changed byte
  the game writes, bit-7 bytes included.

## <a name="sndbrd_taito_sintetizadorpp"></a>SNDBRD_TAITO_SINTETIZADORPP

Taito Sintetizador with the "piggy pack" daughter board (6802 + DAC + 2 AY-3-8910) ·
PinMAME interface `TAITO` (`src/wpc/taitos.c`) · status ✅ · 8 sets, 5 games, 5 sound ROM
ids, 1982-1985, Taito · e.g. Snake Machine (`snake`), Mr. Black (`mrblack`), Space Shuttle
(`sshuttle`), Polar Explorer (`polar`)

- **Hardware**: the Sintetizador plus two AY-3-8910 at 2 MHz on the daughter board
  (`MACHINE_DRIVER_START(taitos_sintetizadorpp)`, `taitospp_readmem`/`writemem`, which
  keep the RAM at `0000`-`007F`); `_nmi` variant with a timed NMI.
- **Commands, list, stop, volume, loops, columns**: as the Sintetizador.
- **Measured**: snake 35 of 40, mrblack 36 of 40, polar 25 of 40, all from silence (0 of
  40 before the stop `00`, [board support](../board-support.md)).
- **Limits and what is missing**: as the Sintetizador.
- **In VPinball**: **the pack plays from VPinball master 3abe805 on**, not in 10.8.1-5436 and older:
  their libaltsound has no case for generation 0 (none) and joins the bytes two by two ([In
  VPinball](common.md#in-vpinball)); `mrblack` sent no sound command in 45 s of attract
  mode with a coin and start, so nothing was measured.

## <a name="sndbrd_taito_sintevox"></a>SNDBRD_TAITO_SINTEVOX

Taito Sintevox (Sintetizador + Votrax SC-01A speech) · PinMAME interface `TAITO`
(`src/wpc/taitos.c`) · status ✅ · 10 sets, 5 games, 7 sound ROM ids, 1981-1982, Taito ·
e.g. Titan (`titan`), Hawkman (`hawkman`), Fire Action (`fireact`), Cavaleiro Negro
(`cavnegro`)

- **Hardware**: the Sintetizador plus a Votrax SC-01A (`TAITO_votrax_sc01_interface`):
  the program writes the phoneme to the PIA's port B, latched to the chip on CB2
  (`pia0b_w`, `pia0cb2_w`, sub-type bit 0), the chip's busy line on CA1
  (`votrax_busy`). On `vegast`/`ladylukt` (`gameSpecific1`) PinMAME takes the phoneme from
  the CPU's A register instead (comment in `pia0b_w`).
- **Commands, list, stop, volume, loops, columns**: as above; one byte per command works on
  these programs.
- **Measured**: `titan`, 38 of 40 ([board support](../board-support.md)), the same with
  the stop `00` (11 board resets instead of 45: these programs lower CB1 themselves, so
  they took commands without it). No full run is recorded.
- **Limits**: only the quick survey; loops and the full sweep are not checked on a full
  run.
- **In VPinball**: **the pack plays from VPinball master 3abe805 on**, not in 10.8.1-5436 and older:
  their libaltsound has no case for generation 0 (none) and joins the bytes two by two ([In
  VPinball](common.md#in-vpinball)). Measured on `cavnegro`: the game sent `00 39 00 3A`,
  AltSound looked up `0039 003A`; with one byte per command
  ([vpinball/libaltsound#20](https://github.com/vpinball/libaltsound/pull/20), in VPinball master from 3abe805) it
  looks up `0000 0039 0000 003A`. This run matched only because each pair started on
  the `00`.

## <a name="sndbrd_taito_sintevoxpp"></a>SNDBRD_TAITO_SINTEVOXPP

Taito Sintevox with the daughter board (6802 + DAC + 2 AY-3-8910 + Votrax SC-01A) ·
PinMAME interface `TAITO` (`src/wpc/taitos.c`) · status ✅ · 2 sets, 2 games, 2 sound ROM
ids, 1982, Taito · e.g. Gork (`gork`), Fire Action Deluxe (`fireactd`)

- **Hardware**: `MACHINE_DRIVER_START(taitos_sintevoxpp)`: the piggy pack board (two
  AY-3-8910) and the SC-01A.
- **Commands, list, stop, volume, loops, columns**: as above.
- **Measured**: `gork`, 37 of 40 ([board support](../board-support.md)), the same with the
  stop `00` (28 board resets instead of 47).
- **Limits**: only the quick survey.
- **In VPinball**: **the pack plays from VPinball master 3abe805 on**, not in 10.8.1-5436 and older:
  their libaltsound has no case for generation 0 (none) and joins the bytes two by two ([In
  VPinball](common.md#in-vpinball)); `fireactd` sent no sound command in 45 s of attract
  mode with a coin and start, so nothing was measured.
