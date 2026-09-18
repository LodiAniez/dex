//! Reloading `config.toml` when it is saved, without a restart.

use std::path::Path;
use std::time::Instant;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use super::{ConfigHandle, SETTLE};
use crate::platform::bus::Bus;

impl ConfigHandle {
    /// Reloads whenever the file changes, announcing `config` on the bus.
    ///
    /// Watches the **directory**, not the file: editors save by writing a temp
    /// file and renaming it over the target, which destroys a watch on the file
    /// itself and would make the first save the last one ever noticed.
    pub fn watch(&self, bus: Bus) -> notify::Result<RecommendedWatcher> {
        let Some(dir) = self.path.parent().map(Path::to_path_buf) else {
            return Err(notify::Error::generic("config path has no directory"));
        };
        // Events name the real path: on macOS a temp folder under `/var` is
        // reported as `/private/var`, so match that spelling too.
        let real = dir.canonicalize().unwrap_or_else(|_| dir.clone());
        let real = real.join(self.path.file_name().unwrap_or_default());
        let handle = self.clone();
        let mut last = Instant::now() - SETTLE;
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let Ok(event) = event else { return };
                if !event.paths.iter().any(|p| *p == *handle.path || *p == real) {
                    return;
                }
                if last.elapsed() < SETTLE {
                    return;
                }
                last = Instant::now();
                for problem in handle.reload() {
                    tracing::warn!("{problem}");
                }
                tracing::info!("configuration reloaded");
                bus.publish("config");
            })?;
        watcher.watch(&dir, RecursiveMode::NonRecursive)?;
        Ok(watcher)
    }
}
