# Ducking, stops and channels from the ROM: feasibility study

Branch `ducking-study`, October 2026. ROM: Attack from Mars `afm_113b` (WPC-95, DCS, sound
ROM `afm_s2.l1`, 1130 catalog slots, 590 populated). Status: research and prototype, not
integrated in the pack writer.

## Summary

- **On DCS, yes, and exactly.** Every DCS track program says, in plain bytecode, how much it
  lowers each other channel and when it gives the level back. The 590 AFM commands take
  0.12 s to analyse. **313 of them duck the music**, and none stops it. The dynamic
  measurement matches the static reading **within 0.2 dB** on all 9 commands tested
  (-2.35 dB predicted, -2.35 measured for "Jackpot!"; -18.82 / -18.94 for the fanfare).
- **The ROM's own mix is gentler than most packs.** AFM callouts lower the music by only
  **2.4 dB (DUCK 76) or 3.5 dB (DUCK 67)**. Only fanfares and a few big effects go deep,
  from -16.5 to -23.5 dB (DUCK 15 down to 7). The music is never paused, and it is only
  stopped by the stop commands, which play nothing.
- **CHANNEL comes straight from the ROM.** Each command has a home DCS channel. On AFM,
  channel 0 is music, 3 is the General's voice, 1/2 are SFX twins and 4/5 are Martian voice
  and SFX twins. A new command on a channel cuts the previous one on that channel and
  nothing else. This also explains the "twins": they are the same sound on two channels, so
  that two can play at once.
- **AltSound/G-Sound can hold most of it**: the music channel, one exclusive voice group,
  and the duck depth and length. They cannot hold the 0.15 s release fade, the way the DCS
  adds overlapping ducks together (AltSound keeps only the deepest), the exclusive groups
  beyond one or two, or the deferred "start at the next musical phrase" mechanism.
- **Non-DCS boards: no static reading, and the dynamic method breaks down.** Apollo 13
  (Whitestar) does not replay its music sample-exactly: two recordings correlate at 0.3-0.99
  per window, where DCS correlates at 0.998+. The waveform fit fails there (residual about
  0 dB). It needs another approach (see "Other boards").
- **Recommendation:** integrate the static DCS reading into rom2altsound: the manifest,
  CHANNEL/DUCK in `altsound.csv`, and TYPE and generated ducking profiles in G-Sound. Keep
  the dynamic fit as an optional check pass. Leave non-DCS boards as they are (gain 100, no
  ducking) until there is an instrumented-chip approach. About 2 days for the integration
  and 1 more for the check pass.

## How DCS encodes it

Sources: mjrgh's DCSExplorer (`DCSDecoderNative.cpp`: `ExecTrack`, `MixingLevelOp`,
`UpdateMixingLevels`, `ResetMixingLevels`, `LoadTrack`; `Doc/DCS_format_reference.html`).

- The board has 8 channels (6 used by WPC games). Each channel has at most one **track
  program** and plays at most one **audio stream**. A command `nnnn` below the catalog count
  is track `nnnn`. Its header gives a type (1 = run now, 2 = deferred) and a **home channel**.
- Starting a type 1 track on channel `c` replaces the program on `c` and clears its stream
  (`LoadTrack`). This is the board's only implicit "stop": **same channel = preempt, other
  channel = overlap**.
- Opcodes `07/08/09` (set/increase/decrease) and `0A/0B/0C` (the same with a fade over N
  frames) change channel `t`'s mixing level. Each channel `t` keeps **one contribution
  slot per source channel**: `mixer[t][source]`. A callout on channel 3 doing `09 00 0A`
  writes -10 into `mixer[0][3]`, the music's slot for channel 3.
- The channel's level is the sum of its slots (clamped to ±127 units). Its gain is
  `0.9733^(127 - sum)`, so **one unit is 0.2352 dB, linear in dB**. A duck of `n` units is
  `-0.2352·n` dB whatever the music's own level. The music sets its own level with `07 00 vv`
  (AFM: 95 to 121).
- A source's slots are reset to 0 when its program ends (`00`), when its channel is stopped
  (`02`), or when a new track starts on its channel. So **a duck lasts as long as the
  program that set it**, and programs give it back with a fade (`0B`) just before they end.
- `02 c` stops channel `c` (program and stream). `03 tttt` queues a track. `05 c` starts the
  track deferred on channel `c` (type 2 tracks set it). This is how music changes at a
  phrase boundary.

`src/dcsrom.rs` already ran this model to find the loops. The prototype records what the
programs do (`track_effects`) and adds the one difference it found with `LoadTrack`: a new
track on a channel clears that channel's stream. This had no effect on AFM, whose tracks
queue nothing. The unit tests still pass.

Example, "Jackpot!" (`0x01B6`), whole program:

```
01 03                    type 1, channel 3
0000 09 00 0A            frame 0: music (ch0) -10 units  (-2.35 dB)
0000 07 03 7D            frame 0: own level 125
0000 01 03 092662 01     frame 0: play stream on ch3 once
0058 0B 00 0A 0014       frame 88 (0.68 s): music +10 units, fade over 20 frames (0.15 s)
0014 00                  20 frames later: end (all contributions reset)
```

## What AFM's programs do

`rom2altsound dcs-effects afm_region.bin afm_113` (full listing in
`afm_effects.txt` and `.json`, not committed: the ROM data is not ours to ship).

### Channel map

| DCS channel | commands | content |
|---|---|---|
| 0 | 25 | the 20 music tracks (0x0001-0x0014), plus 0x0000 (stop all), 0x03E3/E7/E8 (no stream: they stop the music) and 0x0050 (deferred) |
| 1 | 109 | SFX (102), fanfares named "Music:" (5), 2 unnamed |
| 2 | 118 | the same SFX again (107 pairs 1/2 with identical programs) |
| 3 | 172 | the General's voice (171 lines), plus 0x03E6 (stop ch3) |
| 4 | 83 | Martian voices (50), SFX (30), 1 fanfare, 2 unnamed |
| 5 | 83 | the same again (82 pairs 4/5) |

The **twins** (README: "Attack from Mars lists every sound effect twice") are channel
variants. With two copies of an SFX on ch1 and ch2, two of them can sound at once, and the
game CPU picks the free channel. The General has no twins: one line at a time.

### Ducking (what the commands do to the music)

313 of 590 commands lower channel 0. Apart from the music tracks, which set only their own
level, nothing raises a level anywhere.

| units | dB | AltSound DUCK | commands | what |
|---|---|---|---|---|
| -10 | -2.35 | 76 | 74 | General's lines (60 of them), light SFX |
| -15 | -3.53 | 67 | 202 | most voices: General (194 of 256 ducking voices), all Martian lines, impacts |
| -20 | -4.70 | 58 | 16 | impacts, bonus dings, 2 voices |
| -30 | -7.05 | 44 | 2 | Multiball impact |
| -70 | -16.45 | 15 | 10 | goal / extra-ball fanfares, laser beam, big impact |
| -80 | -18.81 | 11 | 5 | "Music: Fanfare" 0x0094/95, Metallic impact 0x0186 |
| -100 | -23.51 | 7 | 4 | Martian whoo, Bonus counter |

By kind: 256 of 271 voice lines duck (the 15 that do not are Martian death cries on ch3, such
as 0x01F9 "Hawhawhaw", 0x01FE "Aaaawwwgh!"). So do 47 of 273 SFX and 10 of 12 fanfares.

**Timing**, as measured in the programs:
- **Onset**: with the sound, frame 0, for 311 of 313. Two (0x0118/9 Growth ray) start
  0.15 s late. 10 (those two included) reach full depth after a 0.15 to 0.48 s ramp.
- **Hold**: for as long as the sound plays. The duck is back to 0 a median **15 ms** before
  the program ends, and the program ends with the stream (0x01B6: program 0.84 s, WAV
  0.83 s).
- **Release**: a 0.15 s fade (20 frames) for 260 of 313. 49 fade over 0.17 to 1.6 s, and 4
  have no fade: the level comes back at once when the program ends (0x0102 Laser beam,
  0x0176 Bonus counter).
- 17 SFX give the level back 0.22 to 0.40 s before their end, and 0x0186 Metallic impact
  2.0 s before (hold 0.23 s, then a 1.6 s fade). These are the only cases where "duck for
  the file's length" is wrong by more than 0.1 s.
- Ducks on channels other than the music: only 3 commands. 0x0138/0x0139 Impact splat
  lowers its twin channel by -15. 0x0269 lowers the music by -15 and ch2 by -10.

### Stops

No program uses opcode `02` on another channel, except 0x0000, which stops channels 0-5.
No program queues anything. What stops a sound on AFM is:
- **the same channel**: a new music track replaces the music, and a General line cuts the
  previous General line;
- **the no-stream tracks** 0x03E3 (ch0), 0x03E4 (ch1), 0x03E5 (ch2), 0x03E6 (ch3),
  0x03E1 (ch4), 0x03E2 (ch5), which only clear their channel. 0x03E7/0x03E8 do the same on
  ch0 and write 0x11/0x00 back to the game (sent at boot). libaltsound hardcodes 0x03E3 as
  "stop music" for DCS, and 0x0000 too, but not the others;
- type 2 tracks 0x0050/51/52 set a deferred track on ch0/1/2 (0x0003, 0x0007, 0x0012). A
  running music track starts it with opcode `05` at its next phrase boundary: a music
  transition on the beat, which AltSound has no way to express.

## Static vs dynamic: validation

Dynamic method: `--only` now takes scenarios (`0x000C+3+0x01B6`: send 000C, wait 3 s, send
01B6, one recording). Each scenario is recorded next to the music alone and the sound alone,
all from the same boot. All three are trimmed at their first sound, so the music lines up
(lag 0 to 1 sample). `duck-fit` then fits `MC ≈ gm·M + gc·C` by least squares in 30 ms
windows and gives the music's gain over time. Before the sound, the fit gives -0.00 to
-0.01 dB with a residual around -30 dB (the 31.25 → 44.1 kHz resampling phase differs
between takes), and gc stays at 0 dB.

Music 0x000C "Martian Attack 3" (own level 121), sound sent at 3 s:

| command | ch | predicted | measured hold (median, sd) | duck end predicted / measured (half depth) |
|---|---|---|---|---|
| 0x01B6 "Jackpot!" | 3 | -2.35 dB | -2.35 (0.03) | 0.82 s / 0.76 s |
| 0x0390 "Ouch!" | 4 | -3.53 | -3.55 (0.01) | 0.81 / 0.75 |
| 0x0392 "Hey look, your shoe is untied..." | 4 | -3.53 | -3.54 (0.02) | 4.29 / 4.24 |
| 0x0064 SFX Whack 1 | 1 | 0 | -0.01 (0.01) | - |
| 0x0094 Music: Fanfare | 1 | -18.82 | -18.94 (0.13) | 3.62 / 3.54 |
| 0x0102 SFX Laser beam (no fade) | 1 | -16.46 | -16.61 (0.15) | 0.61 / 0.60 |
| 0x0176 SFX Bonus counter (no fade) | 1 | -23.52 | -23.70 (0.05) | 1.38 / 1.39 |
| 0x0186 SFX Metallic impact (early 1.6 s fade) | 1 | -18.82 | -18.88 (0.19) | 1.84 / 1.15 (mid-fade) |
| 0x0390 "Ouch!" over music 0x0003 (own level 100) | 4 | -3.53 | -3.55 (0.03) | 0.81 / 0.73 |

The measured end is where the gain comes back above half the depth, halfway through the
release fade, so it should come 0.05 to 0.08 s before the predicted full return, and it
does. The deep ducks measure 0.1-0.2 dB deeper than `0.2352 dB/unit`, which is the rounding
of the board's 1.15 fixed-point power series. The duck is the same over two musics with
different own levels (121 and 100), as the additive model says.

Stops and channels, from the recording lengths:
- `0x000C+3+0x0000`: the recording ends at 3.06 s, so 0x0000 stops the music.
- `0x0392+1+0x0390` (both on ch4): 1.84 s = 1 + 0.82. The second line cuts the first.
- `0x0392+1+0x0391` (ch4 then ch5): 4.25 s. Both play.
- `0x0392+1+0x01B6` (ch4 then ch3): 4.25 s. Both play.
- `0x0094+1+0x0094` (ch1 twice): 4.67 s = 1 + 3.64. The fanfare restarts.

Static and dynamic agree on everything tested. On DCS the static reading is the one to use:
it is exact, instant, and needs no music to measure against. The dynamic run is a check.

## Mapping to the pack formats

### libaltsound semantics (`altsound_processor.cpp`, `gsound_processor.cpp`)

These two files are identical in the local checkout and at the pinned `f4b790a1`.

**altsound.csv** (`ID,CHANNEL,DUCK,GAIN,LOOP,STOP,NAME,FNAME`):
- `CHANNEL` 0 = music: one at a time, a new one stops the old one. 1 = jingle/single: one
  at a time, a new jingle stops the previous jingle. -1 or empty = SFX: polyphonic.
- `DUCK` 0-100: the music's volume factor (`DUCK/100`) while that stream plays. The music
  plays at `gain × min(DUCK of every active stream)`, recomputed when a stream starts or
  ends, with **no fade**. A negative DUCK on a jingle pauses the music until that jingle
  ends. Music rows get 100. Only the music is ducked: nothing ducks SFX or jingles.
- `STOP` = 1 on a jingle stops the music (no effect on other channels).
- `LOOP` = 100 loops.
- Built-in DCS handling: 0x0000 and 0x03E3 stop the music (`altsound_postprocess_commands`).

**g-sound.csv** (`ID,TYPE,GAIN,DUCKING_PROFILE,FNAME`) and `altsound.ini`:
- Types: `music`, `callout`, `solo` and `overlay` are each exclusive within their type (a
  new one stops the previous one). `sfx` is polyphonic.
- Per type, `stops`/`pauses`/`ducks` lists of other types, `group_vol`, and
  `[<type>_ducking_profiles] profileN = music:50, sfx:65, ...`. A row's `DUCKING_PROFILE`
  picks the profile of its own type.
- The ducked volume is the minimum over the active ducking streams, removed when the
  stream ends (no fade). A type cannot duck or pause itself. `music` and `solo` cannot
  duck anything, and `sfx` and `overlay` cannot stop or pause anything.

### Column by column

| column | from the ROM (DCS) | precision |
|---|---|---|
| AltSound `CHANNEL` | 0 = home channel 0 with a stream (music). One exclusive DCS channel (AFM: ch3, the General) can be the jingle channel 1, which gives "a new line cuts the previous one". Everything else -1 | Exact for music and for one voice channel. The DCS's other exclusive channels (ch1, ch2, ch4, ch5: same channel cuts) become polyphonic. The game sends commands to a free twin channel anyway, so the difference is small in play |
| AltSound `DUCK` | `round(100 · 0.9733^units)` from the deepest contribution to channel 0; 100 if none | Depth exact (±0.2 dB). Length = the file's length, which matches the ROM within 0.1 s for 296 of 313 commands. AltSound cannot express the 0.15 s release, the late onset or ramp of 10 commands, or the early release of 17 SFX. Overlapping ducks add up on the DCS (-2.35 and -3.53 give -5.9 dB) but AltSound keeps only the deepest (-3.53). |
| AltSound `STOP` | 0 everywhere on AFM, which is exact: no sound stops the music. The commands that stop channels play nothing, so they have no row (libaltsound handles 0x0000/0x03E3 itself; 0x03E1/E2/E4-E8 are lost, and only 0x03E7/E8 are sent, at boot) | Exact for AFM. A ROM whose sound programs use `02 00` would get STOP = 1 on a jingle, but only for the music |
| AltSound `LOOP` | unchanged (already from the program's loop) | exact |
| AltSound `GAIN` | unchanged: the programs' own levels (`07 c vv`, AFM 60-126) are already in the recorded WAV levels | exact (already) |
| G-Sound `TYPE` | ch0 + stream: `music`. The voice channel without twins (ch3): `callout` (exclusive, as on the DCS). Other channels: `sfx`. Optionally one more exclusive channel as `overlay` | Same limits as CHANNEL. G-Sound has 4 exclusive groups, so up to 4 DCS channels could keep their exclusivity, except that `overlay` cannot stop or pause anything and `solo` cannot duck |
| G-Sound `DUCKING_PROFILE` + `[*_ducking_profiles]` | one profile per depth found in that type: AFM callout `profile1 = music:76`, `profile2 = music:67`, `profile3 = music:58`; sfx `music:76/67/58/44/15/11/7` | Same as DUCK. Ducks of SFX by SFX (the 3 cross-twin cases) could go in `sfx:`, but an sfx cannot duck sfx in G-Sound |
| G-Sound `stops`/`pauses` | none on AFM (callout `stops` = nothing, `pauses` = nothing) | exact for AFM |

### What remains artistic

- **Whether to follow the ROM.** A 2.4-3.5 dB duck under a callout is what the machine does,
  but a remastered or louder music track may need more. The tool can write the ROM's values
  and the author can scale them.
- **Fanfares.** The ROM ducks the music to 11-15 % and keeps it running. An author may
  prefer to pause or stop it (jingle with DUCK < 0 or STOP = 1). The ROM's answer is
  "duck, not pause".
- **Group volumes** (`group_vol`) and the overall music-vs-voice balance, when the files are
  replaced by other recordings.
- Anything the **game CPU** decides (which command, when, which twin, music changes): the
  game sends the same commands to AltSound as to the board, so that part is already the
  ROM's.

## Other boards

**Static**: none of them has a documented bytecode. Their sound programs are 6809 code
(WPC89/WPCS: YM2151 + DAC + CVSD; System 11; Data East/Whitestar: BSMT2000 voices driven
by the 6809). Ducking, if any, is in that code: it would take reverse engineering per board
family, and maybe per game.

**Dynamic, tested on Apollo 13 (Whitestar)**: the music does not replay sample-exactly.
Two takes of 0x03 correlate at 0.29 to 0.99 per 0.25 s window, with lags jumping between
-1310 and +1219 samples. Some takes match closely (0.94-0.99), others not at all. On AFM the
same check gives 0.998-0.9998 at a constant lag (`drift-check`). With that, the
least-squares fit has a residual of -0.1 to -1.4 dB, where AFM gives -30 dB, and its gains
(-5.5 dB before the sound even starts) mean nothing. The README's "Data East and Whitestar
music never repeats exactly" was about loops, but it holds between takes too.

Ways forward, none tried:
- **Instrument the emulated chip** in the shim: log the per-voice volume registers
  (BSMT2000 voice volumes, YM2151 total level, the WPC DAC volume latch) while M then C
  play, and see whether the music's voices get quieter. This is exact when it works, but
  the voices are allocated dynamically, so telling the music's voices apart is per-family
  work.
- **Statistical audio**: several takes, short-term loudness in bands where C has little
  energy, and the median over takes. It gives a rough figure (probably ±2-3 dB) and cannot
  say much under a broadband sound.
- WPCS and System 11 were not tested. If they replay exactly, the dynamic fit as it is
  would work there: `drift-check` on two takes says so in seconds.

## Integration into rom2altsound

1. **Extraction** (DCS only, in-process, region already at hand): call
   `dcsrom::track_effects` for every command. It takes 0.12 s for AFM's 590 commands, 60 s
   cap. Store in `manifest.json` per command: `dcs: { channel, streams, ducks: [{channel,
   units, db, start_s, end_s, restore, release_s}], stops, deferred }`, plus the channel
   map and the stop commands per ROM.
2. **Pack writer** (`altsound.rs`): CHANNEL, DUCK and STOP as above, and G-Sound TYPE plus
   generated `[callout_ducking_profiles]`/`[sfx_ducking_profiles]` (one profile per distinct
   depth) and `ducks = music` in `altsound.ini`. Keep the README's "starting point" wording,
   but say the ducking now comes from the ROM. Twins: keep `--merge-twins` as it is. Merging
   is safe for AltSound SFX (polyphonic), and the manifest can now say why they exist.
3. **Optional check pass** (`--check-ducking`): play the loudest music, then one command per
   distinct depth on top (the scenario machinery from this branch). Compare with the
   prediction and flag a ROM where they differ by more than 0.5 dB. It adds about 20-40 s per
   ROM.
4. **Limits to keep in mind**: 1993 software (Indiana Jones, Judge Dredd) reads opcodes 04
   and 06 differently, and the model follows the 1994+ forms. DCS-95 opcodes 10-12 are not
   modelled. A command whose program cannot be followed gets no ducking data (defaults
   stay). Deferred music transitions are reported, not rendered.
5. **Tests**: synthetic programs in `dcsrom` tests (duck, fade back, program-end reset,
   same-channel preempt, `02` stop), and the `altsound.rs` mapping from a hand-made manifest.

Estimate: steps 1-2 and tests, about 2 days. Step 3, 1 day. Other boards: open research.

## Prototype on this branch

- `src/dcsrom.rs`: `Sim` records `Event`s (Start, Stream, Stop, Queue, Mix).
  `track_effects()` gives the events and per-frame levels. `LoadTrack` now clears the
  channel's stream. `DB_PER_LEVEL`.
- `src/ducking.rs`: `dcs-effects <region.bin> <sounds.dat section> [--json F] [--max-secs S]`,
  `duck-fit <M.wav> <C.wav> <MC.wav> <at_secs> [--win S]`, `drift-check <A.wav> <B.wav>`.
- `src/extract.rs`: `Send::Wait` and scenario ids in `--only` (`0x000C+3+0x01B6`).
  `R2A_DUMP_REGION=<file>` dumps the DCS sound region at the end of boot.

Reproduce (AFM):

```
R2A_DUMP_REGION=afm_region.bin rom2altsound afm_113b --roms <dir> --out runs/probe --only 0x01b6
rom2altsound dcs-effects afm_region.bin afm_113 --json afm_effects.json
rom2altsound afm_113b --roms <dir> --out runs/dyn --loop-max-secs 0 --max-secs 9 \
  --only 0x000C,0x01B6,0x000C+3+0x01B6
rom2altsound duck-fit runs/dyn/afm_113b/0x000C-afm_113b.wav runs/dyn/afm_113b/0x01B6-afm_113b.wav \
  runs/dyn/afm_113b/0x000C+3+0x01B6-afm_113b.wav 3 --win 0.03
```

Build note: on the host `serveur` the PinMAME build needs
`CFLAGS/CXXFLAGS=-I<repo>/vendor/pinmame/ext/zlib`. PinMAME compiles its own zlib, but on
Linux the CMake file does not add the include path, so it relies on the system headers
(`zlib1g-dev`), which this host does not have.
