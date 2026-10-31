//! Files a set needs that sit elsewhere in the ROM folder, under other names.
//!
//! PinMAME loads each ROM file by its name, from the set's zip, its parent's and its
//! system sets'. A collection does not always have them that way: MAME's Pinball 2000
//! sets hold the sound ROMs of `swep1_150` (never downloaded on its own) in `swe1pb.zip`,
//! its flash as `28f800.bin` where PinMAME wants `swe1_28f800.rom`. A file none of the
//! set's zips holds under its name is looked up by CRC32 and size in every zip of the ROM
//! folder, and the set's zip is rebuilt in the private PinMAME directory with it, under
//! PinMAME's name. The user's zips are only read.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::drivers::Driver;
use crate::zipread;

/// A file added to the set's private zip.
#[derive(Debug, Clone)]
pub(crate) struct Filled {
    /// PinMAME's name for it.
    pub name: String,
    /// The zip it was found in, and its name there.
    pub from: PathBuf,
    pub from_name: String,
    pub crc32: u32,
}

/// The sets whose zips PinMAME looks a file of `set` up in: the set, its parent, and
/// their system sets.
fn chain<'a>(drivers: &'a [Driver], set: &str) -> Vec<&'a Driver> {
    let by_name = |n: &str| drivers.iter().find(|d| d.name == n);
    let mut out: Vec<&Driver> = Vec::new();
    let push = |d: &'a Driver, out: &mut Vec<&'a Driver>| {
        if !out.iter().any(|o| o.name == d.name) {
            out.push(d);
        }
    };
    let Some(d) = by_name(set) else {
        return out;
    };
    push(d, &mut out);
    let parent = d.parent.as_deref().and_then(by_name);
    if let Some(p) = parent {
        push(p, &mut out);
    }
    for owner in std::iter::once(d).chain(parent) {
        for s in &owner.systems {
            if let Some(s) = by_name(s) {
                push(s, &mut out);
            }
        }
    }
    out
}

/// The files of `set` that none of its zips in `roms` holds under PinMAME's name.
fn missing<'a>(drivers: &'a [Driver], roms: &Path, set: &str) -> Vec<&'a crate::drivers::RomFile> {
    let chain = chain(drivers, set);
    let Some(d) = chain.first() else {
        return Vec::new();
    };
    let mut have = HashSet::new();
    for c in &chain {
        if let Ok(members) = zipread::list(&roms.join(format!("{}.zip", c.name))) {
            have.extend(members.into_iter().map(|m| m.name.to_ascii_lowercase()));
        }
    }
    d.required()
        .map(|(_, r)| r)
        .filter(|r| !have.contains(&r.name.to_ascii_lowercase()))
        .collect()
}

/// Rebuilds `<vpm_roms>/<set>.zip` with the files of `set` found by CRC elsewhere in
/// `roms`, when some are missing. Its base is the private zip if one was already written
/// (a borrowed sound ROM), else the user's zip of the set (if any). Returns what was added.
pub(crate) fn fill(
    drivers: &[Driver],
    roms: &Path,
    vpm_roms: &Path,
    set: &str,
) -> Result<Vec<Filled>, String> {
    let wanted = missing(drivers, roms, set);
    if wanted.is_empty() {
        return Ok(Vec::new());
    }
    let keys: HashSet<(u32, u64)> = wanted
        .iter()
        .filter_map(|r| Some((r.crc?, r.size)))
        .collect();
    let mut index: HashMap<(u32, u64), (PathBuf, zipread::Entry)> = HashMap::new();
    let mut zips: Vec<PathBuf> = std::fs::read_dir(roms)
        .map_err(|e| format!("{}: {e}", roms.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("zip")))
        .collect();
    zips.sort(); // the same donor from one run to the next
    for z in zips {
        let Ok(members) = zipread::list(&z) else {
            continue;
        };
        for m in members {
            let k = (m.crc32, m.size);
            if keys.contains(&k) {
                index.entry(k).or_insert_with(|| (z.clone(), m));
            }
        }
        if index.len() == keys.len() {
            break;
        }
    }
    let found: Vec<_> = wanted
        .iter()
        .filter_map(|r| Some((*r, index.get(&(r.crc?, r.size))?)))
        .collect();
    if found.is_empty() {
        return Ok(Vec::new());
    }

    let dst = vpm_roms.join(format!("{set}.zip"));
    let base = if dst.is_file() && !dst.is_symlink() {
        Some(dst.clone())
    } else {
        let z = roms.join(format!("{set}.zip"));
        z.is_file().then_some(z)
    };
    let mut w = zipread::Writer::default();
    if let Some(base) = &base {
        let raw: Vec<_> = zipread::list(base)?
            .into_iter()
            .map(|e| zipread::read_raw(base, &e).map(|r| (e, r)))
            .collect::<Result<_, _>>()?;
        for (e, r) in raw {
            w.add_raw(&e.name, e.method, e.crc32, e.size, &r);
        }
    }
    let mut out = Vec::new();
    for (r, (zip, m)) in found {
        let raw = zipread::read_raw(zip, m)?;
        w.add_raw(&r.name, m.method, m.crc32, m.size, &raw);
        out.push(Filled {
            name: r.name.clone(),
            from: zip.clone(),
            from_name: m.name.clone(),
            crc32: m.crc32,
        });
    }
    std::fs::create_dir_all(vpm_roms).map_err(|e| format!("{}: {e}", vpm_roms.display()))?;
    // A link to the user's zip may be there: remove the link, never write through it.
    match std::fs::remove_file(&dst) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("{}: {e}", dst.display())),
    }
    std::fs::write(&dst, w.finish()).map_err(|e| format!("{}: {e}", dst.display()))?;
    Ok(out)
}
