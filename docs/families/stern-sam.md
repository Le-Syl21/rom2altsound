# Stern SAM

The Stern SAM machines (2006-2014 and their later re-releases), which PinMAME runs with no
sound board: the "NONE (Stern SAM)" row of [board support](../board-support.md). Nothing in
[the common method](common.md) applies: there is no command to send, so the sounds are
read from the ROM image without emulation, and PinMAME is booted only to read the factory
volume. The full description of the format is in
[how it works, Stern SAM](../how-it-works.md#stern-sam).

## <a name="sndbrd_none_sam"></a>SNDBRD_NONE (Stern SAM)

Stern SAM system, sound mixed by the main CPU · no sound board (PinMAME's `samIntf` in
`src/wpc/sam.c` has empty handlers) · status ✅ · 406 sets, 27 games, no sound ROM id (the
sound data is in the main image), 2006-2024, Stern, Stern/Destruk · e.g. AC/DC LE
(`acd_168h`, `acd_170h`), TRON: Legacy LE (`trn_174h`), Metallica LE (`mtl_180h`), The
Walking Dead LE (`twd_160h`)

- **Hardware**: one CPU, an Atmel AT91 (ARM7) at 40 MHz, and no sound board. Its FIQ
  handler (4 kHz) mixes up to 8 voices in software and writes the mix to a TI PCM1755 DAC
  through a Xilinx FIFO, 24 kHz stereo. PinMAME emulates the AT91 and plays that stream
  (`sam.c` `sam_sh_update`); the DAC's attenuation registers are written by bit-banging
  the PIO lines, which PinMAME sees as writes to the CPU's I/O port (`sam_port_w`).
- **Commands**: none. The game's code plays a "sound call" itself; nothing goes through
  `sndbrd_data_w`. `samIntf`'s `manCmd_w` (`man3_w`) and `data_w` (`scmd_w`) do nothing,
  which `src/extract.rs` knows (`NOOP_MANCMD`), but a SAM set never reaches the extractor:
  `src/main.rs` `run` sends every set of the SAM driver (`sam::sam_set`, the
  `SAM1_ROM32MB`/`SAM1_ROM128MB` sets `build.rs` reads from `sam.c` into `sam_sets.rs`) to
  `sampack::run`.
- **Sound list**: read from the flash image (`src/sam.rs`; the format from Ashram56's
  reverse engineering of Tron LE 1.74, confirmed on acd_168h). The image is the largest
  member of the zip, inflated in memory and checked against the zip's and the driver's
  CRC32 (`sampack::load_image`). In it: the sample directory (the longest run of words of
  the first 8 MB that are 0 or banked pointers to a valid script header; the language count
  from its stride), the scripts (opcodes with fixed operand counts, `sam::SCRIPT_ARGS`), the
  streams (IMA ADPCM, 24 kHz / divisor) and the sound call table (20-byte records, each
  pointing to the list of samples a call picks from). A script with at most one stream and
  no loop is a **sound**: each distinct stream is decoded once and named after the first
  directory entry that plays it (`s<sample>-<rom>.wav`, `s<sample>-l<language>-<rom>.wav`
  for the other languages). A script with two streams or more, or a `07`...`03` loop, is a
  **music**, rendered on a 24 kHz timeline (chunks joined gaplessly, their first ADPCM
  samples smoothed; roles `teaser`, `main`, `full`, `resume`, `bed`). No silence detection
  or twin test is needed: each stream is decoded once.
- **Stop, boot and resets**: no stop, no reset. Two boots in PinMAME, cold (child process,
  no nvram) then warm from the cold boot's nvram (`sampack::boot`, `factory`), each until
  the DAC has been quiet for 3 s after `--boot-secs`, only to read the DAC writes.
- **Volume**: the shim hooks the CPU's port handler (`shim_sam_hook_dac`) and decodes the
  same serial words as sam.c, logging the PCM1755's attenuation registers (`0x10` left,
  `0x11` right). The factory volume is the value the game writes at the warm boot, the
  operator's volume setting (verified with `--sam-volume-test`: each press of the coin
  door's volume button moves it by 1 dB, and the game writes the new value at its next
  power-up). The files are the decoded samples scaled by that attenuation **as PinMAME
  plays it**, `(v & 7F) * 100 / 7F` percent (sam.c `sam_port_w`; `sampack::pinmame_db`),
  rounded to 16 bits without dither. The datasheet's reading (-0.5 dB per step from `FF`,
  `sampack::dac_db`) is reported as `datasheet_offset_db`. `--volume reference`, or an
  unknown attenuation, leaves the files at full scale. No DC to remove (no emulated
  mix). True peaks use `loudness::measure_fast` (an interpolated true peak; `ebur128`'s
  precise one is too slow for hours of audio).
- **Loops**: from the scripts: a `07` mark ... `03` loop is exact by construction; the
  file gets a `smpl` loop from the mark to the end, and a loop with an intro also gets
  `-loop.wav` and `-extended.wav` (`sampack::write_music`). No audio or state search.
- **DUCK / STOP / CHANNEL**: one row per (call, sample of the call), first language: ID =
  call id, music rows on channel 0 (looping ones `LOOP` 100, the extended ones `LOOP` 0),
  the rest polyphonic, `DUCK` 100, `STOP` 0; a sample whose languages differ is a
  callout. The scripts' own volume ramps (opcode `09`) and the game's mixing are not
  reproduced.
- **Measured** ([how it works](../how-it-works.md#stern-sam), CHANGELOG 0.2.2): acd_168h:
  1054 samples x 5 languages, 951 sound streams (1854 s), 8 stub scripts, 86 music
  scripts (24 songs, 268.5 minutes, 39 loops); 1129 pack rows for 412 calls, 561 files;
  factory `E8`, -1.83 dB in PinMAME (-11.5 dB by the datasheet); at full scale 855 of
  1037 files reach full scale as decoded, none at the factory volume (-11.3 LUFS in all,
  loudest true peak +0.3 dBTP). Checked sample for sample against the study script
  `sam_study.py`. Cost: 49 s wall, peak RSS 956 MB. Board support: acd_170h, 1036 files.
- **Limits and what is missing**: only acd_168h was checked in depth; the scripts' volume
  ramps and the game's mixing are not reproduced; the factory volume follows PinMAME's
  linear reading of the DAC register, about 10 dB louder than the datasheet at `E8`
  ([Limits](../how-it-works.md#limits)).
- **In VPinball**: **the pack does not play.** PinMAME's AltSound is fed by sound
  commands (`snd_cmd_log`, called from `sndbrd_data_w`), and SAM never sends one: the
  game plays its calls in its own code. A PinMAME that reported the call ids would need
  the address of the game's `snd_play` per ROM (as sam.c's `fastflipaddr`) or a
  signature search. Until then the CSVs are for editing and measurement.
