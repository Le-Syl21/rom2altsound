//! What the window asks of the system: open a page or a folder, report a launch failure.

use std::path::Path;
use std::process::{Command, Stdio};

/// Opens a file (a listening page: in the web browser) or a folder (in the file manager).
pub fn open(path: &Path) {
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = Command::new(program)
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

/// Reports an error when there is no window to show it in: on stderr, and in a message box
/// (on Windows this program has no console).
pub fn alert(message: &str) {
    eprintln!("{message}");
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("rom2altsound")
        .set_description(message)
        .show();
}
