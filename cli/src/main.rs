//! The `rom2altsound` command line program (see the library for everything it does), with
//! the CED audio tagger installed.

fn main() {
    rom2altsound::tags::set_tagger(Box::new(rom2altsound_ced::tag));
    rom2altsound::cli_main();
}
