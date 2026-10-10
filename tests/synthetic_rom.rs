//! Extract the synthetic test ROM set (tests/fixtures/s67s, a sound program written for
//! these tests, no original ROM code) with the freshly built binary, and check the pack
//! with tests/check_pack.py: the same check CI runs on every released binary.

use std::path::Path;
use std::process::Command;

/// The Python interpreter: `python3`, else `python` (Windows runners).
fn python() -> Option<&'static str> {
    ["python3", "python"].into_iter().find(|p| {
        Command::new(p)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    })
}

#[test]
fn synthetic_rom_extracts() {
    let Some(python) = python() else {
        eprintln!("skipped: no Python interpreter (python3 or python) to run check_pack.py");
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let work = Path::new(env!("CARGO_TARGET_TMPDIR")).join("synthetic_rom");
    let _ = std::fs::remove_dir_all(&work);
    let status = Command::new(python)
        .arg(root.join("tests").join("check_pack.py"))
        .arg("--bin")
        .arg(env!("CARGO_BIN_EXE_rom2altsound"))
        // A private PinMAME directory: nothing written to the user cache.
        .arg("--vpm")
        .arg(work.join("vpm"))
        .arg("--work")
        .arg(&work)
        .status()
        .expect("run check_pack.py");
    assert!(
        status.success(),
        "check_pack.py failed (its output is above)"
    );
}
