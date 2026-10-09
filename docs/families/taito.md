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
- **rom2altsound**: no sounds.dat section for any Taito game, so the raw sweep `01`..`FF`
  (`sweep`, default range), one byte per `sndbrd_manCmd` every 4 frames. `"TAITO"` is not
  in `BUILTIN_STOPS`: **the stop is a board reset** (`shim_reset_audio_cpus`, the 6802's
  reset line), then 4 s of silence. No volume command is known
  (`volume::none_reason`: "no known volume command for this board family"): recorded at
  the board's level, not scaled. Loops: audio and sequencer state. Pack columns: the
  defaults.
- **In VPinball**: AltSound gets every changed byte the game writes (`taito_sndCmd_w` →
  `sndbrd_0_data_w` → `snd_cmd_log`); Taito's generation has no preprocessing in
  `snd_alt.cpp`. Where the game writes one byte per sound the ids should be the pack's;
  where it writes two (the 1979 boards, below), AltSound sees both. Not tested in VPinball.

## <a name="sndbrd_taito_sintetizador"></a>SNDBRD_TAITO_SINTETIZADOR

Taito Sintetizador (6802 + DAC) · PinMAME interface `TAITO` (`src/wpc/taitos.c`) ·
status ❌ · 18 sets, 14 games, 15 sound ROM ids, 1979-1982, Taito · e.g. Shock (`shock`),
Football (`football`), Oba-Oba (`obaoba`), Gemini 2000 (`gemini`)

- **Hardware**: 6802, one DAC on the PIA's port A (`TAITO_dacInt`, volume 25). Some games
  run the `taitos_sintetizador_nmi` machine (a timed NMI at about 7.5 Hz).
- **Commands**: as above. The survey's boot log of `shock` shows each command written
  twice, with and without bit 7 (`98 18`) ([board support](../board-support.md)), which
  matches the "bit 7 is an enable" comment of `taito.c`. Both bytes are non-zero, so CB1
  stays high and only the port B level changes between them.
- **Sound list**: raw sweep `01`..`FF`, one byte per command.
- **Stop, boot and resets**: board reset after every sound (no stop known).
- **Volume**: the board's only level; DAC DC-corrected by PinMAME.
- **Loops**: audio, sequencer state (6802 RAM).
- **DUCK / STOP / CHANNEL**: defaults.
- **Measured**: `shock`, 0 of 40 ([board support](../board-support.md)).
- **Limits and what is missing**: a single byte per command starts no sound on these
  programs. The cheapest fix identified in the survey: send each command as the game
  does, the byte with bit 7 set, then without (`98 18` for `18`); a change in
  `command_sends` for the `"TAITO"` interface. Whether the later Sintetizador programs
  (1980-82) need it too is not known: only `shock` was tried.
- **In VPinball**: AltSound would see the two bytes the game writes per sound; which one
  libaltsound keys the sound on is not determined. Not tested in VPinball.

## <a name="sndbrd_taito_sintetizadorpp"></a>SNDBRD_TAITO_SINTETIZADORPP

Taito Sintetizador with the "piggy pack" daughter board (6802 + DAC + 2 AY-3-8910) ·
PinMAME interface `TAITO` (`src/wpc/taitos.c`) · status ❌ · 8 sets, 5 games, 5 sound ROM
ids, 1982-1985, Taito · e.g. Snake Machine (`snake`), Mr. Black (`mrblack`), Space Shuttle
(`sshuttle`), Polar Explorer (`polar`)

- **Hardware**: the Sintetizador plus two AY-3-8910 at 2 MHz on the daughter board
  (`MACHINE_DRIVER_START(taitos_sintetizadorpp)`, `taitospp_readmem`/`writemem`, which
  keep the RAM at `0000`-`007F`); `_nmi` variant with a timed NMI.
- **Commands, list, stop, volume, loops, columns**: as the Sintetizador.
- **Measured**: `snake`, 0 of 40, "as shock" ([board support](../board-support.md)).
- **Limits and what is missing**: the same as the Sintetizador (the doubled command with
  and without bit 7, per the survey's note).
- **In VPinball**: as the Sintetizador. Not tested in VPinball.

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
- **Measured**: `titan`, 38 of 40 ([board support](../board-support.md)). No full run is
  recorded.
- **Limits**: only the quick survey; loops, the full sweep and the stop (a reset after
  every sound) are not checked on a full run.
- **In VPinball**: not tested.

## <a name="sndbrd_taito_sintevoxpp"></a>SNDBRD_TAITO_SINTEVOXPP

Taito Sintevox with the daughter board (6802 + DAC + 2 AY-3-8910 + Votrax SC-01A) ·
PinMAME interface `TAITO` (`src/wpc/taitos.c`) · status ✅ · 2 sets, 2 games, 2 sound ROM
ids, 1982, Taito · e.g. Gork (`gork`), Fire Action Deluxe (`fireactd`)

- **Hardware**: `MACHINE_DRIVER_START(taitos_sintevoxpp)`: the piggy pack board (two
  AY-3-8910) and the SC-01A.
- **Commands, list, stop, volume, loops, columns**: as above.
- **Measured**: `gork`, 37 of 40 ([board support](../board-support.md)).
- **Limits**: only the quick survey.
- **In VPinball**: not tested.
