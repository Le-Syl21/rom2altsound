//! The window: the ROM list, the output folder, the options, the run.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Instant;

use eframe::egui::{self, Align, Color32, Layout, RichText};
use rom2altsound::batch::{self, Event, Outcome, Watch};
use rom2altsound::inventory::{Checker, Item, SupportLevel};
use rom2altsound::progress::Step;

use crate::i18n::{Lang, Texts};
use crate::options::{self, AdvOpt};
use crate::platform;

/// The command line's defaults for the options shown in plain words.
const DEFAULT_JOBS: u32 = 2;
const DEFAULT_MAX_SECS: f64 = 120.0;
/// The log keeps this many lines (a ROM writes a few thousand).
const LOG_LINES: usize = 50_000;

pub fn run() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("rom2altsound")
            .with_app_id("rom2altsound")
            .with_inner_size([1100.0, 760.0])
            .with_min_inner_size([720.0, 480.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "rom2altsound",
        options,
        Box::new(|cc| Ok(Box::new(App::new(&cc.egui_ctx)))),
    )
}

/// What the worker threads tell the window.
enum Msg {
    Table(Arc<Checker>),
    Checked(Vec<Item>),
    Started { rom: String, out: PathBuf },
    Log { rom: String, line: String },
    Progress { rom: String, step: Step },
    Finished(Outcome),
    Done(Option<Result<PathBuf, String>>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Volume {
    Factory,
    Reference,
}

enum RunState {
    Idle,
    Waiting,
    Running { step: Option<Step>, since: Instant },
    Done { secs: f64, out: PathBuf },
    Failed(String),
    Cancelled,
}

struct Row {
    item: Item,
    ticked: bool,
    state: RunState,
}

struct Running {
    watch: Arc<Watch>,
    total: usize,
    finished: usize,
    engine: Option<std::thread::JoinHandle<()>>,
}

pub struct App {
    lang: Lang,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    checker: Option<Arc<Checker>>,
    /// Paths given before PinMAME's table was read, or while a check runs.
    pending: Vec<PathBuf>,
    checking: bool,
    rows: Vec<Row>,
    path_input: String,
    out_dir: String,
    /// The output folder follows the ROMs' folder until the user picks one.
    out_auto: bool,
    volume: Volume,
    jobs: u32,
    max_secs: f64,
    sounds_dat: String,
    names_csv: String,
    advanced: Vec<AdvOpt>,
    error: Option<String>,
    running: Option<Running>,
    /// The page of every ROM, once a run wrote it.
    index: Option<PathBuf>,
    last_root: Option<PathBuf>,
    log: Vec<(String, String)>,
    log_filter: Option<String>,
    /// The machine's cores: the most ROMs at the same time offered.
    max_jobs: u32,
}

impl App {
    fn new(ctx: &egui::Context) -> Self {
        let (tx, rx) = channel();
        // PinMAME's table and every driver's sound board: a few seconds, off the UI thread.
        {
            let tx = tx.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let exe =
                    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("rom2altsound-gui"));
                let _ = tx.send(Msg::Table(Arc::new(Checker::new(&exe))));
                ctx.request_repaint();
            });
        }
        let max_jobs = std::thread::available_parallelism()
            .map_or(4, |n| n.get() as u32)
            .max(1);
        Self {
            lang: Lang::from_system(),
            tx,
            rx,
            checker: None,
            pending: Vec::new(),
            checking: false,
            rows: Vec::new(),
            path_input: String::new(),
            out_dir: String::new(),
            out_auto: true,
            volume: Volume::Factory,
            jobs: DEFAULT_JOBS.min(max_jobs),
            max_secs: DEFAULT_MAX_SECS,
            sounds_dat: String::new(),
            names_csv: String::new(),
            advanced: options::advanced(),
            error: None,
            running: None,
            index: None,
            last_root: None,
            log: Vec::new(),
            log_filter: None,
            max_jobs,
        }
    }

    fn t(&self) -> &'static Texts {
        self.lang.t()
    }

    // ------------------------------------------------------------------ messages

    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Table(c) => {
                    self.checker = Some(c);
                    self.dispatch_check(ctx);
                }
                Msg::Checked(items) => {
                    self.checking = false;
                    for item in items {
                        if self.rows.iter().any(|r| r.item.path == item.path) {
                            continue;
                        }
                        let ticked = item.extractable();
                        self.rows.push(Row {
                            item,
                            ticked,
                            state: RunState::Idle,
                        });
                    }
                    self.dispatch_check(ctx);
                }
                Msg::Started { rom, out } => {
                    self.log
                        .push((rom.clone(), format!("--- {rom} -> {}", out.display())));
                    if let Some(r) = self.row_mut(&rom) {
                        r.state = RunState::Running {
                            step: None,
                            since: Instant::now(),
                        };
                    }
                }
                Msg::Log { rom, line } => {
                    self.log.push((rom, line));
                    if self.log.len() > LOG_LINES {
                        self.log.drain(..self.log.len() - LOG_LINES);
                    }
                }
                Msg::Progress { rom, step } => {
                    if let Some(r) = self.row_mut(&rom)
                        && let RunState::Running { step: s, .. } = &mut r.state
                    {
                        *s = Some(step);
                    }
                }
                Msg::Finished(o) => {
                    if let Some(run) = &mut self.running {
                        run.finished += 1;
                    }
                    let line = batch::line(&o);
                    self.log.push((o.rom.clone(), line));
                    if let Some(r) = self.row_mut(&o.rom) {
                        r.state = match o.error {
                            None => RunState::Done {
                                secs: o.secs,
                                out: o.out,
                            },
                            Some(e) if e == batch::CANCELLED => RunState::Cancelled,
                            Some(e) => RunState::Failed(e),
                        };
                    }
                }
                Msg::Done(index) => {
                    self.running = None;
                    for r in &mut self.rows {
                        if matches!(r.state, RunState::Waiting | RunState::Running { .. }) {
                            r.state = RunState::Cancelled;
                        }
                    }
                    match index {
                        Some(Ok(p)) => self.index = Some(p),
                        Some(Err(e)) => self.log.push((String::new(), e)),
                        None => {}
                    }
                }
            }
        }
    }

    fn row_mut(&mut self, rom: &str) -> Option<&mut Row> {
        self.rows
            .iter_mut()
            .find(|r| r.ticked && (r.item.name == rom || r.item.path.as_os_str() == rom))
    }

    /// Adds zips or folders: a folder brings the zips it holds.
    fn add_paths(&mut self, ctx: &egui::Context, paths: Vec<PathBuf>) {
        let mut zips = Vec::new();
        for p in paths {
            if p.is_dir() {
                let mut in_dir: Vec<PathBuf> = std::fs::read_dir(&p)
                    .map(|r| {
                        r.filter_map(|e| e.ok().map(|e| e.path()))
                            .filter(|e| is_zip(e))
                            .collect()
                    })
                    .unwrap_or_default();
                in_dir.sort();
                if self.out_auto && self.out_dir.is_empty() {
                    self.out_dir = p.join("altsound").display().to_string();
                }
                zips.extend(in_dir);
            } else if is_zip(&p) {
                if self.out_auto
                    && self.out_dir.is_empty()
                    && let Some(d) = p.parent()
                {
                    self.out_dir = d.join("altsound").display().to_string();
                }
                zips.push(p);
            } else {
                self.error = Some(format!("{}: not a zip or a folder", p.display()));
            }
        }
        let known: BTreeSet<PathBuf> = self.rows.iter().map(|r| r.item.path.clone()).collect();
        for z in zips {
            let z = std::path::absolute(&z).unwrap_or(z);
            if !known.contains(&z) && !self.pending.contains(&z) {
                self.pending.push(z);
            }
        }
        self.dispatch_check(ctx);
    }

    fn dispatch_check(&mut self, ctx: &egui::Context) {
        if self.checking || self.pending.is_empty() {
            return;
        }
        let Some(checker) = self.checker.clone() else {
            return;
        };
        self.checking = true;
        let paths = std::mem::take(&mut self.pending);
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Checked(checker.check(&paths)));
            ctx.request_repaint();
        });
    }

    // ------------------------------------------------------------------ run

    fn chosen(&self) -> Vec<&Row> {
        self.rows
            .iter()
            .filter(|r| r.ticked && r.item.extractable())
            .collect()
    }

    /// The command line of the run (without the program name).
    fn command_line(&self) -> Vec<String> {
        let mut a: Vec<String> = self
            .chosen()
            .iter()
            .map(|r| r.item.path.display().to_string())
            .collect();
        if !self.out_dir.trim().is_empty() {
            a.push("--out".into());
            a.push(self.out_dir.trim().to_owned());
        }
        a.push("--jobs".into());
        a.push(self.jobs.to_string());
        a.push("--max-secs".into());
        a.push(self.max_secs.to_string());
        if self.volume == Volume::Reference {
            a.push("--volume".into());
            a.push("reference".into());
        }
        if !self.sounds_dat.trim().is_empty() {
            a.push("--sounds-dat".into());
            a.push(self.sounds_dat.trim().to_owned());
        }
        if !self.names_csv.trim().is_empty() {
            a.push("--names".into());
            a.push(self.names_csv.trim().to_owned());
        }
        a.extend(options::args(&self.advanced));
        a
    }

    fn start(&mut self, ctx: &egui::Context) {
        self.error = None;
        let args = self.command_line();
        let cli = match rom2altsound::parse_args(&args) {
            Ok(c) => c,
            Err(e) => {
                // clap's own words, without its usage lines.
                let msg = e.to_string();
                let first = msg
                    .lines()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or_default()
                    .trim_start_matches("error: ")
                    .to_owned();
                self.error = Some(format!("{} {first}", self.t().invalid_options));
                return;
            }
        };
        if let Err(e) = batch::check(&cli) {
            self.error = Some(e);
            return;
        }
        let exe = match std::env::current_exe() {
            Ok(e) => e,
            Err(e) => {
                self.error = Some(format!("cannot find this program's path: {e}"));
                return;
            }
        };
        let root = batch::root(&cli);
        if let Err(e) = std::fs::create_dir_all(&root) {
            self.error = Some(format!("{}: {e}", root.display()));
            return;
        }
        let total = self.chosen().len();
        for r in &mut self.rows {
            r.state = if r.ticked && r.item.extractable() {
                RunState::Waiting
            } else {
                RunState::Idle
            };
        }
        self.index = None;
        self.last_root = Some(root);
        self.log.clear();
        self.log
            .push((String::new(), format!("rom2altsound {}", args.join(" "))));
        let watch = Arc::new(Watch::default());
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        let w = watch.clone();
        let engine = std::thread::spawn(move || {
            let watch = w;
            let send = |m: Msg| {
                let _ = tx.send(m);
                ctx.request_repaint();
            };
            let outcomes = batch::run_with(&cli, &exe, Some(&watch), &mut |e| match e {
                Event::NotStarted(o) | Event::Finished(o) => send(Msg::Finished(o.clone())),
                Event::Started { rom, out, .. } => send(Msg::Started {
                    rom: rom.to_owned(),
                    out: out.to_path_buf(),
                }),
                Event::Log { rom, line } => send(Msg::Log {
                    rom: rom.to_owned(),
                    line: line.to_owned(),
                }),
                Event::Progress { rom, step } => send(Msg::Progress {
                    rom: rom.to_owned(),
                    step: step.clone(),
                }),
            });
            send(Msg::Done(batch::write_index(&cli, &outcomes)));
        });
        self.running = Some(Running {
            watch,
            total,
            finished: 0,
            engine: Some(engine),
        });
    }

    fn cancel(&mut self) {
        if let Some(r) = &self.running {
            r.watch.cancel.store(true, Ordering::Relaxed);
        }
    }

    // ------------------------------------------------------------------ ui

    fn header(&mut self, ui: &mut egui::Ui) {
        let t = self.t();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("rom2altsound").size(22.0).strong());
                ui.label(RichText::new(t.subtitle).weak());
            });
            ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
                egui::ComboBox::from_id_salt("language")
                    .selected_text(match self.lang {
                        Lang::En => "English",
                        Lang::Fr => "Français",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.lang, Lang::En, "English");
                        ui.selectable_value(&mut self.lang, Lang::Fr, "Français");
                    });
                ui.label(RichText::new(t.language).small().weak());
            });
        });
    }

    fn roms_section(&mut self, ui: &mut egui::Ui) {
        let t = self.t();
        let busy = self.running.is_some();
        ui.heading(t.roms_heading);
        ui.label(RichText::new(t.roms_hint).weak());
        ui.add_space(4.0);
        let mut add = Vec::new();
        ui.add_enabled_ui(!busy, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button(t.add_files).clicked()
                    && let Some(files) = rfd::FileDialog::new()
                        .add_filter("ROM zip", &["zip"])
                        .pick_files()
                {
                    add.extend(files);
                }
                if ui.button(t.add_folder).clicked()
                    && let Some(d) = rfd::FileDialog::new().pick_folder()
                {
                    add.push(d);
                }
                let label = ui.label(RichText::new(t.path_hint).weak());
                let edit = ui
                    .add(
                        egui::TextEdit::singleline(&mut self.path_input)
                            .desired_width(320.0)
                            .hint_text("~/vpinball/roms"),
                    )
                    .labelled_by(label.id);
                let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if (ui.button(t.add).clicked() || enter) && !self.path_input.trim().is_empty() {
                    add.push(expand_home(self.path_input.trim()));
                    self.path_input.clear();
                }
                if !self.rows.is_empty() && ui.button(t.clear).clicked() {
                    self.rows.clear();
                    self.index = None;
                }
            });
        });
        if !add.is_empty() {
            self.add_paths(ui.ctx(), add);
        }
        if self.checker.is_none() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(t.loading_table);
            });
        } else if self.checking || !self.pending.is_empty() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(t.checking);
            });
        }
        if self.rows.is_empty() {
            ui.add_space(8.0);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.add_space(18.0);
                ui.vertical_centered(|ui| ui.label(RichText::new(t.drop_here).size(16.0).weak()));
                ui.add_space(18.0);
            });
            return;
        }
        ui.add_space(6.0);
        self.rom_table(ui);
    }

    fn rom_table(&mut self, ui: &mut egui::Ui) {
        let t = self.t();
        let lang = self.lang;
        let busy = self.running.is_some();
        let one_names = !self.names_csv.trim().is_empty();
        let mut remove = None;
        let mut ticks = 0usize;
        egui::Grid::new("roms")
            .striped(true)
            .num_columns(9)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("");
                for h in [
                    t.col_rom,
                    t.col_game,
                    t.col_files,
                    t.col_board,
                    t.col_support,
                    t.col_vpx,
                ] {
                    ui.label(RichText::new(h).strong());
                }
                ui.label("");
                ui.label("");
                ui.end_row();
                for (i, row) in self.rows.iter_mut().enumerate() {
                    let ok = row.item.extractable();
                    ui.add_enabled_ui(ok && !busy, |ui| {
                        ui.checkbox(&mut row.ticked, "")
                            .on_hover_text(&row.item.name)
                    });
                    if ok && row.ticked {
                        ticks += 1;
                    }
                    ui.label(RichText::new(&row.item.name).monospace().strong());
                    match &row.item.set {
                        Some(s) => {
                            ui.label(format!("{} ({} {})", s.description, s.manufacturer, s.year))
                        }
                        // No game: what the check says it is.
                        None => ui.label(
                            RichText::new(row.item.issues.first().map_or("", String::as_str))
                                .weak(),
                        ),
                    };
                    let (files, tip, color) = files_cell(t, &row.item);
                    let r = ui.label(RichText::new(files).color(color));
                    let mut tip = tip.to_owned();
                    for issue in &row.item.issues {
                        tip.push_str("\n• ");
                        tip.push_str(issue);
                    }
                    if !tip.is_empty() {
                        r.on_hover_text(tip);
                    }
                    match &row.item.set {
                        Some(s) => {
                            ui.label(&s.family).on_hover_text(t.board_tip);
                            let (w, tip, c) = support_cell(t, s.support);
                            ui.label(RichText::new(w).color(c)).on_hover_text(tip);
                            match &s.vpx_not_played {
                                None => ui.label(t.vpx_yes).on_hover_text(t.vpx_yes_tip),
                                Some(n) => ui
                                    .label(
                                        RichText::new(t.vpx_no)
                                            .color(Color32::from_rgb(200, 140, 40)),
                                    )
                                    .on_hover_text(match lang {
                                        Lang::En => &n.en,
                                        Lang::Fr => &n.fr,
                                    }),
                            };
                        }
                        None => {
                            ui.label("");
                            ui.label("");
                            ui.label("");
                        }
                    }
                    state_cell(ui, t, &row.state);
                    ui.horizontal(|ui| {
                        if let RunState::Done { out, .. } = &row.state {
                            if let Some(p) = batch::page_of(out)
                                && ui
                                    .button(t.open_page)
                                    .on_hover_text(t.open_page_tip)
                                    .clicked()
                            {
                                platform::open(&p);
                            }
                            if ui.button(t.open_folder).clicked() {
                                platform::open(out);
                            }
                        } else if !busy && ui.small_button("×").on_hover_text(t.remove).clicked() {
                            remove = Some(i);
                        }
                    });
                    ui.end_row();
                }
            });
        if let Some(i) = remove {
            self.rows.remove(i);
        }
        if one_names && ticks > 1 {
            ui.colored_label(Color32::from_rgb(200, 140, 40), t.names_one_rom);
        }
    }

    fn output_section(&mut self, ui: &mut egui::Ui) {
        let t = self.t();
        ui.heading(t.output_heading);
        ui.label(RichText::new(t.output_hint).weak());
        ui.add_enabled_ui(self.running.is_none(), |ui| {
            ui.horizontal(|ui| {
                let label = ui.label(
                    t.output_heading
                        .trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ' '),
                );
                let r = ui
                    .add(egui::TextEdit::singleline(&mut self.out_dir).desired_width(480.0))
                    .labelled_by(label.id);
                if r.changed() {
                    self.out_auto = false;
                }
                if ui.button(t.choose).clicked() {
                    let mut d = rfd::FileDialog::new();
                    if !self.out_dir.is_empty() {
                        d = d.set_directory(&self.out_dir);
                    }
                    if let Some(p) = d.pick_folder() {
                        self.out_dir = p.display().to_string();
                        self.out_auto = false;
                    }
                }
                if !self.out_dir.is_empty()
                    && Path::new(&self.out_dir).is_dir()
                    && ui.button(t.open_folder).clicked()
                {
                    platform::open(Path::new(&self.out_dir));
                }
            });
        });
    }

    fn options_section(&mut self, ui: &mut egui::Ui) {
        let t = self.t();
        ui.heading(t.options_heading);
        ui.add_enabled_ui(self.running.is_none(), |ui| {
            egui::Grid::new("options")
                .num_columns(2)
                .spacing([16.0, 8.0])
                .show(ui, |ui| {
                    ui.label(t.volume);
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.volume, Volume::Factory, t.volume_factory)
                            .on_hover_text(t.volume_factory_tip);
                        ui.radio_value(&mut self.volume, Volume::Reference, t.volume_reference)
                            .on_hover_text(t.volume_reference_tip);
                    });
                    ui.end_row();

                    let l = ui.label(t.jobs).on_hover_text(t.jobs_tip);
                    ui.add(egui::DragValue::new(&mut self.jobs).range(1..=self.max_jobs))
                        .labelled_by(l.id)
                        .on_hover_text(t.jobs_tip);
                    ui.end_row();

                    let l = ui.label(t.max_secs).on_hover_text(t.max_secs_tip);
                    ui.add(
                        egui::DragValue::new(&mut self.max_secs)
                            .range(1.0..=3600.0)
                            .speed(1.0),
                    )
                    .labelled_by(l.id)
                    .on_hover_text(t.max_secs_tip);
                    ui.end_row();

                    file_row(
                        ui,
                        t.sounds_dat,
                        t.sounds_dat_tip,
                        t.built_in,
                        t.choose,
                        &mut self.sounds_dat,
                        "dat",
                    );
                    ui.end_row();
                    file_row(
                        ui,
                        t.names_csv,
                        t.names_csv_tip,
                        t.none,
                        t.choose,
                        &mut self.names_csv,
                        "csv",
                    );
                    ui.end_row();
                });
            ui.add_space(6.0);
            egui::CollapsingHeader::new(t.advanced)
                .id_salt("advanced")
                .show(ui, |ui| {
                    ui.label(RichText::new(t.advanced_hint).weak());
                    if ui.button(t.reset).clicked() {
                        self.advanced = options::advanced();
                    }
                    egui::Grid::new("advanced-grid")
                        .num_columns(2)
                        .striped(true)
                        .spacing([16.0, 6.0])
                        .show(ui, |ui| {
                            for o in &mut self.advanced {
                                let name = format!("--{}", o.long);
                                if o.flag {
                                    ui.checkbox(&mut o.on, RichText::new(&name).monospace());
                                } else {
                                    ui.horizontal(|ui| {
                                        let l = ui.label(RichText::new(&name).monospace());
                                        let hint = o.default.clone().unwrap_or_default();
                                        if o.choices.is_empty() {
                                            ui.add(
                                                egui::TextEdit::singleline(&mut o.value)
                                                    .desired_width(110.0)
                                                    .hint_text(hint),
                                            )
                                            .labelled_by(l.id);
                                        } else {
                                            egui::ComboBox::from_id_salt(&name)
                                                .selected_text(if o.value.is_empty() {
                                                    "-"
                                                } else {
                                                    &o.value
                                                })
                                                .show_ui(ui, |ui| {
                                                    ui.selectable_value(
                                                        &mut o.value,
                                                        String::new(),
                                                        "-",
                                                    );
                                                    for c in &o.choices {
                                                        ui.selectable_value(
                                                            &mut o.value,
                                                            c.clone(),
                                                            c,
                                                        );
                                                    }
                                                });
                                        }
                                    });
                                }
                                ui.add(egui::Label::new(RichText::new(&o.help).small()).wrap());
                                ui.end_row();
                            }
                        });
                });
        });
    }

    fn run_bar(&mut self, ui: &mut egui::Ui) {
        let t = self.t();
        if let Some(e) = &self.error {
            ui.colored_label(Color32::from_rgb(220, 70, 60), e);
        }
        ui.horizontal(|ui| {
            match &self.running {
                None => {
                    let n = self.chosen().len();
                    let can = n > 0 && self.checker.is_some();
                    let label = if can {
                        format!("{} ({n})", t.start)
                    } else {
                        t.start_none.to_owned()
                    };
                    if ui
                        .add_enabled(
                            can,
                            egui::Button::new(RichText::new(label).strong())
                                .min_size([200.0, 30.0].into()),
                        )
                        .clicked()
                    {
                        self.start(ui.ctx());
                    }
                }
                Some(r) => {
                    let stopping = r.watch.cancel.load(Ordering::Relaxed);
                    let label = if stopping { t.cancelling } else { t.cancel };
                    if ui
                        .add_enabled(
                            !stopping,
                            egui::Button::new(label).min_size([120.0, 30.0].into()),
                        )
                        .clicked()
                    {
                        r.watch.cancel.store(true, Ordering::Relaxed);
                    }
                    let done = r.finished as f32;
                    let partial: f32 = self
                        .rows
                        .iter()
                        .filter_map(|row| match &row.state {
                            RunState::Running { step: Some(s), .. } if s.total > 0 => {
                                Some(s.n as f32 / s.total as f32)
                            }
                            _ => None,
                        })
                        .sum();
                    let frac = ((done + partial.min(0.99)) / r.total.max(1) as f32).clamp(0.0, 1.0);
                    ui.label(format!("{} {}/{}", t.overall, r.finished, r.total));
                    ui.add(egui::ProgressBar::new(frac).show_percentage().animate(true));
                }
            }
            if self.running.is_none()
                && let Some(root) = &self.last_root
            {
                let ok = self
                    .rows
                    .iter()
                    .filter(|r| matches!(r.state, RunState::Done { .. }))
                    .count();
                let failed = self
                    .rows
                    .iter()
                    .filter(|r| matches!(r.state, RunState::Failed(_)))
                    .count();
                if ok + failed > 0 {
                    ui.label(format!("{ok} {}, {failed} {}", t.n_done, t.n_failed));
                }
                if let Some(p) = &self.index
                    && ui.button(t.open_all_pages).clicked()
                {
                    platform::open(p);
                }
                if root.is_dir() && ui.button(t.open_folder).clicked() {
                    platform::open(root);
                }
            }
        });
    }

    fn log_section(&mut self, ui: &mut egui::Ui) {
        let t = self.t();
        egui::CollapsingHeader::new(t.log)
            .id_salt("log")
            .show(ui, |ui| {
                let roms: BTreeSet<&str> = self
                    .log
                    .iter()
                    .map(|(r, _)| r.as_str())
                    .filter(|r| !r.is_empty())
                    .collect();
                ui.horizontal(|ui| {
                    ui.label(t.log_rom);
                    let sel = self
                        .log_filter
                        .clone()
                        .unwrap_or_else(|| t.log_all.to_owned());
                    egui::ComboBox::from_id_salt("log-filter")
                        .selected_text(sel)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.log_filter, None, t.log_all);
                            for r in &roms {
                                ui.selectable_value(
                                    &mut self.log_filter,
                                    Some((*r).to_owned()),
                                    *r,
                                );
                            }
                        });
                });
                let lines: Vec<&(String, String)> = self
                    .log
                    .iter()
                    .filter(|(r, _)| self.log_filter.as_ref().is_none_or(|f| f == r))
                    .collect();
                let h = ui.text_style_height(&egui::TextStyle::Monospace);
                egui::ScrollArea::both()
                    .id_salt("log-scroll")
                    .max_height(220.0)
                    .stick_to_bottom(true)
                    .auto_shrink([false, true])
                    .show_rows(ui, h, lines.len(), |ui, range| {
                        for (rom, line) in &lines[range] {
                            let text = if self.log_filter.is_some() || rom.is_empty() {
                                line.clone()
                            } else {
                                format!("[{rom}] {line}")
                            };
                            ui.label(RichText::new(text).monospace().size(11.5));
                        }
                    });
            });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll(&ctx);
        if self.running.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| !p.as_os_str().is_empty())
                .collect()
        });
        if !dropped.is_empty() && self.running.is_none() {
            self.add_paths(&ctx, dropped);
        }
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());

        egui::Frame::central_panel(ui.style())
            .inner_margin(egui::Margin::same(14))
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                egui::Panel::top("header")
                    .frame(egui::Frame::NONE)
                    .show_separator_line(false)
                    .show(ui, |ui| {
                        self.header(ui);
                        ui.add_space(8.0);
                    });
                egui::Panel::bottom("run")
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        ui.add_space(8.0);
                        self.run_bar(ui);
                        ui.add_space(4.0);
                        self.log_section(ui);
                        ui.label(RichText::new(self.t().footer).small().weak());
                    });
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("main")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                self.roms_section(ui);
                                ui.add_space(14.0);
                                self.output_section(ui);
                                ui.add_space(14.0);
                                self.options_section(ui);
                            });
                    });
            });

        if hovering {
            let rect = ctx.content_rect();
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("drop"),
            ));
            painter.rect_filled(rect, 0.0, Color32::from_black_alpha(160));
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                self.t().drop_here,
                egui::FontId::proportional(26.0),
                Color32::WHITE,
            );
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Closing the window stops the extraction (and its processes) too: the engine kills
        // them before it returns.
        self.cancel();
        if let Some(h) = self.running.as_mut().and_then(|r| r.engine.take()) {
            let _ = h.join();
        }
    }
}

fn is_zip(p: &Path) -> bool {
    p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip"))
}

fn expand_home(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(p)
}

const GOOD: Color32 = Color32::from_rgb(70, 170, 90);
const WARN: Color32 = Color32::from_rgb(200, 140, 40);
const BAD: Color32 = Color32::from_rgb(220, 70, 60);

fn files_cell(t: &Texts, item: &Item) -> (&'static str, &'static str, Color32) {
    let set = item.set.as_ref();
    match item.status {
        "ok" if set.is_some_and(|s| s.split) => (t.files_split, t.files_split_tip, GOOD),
        "ok" => (t.files_ok, "", GOOD),
        "split" => (t.files_split, t.files_split_tip, GOOD),
        "misnamed" => (t.files_misnamed, t.files_misnamed_tip, WARN),
        "bad-dump" => (t.files_bad, t.files_bad_tip, BAD),
        "incomplete" if set.is_some_and(|s| s.completable) => {
            (t.files_completable, t.files_completable_tip, WARN)
        }
        "incomplete" => (t.files_missing, t.files_missing_tip, BAD),
        "not-pinmame" | "support" => (t.files_not_game, "", Color32::GRAY),
        _ => (t.files_error, "", BAD),
    }
}

fn support_cell(t: &Texts, s: SupportLevel) -> (&'static str, &'static str, Color32) {
    match s {
        SupportLevel::Works => (t.support_works, t.support_works_tip, GOOD),
        SupportLevel::Partial => (t.support_partial, t.support_partial_tip, WARN),
        SupportLevel::None => (t.support_none, t.support_none_tip, BAD),
        SupportLevel::NoBoard => (t.support_no_board, t.support_no_board_tip, Color32::GRAY),
        SupportLevel::Untested => (t.support_untested, t.support_untested_tip, Color32::GRAY),
    }
}

fn stage_words(t: &Texts, stage: &str) -> &'static str {
    match stage {
        "cold-boot" => t.stage_cold_boot,
        "boot" => t.stage_boot,
        "Main" => t.stage_main,
        "Retry" => t.stage_retry,
        "VolumeCheck" => t.stage_volume,
        "FactoryOffset" => t.stage_factory,
        "Chips" => t.stage_chips,
        "DuckCheck" => t.stage_duck,
        "pack" => t.stage_pack,
        "sam" => t.stage_sam,
        _ => t.stage_starting,
    }
}

fn state_cell(ui: &mut egui::Ui, t: &Texts, s: &RunState) {
    match s {
        RunState::Idle => {
            ui.label("");
        }
        RunState::Waiting => {
            ui.label(RichText::new(t.waiting).weak());
        }
        RunState::Running { step, since } => {
            ui.vertical(|ui| {
                let secs = since.elapsed().as_secs();
                match step {
                    Some(s) if s.total > 0 => {
                        ui.add(
                            egui::ProgressBar::new(s.n as f32 / s.total as f32)
                                .desired_width(180.0)
                                .text(format!("{}/{}", s.n, s.total)),
                        );
                        ui.label(
                            RichText::new(format!(
                                "{} · {} {}",
                                stage_words(t, &s.stage),
                                s.id,
                                s.name
                            ))
                            .small(),
                        );
                    }
                    Some(s) => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(RichText::new(stage_words(t, &s.stage)).small());
                        });
                    }
                    None => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(RichText::new(t.stage_starting).small());
                        });
                    }
                }
                ui.label(
                    RichText::new(format!("{}:{:02}", secs / 60, secs % 60))
                        .small()
                        .weak(),
                );
            });
        }
        RunState::Done { secs, .. } => {
            ui.label(RichText::new(format!("✔ {} {:.0} s", t.done_in, secs)).color(GOOD));
        }
        RunState::Failed(e) => {
            ui.label(RichText::new(format!("✘ {}", t.failed)).color(BAD))
                .on_hover_text(e);
        }
        RunState::Cancelled => {
            ui.label(RichText::new(t.cancelled).weak());
        }
    }
}

/// A file option: its path (typed or picked), empty for the default.
fn file_row(
    ui: &mut egui::Ui,
    label: &str,
    tip: &str,
    empty: &str,
    choose: &str,
    value: &mut String,
    ext: &str,
) {
    let l = ui.label(label).on_hover_text(tip);
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(value)
                .desired_width(360.0)
                .hint_text(empty),
        )
        .labelled_by(l.id)
        .on_hover_text(tip);
        if ui.button(choose).on_hover_text(tip).clicked()
            && let Some(p) = rfd::FileDialog::new().add_filter(ext, &[ext]).pick_file()
        {
            *value = p.display().to_string();
        }
        if !value.is_empty() && ui.small_button("×").clicked() {
            value.clear();
        }
    });
}
