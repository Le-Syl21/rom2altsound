//! Extract one ROM with the library and list its sounds: `cargo run --release --example pack
//! -- <rom.zip|set> <out dir>`. A program that embeds rom2altsound does as this one: the
//! worker check first thing in `main`.

fn main() -> Result<(), String> {
    if rom2altsound::run_worker_if_requested() {
        return Ok(());
    }
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        return Err("usage: pack <rom.zip|set> <out dir>".into());
    }
    let pack = rom2altsound::extract(&args[1], args[2].as_ref(), &Default::default())?;
    println!(
        "{}: {} sounds at {} Hz in {}",
        pack.rom,
        pack.sounds.len(),
        pack.sample_rate,
        pack.dir.display()
    );
    for s in pack.sounds.iter().take(10) {
        let n = s.samples().map(|x| x.data.len()).unwrap_or(0);
        println!(
            "  {} {:?} {:.2} s, {n} samples, loop {:?}",
            s.id, s.name, s.duration, s.loop_points
        );
    }
    Ok(())
}
