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
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bread_converter::queue::{self, Direction, Job, Output, Plan, Way, WhenThere};
use bread_converter::settings::Settings;
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
    /// The folder to open after a run.
    last_output: Option<PathBuf>,
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
    let rows = Rc::new(VecModel::<JobRow>::default());
    ui.set_jobs(ModelRc::from(rows.clone()));
    APP.with(|a| {
        *a.borrow_mut() = Some(App {
            ui: ui.as_weak(),
            jobs: Vec::new(),
            rows,
            settings,
            run: None,
            last_output: None,
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
        app(App::start);
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
    ui.on_language_picked(|i| {
        let language = if i == 1 { "es" } else { "en" };
        let _ = slint::select_bundled_translation(language);
        app(|a| {
            a.settings.language = language.to_owned();
            a.save();
        });
    });

    // Files dropped on the window.
    let weak = ui.as_weak();
    ui.window().on_winit_window_event(move |_window, event| {
        let Some(ui) = weak.upgrade() else {
            return EventResult::Propagate;
        };
        match event {
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

    ui.run()
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

fn row(job: &Job) -> JobRow {
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
    }
}

impl App {
    fn direction(ui: &AppWindow) -> Direction {
        match ui.get_direction() {
            1 => Direction::ToBread,
            2 => Direction::ToStl,
            _ => Direction::Both,
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
            self.reset_rows();
            ui.set_phase(0);
        }
        for job in new {
            self.rows.push(row(&job));
            self.jobs.push(job);
        }
        self.show_count(&ui);
    }

    fn show_count(&self, ui: &AppWindow) {
        ui.set_file_count(i32::try_from(self.jobs.len()).unwrap_or(i32::MAX));
        ui.set_file_size(size(self.jobs.iter().map(|j| j.size).sum()).into());
    }

    fn reset_rows(&self) {
        for (i, job) in self.jobs.iter().enumerate() {
            self.rows.set_row_data(i, row(job));
        }
    }

    fn clear(&mut self) {
        if self.run.is_some() {
            return;
        }
        self.jobs.clear();
        self.rows.set_vec(Vec::new());
        if let Some(ui) = self.ui.upgrade() {
            ui.set_phase(0);
            ui.set_notice(0);
            ui.set_progress(0.0);
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
        self.settings.save();
    }

    fn start(&mut self) {
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        if self.run.is_some() || self.jobs.is_empty() {
            return;
        }
        let output = if ui.get_output_mode() == 1 {
            let dir = ui.get_output_folder().trim().to_owned();
            if dir.is_empty() {
                ui.set_notice(2);
                return;
            }
            Output::Folder {
                dir: PathBuf::from(dir),
                keep_tree: ui.get_keep_tree(),
            }
        } else {
            Output::NextTo
        };
        let when_there = match ui.get_when_there() {
            1 => WhenThere::Replace,
            2 => WhenThere::KeepBoth,
            _ => WhenThere::Skip,
        };
        ui.set_notice(0);
        self.reset_rows();
        let direction = Self::direction(&ui);
        let wanted: Vec<usize> = (0..self.jobs.len())
            .filter(|&i| queue::way_of(&self.jobs[i].source, direction).is_some())
            .collect();
        let chosen: Vec<Job> = wanted.iter().map(|&i| self.jobs[i].clone()).collect();
        let plans = queue::plan(&chosen, &output, when_there);

        let mut tally = Tally::default();
        let mut work = Vec::new();
        for (i, job) in self.jobs.iter().enumerate() {
            let mut r = row(job);
            match wanted.iter().position(|&w| w == i).map(|k| &plans[k]) {
                None => {
                    r.state = 3;
                    r.reason = 2;
                    tally.skipped += 1;
                }
                Some(Plan::Skip(there)) => {
                    r.state = 3;
                    r.reason = 1;
                    // Inside the output folder, from there; else its name.
                    let shown = match &output {
                        Output::Folder { dir, .. } => there.strip_prefix(dir).ok(),
                        Output::NextTo => None,
                    }
                    .or_else(|| there.file_name().map(Path::new))
                    .unwrap_or(there);
                    r.detail = shown.to_string_lossy().as_ref().into();
                    tally.skipped += 1;
                }
                Some(Plan::Write(out)) => work.push((i, job.clone(), out.clone())),
            }
            self.rows.set_row_data(i, r);
        }
        self.last_output = match &output {
            Output::Folder { dir, .. } => Some(dir.clone()),
            Output::NextTo => work
                .first()
                .and_then(|(_, _, out)| out.parent().map(Path::to_path_buf)),
        };
        let total: u64 = work.iter().map(|(_, j, _)| j.size).sum();
        ui.set_run_count(i32::try_from(work.len()).unwrap_or(i32::MAX));
        ui.set_file_size(size(total).into());
        ui.set_can_open_output(false);
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
            stop: stop.clone(),
            tally: tally.clone(),
            started,
            _timer: timer,
        });
        std::thread::spawn(move || convert_all(work, &stop, &tally));
    }

    fn started(&self, i: usize) {
        if let Some(mut r) = self.rows.row_data(i) {
            r.state = 1;
            self.rows.set_row_data(i, r);
        }
    }

    fn finished(&self, i: usize, size_before: u64, result: &Result<(u64, PathBuf), String>) {
        let Some(mut r) = self.rows.row_data(i) else {
            return;
        };
        match result {
            Ok((written, out)) => {
                r.state = 2;
                r.reason = 4;
                r.detail = out
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
                    .into();
                r.result = size(*written).into();
                #[expect(clippy::cast_precision_loss, reason = "a percentage to show")]
                let ratio = 100.0 * *written as f64 / size_before.max(1) as f64;
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
        // What was never started when it stopped goes back to waiting.
        let stopped = run.stop.load(Ordering::Relaxed);
        show_tally(&ui, &tally, 0, run.started);
        ui.set_busy(false);
        ui.set_phase(if stopped { 3 } else { 2 });
        ui.set_progress(if stopped { ui.get_progress() } else { 1.0 });
        ui.set_summary_before(size(tally.before).into());
        ui.set_summary_after(size(tally.after).into());
        ui.set_can_open_output(tally.converted > 0 && self.last_output.is_some());
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

/// Converts `work` side by side, telling the window as each file starts and
/// ends, until all are done or `stop` is set.
fn convert_all(work: Vec<(usize, Job, PathBuf)>, stop: &AtomicBool, tally: &Mutex<Tally>) {
    let next = AtomicUsize::new(0);
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
                    let Some((i, job, out)) = work.get(k) else {
                        break;
                    };
                    let i = *i;
                    let _ = slint::invoke_from_event_loop(move || {
                        app(|a| a.started(i));
                    });
                    let result = queue::convert(job, out).map(|n| (n, out.clone()));
                    {
                        let mut t = tally.lock().unwrap();
                        t.done += 1;
                        t.bytes_done += job.size;
                        match &result {
                            Ok((n, _)) => {
                                t.converted += 1;
                                t.before += job.size;
                                t.after += n;
                            }
                            Err(_) => t.failed += 1,
                        }
                    }
                    let before = job.size;
                    let _ = slint::invoke_from_event_loop(move || {
                        app(|a| a.finished(i, before, &result));
                    });
                }
            });
        }
    });
    let _ = slint::invoke_from_event_loop(|| {
        app(App::done);
    });
}
