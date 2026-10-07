//! The listening page: an `index.html` in each ROM folder that plays every extracted
//! sound from disk (file://, no server), with what `manifest.json` says about it; and, for
//! a batch, an `index.html` at the output root linking each ROM's page.
//!
//! The page is `listen.html` (inline CSS and JS, no external resource) with the data
//! spliced in as JSON in a `<script type="application/json">`: a page opened from disk
//! cannot fetch `manifest.json`. Every `<` of the JSON is written `<`, so nothing in
//! the data (a sound name, a path) can close the script element.

use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// The page's file name, in each ROM folder and at a batch's root.
pub const PAGE: &str = "index.html";
const TEMPLATE: &str = include_str!("listen.html");
/// Marks our pages: a batch index replaces an `index.html` at the root only if it has it.
const MARKER: &str = r#"<meta name="generator" content="rom2altsound">"#;
const DATA_SLOT: &str = "/*DATA*/";
const TITLE_SLOT: &str = "<!--TITLE-->";

/// Writes `<out_dir>/index.html` from `<out_dir>/manifest.json`; returns its path.
pub fn write_page(out_dir: &Path) -> Result<PathBuf, String> {
    let mpath = out_dir.join("manifest.json");
    let text = std::fs::read_to_string(&mpath).map_err(|e| format!("{}: {e}", mpath.display()))?;
    let m: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", mpath.display()))?;
    let data = page_data(&m, |f| out_dir.join(f).is_file());
    let rom = m["rom"].as_str().unwrap_or("rom");
    let path = out_dir.join(PAGE);
    std::fs::write(&path, render(&format!("{rom}: sounds"), &data))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// `rom2altsound page <folder>...`: writes the page again in existing ROM folders (from
/// their `manifest.json`), then the index of their parent folder.
pub fn page_cli(dirs: Vec<String>) {
    let mut roots = Vec::new();
    for d in &dirs {
        let dir = Path::new(d);
        match write_page(dir) {
            Ok(p) => println!("{}", p.display()),
            Err(e) => eprintln!("{d}: {e}"),
        }
        let root = std::path::absolute(dir)
            .ok()
            .and_then(|a| a.parent().map(Path::to_path_buf));
        if let Some(r) = root
            && !roots.contains(&r)
        {
            roots.push(r);
        }
    }
    for r in roots {
        if let Ok(Some(p)) = write_index(&r) {
            println!("{}", p.display());
        }
    }
}

/// Writes `<root>/index.html` linking every ROM folder under `root` that has a page of
/// ours, when there are at least two; never replaces an `index.html` we did not write.
/// Returns its path when written.
pub fn write_index(root: &Path) -> Result<Option<PathBuf>, String> {
    let path = root.join(PAGE);
    if let Ok(old) = std::fs::read_to_string(&path)
        && !old.contains(MARKER)
    {
        return Ok(None);
    }
    let mut roms = Vec::new();
    let entries = std::fs::read_dir(root).map_err(|e| format!("{}: {e}", root.display()))?;
    for e in entries.flatten() {
        let dir = e.path();
        let ours = std::fs::read_to_string(dir.join(PAGE)).is_ok_and(|t| t.contains(MARKER));
        let m: Option<Value> = std::fs::read_to_string(dir.join("manifest.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok());
        if let (true, Some(m), Some(name)) = (ours, m, dir.file_name()) {
            let name = name.to_string_lossy();
            roms.push(json!({
                "dir": name,
                "page": format!("{name}/{PAGE}"),
                "rom": m["rom"],
                "boards": m["boards"],
                "written": m["counts"]["written"],
                "loops": m["counts"]["loops"],
                "lufs": m["loudness"]["all"]["lufs"],
                "factory_offset_db": m["factory_offset_db"],
            }));
        }
    }
    if roms.len() < 2 {
        return Ok(None);
    }
    roms.sort_by(|a, b| a["dir"].as_str().cmp(&b["dir"].as_str()));
    let data = json!({ "version": env!("CARGO_PKG_VERSION"), "roms": roms });
    std::fs::write(&path, render("rom2altsound packs", &data))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Some(path))
}

/// The template with the title and the data in.
fn render(title: &str, data: &Value) -> String {
    TEMPLATE
        .replacen(TITLE_SLOT, &escape_html(title), 1)
        .replacen(DATA_SLOT, &script_json(data), 1)
}

/// JSON that is safe inside a `<script>` element: `<` only appears inside strings in
/// JSON, where `<` reads back the same.
fn script_json(v: &Value) -> String {
    serde_json::to_string(v).unwrap().replace('<', "\\u003c")
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// What the page shows, from the manifest; `exists` says whether a file of the ROM folder
/// is there (only files on disk get a play button).
fn page_data(m: &Value, exists: impl Fn(&str) -> bool) -> Value {
    let rate = m["sample_rate"].as_f64().unwrap_or(44100.0);
    let sounds: Vec<Value> = m["sounds"]
        .as_array()
        .map(|list| list.iter().map(|s| sound(s, rate, &exists)).collect())
        .unwrap_or_default();
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "rom": m["rom"],
        "parent": m["parent"],
        "boards": m["boards"],
        "mode": m["mode"],
        "volume": volume_line(m),
        "levels_note": m["levels_note"],
        "lufs": m["loudness"]["all"]["lufs"],
        "counts": m["counts"],
        "sounds": sounds,
    })
}

/// "Recorded at ...; factory volume ..., offset ... dB", from the reference mode fields.
fn volume_line(m: &Value) -> Value {
    let reference = m["reference_volume"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| m["volume_init"].as_str().map(str::to_owned))
        .or_else(|| {
            (m["boards"][0].as_str()?.starts_with("SAM"))
                .then(|| "full scale (DAC at FF)".to_owned())
        });
    let fo = &m["factory_offset"];
    let mut factory: Vec<String> = fo["boards"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|b| b["factory_volume"].as_str().map(str::to_owned))
        .collect();
    if let Some(f) = fo["factory_volume"].as_str() {
        factory.push(f.to_owned());
    }
    json!({
        "reference": reference,
        "factory": (!factory.is_empty()).then(|| factory.join(", ")),
        "offset_db": m["factory_offset_db"],
    })
}

fn sound(s: &Value, rate: f64, exists: &impl Fn(&str) -> bool) -> Value {
    let rate = s["sample_rate"].as_f64().unwrap_or(rate);
    let lp = &s["loop"];
    let pack = &s["pack"];
    // Every file of this sound that is on disk: the recording, the loop body, the
    // extended file, and the file the pack plays (a merged twin's original).
    let mut files: Vec<(&str, String)> = Vec::new();
    let mut add = |label: &'static str, f: &Value| {
        if let Some(f) = f.as_str()
            && !files.iter().any(|(_, g)| g == f)
            && exists(f)
        {
            files.push((label, f.to_owned()));
        }
    };
    add("play", &s["file"]);
    add("loop", &lp["loop_file"]);
    add("extended", &lp["extended_file"]);
    if let Some(f) = s["file"].as_str() {
        add("extended", &json!(crate::altsound::extended_name(f)));
    }
    add("pack", &pack["file"]);
    let played = pack["file"].as_str();
    let files: Vec<Value> = files
        .into_iter()
        .map(|(label, path)| {
            let pack = played == Some(path.as_str());
            json!({ "label": label, "path": path, "pack": pack })
        })
        .collect();

    let mut flags = Vec::new();
    if s["clipped_samples"].as_u64().unwrap_or(0) > 0 {
        flags.push(format!("clipped ({})", s["clipped_samples"]));
    }
    if s["blip"].as_bool() == Some(true) {
        flags.push("blip".into());
    }
    if s["ended_by"] == "no_sound" {
        flags.push("silent".into());
    }
    if s["clean_start"].as_bool() == Some(false) {
        flags.push("not clean".into());
    }
    if s["ignores_master_volume"].as_bool() == Some(true) {
        flags.push("ignores master volume".into());
    }
    if s["ended_by"] == "max" {
        flags.push("cut at max".into());
    }
    if s.get("loop_unresolved").is_some() {
        flags.push("loop unresolved".into());
    }

    let lp_out = lp.is_object().then(|| {
        json!({
            "method": lp["method"],
            "period_secs": lp["period_secs"],
            "intro_secs": lp["intro_samples"].as_f64().map(|n| (n / rate * 1000.0).round() / 1000.0),
            "confidence": lp["confidence"],
        })
    });
    let pack_out = pack.is_object().then(|| {
        json!({
            "channel": pack["channel"],
            "type": pack["gsound_type"],
            "duck": pack["duck"],
            "stop": pack["stop"],
            "kind": pack["file_kind"],
            "reason": pack["file_reason"],
            "calls": pack["calls"],
        })
    });
    json!({
        "id": s["id"],
        "name": s["name"],
        "board": s["board"],
        "duration": s["duration"],
        "lufs": s["lufs"],
        "true_peak": s["true_peak_dbtp"],
        "peak": s["peak_dbfs"],
        "ended_by": s["ended_by"],
        "files": files,
        "loop": lp_out,
        "loop_unresolved": s["loop_unresolved"],
        "pack": pack_out,
        "dcs_channel": s["dcs"]["channel"],
        "chip": s["mix"]["chip"],
        "twin_of": s["twin_of"],
        "flags": flags,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> Value {
        json!({
            "rom": "test_l1",
            "boards": ["DCS"],
            "sample_rate": 44100,
            "reference_volume": "55AAEF10",
            "factory_offset_db": -22.45,
            "factory_offset": { "boards": [{ "factory_volume": "55AA6798" }] },
            "sounds": [
                {
                    "id": "0x0001", "name": "Music: </script><script>alert(1)</script> <!--",
                    "file": "0x0001-test_l1.wav", "duration": 4.6, "lufs": -24.2,
                    "true_peak_dbtp": -10.0, "peak_dbfs": -10.0, "ended_by": "loop",
                    "clipped_samples": 3, "blip": false,
                    "loop": { "method": "dcs-catalog", "period_secs": 4.0, "intro_samples": 44100,
                              "loop_file": "0x0001-test_l1-loop.wav" },
                    "pack": { "channel": 0, "gsound_type": "music", "duck": 100, "stop": 0,
                              "file": "0x0001-test_l1-loop.wav" },
                    "dcs": { "channel": 0 }
                },
                { "id": "0x0002", "name": "", "file": null, "ended_by": "no_sound", "blip": false,
                  "twin_of": "0x0001" }
            ]
        })
    }

    #[test]
    fn page_embeds_escaped_json() {
        let data = page_data(&manifest(), |f| {
            ["0x0001-test_l1.wav", "0x0001-test_l1-loop.wav"].contains(&f)
        });
        let html = render("test_l1: sounds", &data);
        // The data sits in one script element that nothing in it can close.
        let open = r#"<script type="application/json" id="data">"#;
        let start = html.find(open).expect("data element") + open.len();
        let end = start + html[start..].find("</script>").expect("closed");
        let inner = &html[start..end];
        assert!(!inner.contains('<'), "raw < in the data: {inner}");
        let back: Value = serde_json::from_str(inner).unwrap();
        assert_eq!(back, data);
        assert_eq!(back["sounds"][0]["files"][0]["path"], "0x0001-test_l1.wav");
        assert_eq!(back["sounds"][0]["files"][1]["label"], "loop");
        assert_eq!(back["sounds"][0]["files"][1]["pack"], true);
        assert_eq!(back["sounds"][0]["files"][0]["pack"], false);
        assert_eq!(back["sounds"][0]["files"].as_array().unwrap().len(), 2);
        assert_eq!(back["sounds"][0]["loop"]["intro_secs"], 1.0);
        assert_eq!(back["sounds"][0]["flags"][0], "clipped (3)");
        assert_eq!(back["sounds"][1]["flags"][0], "silent");
        assert_eq!(back["volume"]["factory"], "55AA6798");
        // Well formed enough: one of each, slots filled, every script closed.
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("<title>test_l1: sounds</title>"));
        assert!(!html.contains(DATA_SLOT) && !html.contains(TITLE_SLOT));
        assert!(html.contains(MARKER));
        assert_eq!(
            html.matches("<script").count(),
            html.matches("</script>").count()
        );
        for tag in [
            "<html", "</html>", "<head>", "</head>", "<body>", "</body>", "<style>",
        ] {
            assert_eq!(html.matches(tag).count(), 1, "{tag}");
        }
        // Offline: nothing loaded from elsewhere.
        assert!(!html.contains("http://") && !html.contains("https://"));
    }

    #[test]
    fn only_files_on_disk_get_a_button() {
        let data = page_data(&manifest(), |f| f == "0x0001-test_l1.wav");
        let files = data["sounds"][0]["files"].as_array().unwrap();
        assert_eq!(files.len(), 1);
        assert!(data["sounds"][1]["files"].as_array().unwrap().is_empty());
    }
}
