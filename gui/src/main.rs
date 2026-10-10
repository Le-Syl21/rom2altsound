//! `rom2altsound-gui`: the window program. Pick ROM zips, see what each one holds and how
//! well its sound board is handled, choose where the packs go, and make them.
//!
//! The extraction is the command line program's own engine: each ROM runs in a child
//! process of this program (PinMAME runs one machine per process), started with the
//! command line's arguments. So this program is also the command line when it is given
//! arguments; started with none, it opens the window.

#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod i18n;
mod options;
mod platform;

fn main() {
    // macOS (older versions) hands an app started from the Finder a "-psn_..." argument.
    let args: Vec<_> = std::env::args_os()
        .skip(1)
        .filter(|a| !a.to_string_lossy().starts_with("-psn_"))
        .collect();
    if !args.is_empty() {
        // A child process of an extraction (or the command line used through this program).
        rom2altsound::cli_main();
        return;
    }
    if let Err(e) = app::run() {
        platform::alert(&format!("rom2altsound: cannot open the window: {e}"));
        std::process::exit(1);
    }
}
