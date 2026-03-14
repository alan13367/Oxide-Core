//! Asset watcher for hot reloading

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};

/// Resource to watch asset files for hot-reloading
pub struct AssetWatcher {
    _watcher: RecommendedWatcher,
    receiver: Receiver<notify::Result<Event>>,
    changed_paths: Vec<PathBuf>,
}

impl AssetWatcher {
    /// Creates a new AssetWatcher that watches a specific directory
    pub fn new<P: AsRef<Path>>(watch_path: P) -> Result<Self, notify::Error> {
        let (sender, receiver) = channel();

        let mut watcher = RecommendedWatcher::new(
            move |res| {
                let _ = sender.send(res);
            },
            Config::default().with_poll_interval(Duration::from_millis(100)),
        )?;

        watcher.watch(watch_path.as_ref(), RecursiveMode::Recursive)?;

        Ok(Self {
            _watcher: watcher,
            receiver,
            changed_paths: Vec::new(),
        })
    }

    /// Polls for changed files and returns a list of paths that were modified
    pub fn poll_changed_files(&mut self) -> &[PathBuf] {
        self.changed_paths.clear();

        while let Ok(res) = self.receiver.try_recv() {
            if let Ok(event) = res {
                if event.kind.is_modify() || event.kind.is_create() {
                    for path in event.paths {
                        if !self.changed_paths.contains(&path) {
                            self.changed_paths.push(path);
                        }
                    }
                }
            }
        }

        &self.changed_paths
    }
}
