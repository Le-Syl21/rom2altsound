//! Raw bindings: the public libpinmame API, a few global PinMAME internals reachable
//! because the library is linked statically, and the C shim.

use std::ffi::{c_char, c_int, c_uint, c_void};

pub const PINMAME_MAX_PATH: usize = 512;
pub const AUDIO_FORMAT_INT16: c_int = 0;
pub const STATUS_OK: c_int = 0;
pub const SNDBRD_DOUBLECMD: c_uint = 0x0010;
/// src/wpc/gen.h: the Data East generations (alphanumeric, 128x16, 128x32 and 192x64 DMD).
pub const GEN_DATA_EAST: u64 = 0x1000 | 0x2000 | 0x4000 | 0x8000;

/// `PinmameAudioInfo` (src/libpinmame/libpinmame.h).
#[repr(C)]
pub struct AudioInfo {
    pub format: c_int,
    pub channels: c_int,
    pub sample_rate: f64,
    pub frames_per_second: f64,
    pub samples_per_frame: c_int,
    pub buffer_size: c_int,
}

/// `PinmameGame`.
#[repr(C)]
pub struct Game {
    pub name: *const c_char,
    pub clone_of: *const c_char,
    pub description: *const c_char,
    pub year: *const c_char,
    pub manufacturer: *const c_char,
    pub flags: u32,
    pub found: i32,
}

type Cb = Option<unsafe extern "C" fn()>;
pub type AudioAvailableCb = unsafe extern "C" fn(*mut AudioInfo, *mut c_void) -> c_int;
pub type AudioUpdatedCb = unsafe extern "C" fn(*mut c_void, c_int, *mut c_void) -> c_int;
pub type SoundCommandCb = unsafe extern "C" fn(c_int, c_int, *mut c_void);
pub type StateUpdatedCb = unsafe extern "C" fn(c_int, *mut c_void);
pub type GameCb = unsafe extern "C" fn(*mut Game, *mut c_void);
/// `PinmameIsKeyPressedCallback`: a `PINMAME_KEYCODE`, 1 when pressed.
pub type IsKeyPressedCb = unsafe extern "C" fn(c_int, *mut c_void) -> c_int;
/// `PINMAME_KEYCODE` (src/libpinmame/libpinmame.h).
pub const KEYCODE_NUMBER_8: c_int = 34;
pub const KEYCODE_NUMBER_9: c_int = 35;
pub const KEYCODE_END: c_int = 78;

/// `PinmameConfig`. Unused callbacks are left null; libpinmame checks every one.
#[repr(C)]
pub struct Config {
    pub audio_format: c_int,
    pub sample_rate: c_int,
    pub vpm_path: [c_char; PINMAME_MAX_PATH],
    pub on_state_updated: Option<StateUpdatedCb>,
    pub on_display_available: Cb,
    pub on_display_updated: Cb,
    pub on_audio_available: Option<AudioAvailableCb>,
    pub on_audio_updated: Option<AudioUpdatedCb>,
    pub on_mech_available: Cb,
    pub on_mech_updated: Cb,
    pub on_solenoid_updated: Cb,
    pub on_console_data_updated: Cb,
    pub is_key_pressed: Option<IsKeyPressedCb>,
    pub on_log_message: *const c_void,
    pub on_sound_command: Option<SoundCommandCb>,
}

unsafe extern "C" {
    pub fn PinmameSetConfig(config: *const Config);
    pub fn PinmameGetGame(name: *const c_char, cb: GameCb, user: *mut c_void) -> c_int;
    pub fn PinmameRun(name: *const c_char) -> c_int;
    pub fn PinmameIsRunning() -> c_int;
    pub fn PinmameStop();
    pub fn PinmameSetHandleKeyboard(handle: c_int);

    /// src/libpinmame/video.c: when 0 the emulation runs as fast as the host allows.
    pub static mut throttle: c_int;

    /// src/wpc/sndbrd.c
    pub fn sndbrd_exists(board: c_int) -> c_int;
    pub fn sndbrd_typestr(board: c_int) -> *const c_char;
    pub fn sndbrd_manCmd(board: c_int, cmd: c_int);
    pub fn sndbrd_data_w(board: c_int, data: c_int);
    pub fn sndbrd_ctrl_w(board: c_int, data: c_int);

    /// shim/shim.c
    pub fn shim_board_has_mancmd(board: c_int) -> c_int;
    pub fn shim_board_flags(board: c_int) -> c_uint;
    pub fn shim_halt_game_cpus(halt: c_int) -> c_int;
    pub fn shim_reset_audio_cpus() -> c_int;
    pub fn shim_sound_region(len: *mut c_uint) -> *const u8;
    pub fn shim_game_gen() -> u64;
    pub fn shim_has_bsmt2000() -> c_int;
    pub fn shim_sam_hook_dac() -> c_int;
    pub fn shim_sam_dac_count() -> c_int;
    pub fn shim_sam_dac_get(i: c_int, at: *mut f64, reg: *mut u8, val: *mut u8) -> c_int;
    pub fn shim_audio_cpu(i: c_int) -> c_int;
    pub fn shim_cpu_ram_ranges(
        cpu: c_int,
        start: *mut c_uint,
        end: *mut c_uint,
        max: c_int,
    ) -> c_int;
    pub fn shim_cpu_read(cpu: c_int, addr: c_uint, len: c_uint, out: *mut u8) -> c_int;
    pub fn shim_cpu_reg(cpu: c_int, reg: c_int) -> c_uint;
    pub static mut shim_log_min_level: c_int;
    pub fn shim_log(level: c_int, format: *const c_char, args: *mut c_void, user: *mut c_void);
}

/// `PinmameConfig::vpm_path`: the directory with a trailing separator (libpinmame
/// appends "roms", "nvram"... as is).
pub fn vpm_path(vpm: &std::path::Path) -> [c_char; PINMAME_MAX_PATH] {
    let mut out = [0 as c_char; PINMAME_MAX_PATH];
    let p = format!("{}/", vpm.display());
    for (d, s) in out.iter_mut().zip(p.bytes().take(PINMAME_MAX_PATH - 1)) {
        *d = s as c_char;
    }
    out
}

/// PinMAME's first sound ROM region (DCS: U2 at offset 0), if the machine has one.
/// Only valid while the emulation runs; read it from the emulation thread.
pub fn sound_region() -> Option<&'static [u8]> {
    let mut len: c_uint = 0;
    let p = unsafe { shim_sound_region(&mut len) };
    (!p.is_null() && len > 0).then(|| unsafe { std::slice::from_raw_parts(p, len as usize) })
}

/// True when the running game is a Data East machine (its BSMT board takes 20..2F as a
/// music volume, where Whitestar uses `FE xx FD`).
pub fn is_data_east() -> bool {
    let generation = unsafe { shim_game_gen() };
    generation & GEN_DATA_EAST != 0
}

/// True when the machine has a BSMT2000 sound chip (only valid while the emulation runs).
pub fn has_bsmt2000() -> bool {
    unsafe { shim_has_bsmt2000() != 0 }
}

/// Converts a nullable C string to an owned `String`.
pub fn cstr(p: *const c_char) -> Option<String> {
    (!p.is_null()).then(|| {
        unsafe { std::ffi::CStr::from_ptr(p) }
            .to_string_lossy()
            .into_owned()
    })
}
