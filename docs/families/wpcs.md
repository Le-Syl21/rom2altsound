# WPCS: the WPC pre-DCS sound board

The Williams/Bally WPC sound board of 1990-1993 (part A-12738), `SNDBRD_WPCS`. What is
not said here is the [common method](common.md).

## <a name="sndbrd_wpcs"></a>SNDBRD_WPCS

WPC sound board (YM2151, DAC, CVSD) · PinMAME interface `WPCS` (`src/wpc/wmssnd.c`) ·
status ✅ · 219 sets, 25 games, 35 sound ROM ids, 1990-2026, Bally, Williams · e.g.
Twilight Zone (`tz_94h`, `tz_92`), The Addams Family (`taf_l5`), Terminator 2 (`t2_l8`),
Funhouse (`fh_l9`)

- **Hardware**: a 6809 at 2 MHz (`MACHINE_DRIVER_START(wmssnd_wpcs)`, `CPU_AUDIO_CPU`)
  with 8 KB of RAM at $0000 (`wpcs_readmem`: `MRA_RAM`), banked program/sample ROMs
  (`wpcs_rombank_w`), a YM2151 FM chip at 3.58 MHz whose timer drives the 6809's FIRQ
  (`wpcs_ym2151IRQ`), an 8-bit DAC (AD7524, `DAC_0_data_w`) and an HC55516/HC55536 CVSD
  speech decoder. Mixing levels: DAC 70 %, CVSD 100 %, YM2151 16 %, from the schematics
  (comment above `wpcs_dacInt`; per game overrides in `wpcs_init` from
  `hw.gameSpecific2`). The master volume is a digital pot the program steps
  (`wpcs_volume_w`: one step per write, then `mixer_set_volume(ch, volume * 100 / 127)`
  on every mixer channel). `wpc.c` `MACHINE_INIT(wpc)` starts it for `GEN_WPCALPHA_2`,
  `GEN_WPCDMD` and `GEN_WPCFLIPTRON`.
- **Commands**: the game writes bytes to `WPC_SND_DATA` (`wpc.c`, `wpc_w`:
  `sndbrd_0_data_w`); `wpcs_data_w` latches the byte and raises the 6809's IRQ;
  `WPC_SND_CTRL` resets the board (`wpcs_ctrl_w`: pulse of the CPU's reset line). Most
  sounds are one byte; `7A xx` is the second bank (most voices and effects on some
  games), `79 vv ~vv` the master volume. The interface is flagged `SNDBRD_DOUBLECMD`:
  `sndbrd_manCmd` only acts on byte pairs and `wpcs_manCmd_w` writes both bytes of the
  pair, so a padded one-byte command would also send `00` ("Reset Sound System"). The
  tool therefore bypasses `manCmd_w` (`src/extract.rs`, `board_sends`): a single byte
  goes through `sndbrd_data_w` (`Send::Data`), the game's own path; a longer command
  goes out in one burst, the board's data handler called directly with 12 timeslices
  between the bytes as `wpcs_manCmd_w` does (`shim_data_burst`, `Send::Burst`): The
  Addams Family's program does not wait a frame for the next byte of `79 vv ~vv`, and
  with 1 or 4 frames between them it played the level byte `0C` as a music
  ([WPCS command pacing](../how-it-works.md#reference-volume)).
- **Sound list**: a sounds.dat section where one matches (`cftbl_l4`, `dw_l2`, `fh_l9`,
  `tz_94h`, with PinMAME's prefix rule). sounds.dat writes the second bank `01 7A xx`;
  the filler `01` is dropped (`Extractor::wpcs_bank`: on Twilight Zone `01` fades the
  music out). Otherwise the raw sweep (`sweep`, `"WPCS"`): `01`..`FF` without the tempo,
  volume and prefix bytes of sounds.dat `wpcs:` (`WPCS_STATE`: `1E`..`2F`, `60`..`72`,
  `79`, `7A`), then the whole bank `7A00`..`7AFF` (ids `0x7Axx`); empty slots end as
  `no_sound` (the bank's table in the program is not read).
- **Stop, boot and resets**: stop `00`, "Reset Sound System" in sounds.dat's `wpcs:`
  section (`stop_sends`), sent through `sndbrd_data_w`. A reset goes through the control
  port (`CTRL_RESET`), then the volume is sent again. 0.2.0 reset taf_l5's board forever
  (the volume re-sent after each reset restarted the music `0C`): the 3-waits limit
  (`MAX_STOP_FAILURES`) and the burst came from that.
- **Volume**: master volume `79 vv ~vv`, `vv` `00` (silent) to `1F`; the board ignores
  `20` and above (`volume::decode`). Factory: the game's own at boot (tz_94h and taf_l5:
  `79 0C F3`, level 12); a game that sends none is left at its power-on level and not
  scaled (`factory_master`). Reference `79 0C F3` (`Ref.wpcs`, `--wpcs-volume`), the
  loudest level at which no file of taf_l5 (472 commands) or tz_94h (307) clips: the
  factory offset is 0 dB on both. The steps are uneven (a digital pot). **AC coupling**:
  once booted, the DAC is put through dac.c's 10 Hz high-pass
  (`ac_couples_dac`, `shim_dac_ac_couple`), because PinMAME maps the DAC unsigned while
  the program plays around its middle code and leaves it on its last value: without it
  every taf_l5 file started on a held DC level and its loudest effects clipped on it
  ([Reference volume](../how-it-works.md#reference-volume)). No master volume check:
  `Extractor::alt_volume` has no WPCS case, so no file is flagged
  `ignores_master_volume` on this board.
- **Loops**: the audio of a music never repeats sample-exactly (the 6809's sequencer
  ticks are not locked to the YM2151's sample clock), so loops come from the sound CPU
  state (`sequencer-state`): the 6809's registers and its 8 KB of RAM, checked on the
  audio and cut where two cycles differ least (seam of a few LSB).
- **DUCK / STOP / CHANNEL**: measured chip by chip (the chips pass, `chip_commands`,
  `altsound::apply_chips`): each written non-loop sound replayed with only the CVSD heard,
  then 2 s into the loudest music loop with only the YM2151 heard. A sound mostly on the
  voice chip goes on channel 1 (a callout unless named `SFX:`), `DUCK` is the measured
  lowering of the music, `STOP` 1 for a voice-chip sound that stops it, channel 0 for
  another sound that ends it. Measured over one music only. See
  [Ducking, stops and channels (WPCS and System 11)](../how-it-works.md#ducking-stops-and-channels-wpcs-and-system-11).
- **Measured**: tz_94h 307 commands from sounds.dat, 302 written, no clipped file, no
  reset ([Limits](../how-it-works.md#limits)); 45 musics, 26 looped from the state within
  240 s, 31 with `--loop-max-secs 600` (`03` loops after 271.9 s); chips pass over music
  `1C`: 141 of 257 sounds on the voice chip, 36 jingles and effects lower the music by
  1.7 to 13.8 dB, the tilts `D3`/`D4` stop it ([Loops](../how-it-works.md#loops),
  chips section). taf_l5 (no sounds.dat section): the raw sweep wrote 268 sounds, 137 of
  them in the bank `7A00`..`7A88`, 15 of its 33 musics with an exact loop, none clipped at
  `79 0C F3`, 5 at `79 14 EB` (README, Supported boards, note 2). Quick survey: tz_92 37
  of 40 ([board support](../board-support.md)).
- **Limits**: five taf_l5 effects that play speech, FM and DAC together clip on their
  sum in PinMAME's mix above level 12 (README, note 8); a music that does not loop
  within the search is cut at 2 minutes; ducks are measured over one music. The chips
  pass adds two takes per sound (1976 s of emulation on Twilight Zone).
- **In VPinball**: the game writes each byte through `sndbrd_data_w`; PinMAME's AltSound
  preprocessing for `GEN_WPCALPHA_2`, `GEN_WPCDMD` and `GEN_WPCFLIPTRON`
  (`snd_alt.cpp`, `preprocess_commands`) reads `79 vv ~vv` as the volume and joins
  `7A xx` into `0x7Axx`, the ids the pack uses. The README says the packs play in
  VPinball (0.2.2); no WPCS-specific test is recorded.
