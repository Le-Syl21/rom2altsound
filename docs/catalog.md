# The catalog (`catalog.json`)

`rom2altsound catalog` writes `docs/catalog.json`, the data behind the
[catalog site](https://le-syl21.github.io/rom2altsound/): one entry per sound ROM of the
PinMAME built in, and, read from extraction runs, the loudness of each one's sounds at the
game's factory volume and how complete its extraction is. It is meant as the reference
dataset of the [loudness](https://github.com/Le-Syl21/loudness) plugin, which gives each
VPinball table one static gain.

Metadata only: names, sizes, checksums, counts and levels. Nothing of a ROM's content, no
sound, no sounds.dat name.

## The file

A header line, then one entry per line (small diffs when it is regenerated).

Header:
- `format` (1), `generator` (`rom2altsound <version> catalog`), `units_read`;
- `family_docs`, `family_docs_default`: the doc link of each family;
- `measurement`: what an extraction with this version's default settings records (see
  [Staleness](#staleness));
- `stats`: counts over the entries (`entries`, `sound_rom_ids`, `sets`, `sets_found`, per
  support level and per family), `entries_with_counts`, `entries_with_names`,
  `entries_not_applicable`, `sets_not_applicable`, `entries_with_loudness`,
  `entries_with_current_loudness`, `sets_with_factory_volume`, `sets_volume_differs`,
  `sets_volume_differs_unmeasured`.

Entry:
- `id` (the sound ROM id, or null), `key` (the id, or `set:<name>` for a set without sound
  ROMs: Stern SAM, game-driven and no-board games), `family` (`SNDBRD_*`), `label`,
  `support`, `title`, `manufacturer`, `year`;
- `sets`: every PinMAME set sharing the sound ROMs: `name`, `parent`, `description`,
  `manufacturer`, `year`, `found` (in the ROM folders read), `names` (sounds.dat), and when
  known `not_applicable`, `factory_volume`, `loudness` (below);
- `roms`: the sound ROM files (name, size, CRC32, SHA-1, found);
- `sounds`: what the ROMs tell without running them (`dcs-tracks`, `sam-calls`);
- `names`: distinct commands sounds.dat names;
- `not_applicable`: why the entry has nothing to extract: no sound board (`NONE
  (other)`: chimes, or sound made by the CPU board), or every set not applicable;
- `loudness` and `coverage`: read from the runs (`--manifests`), below.

## Loudness

`loudness` is the measured set's `manifest.json` `loudness`, at the game's factory volume,
as PinMAME plays the ROM in VPinball:

| field | what |
|---|---|
| `set` | the set the figures were measured on (the game's head set when it was extracted) |
| `lufs` | **the main figure**: EBU R128 gated integrated loudness of every written file played back to back (the gating blocks of all the files together; a file under 400 ms counts as one block padded with silence; the files flagged `ignores_master_volume` are left out). A mono ROM is measured as two identical channels, as VPX plays it |
| `lufs_excluding_loops` | the same without the loops and the sounds cut at `--max-secs` |
| `median_file_lufs` | the median per-file loudness of the non-loop files: a typical sound |
| `true_peak_dbtp` | the loudest file's true peak |
| `seconds`, `files` | the material in `lufs` |
| `factory_volume` | the master volume the files are at, per board with one: `board`, `family`, `bytes` (the command the game sent, e.g. `55AA6798`, `FE2C`, `790CF3`; Stern SAM: the DAC attenuation `E8E8`), `level` / `level_max` on the game's scale. Empty: the board has no master volume, or the game sends none (the files are at the board's only level) |
| `factory_volume_from` | where that volume comes from, per board |
| `source` | `emulation` (the board ran in PinMAME and its output was recorded) or `static` (Stern SAM) |
| `caveat` | what the figure is not (Stern SAM, a partial run, a borrowed sound ROM, not the factory volume) |
| `at_reference_volume` | a board with a master volume whose files stayed at the reference volume, because the factory offset could not be measured (a board that went silent in emulation): their levels, `lufs` and the levels above being null |
| `static_decode` | Stern SAM only: the levels of the static decode, which are not a game's (`lufs` and the levels above are then null) |
| `measurement`, `current` | the run's tool version and settings, and whether they are this catalog's ([Staleness](#staleness)) |

The figure is the loudness of the **material** (every sound once, the music's first
`--max-secs` or one loop cycle), not of a game being played: how often each sound plays,
and what plays together, is the game's. It is what a static per-table gain can be computed
from: two ROMs whose `lufs` differ by 6 LU differ by about 6 dB at the same VPX volume.

### Factory volume per set

The master volume is set by the game program (the CPU ROM), so sets sharing a sound ROM can
ship with different factory volumes. Each set read gets its `factory_volume` (from its own
extraction, or from a `--boot-only` run: the warm boot from the factory nvram, stopped
before the first sound). A set whose factory volume differs from the measured set's gets a
`loudness` of its own: its own extraction's figures when it was extracted, else `lufs`
null and a `caveat` saying so (it is then listed in `differs.txt` by `--plan`).

### Stern SAM

SAM has no sound board: its sounds are decoded from the image, at the factory DAC
attenuation as PinMAME plays it, every sound and song at its full bus level. The game's own
bus levels and its mix are not applied, and measured against a game they matter: on
acd_168h, PinMAME's output in a started game (the ball in the shooter lane) is a music bed
at -13.1 LUFS where the pack's files read -11.3 LUFS together, and a coin sound plays 6.6
dB below its file ([Stern SAM](families/stern-sam.md#sndbrd_none_sam), "In a game"). So a
SAM entry has `source` `static`, **`lufs` null** (and the other levels), its figures under
`static_decode` (`lufs`, `lufs_excluding_loops`, `median_file_lufs`, `true_peak_dbtp`),
and a `caveat`: not measured in a game. The site shows them in italics.

## Coverage

`coverage` is the measured set's `manifest.json` `coverage` (plus `twins`, from the pack):

- `sources`: where the command list came from and how many commands each gave:
  `sounds.dat` (the game's section), `sweep` (the board's command space,
  [the list of commands](families/common.md#the-list-of-commands)), `rom track catalog`
  (DCS, Pinball 2000), `game sound layer` (game-driven boards), `rom call table` (Stern
  SAM);
- `expected`: the commands of the list (the union of the sources), `tried`, `with_sound`,
  `written`;
- `not_written`: the others by reason: `silent`, `blip`, `stop_command`,
  `volume_command`, `undumped_sample_rom`, `empty_after_trimming`, `not_tried`; Stern SAM:
  `some_samples_without_a_stream`, `no_sample_with_a_stream`;
- `twins`: written files that play the same audio as an earlier one.

`written` plus the `not_written` classes is `expected`: every command of the board's space
is accounted for. A `silent` command is one the board does nothing with (an empty slot of
its table) or one that only acts in context (it changes a running background); the family
docs say which.

### Not applicable

Sets with nothing to extract are `not_applicable` with the reason, not failures: `sam_iv`
(the tester's own program has no sound code), `mideartp` (a bad dump), `tts_l1`/`tts_l2`
(sound ROMs never dumped), `bstrk_l1`/`tstrk_l1` (chimes), the `sam1_flashb_*` boot
flashes, `xforce` (sound and sample ROMs never dumped; `--sound-rom-from spcteam` gives an
approximate pack), every set whose sound ROMs PinMAME lists all as `NO_DUMP`
(`catalog::driver_not_applicable`: the board has no program to run; 12 sets in October
2026: `blackbl2`, the unreleased `brooks`, the prototype `cv_d52`, `eballch2`, the
`hh_4`/`hh_4a`/`hh_4b` speech mods, `ladyshot`/`ldyshot2`, `movmastr`, `mrblkz80`,
`myststar`), and every `NONE (other)` entry (no sound
board).
`rom2altsound <set>` writes a manifest with `not_applicable` for them and exits 0.

## Staleness

Every manifest carries `measurement`: the tool, its version, `method` (a number bumped when
a change of the tool moves the figures of an unchanged ROM: a new sweep range, another
gating) and the settings the figures depend on (factory mode, volume mode and reference
volumes, `max_secs`, `loop_max_secs`, `no_sound_secs`, boot times, the BSMT2000 emulation,
another sounds.dat or stop, a borrowed sound ROM, and `complete`: no `--only` or
`--limit`). The catalog's header has the `measurement` of this version's defaults; an
entry's `loudness.current` is true when its run had the same method and settings (the
version aside). A consumer should only compare `current` figures with each other.

## Building it

```
# 1. The set lists (from the ROM folder): measure.txt, boot.txt
rom2altsound catalog ~/roms --out /tmp/c.json --plan plan/

# 2. One full extraction per entry, the audio dropped once measured, resumable
rom2altsound $(cat plan/measure.txt) --roms ~/roms --out runs/full -j2 --drop-audio --skip-done

# 3. The factory volume of every other set
rom2altsound $(cat plan/boot.txt) --roms ~/roms --out runs/boot -j2 --boot-only --skip-done

# 4. The sets whose factory volume differs from their entry's measured set
rom2altsound catalog ~/roms --out /tmp/c.json --manifests runs/full --manifests runs/boot --plan plan/
rom2altsound $(cat plan/differs.txt) --roms ~/roms --out runs/full -j2 --drop-audio --skip-done

# 5. The catalog, then the site
rom2altsound catalog ~/roms --manifests runs/full --manifests runs/boot --out docs/catalog.json
python3 docs/build_site.py
```

`--drop-audio` deletes each ROM's WAV files once it is done and keeps `manifest.json`, the
CSVs and the boot reports: every figure the catalog reads. `--skip-done` skips a ROM whose
folder already has a current result (same `measurement`, sounds written or not
applicable), so a stopped run starts again where it was.

### Why a full extraction of the sets whose volume differs

`boot.txt` holds only the sets of the families whose master volume the tool decodes (DCS,
DCS95, Pinball 2000, WPCS, Whitestar, DE3S: `catalog::MASTER_VOLUME_LABELS`); on the
others every set records at the board's only level. Measured with `--boot-only` on every
set of those families' multi-set entries (884 sets, October 2026): **45 sets (5.1 %) in
17 entries** ship with another factory volume than their entry's head set, nearly all of
them free-play or home ROMs (`afm_f20`, `tz_f19`, `t2_f19`, `wcs_f50`, `dm_dt101`: level 30
or 31 where the head set has 12) and two real revisions (Harley-Davidson 1.08 and later:
`FE2A`, level 5 against 15; `xfiles304`: `FE2B`). No DE3S set differs. A full extraction of
those 45 costs about 21 job-hours (+10 % on the whole run) and measures what PinMAME plays
at that volume, clipping included (`afm_f20` at level 30: -16.3 LUFS against -40.3 for
`afm_113b` at 12); a volume law (dB per level, measured on each sound ROM) would cost a
replay pass on every one of the ~170 sound ROMs with a master volume, and its levels near
the top are where the emulated output clips, so it would be the less exact of the two.
The `--boot-only` pass itself costs 9 s a set on average (25 s on DE3S).

### Cost of the whole run

From the pilot (24 sets over the families, [board support](board-support.md#production-runs-the-catalog-pilot-03)):
25 s (Stern SAM) to 59 min (GTS80B badgirls) per set, 1193 sets in `measure.txt`: about
215 job-hours, **about 4.5 days at `-j2`** on the 8-core host, Whitestar, WPCS, GTS80B and
DE3S taking two thirds of it (their musics run to the 240 s loop search; the DE3S AT91 runs
at x1.5 real time). Then `--boot-only` on 738 sets (about 1 hour at `-j2`) and the sets
whose volume differs (about 10 hours at `-j2`): **5 days in all**. Disk: at most two sets'
audio at a time (up to 1.1 GB each) before `--drop-audio` deletes it, and under 1.5 MB kept
per set: about 1 GB for the whole catalog.
