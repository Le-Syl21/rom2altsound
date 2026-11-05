//! Tags every WAV of the folders given (a check of `ced` against another runtime).
use std::path::Path;

fn main() {
    for d in std::env::args().skip(1) {
        let mut files: Vec<_> = std::fs::read_dir(&d)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "wav"))
            .collect();
        files.sort();
        for f in files {
            let t = rom2altsound::ced::tag(Path::new(&f));
            let name = f.file_name().unwrap().to_string_lossy().into_owned();
            match t {
                Some(t) => println!(
                    "{}\t{name}\t{:.3}\t{:.3}\t{}",
                    Path::new(&d).file_name().unwrap().to_string_lossy(),
                    t.music,
                    t.speech,
                    t.top
                        .iter()
                        .map(|(n, p)| format!("{n} {p:.2}"))
                        .collect::<Vec<_>>()
                        .join("; ")
                ),
                None => println!("{name}\tFAILED"),
            }
        }
    }
}
