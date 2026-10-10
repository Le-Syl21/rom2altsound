//! The "Advanced options" section: every option of the command line program that the main
//! section does not show, read from its own definition (name, help, default), so that the
//! window never lags behind the command line.

use clap::ArgAction;

/// The options the main section shows in plain words (and the ROMs and the output, which
/// have their own sections).
const MAIN: &[&str] = &[
    "rom_args",
    "roms",
    "out",
    "jobs",
    "volume",
    "max_secs",
    "sounds_dat",
    "names",
    "sound_rom_from",
    "help",
    "version",
];

/// One advanced option and what the user set.
pub struct AdvOpt {
    /// `--name`, without the dashes.
    pub long: String,
    pub help: String,
    /// An on/off switch (else it takes a value).
    pub flag: bool,
    pub default: Option<String>,
    /// The values it takes, when it is a choice.
    pub choices: Vec<String>,
    pub on: bool,
    pub value: String,
}

/// Every option of the command line not in [`MAIN`] and not hidden (the hidden ones are
/// internal or diagnostics, not shown by `--help` either).
pub fn advanced() -> Vec<AdvOpt> {
    rom2altsound::command()
        .get_arguments()
        .filter(|a| !a.is_hide_set() && !MAIN.contains(&a.get_id().as_str()))
        .filter_map(|a| {
            let long = a.get_long()?.to_owned();
            let help = a
                .get_long_help()
                .or_else(|| a.get_help())
                .map(|h| h.to_string())
                .unwrap_or_default();
            Some(AdvOpt {
                long,
                help: help.split_whitespace().collect::<Vec<_>>().join(" "),
                flag: matches!(a.get_action(), ArgAction::SetTrue),
                default: a
                    .get_default_values()
                    .first()
                    .map(|v| v.to_string_lossy().into_owned()),
                choices: a
                    .get_possible_values()
                    .iter()
                    .map(|v| v.get_name().to_owned())
                    .collect(),
                on: false,
                value: String::new(),
            })
        })
        .collect()
}

/// The command line arguments of what the user set.
pub fn args(opts: &[AdvOpt]) -> Vec<String> {
    let mut out = Vec::new();
    for o in opts {
        if o.flag {
            if o.on {
                out.push(format!("--{}", o.long));
            }
        } else if !o.value.trim().is_empty() {
            out.push(format!("--{}", o.long));
            out.push(o.value.trim().to_owned());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_visible_option_is_offered_once() {
        let adv = advanced();
        let names: Vec<&str> = adv.iter().map(|o| o.long.as_str()).collect();
        for want in [
            "no-factory",
            "loop-max-secs",
            "merge-twins",
            "only",
            "dc-block",
        ] {
            assert!(names.contains(&want), "{want} missing from {names:?}");
        }
        for not in ["in-process", "out", "volume", "firmware-dir"] {
            assert!(!names.contains(&not), "{not} should not be offered");
        }
        let lms = adv.iter().find(|o| o.long == "loop-max-secs").unwrap();
        assert!(!lms.flag && lms.default.as_deref() == Some("240"));
        assert!(adv.iter().find(|o| o.long == "no-factory").unwrap().flag);
    }

    #[test]
    fn args_parse_back() {
        let mut adv = advanced();
        for o in &mut adv {
            match o.long.as_str() {
                "no-html" => o.on = true,
                "loop-max-secs" => o.value = " 30 ".into(),
                _ => {}
            }
        }
        let a = args(&adv);
        assert_eq!(a.len(), 3);
        assert!(a.contains(&"--no-html".to_owned()));
        let i = a.iter().position(|x| x == "--loop-max-secs").unwrap();
        assert_eq!(a[i + 1], "30");
        let mut full = vec!["afm_113b".to_owned()];
        full.extend(a);
        assert!(rom2altsound::parse_args(full).is_ok());
    }
}
