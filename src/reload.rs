use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::Arc,
    thread,
    time::Duration,
};

use arc_swap::ArcSwap;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use tracing::{error, info, warn};

use crate::{
    config::PrxConfig,
    runtime::RuntimeConfig,
    stats::{self, EventLevel},
};

static RELOADS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How many times the watcher has swapped in a config. A write through the
/// admin API must not add to it: that config is already live.
pub fn reload_count() -> u64 {
    RELOADS.load(std::sync::atomic::Ordering::SeqCst)
}

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

                // Trailing-edge debounce: wait until the file has been quiet
                // for `debounce`, then read it once. Dropping the events that
                // arrive inside the window (as this used to) loses the last
                // of a quick series of edits.
                loop {
                    match rx.recv_timeout(debounce) {
                        Ok(_) => continue,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break,
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
                    }
                }

                // The admin API already applied exactly these bytes.
                if std::fs::read(&config_path).is_ok_and(|bytes| crate::history::is_applied(&bytes))
                {
                    continue;
                }

                match PrxConfig::from_file(&config_path).map(RuntimeConfig::from_config) {
                    Ok(next_config) => {
                        active_config.store(Arc::new(next_config));
                        RELOADS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        if let Ok(bytes) = std::fs::read(&config_path) {
                            crate::history::mark_applied(&bytes);
                        }
                        info!(
                            config = %config_path.to_string_lossy(),
                            "reloaded config from disk"
                        );
                        // Someone editing the file by hand is invisible to the
                        // Web UI otherwise: the page would show new values with
                        // no idea where they came from.
                        stats::record_event(
                            EventLevel::Info,
                            "config_reload",
                            "Reloaded Prx.toml from disk",
                        );
                    }
                    Err(err) => {
                        error!(
                            error = %err,
                            config = %config_path.to_string_lossy(),
                            "failed to reload config, keeping previous version"
                        );
                        stats::record_event(
                            EventLevel::Error,
                            "config_reload",
                            format!("Reload failed, keeping the previous config: {err}"),
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

    /// `tag` names the one route, which is how a test tells configs apart.
    fn config_text(tag: &str) -> String {
        format!(
            r#"[server]
listen = ["127.0.0.1:0"]
[[service]]
name = "s"
[[service.upstream]]
addr = "127.0.0.1:9"
[[route]]
name = "{tag}"
service = "s"
is_default = true
"#
        )
    }

    fn live(path: &Path) -> Arc<ArcSwap<RuntimeConfig>> {
        let config = PrxConfig::from_file(path).unwrap();
        Arc::new(ArcSwap::from_pointee(RuntimeConfig::from_config(config)))
    }

    fn settle(debounce: Duration) {
        thread::sleep(debounce * 6 + Duration::from_millis(200));
    }

    #[test]
    fn an_edit_by_hand_reloads_and_quick_edits_end_on_the_last() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Prx.toml");
        std::fs::write(&path, config_text("r1")).unwrap();
        let active = live(&path);
        let debounce = Duration::from_millis(80);
        spawn_config_watcher(path.clone(), debounce, Arc::clone(&active)).unwrap();
        thread::sleep(Duration::from_millis(300));

        // Three edits inside one debounce window: the old watcher dropped the
        // later ones and kept the first.
        for tag in ["e2", "e3", "e4"] {
            std::fs::write(&path, config_text(tag)).unwrap();
            thread::sleep(Duration::from_millis(10));
        }
        settle(debounce);
        let name = active.load().route(0).map(|r| r.name.clone());
        assert_eq!(name.as_deref(), Some("e4"), "the last edit must win");
    }

    #[test]
    fn an_api_apply_is_not_reloaded_a_second_time() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Prx.toml");
        std::fs::write(&path, config_text("a1")).unwrap();
        let active = live(&path);
        let debounce = Duration::from_millis(80);
        spawn_config_watcher(path.clone(), debounce, Arc::clone(&active)).unwrap();
        thread::sleep(Duration::from_millis(300));

        let before = reload_count();
        let before_ptr = Arc::as_ptr(&active.load_full());
        let admin = crate::admin::ConfigAdmin::new(path.clone());
        admin
            .apply_config_text(&config_text("a2"), &active)
            .unwrap();
        let applied_ptr = Arc::as_ptr(&active.load_full());
        assert_ne!(before_ptr, applied_ptr, "the apply swapped the runtime");
        settle(debounce);
        assert_eq!(
            reload_count(),
            before,
            "the watcher reloaded an applied config"
        );
        assert_eq!(
            Arc::as_ptr(&active.load_full()),
            applied_ptr,
            "the live runtime must be the one the apply built"
        );
    }

    #[test]
    fn a_rejected_apply_leaves_the_file_and_the_runtime_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Prx.toml");
        let good = config_text("g1");
        std::fs::write(&path, &good).unwrap();
        let active = live(&path);
        let before_ptr = Arc::as_ptr(&active.load_full());
        let mtime = std::fs::metadata(&path).unwrap().modified().unwrap();
        let admin = crate::admin::ConfigAdmin::new(path.clone());
        let err = admin
            .apply_config_text("[server]\nlisten = 3\n", &active)
            .unwrap_err();
        assert!(!format!("{err:#}").is_empty());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), good);
        assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), mtime);
        assert_eq!(Arc::as_ptr(&active.load_full()), before_ptr);
    }
}
