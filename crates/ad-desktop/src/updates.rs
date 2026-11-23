//! ADR-0049: checking for and installing updates in the background. The
//! update window (`ViewerUi::update_window`) shows `Panel`; the application
//! polls `Updates` every frame.
use ad_update::{Feed, Release, Staged, Version, release, stage};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

/// What the update window shows.
#[derive(Clone, Debug, Default)]
pub enum Phase {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Downloading {
        done: u64,
        total: u64,
    },
    /// The updater runs; the program is closing.
    Installing,
    Failed(String),
}

/// Update state shared with the interface.
#[derive(Clone, Debug)]
pub struct Panel {
    /// «Проверять обновления при запуске» (view settings, default on).
    pub check_on_start: bool,
    pub open: bool,
    pub phase: Phase,
    /// False while the model has unsaved changes.
    pub can_install: bool,
}

impl Default for Panel {
    fn default() -> Self {
        Self {
            check_on_start: true,
            open: false,
            phase: Phase::Idle,
            can_install: true,
        }
    }
}

enum Message {
    Checked(Result<Option<Release>, String>),
    Staged(Result<Staged, String>),
}

pub struct Updates {
    pub feed: Feed,
    /// Updates folder override (smokes); None is %LOCALAPPDATA%.
    pub root: Option<PathBuf>,
    /// Arguments the updater passes on to the restarted program (smokes).
    pub restart_args: Vec<String>,
    /// Smoke: install a found update without asking.
    pub auto_install: bool,
    job: Option<Receiver<Message>>,
    cancel: Arc<AtomicBool>,
    done: Arc<AtomicU64>,
    /// The running check was started by the program, not by the user: a
    /// quiet result does not open the window.
    quiet: bool,
}

impl Default for Updates {
    fn default() -> Self {
        Self {
            feed: Feed::GitHub,
            root: None,
            restart_args: Vec::new(),
            auto_install: false,
            job: None,
            cancel: Arc::new(AtomicBool::new(false)),
            done: Arc::new(AtomicU64::new(0)),
            quiet: false,
        }
    }
}

impl Updates {
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    /// Looks for a newer release; `quiet` (the start-up check) opens the
    /// window only when one is found.
    pub fn check(&mut self, panel: &mut Panel, quiet: bool) {
        if self.busy() {
            return;
        }
        self.quiet = quiet;
        if !quiet {
            panel.open = true;
        }
        panel.phase = Phase::Checking;
        let (send, receive) = channel();
        let feed = self.feed.clone();
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = Arc::clone(&self.cancel);
        let spawned = std::thread::Builder::new()
            .name("update-check".into())
            .spawn(move || {
                let found = feed
                    .releases(&cancel)
                    .and_then(|json| release::newest(&json, Version::current()));
                let _ = send.send(Message::Checked(found));
            });
        match spawned {
            Ok(_) => self.job = Some(receive),
            Err(error) => panel.phase = Phase::Failed(error.to_string()),
        }
        eprintln!("update_check feed={:?} quiet={quiet}", self.feed);
    }

    /// Downloads, verifies and unpacks `release`.
    pub fn install(&mut self, panel: &mut Panel, release: Release) {
        if self.busy() {
            return;
        }
        let root = match ad_update::updates_root(self.root.as_deref()) {
            Ok(root) => root,
            Err(error) => {
                panel.phase = Phase::Failed(error);
                return;
            }
        };
        panel.open = true;
        panel.phase = Phase::Downloading {
            done: 0,
            total: release.zip_size,
        };
        let (send, receive) = channel();
        let feed = self.feed.clone();
        self.cancel = Arc::new(AtomicBool::new(false));
        self.done = Arc::new(AtomicU64::new(0));
        let (cancel, done) = (Arc::clone(&self.cancel), Arc::clone(&self.done));
        let spawned = std::thread::Builder::new()
            .name("update-download".into())
            .spawn(move || {
                let staged = stage::stage(
                    &feed,
                    &release,
                    &root,
                    &|bytes, _| done.store(bytes, Ordering::Relaxed),
                    &cancel,
                );
                let _ = send.send(Message::Staged(staged));
            });
        match spawned {
            Ok(_) => self.job = Some(receive),
            Err(error) => panel.phase = Phase::Failed(error.to_string()),
        }
    }

    /// Stops a running check or download (its result is dropped).
    pub fn cancel(&mut self, panel: &mut Panel) {
        self.cancel.store(true, Ordering::Relaxed);
        self.job = None;
        panel.phase = Phase::Idle;
    }

    /// Takes a finished result into `panel`; returns a staged update ready
    /// for the updater.
    pub fn poll(&mut self, panel: &mut Panel) -> Option<Staged> {
        if let Phase::Downloading { done, .. } = &mut panel.phase {
            *done = self.done.load(Ordering::Relaxed);
        }
        let message = match self.job.as_ref()?.try_recv() {
            Ok(message) => message,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => {
                Message::Checked(Err("the update task stopped".into()))
            }
        };
        self.job = None;
        match message {
            Message::Checked(Ok(Some(found))) => {
                eprintln!("update_found tag={} size={}", found.tag, found.zip_size);
                panel.open = true;
                panel.phase = Phase::Available(found.clone());
                if self.auto_install {
                    self.install(panel, found);
                }
            }
            Message::Checked(Ok(None)) => {
                eprintln!("update_none current={}", Version::current());
                panel.phase = Phase::UpToDate;
            }
            Message::Checked(Err(error)) => {
                eprintln!("update_check_failed {error}");
                // The start-up check stays silent when offline.
                panel.phase = if self.quiet {
                    Phase::Idle
                } else {
                    Phase::Failed(error)
                };
            }
            Message::Staged(Ok(staged)) => {
                eprintln!(
                    "update_staged tag={} payload={}",
                    staged.release.tag,
                    staged.payload.display()
                );
                return Some(staged);
            }
            Message::Staged(Err(error)) => {
                eprintln!("update_stage_failed {error}");
                panel.phase = Phase::Failed(error);
            }
        }
        None
    }

    /// Starts the updater for `staged`; the program must exit next.
    pub fn launch(&self, staged: &Staged) -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let target = exe.parent().ok_or("no program folder")?;
        let mut command = std::process::Command::new(staged.updater());
        command
            .current_dir(&staged.payload)
            .arg("--source")
            .arg(&staged.payload)
            .arg("--target")
            .arg(target)
            .arg("--pid")
            .arg(std::process::id().to_string())
            .arg("--from")
            .arg(Version::current().to_string())
            .arg("--to")
            .arg(&staged.release.tag);
        if let Some(root) = &self.root {
            command.arg("--root").arg(root);
        }
        for arg in &self.restart_args {
            command.arg("--restart-arg").arg(arg);
        }
        command
            .spawn()
            .map_err(|e| format!("the updater did not start: {e}"))?;
        eprintln!(
            "update_launch tag={} target={}",
            staged.release.tag,
            target.display()
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quiet_check_without_network_stays_silent() {
        let mut updates = Updates {
            feed: Feed::Local(PathBuf::from("Z:/no/such/folder")),
            ..Updates::default()
        };
        let mut panel = Panel::default();
        updates.check(&mut panel, true);
        assert!(!panel.open);
        let started = std::time::Instant::now();
        while updates.busy() {
            assert!(updates.poll(&mut panel).is_none());
            assert!(started.elapsed().as_secs() < 10);
            std::thread::yield_now();
        }
        assert!(matches!(panel.phase, Phase::Idle), "{:?}", panel.phase);
        assert!(!panel.open);
        // The same failure asked for by the user is shown.
        updates.check(&mut panel, false);
        assert!(panel.open);
        while updates.busy() {
            updates.poll(&mut panel);
            std::thread::yield_now();
        }
        assert!(matches!(panel.phase, Phase::Failed(_)), "{:?}", panel.phase);
    }
}
