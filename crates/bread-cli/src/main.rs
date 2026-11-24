/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! `bread`: converts STL to `.bread` and back, and looks inside `.bread`
//! files. Run `bread --help` for how.

use std::ffi::OsString;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bread_cli::{Credit, Error, StlFile, check, from_stl_with, grouped, info, now, to_stl};
use bread_format::read_file;

const HELP: &str = "\
bread: converts STL to .bread and back, and looks inside .bread files.

Usage:
  bread <file>...                     each .stl to .bread and each .bread to .stl,
                                      next to it (or drop files on bread)
  bread from-stl <file.stl>... [-o <out.bread>] [--name <name>] [credit]
                                      STL files as one .bread file, a part each
  bread to-stl <file.bread> [-o <out.stl>]
                                      everything a .bread file shows, as one STL
  bread info <file.bread>...          what a .bread file holds
  bread check <file.bread>...         whether a .bread file reads, and if not, why

Options:
  -o, --output <path>   where to write; by default, next to the first file
  --name <name>         a name for the scene (from-stl)
  -f, --force           write over a file that is already there
  -h, --help            this help
  -V, --version         the version

Credit, written into the .bread file as its metadata (from-stl):
  --title <title>       the model's title
  --designer <name>     who designed it
  --license <license>   its license, as an SPDX expression such as CC-BY-4.0
  --source <url>        where it comes from, such as https://example.com/model

Every conversion to .bread is read back and checked before it is written:
each mesh as read from its STL, and each corner of a binary STL, bit for bit.
Every .bread file written also says that bread wrote it, and when: set
SOURCE_DATE_EPOCH to a time in seconds to make a conversion again byte for byte.
";

/// What went wrong: the arguments, or the work.
enum Failure {
    Usage(String),
    Work(Error),
}

impl From<Error> for Failure {
    fn from(e: Error) -> Self {
        Self::Work(e)
    }
}

#[derive(Default)]
struct Args {
    command: Option<String>,
    files: Vec<PathBuf>,
    output: Option<PathBuf>,
    name: Option<String>,
    credit: Credit,
    force: bool,
}

const COMMANDS: [&str; 4] = ["from-stl", "to-stl", "info", "check"];

fn parse(raw: Vec<OsString>) -> Result<Args, Failure> {
    let mut args = Args::default();
    let mut raw = raw.into_iter();
    let mut options = true;
    while let Some(arg) = raw.next() {
        let text = arg.to_str();
        if options {
            match text {
                Some("--") => {
                    options = false;
                    continue;
                }
                Some("-o" | "--output") => {
                    let path = raw.next().ok_or_else(|| usage("-o needs a path"))?;
                    args.output = Some(path.into());
                    continue;
                }
                Some("--name") => {
                    args.name = Some(text_of(raw.next(), "--name", "a name")?);
                    continue;
                }
                Some(flag @ ("--title" | "--designer" | "--license" | "--source")) => {
                    let value = text_of(raw.next(), flag, "a value")?;
                    let field = match flag {
                        "--title" => &mut args.credit.title,
                        "--designer" => &mut args.credit.designer,
                        "--license" => &mut args.credit.license,
                        _ => &mut args.credit.source,
                    };
                    *field = Some(value);
                    continue;
                }
                Some("-f" | "--force") => {
                    args.force = true;
                    continue;
                }
                Some(flag) if flag.starts_with('-') && flag.len() > 1 => {
                    return Err(usage(&format!("unknown option {flag}")));
                }
                _ => {}
            }
        }
        if args.command.is_none()
            && args.files.is_empty()
            && let Some(command) = text.filter(|t| COMMANDS.contains(t))
        {
            args.command = Some(command.to_owned());
            continue;
        }
        args.files.push(arg.into());
    }
    Ok(args)
}

/// The text an option is given, which must be there and be Unicode.
fn text_of(value: Option<OsString>, flag: &str, what: &str) -> Result<String, Failure> {
    value
        .ok_or_else(|| usage(&format!("{flag} needs {what}")))?
        .into_string()
        .map_err(|_| usage(&format!("what {flag} is given is not valid Unicode")))
}

fn usage(message: &str) -> Failure {
    Failure::Usage(message.to_owned())
}

fn main() -> ExitCode {
    let raw: Vec<OsString> = std::env::args_os().skip(1).collect();
    let first = raw.first().and_then(|a| a.to_str());
    if raw.is_empty() || matches!(first, Some("-h" | "--help" | "help")) {
        print!("{HELP}");
        return ExitCode::SUCCESS;
    }
    if matches!(first, Some("-V" | "--version")) {
        println!("bread {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    match parse(raw).and_then(|args| run(&args)) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(Failure::Usage(message)) => {
            eprintln!("bread: {message}\nRun bread --help for how to use it.");
            ExitCode::from(2)
        }
        Err(Failure::Work(e)) => {
            eprintln!("bread: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Runs the command; `Ok(false)` when some file failed and was said so.
fn run(args: &Args) -> Result<bool, Failure> {
    if args.files.is_empty() {
        return Err(usage("no file given"));
    }
    if args.name.is_some() && args.command.as_deref() != Some("from-stl") {
        return Err(usage("--name is for from-stl"));
    }
    if args.credit != Credit::default() && args.command.as_deref() != Some("from-stl") {
        return Err(usage(
            "--title, --designer, --license and --source are for from-stl",
        ));
    }
    let credit = args.credit.clone().tidy();
    credit.check().map_err(|e| usage(&e.0))?;
    match args.command.as_deref() {
        Some("from-stl") => {
            stl_to_bread(
                &args.files,
                args.output.as_deref(),
                args.name.as_deref(),
                &credit,
                args.force,
            )?;
            Ok(true)
        }
        Some("to-stl") => {
            let [file] = args.files.as_slice() else {
                return Err(usage("to-stl takes one .bread file"));
            };
            bread_to_stl(file, args.output.as_deref(), args.force)?;
            Ok(true)
        }
        Some("info") => Ok(each(&args.files, |path| {
            let lines = info(&read(path)?)?;
            println!("{}", path.display());
            for line in lines {
                println!("  {line}");
            }
            Ok(())
        })),
        Some("check") => {
            let mut all = true;
            for path in &args.files {
                match read(path).and_then(|bytes| check(&bytes)) {
                    Ok(summary) => println!("reads    {}: {summary}", path.display()),
                    Err(e) => {
                        println!("refused  {}: {e}", path.display());
                        all = false;
                    }
                }
            }
            Ok(all)
        }
        _ => {
            if args.output.is_some() && args.files.len() > 1 {
                return Err(usage("-o names one file: give one file to convert"));
            }
            Ok(each(&args.files, |path| match extension(path).as_deref() {
                Some("stl") => stl_to_bread(
                    &[path.to_path_buf()],
                    args.output.as_deref(),
                    None,
                    &credit,
                    args.force,
                ),
                Some("bread") => bread_to_stl(path, args.output.as_deref(), args.force),
                _ => Err(Error(
                    "neither .stl nor .bread: use from-stl or to-stl".into(),
                )),
            }))
        }
    }
}

/// Runs `f` on every file, saying which failed and why; whether all went well.
fn each(files: &[PathBuf], mut f: impl FnMut(&Path) -> Result<(), Error>) -> bool {
    let mut all = true;
    for path in files {
        if let Err(e) = f(path) {
            eprintln!("bread: {}: {e}", path.display());
            all = false;
        }
    }
    all
}

fn extension(path: &Path) -> Option<String> {
    Some(path.extension()?.to_str()?.to_ascii_lowercase())
}

fn read(path: &Path) -> Result<Vec<u8>, Error> {
    fs::read(path).map_err(|e| Error(format!("cannot read {}: {e}", path.display())))
}

/// Writes `bytes` to `path`, but not over a file that is there unless
/// `force`.
fn write(path: &Path, bytes: &[u8], force: bool) -> Result<(), Error> {
    let cannot = |e: std::io::Error| Error(format!("cannot write {}: {e}", path.display()));
    let mut file = if force {
        fs::File::create(path)
    } else {
        fs::File::create_new(path)
    }
    .map_err(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            Error(format!(
                "{} is already there: --force writes over it",
                path.display()
            ))
        } else {
            cannot(e)
        }
    })?;
    file.write_all(bytes).map_err(cannot)
}

/// `path` with its extension changed: `kit.stl` to `kit.bread`.
fn next_to(path: &Path, extension: &str) -> PathBuf {
    path.with_extension(extension)
}

fn stl_to_bread(
    files: &[PathBuf],
    output: Option<&Path>,
    name: Option<&str>,
    credit: &Credit,
    force: bool,
) -> Result<(), Error> {
    let stls = files
        .iter()
        .map(|path| {
            Ok(StlFile {
                name: path
                    .file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
                bytes: read(path)?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let generator = format!("bread {}", env!("CARGO_PKG_VERSION"));
    let metadata = credit.metadata(&now(), &generator);
    let (bytes, parts) = from_stl_with(&stls, name, metadata)?;
    let out = output.map_or_else(|| next_to(&files[0], "bread"), Path::to_path_buf);
    write(&out, &bytes, force)?;
    let stl_bytes: usize = stls.iter().map(|f| f.bytes.len()).sum();
    let triangles: usize = parts.iter().map(|p| p.triangles).sum();
    println!(
        "wrote {}: {} part{}, {} triangles, {} bytes from {} bytes of STL ({:.1}%), every corner checked",
        out.display(),
        parts.len(),
        if parts.len() == 1 { "" } else { "s" },
        grouped(triangles),
        grouped(bytes.len()),
        grouped(stl_bytes),
        percent(bytes.len(), stl_bytes),
    );
    Ok(())
}

#[expect(clippy::cast_precision_loss, reason = "a percentage to show")]
fn percent(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        0.0
    } else {
        100.0 * part as f64 / whole as f64
    }
}

fn bread_to_stl(path: &Path, output: Option<&Path>, force: bool) -> Result<(), Error> {
    let file = read_file(read(path)?.as_slice()).map_err(|e| Error(e.to_string()))?;
    let (bytes, parts) = to_stl(&file)?;
    let out = output.map_or_else(|| next_to(path, "stl"), Path::to_path_buf);
    write(&out, &bytes, force)?;
    println!(
        "wrote {}: {} part{}, {} triangles, {} bytes",
        out.display(),
        parts,
        if parts == 1 { "" } else { "s" },
        grouped((bytes.len() - 84) / 50),
        grouped(bytes.len()),
    );
    Ok(())
}
