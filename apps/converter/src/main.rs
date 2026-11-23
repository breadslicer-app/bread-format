/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! Bread Converter: STL and `.bread`, both ways, many files at once, in a
//! window drawn with Breadslicer's kit.

// A window program: no console behind it in a release build.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bread_cli::Credit;
use bread_converter::queue::{self, Direction, Job, Output, Plan, Stamp, Way, WhenThere};
use bread_converter::settings::Settings;
use slint::winit_030::winit::window::ResizeDirection;
use slint::winit_030::{EventResult, WinitWindowAccessor, winit};
use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};

/// The window, as Slint compiles it from `ui/app.slint`. Its code is
/// generated, so the workspace's style lints are not asked of it.
#[allow(
    unsafe_code,
    unused_qualifications,
    unreachable_pub,
    missing_debug_implementations,
    clippy::all
)]
mod window {
    slint::include_modules!();
}
use window::{AppWindow, JobRow};

/// How many files are converted side by side, at most: each holds a whole
/// file and its conversion in memory.
const SIDE_BY_SIDE: usize = 4;

/// What is counted while a run goes on, shared with the threads that convert.
#[derive(Default)]
struct Tally {
    done: usize,
    bytes_done: u64,
    converted: usize,
    skipped: usize,
    failed: usize,
    before: u64,
    after: u64,
}

struct Run {
    kind: Kind,
    stop: Arc<AtomicBool>,
    tally: Arc<Mutex<Tally>>,
    started: Instant,
    /// Refreshes the progress while the run goes on.
    _timer: Timer,
}

struct App {
    ui: slint::Weak<AppWindow>,
    jobs: Vec<Job>,
    rows: Rc<VecModel<JobRow>>,
    settings: Settings,
    run: Option<Run>,
    /// What the last measuring run made of each file, by the list's order.
    made: Vec<Option<Made>>,
    /// The folder to open after a run.
    last_output: Option<PathBuf>,
    /// Each file's title, when it was changed from its name's.
    titles: Vec<Option<String>>,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Runs `f` on the app, on the window's thread.
fn app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.borrow_mut().as_mut().map(f))
}

fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    let settings = Settings::load();
    // The language kept, or the first of the user's languages that the
    // window speaks.
    let spanish = match settings.language.as_str() {
        "es" => true,
        "en" => false,
        _ => sys_locale::get_locales()
            .map(|l| l.to_ascii_lowercase())
            .find(|l| l.starts_with("es") || l.starts_with("en"))
            .is_some_and(|l| l.starts_with("es")),
    };
    let _ = slint::select_bundled_translation(if spanish { "es" } else { "en" });
    ui.set_language(i32::from(spanish));
    ui.set_version(env!("CARGO_PKG_VERSION").into());
    ui.set_direction(settings.direction);
    ui.set_subfolders(settings.subfolders);
    ui.set_output_mode(settings.output_mode);
    ui.set_output_folder(settings.output_folder.clone().into());
    ui.set_keep_tree(settings.keep_tree);
    ui.set_when_there(settings.when_there);
    ui.set_export_mode(settings.export_mode);
    ui.set_designer(settings.designer.clone().into());
    ui.set_license(settings.license.clone().into());
    ui.set_source(settings.source.clone().into());
    ui.set_credit_problem(credit_problem(&ui).into());
    let rows = Rc::new(VecModel::<JobRow>::default());
    ui.set_jobs(ModelRc::from(rows.clone()));
    APP.with(|a| {
        *a.borrow_mut() = Some(App {
            ui: ui.as_weak(),
            jobs: Vec::new(),
            rows,
            settings,
            run: None,
            made: Vec::new(),
            last_output: None,
            titles: Vec::new(),
        });
    });

    ui.on_add_files(|| {
        let picked = app(|a| {
            dialog(a)
                .add_filter("STL and .bread", &["stl", "bread"])
                .pick_files()
        })
        .flatten();
        if let Some(files) = picked {
            remember_folder(files.first().and_then(|f| f.parent()));
            app(|a| a.add(&files));
        }
    });
    ui.on_add_folder(|| {
        if let Some(dir) = app(|a| dialog(a).pick_folder()).flatten() {
            remember_folder(Some(&dir));
            app(|a| a.add(&[dir]));
        }
    });
    ui.on_choose_output(|| {
        if let Some(dir) = app(|a| dialog(a).pick_folder()).flatten() {
            app(|a| {
                if let Some(ui) = a.ui.upgrade() {
                    ui.set_output_folder(dir.to_string_lossy().as_ref().into());
                    ui.set_notice(0);
                }
                a.save();
            });
        }
    });
    ui.on_clear(|| {
        app(App::clear);
    });
    ui.on_convert(|| {
        app(|a| {
            let measure = a.ui.upgrade().is_some_and(|ui| ui.get_export_mode() == 1);
            a.start(if measure { Kind::Measure } else { Kind::Write });
        });
    });
    ui.on_export(|| {
        app(|a| a.start(Kind::Export));
    });
    ui.on_cancel(|| {
        app(|a| {
            if let Some(run) = &a.run {
                run.stop.store(true, Ordering::Relaxed);
            }
        });
    });
    ui.on_open_output(|| {
        if let Some(Some(dir)) = app(|a| a.last_output.clone()) {
            open(dir.as_os_str());
        }
    });
    ui.on_open_url(|url| open(url.as_str().as_ref()));
    ui.on_settings_changed(|| {
        app(App::save);
    });
    ui.on_credit_changed(|| {
        app(App::credit_changed);
    });
    ui.on_edit_title(|i| {
        app(|a| a.edit_title(i));
    });
    ui.on_title_done(|i, text| {
        app(|a| a.set_title(i, Some(text.to_string())));
    });
    ui.on_title_reset(|i| {
        app(|a| a.set_title(i, None));
    });
    ui.on_language_picked(|i| {
        let language = if i == 1 { "es" } else { "en" };
        let _ = slint::select_bundled_translation(language);
        app(|a| {
            a.settings.language = language.to_owned();
            a.save();
        });
    });

    wire_chrome(&ui);

    // Files dropped on the window, and the maximise button kept in step.
    let weak = ui.as_weak();
    ui.window().on_winit_window_event(move |_window, event| {
        let Some(ui) = weak.upgrade() else {
            return EventResult::Propagate;
        };
        match event {
            winit::event::WindowEvent::Resized(_) => {
                ui.set_window_maximized(ui.window().is_maximized());
                return EventResult::Propagate;
            }
            winit::event::WindowEvent::HoveredFile(_) => ui.set_dropping(!ui.get_busy()),
            winit::event::WindowEvent::HoveredFileCancelled => ui.set_dropping(false),
            winit::event::WindowEvent::DroppedFile(path) => {
                ui.set_dropping(false);
                if !ui.get_busy() {
                    let path = path.clone();
                    app(|a| a.add(&[path]));
                }
            }
            _ => return EventResult::Propagate,
        }
        EventResult::Propagate
    });

    // Files given on the command line, as when dropped on the program.
    let given: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if !given.is_empty() {
        app(|a| a.add(&given));
    }

    ui.show()?;
    // The shadow a framed window has.
    #[cfg(windows)]
    if CUSTOM_CHROME {
        ui.window().with_winit_window(|w| {
            use slint::winit_030::winit::platform::windows::WindowExtWindows;
            w.set_undecorated_shadow(true);
        });
    }
    slint::run_event_loop()?;
    ui.hide()
}

/// Whether the window draws its own title bar, as Breadslicer does where it
/// can: on Windows. Elsewhere the system's frame stays.
const CUSTOM_CHROME: bool = cfg!(windows);

/// Turns the window's own title bar on and wires its buttons, its drag and
/// its edges. Before the window is shown.
fn wire_chrome(ui: &AppWindow) {
    if !CUSTOM_CHROME {
        return;
    }
    ui.set_custom_chrome(true);
    let weak = ui.as_weak();
    ui.on_window_drag(move || {
        if let Some(ui) = weak.upgrade() {
            ui.window().with_winit_window(|w| {
                let _ = w.drag_window();
            });
        }
    });
    let weak = ui.as_weak();
    ui.on_window_minimize(move || {
        if let Some(ui) = weak.upgrade() {
            ui.window().set_minimized(true);
        }
    });
    let weak = ui.as_weak();
    ui.on_window_maximize_toggle(move || {
        if let Some(ui) = weak.upgrade() {
            let maximized = !ui.window().is_maximized();
            ui.window().set_maximized(maximized);
            ui.set_window_maximized(maximized);
        }
    });
    let weak = ui.as_weak();
    ui.on_window_close(move || {
        if let Some(ui) = weak.upgrade() {
            let _ = ui.hide();
        }
    });
    let weak = ui.as_weak();
    ui.on_window_resize(move |side| {
        let direction = match side {
            0 => ResizeDirection::North,
            1 => ResizeDirection::South,
            2 => ResizeDirection::East,
            3 => ResizeDirection::West,
            4 => ResizeDirection::NorthEast,
            5 => ResizeDirection::NorthWest,
            6 => ResizeDirection::SouthEast,
            7 => ResizeDirection::SouthWest,
            _ => return,
        };
        if let Some(ui) = weak.upgrade() {
            ui.window().with_winit_window(|w| {
                let _ = w.drag_resize_window(direction);
            });
        }
    });
}

/// A native file dialog over the window, starting where the last one was.
fn dialog(a: &App) -> rfd::FileDialog {
    let mut d =
        a.ui.upgrade()
            .and_then(|ui| {
                ui.window()
                    .with_winit_window(|w| rfd::FileDialog::new().set_parent(w))
            })
            .unwrap_or_default();
    if !a.settings.last_folder.is_empty() {
        d = d.set_directory(&a.settings.last_folder);
    }
    d
}

fn remember_folder(dir: Option<&Path>) {
    if let Some(dir) = dir {
        app(|a| {
            a.settings.last_folder = dir.to_string_lossy().into_owned();
            a.save();
        });
    }
}

/// Opens a folder or a web address with what the system opens them with.
fn open(what: &std::ffi::OsStr) {
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = Command::new(program).arg(what).spawn();
}

/// A size for people: 512 B, 980 KB, 95.2 MB, 2.1 GB.
fn size(bytes: u64) -> String {
    #[expect(clippy::cast_precision_loss, reason = "a size to show")]
    let b = bytes as f64;
    match bytes {
        0..1_000 => format!("{bytes} B"),
        1_000..1_000_000 => format!("{:.0} KB", b / 1e3),
        1_000_000..1_000_000_000 => format!("{:.1} MB", b / 1e6),
        _ => format!("{:.2} GB", b / 1e9),
    }
}

/// A time for people: 0:42, 12:05, 1:02:03.
fn clock(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

fn way_text(way: Way) -> &'static str {
    match way {
        Way::ToBread => "STL → .bread",
        Way::ToStl => ".bread → STL",
    }
}

/// Where a file is: under the folder it came with, from that folder's name
/// down, or its whole folder.
fn folder_of(job: &Job) -> String {
    let Some(parent) = job.source.parent() else {
        return String::new();
    };
    job.root
        .as_deref()
        .and_then(Path::parent)
        .and_then(|above| parent.strip_prefix(above).ok())
        .unwrap_or(parent)
        .to_string_lossy()
        .into_owned()
}

/// The credit the window shows, without its title.
fn credit_of(ui: &AppWindow) -> Credit {
    Credit {
        title: None,
        designer: Some(ui.get_designer().to_string()),
        license: Some(ui.get_license().to_string()),
        source: Some(ui.get_source().to_string()),
    }
    .tidy()
}

/// Why the credit the window shows cannot be written, or nothing.
fn credit_problem(ui: &AppWindow) -> String {
    credit_of(ui).check().err().map(|e| e.0).unwrap_or_default()
}

fn row(job: &Job, title: &str) -> JobRow {
    JobRow {
        name: job
            .source
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
            .into(),
        detail: folder_of(job).into(),
        reason: 0,
        way: way_text(job.way).into(),
        size: size(job.size).into(),
        result: SharedString::new(),
        ratio: SharedString::new(),
        state: 0,
        title: if job.way == Way::ToBread {
            title.into()
        } else {
            SharedString::new()
        },
    }
}

/// What a run does with each file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Converts it and writes it.
    Write,
    /// Converts it in memory only, to see how much room it would take.
    Measure,
    /// Writes what a measuring run made: kept in memory, or made again.
    Export,
}

/// What a measuring run made of a file: its size, and its bytes when they
/// fitted in what is kept in memory.
struct Made {
    size: u64,
    bytes: Option<Arc<Vec<u8>>>,
}

/// How much a measuring run keeps in memory for Export, at most: what does
/// not fit is converted again when it is exported.
const KEEP_IN_MEMORY: u64 = 1 << 30;

/// A file of a run: its row, the job, where it goes (none when only
/// measured), and what a measuring run kept of it.
struct Work {
    i: usize,
    job: Job,
    out: Option<PathBuf>,
    kept: Option<Arc<Vec<u8>>>,
    /// What its `.bread` file carries.
    stamp: Stamp,
}

/// How a file of a run ended: the size made, where it went, and what is kept
/// of it in memory; or why it failed.
type Outcome = Result<(u64, Option<PathBuf>, Option<Arc<Vec<u8>>>), String>;

impl App {
    fn direction(ui: &AppWindow) -> Direction {
        match ui.get_direction() {
            1 => Direction::ToBread,
            2 => Direction::ToStl,
            _ => Direction::Both,
        }
    }

    /// Where files go, as the window says, or `None` when it names no folder.
    fn output(ui: &AppWindow) -> Option<Output> {
        if ui.get_output_mode() == 1 {
            let dir = ui.get_output_folder().trim().to_owned();
            if dir.is_empty() {
                return None;
            }
            Some(Output::Folder {
                dir: PathBuf::from(dir),
                keep_tree: ui.get_keep_tree(),
            })
        } else {
            Some(Output::NextTo)
        }
    }

    fn add(&mut self, paths: &[PathBuf]) {
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        if self.run.is_some() {
            return;
        }
        let known: HashSet<PathBuf> = self.jobs.iter().map(|j| j.source.clone()).collect();
        let new = queue::collect(paths, ui.get_subfolders(), Self::direction(&ui), &known);
        ui.set_notice(if new.is_empty() { 1 } else { 0 });
        // A list run before starts again.
        if ui.get_phase() >= 2 {
            self.forget(&ui);
        }
        for job in new {
            self.rows.push(row(&job, &queue::title_of(&job.source)));
            self.jobs.push(job);
            self.made.push(None);
            self.titles.push(None);
        }
        self.show_count(&ui);
    }

    /// Forgets the last run: rows back to waiting, nothing kept to export.
    fn forget(&mut self, ui: &AppWindow) {
        self.reset_rows();
        self.made = self.jobs.iter().map(|_| None).collect();
        ui.set_phase(0);
        ui.set_can_export(false);
        ui.set_short_of_room(false);
        ui.set_progress(0.0);
    }

    fn show_count(&self, ui: &AppWindow) {
        ui.set_file_count(i32::try_from(self.jobs.len()).unwrap_or(i32::MAX));
        ui.set_file_size(size(self.jobs.iter().map(|j| j.size).sum()).into());
    }

    fn reset_rows(&self) {
        for (i, job) in self.jobs.iter().enumerate() {
            self.rows.set_row_data(i, row(job, &self.title(i)));
        }
    }

    /// The title of the file at `i`: as changed, or its name's.
    fn title(&self, i: usize) -> String {
        match self.titles.get(i) {
            Some(Some(title)) => title.clone(),
            _ => queue::title_of(&self.jobs[i].source),
        }
    }

    /// The credit changed: kept, and what a measuring run made with the old
    /// one is forgotten, since it would be written with the new one.
    fn credit_changed(&mut self) {
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        ui.set_credit_problem(credit_problem(&ui).into());
        if ui.get_notice() == 3 {
            ui.set_notice(0);
        }
        if ui.get_can_export() {
            self.forget(&ui);
        }
        self.save();
    }

    /// Opens the title of the file at `i` to be changed.
    fn edit_title(&self, i: i32) {
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        let Some(job) = usize::try_from(i).ok().and_then(|i| self.jobs.get(i)) else {
            return;
        };
        if self.run.is_some() || job.way != Way::ToBread {
            return;
        }
        let i = usize::try_from(i).unwrap_or_default();
        ui.set_title_file(
            job.source
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
                .into(),
        );
        ui.set_title_text(self.title(i).into());
        ui.set_title_row(i32::try_from(i).unwrap_or(-1));
    }

    /// Gives the file at `i` a title, or its name's back with `None`.
    fn set_title(&mut self, i: i32, title: Option<String>) {
        let Some(i) = usize::try_from(i).ok().filter(|&i| i < self.jobs.len()) else {
            return;
        };
        if self.run.is_some() {
            return;
        }
        self.titles[i] = title;
        if let Some(ui) = self.ui.upgrade() {
            if ui.get_notice() == 3 {
                ui.set_notice(0);
            }
            if ui.get_can_export() {
                self.forget(&ui);
            }
        }
        if let Some(mut r) = self.rows.row_data(i) {
            r.title = self.title(i).into();
            self.rows.set_row_data(i, r);
        }
    }

    fn clear(&mut self) {
        if self.run.is_some() {
            return;
        }
        self.jobs.clear();
        self.made.clear();
        self.titles.clear();
        self.rows.set_vec(Vec::new());
        if let Some(ui) = self.ui.upgrade() {
            self.forget(&ui);
            ui.set_notice(0);
            self.show_count(&ui);
        }
    }

    fn save(&mut self) {
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        self.settings.direction = ui.get_direction();
        self.settings.subfolders = ui.get_subfolders();
        self.settings.output_mode = ui.get_output_mode();
        self.settings.output_folder = ui.get_output_folder().to_string();
        self.settings.keep_tree = ui.get_keep_tree();
        self.settings.when_there = ui.get_when_there();
        self.settings.export_mode = ui.get_export_mode();
        self.settings.designer = ui.get_designer().to_string();
        self.settings.license = ui.get_license().to_string();
        self.settings.source = ui.get_source().to_string();
        self.settings.save();
        if ui.get_can_export() {
            self.show_room(&ui);
        }
    }

    /// How much room the files converted in memory would take, against the
    /// room free where they would be written: in the output folder, or next
    /// to the first of them.
    fn show_room(&self, ui: &AppWindow) {
        let needed: u64 = self.made.iter().flatten().map(|m| m.size).sum();
        let first = self
            .made
            .iter()
            .position(Option::is_some)
            .and_then(|i| self.jobs[i].source.parent());
        let place = match Self::output(ui) {
            Some(Output::Folder { dir, .. }) => Some(dir),
            _ => first.map(Path::to_path_buf),
        };
        let free = place.as_deref().and_then(queue::free_space);
        ui.set_free_space(free.map(size).unwrap_or_else(|| "?".into()).into());
        ui.set_short_of_room(free.is_some_and(|f| f < needed));
    }

    fn start(&mut self, kind: Kind) {
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        if self.run.is_some() || self.jobs.is_empty() {
            return;
        }
        let output = if kind == Kind::Measure {
            None
        } else if let Some(output) = Self::output(&ui) {
            Some(output)
        } else {
            ui.set_notice(2);
            return;
        };
        let when_there = match ui.get_when_there() {
            1 => WhenThere::Replace,
            2 => WhenThere::KeepBoth,
            _ => WhenThere::Skip,
        };
        ui.set_notice(0);
        if kind != Kind::Export {
            self.made = self.jobs.iter().map(|_| None).collect();
        }
        let direction = Self::direction(&ui);
        let this_way = |i: usize| queue::way_of(&self.jobs[i].source, direction).is_some();
        // Export writes what was converted in memory, and only that.
        let wanted: Vec<usize> = (0..self.jobs.len())
            .filter(|&i| this_way(i) && (kind != Kind::Export || self.made[i].is_some()))
            .collect();
        // Every .bread file carries the credit: what cannot be written is
        // said before anything is converted.
        let base = Stamp {
            credit: credit_of(&ui),
            created: bread_cli::now(),
            generator: format!("Bread Converter {}", env!("CARGO_PKG_VERSION")),
        };
        let stamp_of = |i: usize| -> Result<Stamp, String> {
            let credit = Credit {
                title: Some(self.title(i)),
                ..base.credit.clone()
            }
            .tidy();
            credit.check().map_err(|e| {
                if base.credit.check().is_err() {
                    e.0
                } else {
                    format!("{}: {}", self.jobs[i].source.display(), e.0)
                }
            })?;
            Ok(Stamp {
                credit,
                ..base.clone()
            })
        };
        let mut stamps = Vec::with_capacity(wanted.len());
        for &i in &wanted {
            if self.jobs[i].way == Way::ToBread {
                match stamp_of(i) {
                    Ok(stamp) => stamps.push(stamp),
                    Err(problem) => {
                        ui.set_credit_problem(problem.into());
                        ui.set_notice(3);
                        return;
                    }
                }
            } else {
                stamps.push(base.clone());
            }
        }
        let plans = match &output {
            Some(output) => {
                let chosen: Vec<Job> = wanted.iter().map(|&i| self.jobs[i].clone()).collect();
                queue::plan(&chosen, output, when_there)
            }
            None => Vec::new(),
        };

        let mut tally = Tally::default();
        let mut work = Vec::new();
        for (i, job) in self.jobs.iter().enumerate() {
            let mut r = row(job, &self.title(i));
            let Some(k) = wanted.iter().position(|&w| w == i) else {
                // A file that failed when it was converted in memory keeps
                // saying why.
                if kind == Kind::Export && this_way(i) {
                    continue;
                }
                r.state = 3;
                r.reason = 2;
                tally.skipped += 1;
                self.rows.set_row_data(i, r);
                continue;
            };
            match plans.get(k) {
                None => work.push(Work {
                    i,
                    job: job.clone(),
                    out: None,
                    kept: None,
                    stamp: stamps[k].clone(),
                }),
                Some(Plan::Skip(there)) => {
                    r.state = 3;
                    r.reason = 1;
                    // Inside the output folder, from there; else its name.
                    let shown = match &output {
                        Some(Output::Folder { dir, .. }) => there.strip_prefix(dir).ok(),
                        _ => None,
                    }
                    .or_else(|| there.file_name().map(Path::new))
                    .unwrap_or(there);
                    r.detail = shown.to_string_lossy().as_ref().into();
                    tally.skipped += 1;
                }
                Some(Plan::Write(out)) => work.push(Work {
                    i,
                    job: job.clone(),
                    out: Some(out.clone()),
                    kept: self.made[i].as_ref().and_then(|m| m.bytes.clone()),
                    stamp: stamps[k].clone(),
                }),
            }
            self.rows.set_row_data(i, r);
        }
        if kind != Kind::Measure {
            self.last_output = match &output {
                Some(Output::Folder { dir, .. }) => Some(dir.clone()),
                _ => work
                    .first()
                    .and_then(|w| w.out.as_deref()?.parent().map(Path::to_path_buf)),
            };
        }
        let total: u64 = work.iter().map(|w| w.job.size).sum();
        ui.set_run_count(i32::try_from(work.len()).unwrap_or(i32::MAX));
        ui.set_file_size(size(total).into());
        ui.set_can_open_output(false);
        ui.set_can_export(false);
        ui.set_short_of_room(false);
        ui.set_busy(true);
        ui.set_phase(1);

        let stop = Arc::new(AtomicBool::new(false));
        let tally = Arc::new(Mutex::new(tally));
        let started = Instant::now();
        let timer = Timer::default();
        {
            let tally = tally.clone();
            let weak = self.ui.clone();
            timer.start(TimerMode::Repeated, Duration::from_millis(200), move || {
                if let Some(ui) = weak.upgrade() {
                    show_tally(&ui, &tally.lock().unwrap(), total, started);
                }
            });
        }
        show_tally(&ui, &tally.lock().unwrap(), total, started);
        self.run = Some(Run {
            kind,
            stop: stop.clone(),
            tally: tally.clone(),
            started,
            _timer: timer,
        });
        std::thread::spawn(move || convert_all(work, kind, &stop, &tally));
    }

    fn started(&self, i: usize) {
        if let Some(mut r) = self.rows.row_data(i) {
            r.state = 1;
            self.rows.set_row_data(i, r);
        }
    }

    fn finished(&mut self, i: usize, size_before: u64, outcome: Outcome) {
        let Some(mut r) = self.rows.row_data(i) else {
            return;
        };
        match outcome {
            Ok((made, out, kept)) => {
                match out {
                    Some(out) => {
                        r.state = 2;
                        r.reason = 4;
                        r.detail = out
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default()
                            .into();
                    }
                    None => {
                        r.state = 5;
                        if let Some(slot) = self.made.get_mut(i) {
                            *slot = Some(Made {
                                size: made,
                                bytes: kept,
                            });
                        }
                    }
                }
                r.result = size(made).into();
                #[expect(clippy::cast_precision_loss, reason = "a percentage to show")]
                let ratio = 100.0 * made as f64 / size_before.max(1) as f64;
                r.ratio = if ratio < 10.0 {
                    format!("{ratio:.1}%")
                } else if ratio < 1000.0 {
                    format!("{ratio:.0}%")
                } else {
                    format!("×{:.0}", ratio / 100.0)
                }
                .into();
            }
            Err(message) => {
                r.state = 4;
                r.reason = 3;
                r.detail = message.as_str().into();
            }
        }
        self.rows.set_row_data(i, r);
    }

    fn done(&mut self) {
        let Some(run) = self.run.take() else {
            return;
        };
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        let tally = run.tally.lock().unwrap();
        let stopped = run.stop.load(Ordering::Relaxed);
        show_tally(&ui, &tally, 0, run.started);
        ui.set_busy(false);
        ui.set_phase(match (run.kind, stopped) {
            (_, true) => 3,
            (Kind::Measure, false) => 4,
            _ => 2,
        });
        if !stopped {
            ui.set_progress(1.0);
        }
        ui.set_summary_before(size(tally.before).into());
        ui.set_summary_after(size(tally.after).into());
        ui.set_grows(tally.after > tally.before);
        ui.set_saved(size(tally.after.abs_diff(tally.before)).into());
        // What was converted in memory can be written, now or again.
        let can_export = self.made.iter().any(Option::is_some);
        ui.set_can_export(can_export);
        if can_export {
            self.show_room(&ui);
        }
        ui.set_can_open_output(
            run.kind != Kind::Measure && tally.converted > 0 && self.last_output.is_some(),
        );
    }
}

fn show_tally(ui: &AppWindow, t: &Tally, total: u64, started: Instant) {
    let count = |n: usize| i32::try_from(n).unwrap_or(i32::MAX);
    ui.set_done_count(count(t.done));
    ui.set_converted(count(t.converted));
    ui.set_skipped(count(t.skipped));
    ui.set_failed(count(t.failed));
    ui.set_bytes_done(size(t.bytes_done).into());
    ui.set_elapsed(clock(started.elapsed()).into());
    if total > 0 {
        #[expect(clippy::cast_precision_loss, reason = "a fraction to draw")]
        ui.set_progress((t.bytes_done as f64 / total as f64) as f32);
    }
}

/// Converts `work` side by side as `kind` says, telling the window as each
/// file starts and ends, until all are done or `stop` is set.
fn convert_all(work: Vec<Work>, kind: Kind, stop: &AtomicBool, tally: &Mutex<Tally>) {
    let next = AtomicUsize::new(0);
    let kept_bytes = AtomicU64::new(0);
    let threads = std::thread::available_parallelism()
        .map_or(2, usize::from)
        .clamp(1, SIDE_BY_SIDE)
        .min(work.len().max(1));
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                loop {
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let k = next.fetch_add(1, Ordering::Relaxed);
                    let Some(w) = work.get(k) else {
                        break;
                    };
                    let i = w.i;
                    let _ = slint::invoke_from_event_loop(move || {
                        app(|a| a.started(i));
                    });
                    let outcome: Outcome = match (&w.out, &w.kept) {
                        (None, _) => queue::make(&w.job, &w.stamp).map(|bytes| {
                            let n = bytes.len() as u64;
                            let fits =
                                kept_bytes.fetch_add(n, Ordering::Relaxed) + n <= KEEP_IN_MEMORY;
                            if !fits {
                                kept_bytes.fetch_sub(n, Ordering::Relaxed);
                            }
                            (n, None, fits.then(|| Arc::new(bytes)))
                        }),
                        (Some(out), Some(bytes)) => queue::write(bytes, out)
                            .map(|()| (bytes.len() as u64, Some(out.clone()), None)),
                        (Some(out), None) => queue::convert(&w.job, out, &w.stamp)
                            .map(|n| (n, Some(out.clone()), None)),
                    };
                    debug_assert!(kind == Kind::Measure || w.out.is_some());
                    {
                        let mut t = tally.lock().unwrap();
                        t.done += 1;
                        t.bytes_done += w.job.size;
                        match &outcome {
                            Ok((n, _, _)) => {
                                t.converted += 1;
                                t.before += w.job.size;
                                t.after += n;
                            }
                            Err(_) => t.failed += 1,
                        }
                    }
                    let before = w.job.size;
                    let _ = slint::invoke_from_event_loop(move || {
                        app(|a| a.finished(i, before, outcome));
                    });
                }
            });
        }
    });
    let _ = slint::invoke_from_event_loop(|| {
        app(App::done);
    });
}
