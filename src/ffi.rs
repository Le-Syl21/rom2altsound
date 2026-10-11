//! Raw bindings: the public libpinmame API, a few global PinMAME internals reachable
//! because the library is linked statically, and the C shim.

use std::ffi::{c_char, c_int, c_uint, c_void};

pub const PINMAME_MAX_PATH: usize = 512;
pub const AUDIO_FORMAT_INT16: c_int = 0;
pub const STATUS_OK: c_int = 0;
pub const SNDBRD_DOUBLECMD: c_uint = 0x0010;
/// src/wpc/sndbrd.h: `SNDBRD_TYPE(3, 3)`, the Pinball 2000 DCS2 board.
pub const SNDBRD_DCSP2K: c_int = 0x0303;
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

/// `struct shim_rom` (shim/shim.c): one ROM file of a driver.
#[repr(C)]
pub struct ShimRom {
    pub name: *const c_char,
    pub length: c_uint,
    pub region: c_uint,
    pub region_flags: c_uint,
    pub sound_only: c_int,
    pub optional: c_int,
    pub no_dump: c_int,
    pub bad_dump: c_int,
    pub bios: c_int,
    pub crc: [c_char; 16],
    pub sha1: [c_char; 48],
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
    /// A playfield switch, by PinMAME's switch number (`vp_putSwitch`).
    pub fn PinmameSetSwitch(sw: c_int, state: c_int);
    /// A DIP switch bank (`vp_setDIP`), read by the drivers through `core_getDip`.
    pub fn PinmameSetDIP(bank: c_int, value: c_int);
    pub fn PinmameGetDIP(bank: c_int) -> c_int;

    /// src/libpinmame/video.c: when 0 the emulation runs as fast as the host allows.
    pub static mut throttle: c_int;

    /// src/sound/mixer.c: the mixer's channels (one per chip output, `MIXER_MAX_CHANNELS`),
    /// by name ("YM2151 #0 Ch1", "DAC #0", "HC55516 #0"...), and their mixing level (0..100).
    pub fn mixer_get_name(ch: c_int) -> *const c_char;
    pub fn mixer_get_mixing_level(ch: c_int) -> c_int;
    pub fn mixer_set_mixing_level(ch: c_int, level: c_int);

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
    pub fn shim_reset_sound_chips();
    pub fn shim_sound_region(len: *mut c_uint) -> *const u8;
    pub fn shim_game_gen() -> u64;
    pub fn shim_has_bsmt2000() -> c_int;
    pub fn shim_dac_ac_couple() -> c_int;
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
    pub fn shim_cpu_region(cpu: c_int, len: *mut c_uint) -> *const u8;
    pub fn shim_region_after_cpu1(k: c_int, len: *mut c_uint) -> *const u8;
    pub fn shim_game_cpu() -> c_int;
    pub fn shim_game_pokes(cpu: c_int, addr: *const c_uint, data: *const u8, n: c_int);
    pub fn shim_m68k_call(
        cpu: c_int,
        code: *const u8,
        len: c_int,
        lock: c_uint,
        busy_lo: c_uint,
        busy_hi: c_uint,
    ) -> c_int;
    pub fn shim_user1_region(len: *mut c_uint) -> *const u8;
    pub fn shim_board_type(board: c_int) -> c_int;
    pub fn shim_nibble_hook(board: c_int) -> c_int;
    pub fn shim_nibble_cmd(board: c_int, data: c_int);
    pub fn shim_nibble_reads() -> c_int;
    pub fn shim_nibble_after(n: c_int);
    pub fn shim_spinb_own(board: c_int) -> c_int;
    pub fn shim_spinb_latch_cpus() -> c_int;
    pub fn shim_inder_cpu_sound() -> c_int;
    pub fn shim_inder_idle_hook(idle: c_int, keep: u64) -> c_int;
    pub fn shim_by45_p21(on: c_int);
    pub fn shim_mancmd_pairs(board: c_int, a: c_int, b: c_int, n: c_int, slices: c_int);
    pub fn shim_trace_hook(n: c_int, start: c_uint, end: c_uint) -> c_int;
    pub fn shim_trace_get(
        i: c_uint,
        at: *mut f64,
        pc: *mut c_uint,
        addr: *mut c_uint,
        data: *mut c_uint,
        write: *mut c_int,
    ) -> c_int;
    pub fn shim_trace_count(lost: *mut c_uint) -> c_uint;
    pub fn shim_data_burst(board: c_int, bytes: *const u8, n: c_int) -> c_int;
    pub fn shim_p2k_hook() -> c_int;
    pub fn shim_p2k_take(
        at: *mut f64,
        word: *mut u16,
        reply: *mut u8,
        max: c_int,
        lost: *mut c_uint,
    ) -> c_int;
    pub fn shim_p2k_word(word: u16);
    pub fn shim_p2k_take_reply() -> c_int;
    pub fn shim_driver_count() -> c_int;
    pub fn shim_driver_nvram(name: *const c_char) -> c_int;
    pub fn shim_driver_text(i: c_int, field: c_int) -> *const c_char;
    pub fn shim_driver_flags(i: c_int) -> c_uint;
    pub fn shim_driver_rom(i: c_int, j: c_int, out: *mut ShimRom) -> c_int;
    pub fn shim_driver_machine(i: c_int, audio_mask: *mut c_uint) -> c_int;
    pub fn shim_machine_cpu(k: c_int) -> *const c_char;
    pub fn shim_machine_sound(k: c_int) -> *const c_char;
    pub fn shim_print_driver_boards(start: c_int);
    pub fn shim_sndbrd_name(board: c_uint) -> *const c_char;
    pub fn shim_region_name(region: c_uint) -> *const c_char;
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

/// The memory region of CPU `cpu` (where its program is loaded), if it has one. Only valid
/// while the emulation runs; read it from the emulation thread.
pub fn cpu_region(cpu: c_int) -> Option<&'static [u8]> {
    let mut len: c_uint = 0;
    let p = unsafe { shim_cpu_region(cpu, &mut len) };
    (!p.is_null() && len > 0).then(|| unsafe { std::slice::from_raw_parts(p, len as usize) })
}

/// The memory region `REGION_CPU1 + k`, with or without a CPU running from it (the
/// ST300V's speech ROM, `VSU100_ROMREGION`). Only valid while the emulation runs.
pub fn region_after_cpu1(k: c_int) -> Option<&'static [u8]> {
    let mut len: c_uint = 0;
    let p = unsafe { shim_region_after_cpu1(k, &mut len) };
    (!p.is_null() && len > 0).then(|| unsafe { std::slice::from_raw_parts(p, len as usize) })
}

/// PinMAME's first user region (`REGION_USER1`), where Capcom's drivers load the game's
/// program before copying it to the CPU's address space. Only valid while the emulation
/// runs.
pub fn user1_region() -> Option<&'static [u8]> {
    let mut len: c_uint = 0;
    let p = unsafe { shim_user1_region(&mut len) };
    (!p.is_null() && len > 0).then(|| unsafe { std::slice::from_raw_parts(p, len as usize) })
}

/// The DCS ROM the track catalog is read in (`dcsrom`): PinMAME's sound region on WPC DCS
/// boards; on Pinball 2000 its byte image (`dcsrom::p2k_image`), made once (one machine
/// per process). Only valid while the emulation runs.
pub fn dcs_rom() -> Option<&'static [u8]> {
    static P2K: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    let region = sound_region()?;
    if unsafe { shim_board_type(0) } != SNDBRD_DCSP2K {
        return Some(region);
    }
    Some(P2K.get_or_init(|| crate::dcsrom::p2k_image(region)))
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

/// `MIXER_MAX_CHANNELS` (src/sound/mixer.h).
pub const MIXER_MAX_CHANNELS: c_int = 25;

/// The mixer's channels in use: (number, name, mixing level).
pub fn mixer_channels() -> Vec<(c_int, String, c_int)> {
    (0..MIXER_MAX_CHANNELS)
        .filter_map(|ch| {
            let name = cstr(unsafe { mixer_get_name(ch) })?;
            Some((ch, name, unsafe { mixer_get_mixing_level(ch) }))
        })
        .collect()
}

/// Converts a nullable C string to an owned `String`.
pub fn cstr(p: *const c_char) -> Option<String> {
    (!p.is_null()).then(|| {
        unsafe { std::ffi::CStr::from_ptr(p) }
            .to_string_lossy()
            .into_owned()
    })
}

/// One word between a Pinball 2000 game and its DCS2 board (`shim_p2k_hook`).
#[derive(Clone, Copy, Debug)]
pub struct P2kWord {
    /// Emulated time (s).
    pub at: f64,
    pub word: u16,
    /// A reply of the board's DSP (else a word the DSP took from the host).
    pub reply: bool,
}

/// The words logged since the last call, and how many did not fit in the shim's log.
pub fn p2k_take() -> (Vec<P2kWord>, u32) {
    const MAX: usize = 4096;
    let (mut at, mut word, mut reply) = (vec![0f64; MAX], vec![0u16; MAX], vec![0u8; MAX]);
    let mut out = Vec::new();
    let mut lost_all = 0;
    loop {
        let mut lost: c_uint = 0;
        let n = unsafe {
            shim_p2k_take(
                at.as_mut_ptr(),
                word.as_mut_ptr(),
                reply.as_mut_ptr(),
                MAX as c_int,
                &mut lost,
            )
        } as usize;
        lost_all += lost;
        out.extend((0..n).map(|i| P2kWord {
            at: at[i],
            word: word[i],
            reply: reply[i] != 0,
        }));
        if n < MAX {
            return (out, lost_all);
        }
    }
}
