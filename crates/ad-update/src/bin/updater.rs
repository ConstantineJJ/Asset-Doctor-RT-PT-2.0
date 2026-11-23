//! ADR-0049 updater: waits for Asset Doctor to exit, copies the new files
//! into the program folder with a backup, starts the new program and waits
//! for its "started" marker; without it, puts the old files back and starts
//! the old program with the reason.
//!
//! `AssetDoctorRTPT-Updater --source <payload> --target <program folder>
//!   --pid <program pid> --from <version> --to <tag> [--root <updates folder>]
//!   [--restart-arg <arg>]…`
#![windows_subsystem = "windows"]

use ad_update::{PROGRAM_EXE, apply};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const EXIT_WAIT: Duration = Duration::from_secs(30);
const FILE_WAIT: Duration = Duration::from_secs(15);
const START_WAIT: Duration = Duration::from_secs(30);

struct Options {
    source: PathBuf,
    target: PathBuf,
    pid: u32,
    from: String,
    to: String,
    root: Option<PathBuf>,
    restart_args: Vec<String>,
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut source = None;
    let mut target = None;
    let mut pid = None;
    let mut from = String::from("?");
    let mut to = String::from("?");
    let mut restart_args = Vec::new();
    let mut root = None;
    let mut it = args.iter();
    while let Some(key) = it.next() {
        let mut value = || it.next().cloned().ok_or(format!("{key} needs a value"));
        match key.as_str() {
            "--source" => source = Some(PathBuf::from(value()?)),
            "--target" => target = Some(PathBuf::from(value()?)),
            "--pid" => pid = Some(value()?.parse().map_err(|_| "bad --pid")?),
            "--from" => from = value()?,
            "--to" => to = value()?,
            "--restart-arg" => restart_args.push(value()?),
            "--root" => root = Some(PathBuf::from(value()?)),
            other => return Err(format!("unknown option {other}")),
        }
    }
    Ok(Options {
        source: source.ok_or("--source is required")?,
        target: target.ok_or("--target is required")?,
        pid: pid.ok_or("--pid is required")?,
        from,
        to,
        root,
        restart_args,
    })
}

struct Log(Option<std::fs::File>);
impl Log {
    fn line(&mut self, text: &str) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        if let Some(file) = self.0.as_mut() {
            let _ = writeln!(file, "{now} {text}");
        }
    }
}

#[cfg(windows)]
fn wait_for_exit(pid: u32, wait: Duration) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };
    // SAFETY: a synchronize-only handle to a process id; null means it is gone.
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if handle == 0 {
        return true;
    }
    // SAFETY: a live process handle, closed right after the wait.
    let result = unsafe { WaitForSingleObject(handle, wait.as_millis() as u32) };
    // SAFETY: the handle from OpenProcess, closed once.
    unsafe { CloseHandle(handle) };
    result == WAIT_OBJECT_0
}

#[cfg(not(windows))]
fn wait_for_exit(_: u32, _: Duration) -> bool {
    true
}

fn start(target: &Path, args: &[String]) -> std::io::Result<std::process::Child> {
    Command::new(target.join(PROGRAM_EXE))
        .current_dir(target)
        .args(args)
        .spawn()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = parse(&args);
    let chosen = options.as_ref().ok().and_then(|o| o.root.clone());
    let root = ad_update::updates_root(chosen.as_deref()).ok();
    let mut log = Log(root.as_ref().and_then(|r| {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(r.join("updater.log"))
            .ok()
    }));
    let options = match options {
        Ok(o) => o,
        Err(error) => {
            log.line(&format!("bad arguments: {error}"));
            std::process::exit(2);
        }
    };
    std::process::exit(run(&options, root, &mut log));
}

fn run(o: &Options, root: Option<PathBuf>, log: &mut Log) -> i32 {
    log.line(&format!(
        "update {} -> {}: source={} target={}",
        o.from,
        o.to,
        o.source.display(),
        o.target.display()
    ));
    let Some(root) = root else {
        log.line("no updates folder");
        return 1;
    };
    let failed = |log: &mut Log, reason: &str| {
        log.line(&format!("update failed: {reason}"));
        let mut args = vec!["--update-failed".to_owned(), reason.to_owned()];
        args.extend(o.restart_args.iter().cloned());
        match start(&o.target, &args) {
            Ok(_) => log.line("old program started"),
            Err(error) => log.line(&format!("old program not started: {error}")),
        }
    };
    if !wait_for_exit(o.pid, EXIT_WAIT) {
        log.line("the program did not exit within 30 s; nothing changed");
        return 1;
    }
    if !o.source.join(PROGRAM_EXE).is_file() || !o.target.is_dir() {
        failed(log, "the update or the program folder is missing");
        return 1;
    }
    let backup = root.join(format!("backup-{}", std::process::id()));
    let applied = match apply::apply(&o.source, &o.target, &backup, FILE_WAIT) {
        Ok(applied) => applied,
        Err((error, applied)) => {
            log.line(&format!("copy failed: {error}"));
            if let Err(e) = apply::rollback(&o.target, &backup, &applied, FILE_WAIT) {
                log.line(&e);
            }
            failed(log, &format!("files could not be replaced ({error})"));
            return 1;
        }
    };
    log.line(&format!(
        "copied: {} replaced, {} new",
        applied.replaced.len(),
        applied.created.len()
    ));
    let marker = root.join(format!("started-{}.ok", std::process::id()));
    let _ = std::fs::remove_file(&marker);
    let mut args = vec![
        "--update-ok-file".to_owned(),
        marker.display().to_string(),
        "--updated-from".to_owned(),
        o.from.clone(),
        "--updated-to".to_owned(),
        o.to.clone(),
    ];
    args.extend(o.restart_args.iter().cloned());
    let mut child = match start(&o.target, &args) {
        Ok(child) => child,
        Err(error) => {
            log.line(&format!("new program not started: {error}"));
            let _ = apply::rollback(&o.target, &backup, &applied, FILE_WAIT);
            failed(log, "the new version did not start");
            return 1;
        }
    };
    let started = Instant::now();
    while started.elapsed() < START_WAIT {
        // Exit state first: a program that wrote the marker and then quit
        // (a smoke run) still counts as a good start.
        let exited = child.try_wait().ok().flatten();
        if marker.is_file() {
            let _ = std::fs::remove_file(&marker);
            log.line("new program reported a good start; update complete");
            return 0;
        }
        if let Some(status) = exited {
            log.line(&format!(
                "new program exited before reporting a start ({status})"
            ));
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    log.line("no start marker; rolling back");
    let _ = child.kill();
    let _ = child.wait();
    match apply::rollback(&o.target, &backup, &applied, FILE_WAIT) {
        Ok(()) => log.line("old files restored"),
        Err(error) => log.line(&error),
    }
    failed(
        log,
        "the new version did not start correctly; the previous one was restored",
    );
    2
}
