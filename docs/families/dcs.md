# DCS: WPC DCS, DCS-95 and Pinball 2000

The Williams/Bally DCS sound boards: `SNDBRD_DCS` (WPC DCS, Security and the first
WPC-95 games), `SNDBRD_DCS95` (WPC-95) and `SNDBRD_DCSP2K` (Pinball 2000's DCS2). All
three are PinMAME's `"DCS"` board interface (`src/wpc/wmssnd.c`, `dcsIntf`), and the
three share the track catalog and the track programs that rom2altsound reads. What is
not said here is the [common method](common.md).

## <a name="sndbrd_dcs"></a>SNDBRD_DCS

WPC DCS sound board · PinMAME interface `DCS` (`src/wpc/wmssnd.c`) · status ✅ · 191 sets,
19 games, 39 sound ROM ids, 1993-2026, Williams, Bally · e.g. Indiana Jones (`ij_l7`),
Theatre of Magic (`tom_13`), Red & Ted's Road Show (`rs_l6`), Who Dunnit (`wd_12`)

- **Hardware**: an Analog Devices ADSP-2105 DSP at 10 MHz (`MACHINE_DRIVER_START(wmssnd_dcs1)`,
  `CPU_AUDIO_CPU`) running the board's program from ROM U2, the compressed audio in U2
  and the further sound ROMs, mixed to one mono stream at 31250 Hz
  (`DCS_DEFAULT_SAMPLE_RATE`; `dcs_custStart`, `dcs_txData` take the DSP's autobuffered
  serial output). PinMAME runs the DSP's real program (low level); `DCS_useSpeedup`
  replaces its decoder loop with hand-written C on these boards. `src/wpc/wpc.c`
  (`MACHINE_INIT(wpc)`) starts `SNDBRD_DCS` for the generations `GEN_WPCDCS` (1993-1994:
  ij, jd, afv, sttng, pop, dm), `GEN_WPCSECURITY` (wcs, fs, rs, ts, tom, i500...) and
  `GEN_WPC95DCS` (jb, wd: WPC-95 CPU board, older sound board); all use the machine
  driver `wpc_dcsS` (`wpc.h`: `wpc_mDCSS`, `wpc_mSecurityS`, `wpc_m95DCSS`).
- **Commands**: the game writes one byte at a time to `WPC_SND_DATA` (`wpc.c`,
  `wpc_w`: `sndbrd_0_data_w`), which `dcs_data_w` puts in the latch and signals with the
  DSP's IRQ2; `WPC_SND_CTRL` (`sndbrd_0_ctrl_w`, `dcs_ctrl_w`) pulses the DSP's reset
  and reboots it (`adsp_boot`). A sound is a 16-bit track number sent as two bytes; the
  specials are `55 xx vv ~vv` (`55 AA` master volume, `55 AB`..`B0` channel levels,
  `~vv` must be the complement or the firmware drops it). `manCmd_w` is `dcs_data_w`
  itself (`dcsIntf`), so the tool's bytes take the game's path. rom2altsound sends the
  bytes one frame apart (`DCS_FRAMES_PER_SEND` = 1, `Extractor::tick_sender`), not four:
  the firmware drops the first byte of a pair whose second byte comes about 100 ms later
  (13 main-loop passes, mjrgh's DCSExplorer `dataPortTimeout`), and with 6 frames between
  them every command of rs_l6 was lost
  ([how it works, Per family](../how-it-works.md#per-family)).
- **Sound list**: the populated tracks of the ROM's own catalog (`src/dcsrom.rs`,
  `tracks`: U2 at $3000, $4000 or $6000, track index at +$40, count at +$46, `FF`xxxx =
  empty slot; layout from mjrgh's DCSExplorer), `0000` (stop) left out
  (`src/extract.rs`, `sweep`). Without a catalog: `0001`..`03FF` (`DCS_FALLBACK_LAST`). A
  game with a sounds.dat section (`dm_lx4`, `wcs_l2`, `tom_13` and the sets their prefix
  matches) gets the union: the section's named commands plus the catalog tracks it
  leaves out (`build_commands`). Twins: the same audio on two channels is common on DCS
  (see [Twins](../how-it-works.md#twins)); each keeps its own file and rows, with
  `twin_reason` naming the two channels.
- **Stop, boot and resets**: the stop is `0000` ("All sound off" in sounds.dat's `dcs:`
  section, `stop_sends` via `family_entries`). The boot also waits until the game's
  master volume was seen (`Extractor::step`, `Phase::Boot`): with no nvram (the cold
  boot) these games send no sound byte at all for 60 s; on the warm boot they send
  `55 AA 67 98` after 6 to 12 s. A reset goes through the control port
  (`CTRL_RESET`, `sndbrd_ctrl_w`), the game's own path, which reboots the DSP. After a
  reset the master volume (and the game's last channel levels, byte for byte) is sent
  again (`volume_sends`).
- **Volume**: master volume `55 AA vv ~vv`, level `(vv - 7) / 8`, 8..31
  (`src/volume.rs`, `decode`). Factory volume: the game's last one at boot, or the
  board's reset default `67` (level 12) when it sent none (`factory_master`,
  `DCS_RESET_DEFAULT`); `67` is what all four WPC DCS games measured send. Reference:
  `55 AA EF 10`, level 29 (`--dcs-volume`), the loudest at which no file of afm_113b,
  cv_20h, mm_109c and rs_l6 clips but for isolated clicks. The files are recorded at the
  reference and scaled to the factory volume by the measured offset (-22.44 dB on all
  four), see [Factory mode](../how-it-works.md#factory-mode-the-default---no-factory-turns-it-off).
  The master volume check runs (files more than 5 LU above the median played 8 levels
  away). No AC coupling: the DCS output has no held DAC level.
- **Loops**: the track program first (`dcs-catalog`, `dcsrom::track_run`): the program
  is run frame by frame (7.68 ms) without decoding audio until its whole state repeats,
  and its period is checked on the audio over one full period (20 to 60 s), recording up
  to 900 s if needed (`LOOP_HINT_MAX_SECS`, `Extractor::loop_search`). The audio method
  is taken when it equals or divides the program's period, or when the program's period
  failed. No sound CPU state method (the DSP has a 16-bit bus: `seqstate::Probe` finds no
  audio CPU). "Exact" here is sample-exact: the DCS emulation replays a sound
  sample-exactly (two takes correlate at 0.998+).
- **DUCK / STOP / CHANNEL**: read from the track programs (`dcsrom::command_effects`,
  `Extractor::read_dcs_programs`, `altsound::apply_dcs`): `CHANNEL` 0 for channel-0
  stream tracks (the music), 1 for the voice channel (the channel with the most voice
  lines and no twin channel), -1 otherwise; `DUCK` = `round(100 * 0.9733^units)` of the
  deepest duck of channel 0; `STOP` 1 for a voice-channel row whose program stops channel
  0; one ducking profile per DUCK value in `altsound.ini`. `--check-ducking` plays some
  of them back and fits the music's gain. See
  [Ducking, stops and channels (DCS)](../how-it-works.md#ducking-stops-and-channels-dcs).
- **Measured**: rs_l6, full run in factory mode: 480 commands, 455 written, 34 loops (26
  from the track programs, 8 from the audio), factory offset -22.45 dB, no clipped file
  ([Factory results](../how-it-works.md#factory-results-for-our-roms),
  [Loops](../how-it-works.md#loops)). Quick survey: ij_l7 40 of 40, tom_13 39 of 40, all
  from silence ([board support](../board-support.md)). The pre-WPC95 games are verified
  for sounds only (README, Supported boards, note 1).
- **Limits**: about one command in 200 plays nothing on its first try (rs_l6; root cause
  not found, the retry pass recovers them all). The program simulation reads opcodes `04`
  and `06` with their 1994+ operands: the 1993 software (`GEN_WPCDCS`) reads them
  differently, so a 1993 program may be misread (`dcsrom::track_run`; the audio check
  catches it). Type 3 tracks are not modelled. A type 2 track only arms a deferred track
  that the next `05` opcode on its channel starts, and the stop `0000` does not clear it
  (cv_20h `0001` then `0015`, [Limits](../how-it-works.md#limits)); deferred tracks cannot be
  expressed in AltSound's CSVs. On rs_l6, six tracks whose program period (129 to 336 s)
  comes from long streams are contradicted by the audio and looped from the audio: the
  1994 software probably lays out those streams differently. AltSound gives a duck back
  at once where the board fades it, and uses only the deepest of overlapping ducks
  ([libaltsound #15](https://github.com/vpinball/libaltsound/issues/15)).
- **In VPinball**: **plays as written**: the game sends each track number as two bytes
  through `sndbrd_data_w`, and libaltsound's case for `GEN_WPCDCS`, `GEN_WPCSECURITY`,
  `GEN_WPC95DCS` and `GEN_WPC95` joins them into the 16-bit id and filters the `55 xx`
  specials ([In VPinball](common.md#in-vpinball)): measured on `afv_l4`, `03 E3` looked up
  as `03E3`, `55 AA 67 98` taken as the volume. The README says the packs play in VPinball
  (0.2.2); no DCS pre-WPC95 pack test is recorded.

## <a name="sndbrd_dcs95"></a>SNDBRD_DCS95

DCS-95 sound board (WPC-95) · PinMAME interface `DCS` (`src/wpc/wmssnd.c`) · status ✅ ·
82 sets, 16 games, 30 sound ROM ids, 1995-2019, Bally, Williams · e.g. Attack from Mars
(`afm_113b`), Medieval Madness (`mm_109c`, `mm_10`), Cirqus Voltaire (`cv_20h`)

- **Hardware**: the same ADSP-2105 at 10 MHz and the same DSP program family, with the
  WPC-95 board's memory map: `MACHINE_DRIVER_START(wmssnd_dcs2)` (data RAM banking,
  `dcs2_RAMbank_r/w`, two ROM bank selects, the latch at data $3300), where the WPC DCS
  board (`wmssnd_dcs1`) has the latch on program address $3000, handled in the ADSP core
  when `WPC_gWPC95` is 0 (`src/cpu/adsp2100/adsp2100.c`; `WPC_gWPC95` is the board's
  sub-type, set in `dcs_init`). `wpc.c` `MACHINE_INIT(wpc)` starts `SNDBRD_DCS95`
  (`SNDBRD_TYPE(3,2)`) for `GEN_WPC95` only; machine driver `wpc_95S`.
- **Commands, sound list, stop, boot, volume, loops, DUCK / STOP / CHANNEL**: as
  [SNDBRD_DCS](#sndbrd_dcs). rom2altsound does not tell the two apart: both report the
  type string `"DCS"` (`board_typestr`), and every DCS rule applies. sounds.dat has the
  `afm_113` section (575 commands); its catalog adds 14 populated tracks the section
  misses, among them `0013`, a 120 s loop.
- **Measured** ([Factory results](../how-it-works.md#factory-results-for-our-roms),
  [Loops](../how-it-works.md#loops),
  [Ducking](../how-it-works.md#ducking-stops-and-channels-dcs)): afm_113b 589 commands,
  576 written, 20 loops (19 from the track programs, 1 from the audio), offset -22.45 dB,
  `0186` clipped by 1 sample at the reference; cv_20h 558 written, 27 loops (26 / 1);
  mm_109c 871 written, 17 loops (all from the programs). Every DCS loop came out as an
  exact cycle. afm_113b: 313 of 576 commands duck the music, channel 3 (the General) is
  the voice channel, twins on channels 1/2 and 4/5. The full pipeline (loops, factory
  volume, ducking) is verified on afm_113b (README, Supported boards). Quick survey: mm_10
  39 of 40.
- **Limits**: as SNDBRD_DCS, plus: the DCS-95 opcodes `10`..`12` are not modelled, so
  such a program ends in `error` and its command keeps the plain rows
  ([Ducking](../how-it-works.md#ducking-stops-and-channels-dcs)). Clicks that ignore the
  master volume (cv_20h `03DE`) are flagged only when loud enough to be replayed.
- **In VPinball**: as SNDBRD_DCS (`GEN_WPC95` is in the same case; measured on `afm_03`:
  `03 D3` looked up as `03D3`). The README says the packs play in VPinball; an Attack from
  Mars pack played there by a user led to the extended files for loops with an intro
  ([AltSound pack](../how-it-works.md#altsound-pack)).

## <a name="sndbrd_dcsp2k"></a>SNDBRD_DCSP2K

Pinball 2000 DCS2 sound board · PinMAME interface `DCS`, sub-type 3 (`src/wpc/wmssnd.c`,
`src/wpc/p2k.c`; reported `DCSP2K`) · status ✅ · 52 sets, 2 games, 6 sound ROM ids,
1999-2025, Midway, Midway / mypinballs · e.g. Revenge From Mars (`rfm_120`, `rfm_160`),
Star Wars Episode I (`swep1_130`, `swep1_150`)

- **Hardware**: an ADSP-2104 at 16 MHz with the SDRC ASIC, stereo
  (`MACHINE_DRIVER_START(wmssnd_dcs3)`, `SOUND_SUPPORTS_STEREO`, `p2k_custStart`); the
  sound flash plus U109 and U110, word-wide. The game is a PC (MediaGX, `src/p2k`).
  `dcs_init` turns the WPC decoder speedup off on this board (`DCS_useSpeedup = 0`).
- **Commands**: 16-bit words. The PC writes the board's host port directly
  (`p2k.c` `p2k_dcs_write` → `dcs_p2k_data_w`), never through `sndbrd_data_w`; the
  8-bit `manCmd_w` (`dcs_data_w`) truncates to a byte. So the shim hooks the DSP's host
  latches (`shim_p2k_hook`: `0400` acknowledge, `0401` reply), logs every word, takes the
  board's replies in the halted PC's place (`shim_p2k_take_reply`, from
  `Extractor::take_p2k_words`), and sends words with `dcs_p2k_data_w`
  (`shim_p2k_word`). Inside the tool a word is two bytes, high byte first
  (`p2k_sends`). A sound is the game's `DCSRequest`, read in its code: track, then
  `FF pp` (volume FF, pan `7F`), then `8000 | trk << 7` (`p2k_request`; the tool plays
  every track on board channel 0, `P2K_TRK`). Volumes: `55AA vv~vv` (`vv` = level * 8,
  FF = 31), `55AB mm vv` (channel volumes), `55AC mm pp` (pans). See
  [Pinball 2000](../how-it-works.md#per-family).
- **Sound list**: the same catalog in the board's words (`dcsrom::p2k_image`, catalog at
  $10000; `ffi::dcs_rom`): swep1_130 690 populated tracks of 2301, rfm_120 1557 of 2940.
  No sounds.dat section: files are named by track number.
- **Stop, boot and resets**: stop `55AE 3F00` (`DCSQuietAllTracks`, `BUILTIN_STOPS`). The
  game's boot uploads a boot block (`000E`...), checks the ROMs, then opens the DCS2
  protocol with `ACE1` twice (answered `0100 000C`). rfm_120's warm boot never opens it
  (XINA 1.12): the tool resets the board before the main pass (`reset_before_pass`,
  `p2k_opened`). A reset (`sndbrd_ctrl_w`, `dcs_ctrl_w`) leaves the DSP in its loader:
  the tool then sends the game's boot block upload, `ACE1` twice and the volumes
  (`p2k_reboot`); `boot.p2k` in `manifest.json` says what happened.
- **Volume**: factory = the game's `55AA 609F` (level 12), or level 12
  (`P2K_FACTORY_DEFAULT`) when the boot sent none; reference `55AA A05F` (level 20,
  `Ref.p2k`), since `FF` clips most sounds; the channel volumes and pans are replayed as
  the game sent them. Measured offset -10.56 / -10.57 dB. The master volume check runs
  (`alt_volume`, `DCSP2K`): swep1_130's 11 loudest files, 8 levels down, moved -10.6 LU
  each.
- **Loops**: from the track programs, as on DCS (`dcs-catalog`); DCS2's opcodes `13 ll`
  and `14 ll nnnn` (a level and a fade of the program's own channel) are skipped, not
  modelled (`dcsrom.rs`, `Sim`).
- **DUCK / STOP / CHANNEL**: defaults. `read_dcs_programs` only reads the effects on a
  board reported `"DCS"`: a program's channels are relative to the board channel the
  game picks, which the tool does not know.
- **Measured** ([Factory results](../how-it-works.md#factory-results-for-our-roms)):
  swep1_130 683 of 690 tracks written, 26 loops (24 from the programs, 2 from the audio);
  rfm_120 1538 of 1557, 34 loops (all from the programs), 631 twins; none clipped, all
  from silence, loudest true peak -16.4 dBTP. Not in the full VPinMAME set (its zips hold
  only update files); `rom2altsound roms --fix-names` builds complete sets with MAME's
  `rfmpb.zip` / `swe1pb.zip` ([ROM verification](../how-it-works.md#rom-verification)).
- **Limits**: no ducking, stop or channel analysis; the game's per-channel choice of
  board channel is not reproduced.
- **In VPinball**: **the pack does not play**: the game's requests go from the PC to the
  board without `sndbrd_data_w`, so `snd_cmd_log` never sees them and AltSound receives
  no command (README, Supported boards, note 16).
