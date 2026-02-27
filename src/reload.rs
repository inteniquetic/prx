use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use arc_swap::ArcSwap;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use tracing::{error, info, warn};

use crate::{config::PrxConfig, runtime::RuntimeConfig};

pub fn spawn_config_watcher(
    config_path: PathBuf,
    debounce: Duration,
    active_config: Arc<ArcSwap<RuntimeConfig>>,
) -> anyhow::Result<()> {
    let watched_file = config_path
        .file_name()
        .map(|name| name.to_owned())
        .unwrap_or_else(|| OsStr::new("Prx.toml").to_owned());
    let watched_dir = resolve_watch_dir(&config_path);

    thread::Builder::new()
        .name("prx-config-watcher".to_string())
        .spawn(move || {
            let (tx, rx) = std::sync::mpsc::channel();
            let mut watcher: RecommendedWatcher = match notify::recommended_watcher(move |res| {
                let _ = tx.send(res);
            }) {
                Ok(watcher) => watcher,
                Err(err) => {
                    error!(error = %err, "failed to start config watcher");
                    return;
                }
            };

            if let Err(err) = watcher.watch(&watched_dir, RecursiveMode::NonRecursive) {
                error!(
                    error = %err,
                    directory = %watched_dir.to_string_lossy(),
                    "failed to watch config directory"
                );
                return;
            }

            info!(
                config = %config_path.to_string_lossy(),
                debounce_ms = debounce.as_millis(),
                "auto reload for Prx.toml is active"
            );

            let mut last_reload = Instant::now()
                .checked_sub(debounce)
                .unwrap_or_else(Instant::now);

            loop {
                let event = match rx.recv() {
                    Ok(Ok(event)) => event,
                    Ok(Err(err)) => {
                        warn!(error = %err, "watch event error");
                        continue;
                    }
                    Err(err) => {
                        warn!(error = %err, "config watcher channel closed");
                        return;
                    }
                };

                if !event_touches_file(&event, &watched_file) {
                    continue;
                }

                let now = Instant::now();
                if now.duration_since(last_reload) < debounce {
                    continue;
                }
                last_reload = now;

                match PrxConfig::from_file(&config_path).map(RuntimeConfig::from_config) {
                    Ok(next_config) => {
                        active_config.store(Arc::new(next_config));
                        info!(
                            config = %config_path.to_string_lossy(),
                            "reloaded config from disk"
                        );
                    }
                    Err(err) => {
                        error!(
                            error = %err,
                            config = %config_path.to_string_lossy(),
                            "failed to reload config, keeping previous version"
                        );
                    }
                }
            }
        })?;

    Ok(())
}

fn resolve_watch_dir(config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn event_touches_file(event: &Event, file_name: &OsStr) -> bool {
    event
        .paths
        .iter()
        .any(|path| path.file_name().is_some_and(|name| name == file_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_watch_dir_uses_current_dir_for_relative_file() {
        let dir = resolve_watch_dir(Path::new("Prx.toml"));
        assert_eq!(dir, PathBuf::from("."));
    }

    #[test]
    fn resolve_watch_dir_uses_parent_for_absolute_file() {
        let dir = resolve_watch_dir(Path::new("/tmp/prx/Prx.toml"));
        assert_eq!(dir, PathBuf::from("/tmp/prx"));
    }
}
