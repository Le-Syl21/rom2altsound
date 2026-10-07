//! Recognizes the master-volume commands a game sends to its sound board.
//!
//! The conventions are those of PinMAME's altsound preprocessor (src/wpc/altsound/snd_alt.cpp
//! `preprocess_commands`) and of sounds.dat's generic sections:
//! - DCS: `55 AA vv ~vv`, `vv` = 47..FF in steps of 08 = level 8..31 (`vv = level * 8 + 7`).
//!   `55 AB..B0 vv ~vv` set the mixing level of one DCS channel (rs_l6 fades channel AB
//!   from FF to 07 and back to FF during its boot); they are kept as `channel` levels.
//! - WPCS: `79 vv ~vv`.
//! - Whitestar BSMT2000 (Sega/Stern, from Apollo 13 on): `FE xx`, `xx` = 10 (31, loudest)
//!   to 2F (0), completed by `FD`. System 11 boards have no volume stage at all.
//! - Data East BSMT2000 (same board, same PinMAME type string "BSMT"): no master volume (a
//!   hardware pot), but the single bytes 20..2F set the music volume, 20 loudest and each
//!   step quieter. The stop `00` does not reset it, and some music tracks fade it out when
//!   they end by themselves (gnr_300 `10`, btmn_106 `01`). Measured on gnr_300 with music
//!   `30`: -15.7 LUFS after `20`, -46.1 after `10` ended; `21`..`2F` alone play nothing,
//!   except `2E`, which is also a loop on gnr_300.
//!
//! Measured on apollo13 (music 06, -54.4 LUFS after the game's own `FE 2C`): `FE 10 FD`
//! gives -17.0 LUFS, `FE 2C FD` -54.7, `FE 2F FD` silence. Without the `FD` (which the game
//! always sends right after) the board waits for it and swallows every later command, the
//! `00` stop included.

use serde::Serialize;

/// One volume command seen on a board.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct VolumeCmd {
    pub board: i32,
    pub family: String,
    /// "master", or "channel AB".."channel B0" for the DCS per-channel mixing levels.
    pub kind: String,
    /// The command bytes as sent, hex.
    pub bytes: String,
    /// What to send to restore this volume (after a board reset), hex.
    pub replay: String,
    /// The volume byte (`vv` or `xx`).
    pub value: u8,
    /// Level on the game's own scale, and that scale's maximum (31 on DCS and Whitestar).
    pub level: u32,
    pub level_max: u32,
    /// Emulated time of the last byte, in seconds.
    pub at: f64,
}

/// The byte that completes a Whitestar `FE xx` command.
pub const BSMT_END: u8 = 0xFD;
/// Data East music volume bytes, loudest first; `20` is also the board's own default.
pub const DE_MUSIC_VOLUME: std::ops::RangeInclusive<u8> = 0x20..=0x2F;

/// Recognizes a volume command at the end of one board's byte stream (`recent`: the last
/// bytes this board received, oldest first). `family` is the board's PinMAME type string.
pub fn decode(family: &str, board: i32, recent: &[u8], at: f64) -> Option<VolumeCmd> {
    let tail = |n: usize| recent.len().checked_sub(n).map(|i| &recent[i..]);
    let mut kind = "master".to_string();
    let (bytes, value, level, level_max) = match family {
        "DCS" => match *tail(4)? {
            [0x55, 0xAA, v, nv] if v == !nv => {
                (tail(4)?, v, u32::from(v.saturating_sub(7)) / 8, 31)
            }
            [0x55, ch @ 0xAB..=0xB0, v, nv] if v == !nv => {
                kind = format!("channel {ch:02X}");
                (tail(4)?, v, u32::from(v), 255)
            }
            _ => return None,
        },
        "WPCS" => match *tail(3)? {
            [0x79, v, nv] if v == !nv => (tail(3)?, v, u32::from(v), 255),
            _ => return None,
        },
        "BSMT" | "AT91" => match *tail(2)? {
            [0xFE, x] if (0x10..=0x2F).contains(&x) => (tail(2)?, x, u32::from(0x2F - x), 31),
            _ => return None,
        },
        _ => return None,
    };
    let hex = |b: &[u8]| -> String { b.iter().map(|b| format!("{b:02X}")).collect() };
    let replay = match family {
        "BSMT" | "AT91" => [bytes, &[BSMT_END]].concat(),
        _ => bytes.to_vec(),
    };
    Some(VolumeCmd {
        board,
        family: family.to_string(),
        kind,
        bytes: hex(bytes),
        replay: hex(&replay),
        value,
        level,
        level_max,
        at: (at * 1000.0).round() / 1000.0,
    })
}

/// Recognizes a Data East music volume byte (`20`..`2F`, level 15 down to 0).
pub fn decode_de_music(board: i32, byte: u8, at: f64) -> Option<VolumeCmd> {
    DE_MUSIC_VOLUME.contains(&byte).then(|| VolumeCmd {
        board,
        family: "BSMT".into(),
        kind: "music".into(),
        bytes: format!("{byte:02X}"),
        replay: format!("{byte:02X}"),
        value: byte,
        level: u32::from(0x2F - byte),
        level_max: 15,
        at: (at * 1000.0).round() / 1000.0,
    })
}

/// The manifest's `reference_volume` for a board whose output is always at full scale,
/// which is therefore its reference volume, or None for a board with a volume the tool can
/// set. `family` is a label from `Extractor::family_label`.
///
/// Checked in PinMAME's board code: the System 11 boards (wmssnd.c `s11s`, `s11cs`, `s11js`)
/// and Bally's Cheap Squeak and Turbo Cheap Squeak (by35snd.c `by45`, `byTCS`) write their
/// DACs, CVSD and YM2151 directly, with no volume register and no `mixer_set_volume`; Data
/// East's BSMT board (desound.c `de2s`) has its master volume on a pot in the power
/// junction box ("it was not done through the software"), its bytes `20`..`2F` being a
/// music level the game drives, set to the loudest (`20`) for the recordings. The early
/// Bally boards: the -32/-50 (`by32`) plays its tone at one level with its own decay; the
/// Sounds Plus -51 and -56 (`sp51`) leave the AY-3-8910 and the speech chip at their level
/// (the PIA's CB2 line, which PinMAME turns into a 75 % mute, stays low: vikingb and xenon
/// set it once, at reset).
///
/// The Squawk & Talk -61 has volume lines on its PIAs (four bits for the sounds, four for
/// the speech, by35snd.c "Sound volume", "Speech volume"), which the game sets with
/// commands (eballdlx `DF`..`FE`), but PinMAME does not emulate them: in emulation it is at
/// full scale whatever the game sends.
pub fn full_scale(family: &str) -> Option<&'static str> {
    if family.starts_with("WMSS11")
        || matches!(
            family,
            "BY45" | "BYTCS" | "BSMT (Data East)" | "BY32" | "BY51" | "BY56"
        )
    {
        Some(FULL_SCALE)
    } else if family == "BYSNT" {
        Some(FULL_SCALE_NOT_EMULATED)
    } else {
        None
    }
}

/// The manifest's `reference_volume` for a board without a volume stage.
pub const FULL_SCALE: &str = "full_scale (no volume stage)";
/// ...and for a board whose volume stage PinMAME does not emulate.
pub const FULL_SCALE_NOT_EMULATED: &str = "full_scale (volume lines not emulated in PinMAME)";

/// Why a family never reports a volume, for the summary.
pub fn none_reason(family: &str) -> &'static str {
    match family {
        "DCS" => {
            "the game sent no 55 AA vv ~vv during boot (the board keeps its reset default, 67)"
        }
        "WPCS" => "the game sent no 79 vv ~vv during boot",
        "BSMT" | "AT91" => "no FE 10..2F seen (Whitestar's master volume command)",
        "BSMT (Data East)" => "Data East sets the master volume with a hardware pot",
        f if f.starts_with("WMSS11") => "System 11 sound boards have no volume stage",
        "BY45" | "BYTCS" => "Cheap Squeak boards have no volume stage",
        "BY32" | "BY51" | "BY56" => "the early Bally sound boards have no volume stage",
        "BYSNT" => {
            "the Squawk & Talk's volume lines are not emulated in PinMAME (its commands DF..FE)"
        }
        _ => "no known volume command for this board family",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dcs() {
        let v = decode("DCS", 0, &[0x03, 0x55, 0xAA, 0x67, 0x98], 10.4).unwrap();
        assert_eq!((v.value, v.level, v.bytes.as_str()), (0x67, 12, "55AA6798"));
        assert_eq!(decode("DCS", 0, &[0x55, 0xAA, 0x67, 0x99], 0.0), None);
        assert_eq!(
            decode("DCS", 0, &[0x55, 0xAA, 0xFF, 0x00], 0.0)
                .unwrap()
                .level,
            31
        );
    }

    #[test]
    fn dcs_channel_level() {
        let v = decode("DCS", 0, &[0x55, 0xAB, 0xF7, 0x08], 0.0).unwrap();
        assert_eq!((v.kind.as_str(), v.level), ("channel AB", 0xF7));
    }

    #[test]
    fn whitestar() {
        let v = decode("BSMT", 1, &[0xFD, 0xFE, 0x2C], 1.0).unwrap();
        assert_eq!((v.value, v.level, v.replay.as_str()), (0x2C, 3, "FE2CFD"));
        assert_eq!(decode("BSMT", 1, &[0xFE, 0x10], 0.0).unwrap().level, 31);
        assert_eq!(decode("BSMT", 1, &[0xFE, 0x05], 0.0), None);
    }

    #[test]
    fn data_east_music() {
        let v = decode_de_music(1, 0x20, 1.183).unwrap();
        assert_eq!(
            (v.kind.as_str(), v.level, v.replay.as_str()),
            ("music", 15, "20")
        );
        assert_eq!(decode_de_music(1, 0x2F, 0.0).unwrap().level, 0);
        assert_eq!(decode_de_music(1, 0x30, 0.0), None);
    }

    #[test]
    fn full_scale_families() {
        for f in [
            "WMSS11",
            "WMSS11C",
            "WMSS11J",
            "BY45",
            "BYTCS",
            "BSMT (Data East)",
            "BY32",
            "BY51",
            "BY56",
            "BYSNT",
        ] {
            assert!(full_scale(f).is_some(), "{f}");
        }
        assert_eq!(full_scale("BY51"), Some(FULL_SCALE));
        assert_eq!(full_scale("BYSNT"), Some(FULL_SCALE_NOT_EMULATED));
        for f in ["DCS", "WPCS", "BSMT", "AT91"] {
            assert!(full_scale(f).is_none(), "{f}");
        }
    }

    #[test]
    fn system11_has_none() {
        assert_eq!(decode("WMSS11", 0, &[0x55, 0xAA, 0xFF, 0x00], 0.0), None);
    }
}
