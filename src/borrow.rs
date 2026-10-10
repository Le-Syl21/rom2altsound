//! `--sound-rom-from`: extracting a set whose sound ROM was never dumped with the sound ROM
//! of a compatible set the user has. The combined set is built in the private PinMAME
//! directory (never in the user's ROM folders): the set's own files, plus the other set's
//! sound ROM under the name the set's driver expects. PinMAME loads a file it lists as
//! `NO_DUMP` when the zip holds it (with a "no good dump known" warning).
//!
//! Only known pairs are accepted (each with the evidence that both games run the same
//! sound program), any other with `--force-sound-rom`. The pack says it everywhere
//! (manifest.json `sound_rom_borrowed`, the listening page, a README.txt): its sounds are
//! the other game's program answering this game's commands, an approximation.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::drivers::{Driver, RomFile};
use crate::zipread;

/// A set whose sound ROM is not dumped and the set whose sound ROM stands in for it.
pub struct Pair {
    pub set: &'static str,
    pub donor: &'static str,
    /// Why the donor's sound program is believed to be the set's own.
    pub why: &'static str,
    /// What the borrowed ROM cannot give back.
    pub missing: &'static str,
    /// Names of the set's commands (a names.csv), read in its game program, applied to the
    /// pack unless the user gives `--names`.
    pub names: Option<&'static str>,
}

/// The known pairs.
pub const KNOWN: &[Pair] = &[Pair {
    set: "xforce",
    donor: "spcteam",
    why: "Tecnoplay X Force (1987) and Space Team (1988) have the same sound board and the same game-side send routine, and Space Team's sound program (ic12) carries content only X Force uses: tunes 36 and 37, which X Force's game program sends and Space Team's never does, and DAC sample entries cut for X Force's commands 40, 4C and 4D. Every command X Force's game sends has an entry in that program (docs/families/other-makers.md, Tecnoplay).",
    missing: "the DAC samples 40, 4C and 4D: X Force's sample ROMs (ic8-ic11) were never dumped either, so these commands are silent",
    names: Some(include_str!("borrow/xforce-names.csv")),
}];

/// The known pair of `set`, if any.
pub fn known(set: &str) -> Option<&'static Pair> {
    KNOWN.iter().find(|p| p.set.eq_ignore_ascii_case(set))
}

/// The set's sound ROMs that PinMAME lists without a dump.
pub(crate) fn undumped_sound(d: &Driver) -> Vec<&RomFile> {
    d.roms
        .iter()
        .filter(|r| r.sound && r.no_dump && !r.optional)
        .collect()
}

/// One borrowed file, for the manifest.
#[derive(Debug, Clone, Serialize)]
pub struct BorrowedFile {
    /// The name the set's driver loads it as.
    pub name: String,
    /// Its name in the donor's set.
    pub from_name: String,
    pub size: u64,
    pub crc32: String,
    pub sha1: Option<String>,
}

/// What a pack borrowed (manifest.json `sound_rom_borrowed`).
#[derive(Debug, Clone, Serialize)]
pub struct Borrowed {
    pub from: String,
    pub files: Vec<BorrowedFile>,
    pub known_pair: bool,
    pub approximate: bool,
    pub note: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missing: Option<String>,
}

/// A borrow decided before the extraction: where the donor is and which files it gives.
pub(crate) struct Plan {
    pub donor: String,
    pub donor_zip: PathBuf,
    pub pair: Option<&'static Pair>,
    /// (the set's file, the donor's file).
    files: Vec<(RomFile, RomFile)>,
}

/// The line every borrowed pack carries.
pub fn note(set: &str, donor: &str, known: bool) -> String {
    format!(
        "sound ROM borrowed from {donor}: {set}'s own sound ROM was never dumped; {}. The sounds are {donor}'s sound program answering {set}'s commands: approximate.",
        if known {
            "both games run the same sound program (see docs/families/other-makers.md)"
        } else {
            "the pair was forced (--force-sound-rom), nothing says the programs match"
        }
    )
}

/// Decides the borrow for `set`: None when the set has every sound ROM dumped (nothing to
/// borrow: the option does not apply to it). `donor_arg` is a set name (looked up in
/// `dirs`, in order) or a path to its zip.
pub(crate) fn plan(
    drivers: &[Driver],
    set: &str,
    donor_arg: &str,
    dirs: &[PathBuf],
    force: bool,
) -> Result<Option<Plan>, String> {
    let find = |n: &str| drivers.iter().find(|d| d.name.eq_ignore_ascii_case(n));
    let d = find(set).ok_or_else(|| format!("{set}: not a PinMAME set"))?;
    let missing = undumped_sound(d);
    if missing.is_empty() {
        return Ok(None);
    }
    let p = Path::new(donor_arg);
    let (donor, donor_zip) =
        if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")) || p.is_file() {
            let stem = p
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| format!("--sound-rom-from {donor_arg}: not a ROM zip name"))?;
            (stem.to_ascii_lowercase(), p.to_path_buf())
        } else {
            let zip = dirs
                .iter()
                .map(|d| d.join(format!("{donor_arg}.zip")))
                .find(|z| z.is_file())
                .ok_or_else(|| {
                    format!(
                        "--sound-rom-from {donor_arg}: {donor_arg}.zip not found in {}",
                        dirs.iter()
                            .map(|d| d.display().to_string())
                            .collect::<Vec<_>>()
                            .join(" or ")
                    )
                })?;
            (donor_arg.to_ascii_lowercase(), zip)
        };
    if !donor_zip.is_file() {
        return Err(format!(
            "--sound-rom-from: {}: no such file",
            donor_zip.display()
        ));
    }
    let pair = known(set).filter(|p| p.donor.eq_ignore_ascii_case(&donor));
    if pair.is_none() && !force {
        let pairs = KNOWN
            .iter()
            .map(|p| format!("{} from {}", p.set, p.donor))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "--sound-rom-from {donor}: {set} from {donor} is not a known pair (known: {pairs}); --force-sound-rom borrows it anyway, with no evidence that the sound programs match"
        ));
    }
    let dd = find(&donor).ok_or_else(|| format!("--sound-rom-from {donor}: not a PinMAME set"))?;
    let mut files = Vec::new();
    for r in missing {
        // The donor's dumped sound ROM loaded into the same region with the same size,
        // the one with the same name first.
        let mut c: Vec<&RomFile> = dd
            .roms
            .iter()
            .filter(|x| {
                x.sound && !x.no_dump && x.crc.is_some() && x.size == r.size && x.region == r.region
            })
            .collect();
        c.sort_by_key(|x| !x.name.eq_ignore_ascii_case(&r.name));
        let x = c.first().ok_or_else(|| {
            format!(
                "--sound-rom-from {donor}: {donor} has no dumped sound ROM of {} bytes in region {} for {set}'s {}",
                r.size, r.region, r.name
            )
        })?;
        files.push((r.clone(), (*x).clone()));
    }
    Ok(Some(Plan {
        donor,
        donor_zip,
        pair,
        files,
    }))
}

/// Writes the combined set to `dst` (`<vpm>/roms/<set>.zip`): every member of `set_zip`
/// as stored, then the donor's files, checked against PinMAME's CRC, under the set's names.
pub(crate) fn stage(
    plan: &Plan,
    set: &str,
    set_zip: &Path,
    dst: &Path,
) -> Result<Borrowed, String> {
    let donor_members = zipread::list(&plan.donor_zip)?;
    let mut w = zipread::Writer::default();
    let replaced: Vec<String> = plan
        .files
        .iter()
        .map(|(r, _)| r.name.to_ascii_lowercase())
        .collect();
    for e in zipread::list(set_zip)? {
        if replaced.contains(&e.name.to_ascii_lowercase()) {
            continue; // a file of that name already in the set's zip: the donor's wins
        }
        let raw = zipread::read_raw(set_zip, &e)?;
        w.add_raw(&e.name, e.method, e.crc32, e.size, &raw);
    }
    let mut out = Vec::new();
    for (r, x) in &plan.files {
        let crc = x.crc.unwrap_or_default();
        let m = donor_members
            .iter()
            .find(|m| m.crc32 == crc && m.size == x.size)
            .ok_or_else(|| {
                format!(
                    "{}: no file with CRC32 {crc:08x} ({}'s {}, {} bytes)",
                    plan.donor_zip.display(),
                    plan.donor,
                    x.name,
                    x.size
                )
            })?;
        let data = zipread::read(&plan.donor_zip, m)?;
        w.add(&r.name, &data);
        out.push(BorrowedFile {
            name: r.name.clone(),
            from_name: x.name.clone(),
            size: x.size,
            crc32: format!("{crc:08x}"),
            sha1: x.sha1.clone(),
        });
    }
    if let Some(dir) = dst.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    // A link to the user's zip may be there: remove the link, never write through it.
    match std::fs::remove_file(dst) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("{}: {e}", dst.display())),
    }
    std::fs::write(dst, w.finish()).map_err(|e| format!("{}: {e}", dst.display()))?;
    Ok(Borrowed {
        from: plan.donor.clone(),
        files: out,
        known_pair: plan.pair.is_some(),
        approximate: true,
        note: note(set, &plan.donor, plan.pair.is_some()),
        why: plan.pair.map(|p| p.why.to_owned()),
        missing: plan.pair.map(|p| p.missing.to_owned()),
    })
}

/// The README.txt of a borrowed pack.
pub(crate) fn readme(set: &str, b: &Borrowed) -> String {
    let mut s = format!(
        "{set}: AltSound pack made by rom2altsound with a BORROWED sound ROM\r\n\r\n{}\r\n\r\n",
        b.note
    );
    for f in &b.files {
        s.push_str(&format!(
            "Borrowed: {} = {}'s {} ({} bytes, CRC32 {})\r\n",
            f.name, b.from, f.from_name, f.size, f.crc32
        ));
    }
    if let Some(w) = &b.why {
        s.push_str(&format!("\r\nWhy: {w}\r\n"));
    }
    if let Some(m) = &b.missing {
        s.push_str(&format!("\r\nMissing: {m}.\r\n"));
    }
    s.push_str("\r\nIf a dump of this game's own sound ROM turns up, extract it again without --sound-rom-from.\r\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rom(name: &str, size: u64, crc: Option<u32>, sound: bool, no_dump: bool) -> RomFile {
        RomFile {
            name: name.into(),
            size,
            crc,
            sha1: None,
            region: if sound { "cpu2" } else { "cpu1" }.into(),
            sound,
            optional: false,
            no_dump,
            bad_dump: false,
        }
    }

    fn driver(name: &str, roms: Vec<RomFile>) -> Driver {
        Driver {
            name: name.into(),
            parent: None,
            description: name.into(),
            year: "1987".into(),
            manufacturer: "Tecnoplay".into(),
            source: "wpc/techno.c".into(),
            roms,
            cpus: vec![],
            sound_chips: vec![],
            system: false,
            systems: vec![],
        }
    }

    fn zip(path: &Path, members: &[(&str, &[u8])]) {
        let mut w = zipread::Writer::default();
        for (n, d) in members {
            w.add(n, d);
        }
        std::fs::write(path, w.finish()).unwrap();
    }

    #[test]
    fn known_pair_is_staged_and_unknown_refused() {
        let dir = std::env::temp_dir().join(format!("r2a-borrow-{}", std::process::id()));
        let roms = dir.join("roms");
        std::fs::create_dir_all(&roms).unwrap();
        let snd: &[u8] = &[7u8; 64];
        let cpu: &[u8] = &[1u8; 32];
        zip(&roms.join("xforce.zip"), &[("ic15", cpu)]);
        zip(
            &roms.join("spcteam.zip"),
            &[("sound.bin", snd), ("cpu_top.bin", cpu)],
        );
        zip(&roms.join("other.zip"), &[("sound.bin", snd)]);
        let crc = zipread::crc32(snd);
        let drivers = vec![
            driver(
                "xforce",
                vec![
                    rom("ic15", 32, Some(zipread::crc32(cpu)), false, false),
                    rom("sound.bin", 64, None, true, true),
                ],
            ),
            driver(
                "spcteam",
                vec![rom("sound.bin", 64, Some(crc), true, false)],
            ),
            driver("other", vec![rom("sound.bin", 64, Some(crc), true, false)]),
        ];
        let dirs = vec![roms.clone()];
        // A set with its own sound ROM: nothing to borrow.
        assert!(
            plan(&drivers, "spcteam", "xforce", &dirs, false)
                .unwrap()
                .is_none()
        );
        // Not a known pair: refused, unless forced.
        let e = plan(&drivers, "xforce", "other", &dirs, false)
            .err()
            .unwrap();
        assert!(e.contains("not a known pair"), "{e}");
        assert!(
            plan(&drivers, "xforce", "other", &dirs, true)
                .unwrap()
                .is_some()
        );
        // The known pair: the set's files plus the donor's sound ROM, in the private zip.
        let p = plan(&drivers, "xforce", "spcteam", &dirs, false)
            .unwrap()
            .unwrap();
        let dst = dir.join("vpm/roms/xforce.zip");
        let b = stage(&p, "xforce", &roms.join("xforce.zip"), &dst).unwrap();
        assert!(b.known_pair && b.approximate && b.note.contains("approximate"));
        let names: Vec<String> = zipread::list(&dst)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(names, ["ic15", "sound.bin"]);
        // The user's zip is untouched.
        assert_eq!(zipread::list(&roms.join("xforce.zip")).unwrap().len(), 1);
        assert!(readme("xforce", &b).contains("BORROWED"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn built_in_names_parse() {
        for p in KNOWN {
            if let Some(n) = p.names {
                let n = crate::names::parse(n).unwrap();
                assert!(n.rows.len() > 10, "{}", p.set);
                assert_eq!(n.rom.as_deref(), Some(p.set));
            }
        }
    }
}
