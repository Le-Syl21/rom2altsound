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
- **Firmware generations**: the script interpreter is an array of handlers indexed by the
  opcode, and the older firmware has fewer handlers in another order: 17 (the numbering
  above; AC/DC, Metallica, TRON 1.74, The Walking Dead...), 15 (24, Batman, CSI, Indiana
  Jones, NBA, Shrek, TRON up to 1.70...: `08` starts a stream, `07` ramps), 14 (Family Guy,
  Pirates, Wheel of Fortune, World Poker Tour 1.11 on...) and an early 14 whose script
  header is `04` (World Poker Tour 1.03-1.09, Simpsons Kooky Carnival 9nj and 1.03), each
  read in the program's handler array (`sam::LAYOUTS`, [how it
  works](../how-it-works.md#stern-sam)). `sam::detect_layout` picks, per image, the layout
  under which its scripts read cleanly; on all 375 images with sound data it is the one of
  the handler array in the program (20 early, 129 with 14 handlers, 97 with 15, 129 with
  17). Until this was read, the 226 sets of the 15- and 14-handler generations decoded no
  stream and reported success, and the 20 early ones found no directory; a directory
  whose scripts start no stream is now an error.
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
  reproduced. The ramps are per script, not a bus level: about 870 of trn_174's 1115
  scripts carry the same ones (targets 0 and 1 set to 33 and 40 at once, then stepped by
  1 every 36 and 15 ticks), so they hardly change one sound against another; their
  operands' meaning (the FIQ mixer's side) is not read. The levels that would matter for loudness, the game's
  music and effects buses (`02 <voice> <bus>`), are set by the game's code, not by the
  scripts.
- **Measured** ([how it works](../how-it-works.md#stern-sam), CHANGELOG 0.2.2): acd_168h:
  1054 samples x 5 languages, 951 sound streams (1854 s), 8 stub scripts, 86 music
  scripts (24 songs, 268.5 minutes, 39 loops); 1129 pack rows for 412 calls, 561 files;
  factory `E8`, -1.83 dB in PinMAME (-11.5 dB by the datasheet); at full scale 855 of
  1037 files reach full scale as decoded, none at the factory volume (-11.3 LUFS in all,
  loudest true peak +0.3 dBTP). Checked sample for sample against the study script
  `sam_study.py`. Cost: 49 s wall, peak RSS 956 MB. Board support: acd_170h, 1036 files.
- **Measured, older firmware** (`--no-factory`): trn_150 (15) 1061 sound streams, 39
  music scripts, 1100 files, against trn_174 (17) 1061, 41, 1102: 1097 files identical,
  265 of trn_150's 270 calls play the same audio in trn_174; fg_1200af (14) 1529 files;
  wpt_106a (early 14) 1110 files, 599 calls. acd_168h's files are byte-identical to
  before.
- **In a game** (0.3, a measurement harness on the tool's own libpinmame, acd_168h cold
  booted: four coins, Start (`KEYCODE_1`) once credited, troughs 18-21 closed, nothing
  else touched): PinMAME's output in a started game, the ball in the shooter lane, is a
  music bed at **-13.1 LUFS** (110 s, stereo as VPX plays it), where the pack's files
  together read -11.3 LUFS at the same factory attenuation, and the songs one by one about
  -9.8 to -11.8 LUFS. A coin sound recognised in the recording (the level envelope of
  `s00F6` matches within 0.7 dB) plays **6.6 dB** below its file at the factory
  attenuation. So the static files are louder than the game plays them, by an amount that
  depends on the sound (the game's bus levels and mix), and the catalog does not give a
  SAM entry a `lufs`: its figures are under `static_decode`, with a caveat
  ([the catalog](../catalog.md#stern-sam)).
- **Limits and what is missing**: only acd_168h was checked in depth against the study
  script; the scripts' volume ramps and the game's mixing are not reproduced (above: 2 to
  7 dB too loud against a game on acd_168h); the factory volume follows PinMAME's
  linear reading of the DAC register, about 10 dB louder than the datasheet at `E8`
  ([Limits](../how-it-works.md#limits)).
- **In VPinball**: **the pack does not play.** PinMAME's AltSound is fed by sound
  commands (`snd_cmd_log`, called from `sndbrd_data_w`), and SAM never sends one: the
  game plays its calls in its own code. A PinMAME that reported the call ids would need
  the address of the game's `snd_play` per ROM (as sam.c's `fastflipaddr`) or a
  signature search. Until then the CSVs are for editing and measurement.
