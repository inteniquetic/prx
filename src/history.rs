//! Config history (T202) and the admin audit log (T206).
//!
//! Both live next to `Prx.toml`, mode 0600 in a 0700 directory, and are bounded:
//! a control plane that can be asked to write forever is its own outage.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, bail};
use serde::Serialize;

const DEFAULT_MAX_VERSIONS: usize = 50;
const AUDIT_MAX_BYTES: u64 = 5 * 1024 * 1024;
const AUDIT_KEEP: usize = 3;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Not cryptographic: it names a version and detects "same bytes", exactly what
/// the ETag already does.
pub fn fingerprint(bytes: &[u8]) -> String {
    use std::hash::Hasher;
    let mut hasher = rustc_hash::FxHasher::default();
    hasher.write(bytes);
    format!("{:x}-{:016x}", bytes.len(), hasher.finish())
}

/// Fingerprint of the config text the running config was last built from, by
/// whoever built it (the admin API or the file watcher). The watcher uses it to
/// not reload what is already live: a second reload would rebuild the runtime
/// and with it every circuit breaker and health state.
static APPLIED: Mutex<Option<String>> = Mutex::new(None);

pub fn mark_applied(bytes: &[u8]) {
    if let Ok(mut applied) = APPLIED.lock() {
        *applied = Some(fingerprint(bytes));
    }
}

pub fn is_applied(bytes: &[u8]) -> bool {
    APPLIED
        .lock()
        .is_ok_and(|applied| applied.as_deref() == Some(fingerprint(bytes).as_str()))
}

fn private_file(path: &Path) -> std::io::Result<fs::File> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

fn private_dir(path: &Path) -> std::io::Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Version {
    /// `<unix ms>-<fingerprint>`; what rollback is asked for.
    pub id: String,
    pub ts_ms: u64,
    pub fingerprint: String,
    pub bytes: u64,
}

#[derive(Clone)]
pub struct History {
    dir: PathBuf,
    max: usize,
    lock: Arc<Mutex<()>>,
}

impl History {
    pub fn beside(config_path: &Path) -> Self {
        let parent = config_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let max = std::env::var("PRX_CONFIG_HISTORY_MAX")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|n| *n > 0)
            .unwrap_or(DEFAULT_MAX_VERSIONS);
        Self {
            dir: parent.join(".prx-history"),
            max,
            lock: Arc::new(Mutex::new(())),
        }
    }

    fn file(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.toml"))
    }

    /// Newest first.
    pub fn list(&self) -> Vec<Version> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut versions: Vec<Version> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name().into_string().ok()?;
                let id = name.strip_suffix(".toml")?.to_string();
                let (ts, rest) = id.split_once('-')?;
                Some(Version {
                    ts_ms: ts.parse().ok()?,
                    fingerprint: rest.to_string(),
                    bytes: entry.metadata().ok()?.len(),
                    id,
                })
            })
            .collect();
        versions.sort_by(|a, b| b.id.cmp(&a.id));
        versions
    }

    pub fn read(&self, id: &str) -> anyhow::Result<String> {
        // The id becomes a file name: nothing but what `record` produces.
        if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            bail!("invalid history id");
        }
        fs::read_to_string(self.file(id)).with_context(|| format!("no config version {id}"))
    }

    /// Keeps `new` as a version. The first time, the file as it was before this
    /// change is kept too, so the very first apply can be undone. Identical
    /// consecutive content is not stored twice.
    pub fn record(&self, previous: Option<&[u8]>, new: &[u8]) -> anyhow::Result<()> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| anyhow::anyhow!("history lock poisoned"))?;
        private_dir(&self.dir)?;
        let existing = self.list();
        let now = now_ms();
        if existing.is_empty()
            && let Some(previous) = previous
            && previous != new
        {
            self.write(now.saturating_sub(1), previous)?;
        }
        let latest = existing.first().map(|v| v.fingerprint.clone());
        if latest.as_deref() != Some(fingerprint(new).as_str()) {
            // Never reuse an id: two writes in one millisecond must not collide.
            let ts = existing
                .first()
                .map_or(now, |v| now.max(v.ts_ms.saturating_add(1)));
            self.write(ts, new)?;
        }
        let versions = self.list();
        for old in versions.iter().skip(self.max) {
            let _ = fs::remove_file(self.file(&old.id));
        }
        Ok(())
    }

    fn write(&self, ts_ms: u64, bytes: &[u8]) -> anyhow::Result<()> {
        let id = format!("{ts_ms:013}-{}", fingerprint(bytes));
        let tmp = self.dir.join(format!(".{id}.tmp"));
        let _ = fs::remove_file(&tmp);
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&tmp, self.file(&id))?;
        Ok(())
    }
}

/// One line of the audit log. No request body and no credential is ever in it;
/// the config is referred to by fingerprint, matching a [`Version`].
#[derive(Debug, Serialize)]
pub struct AuditEntry<'a> {
    pub ts_ms: u64,
    pub actor: &'a str,
    pub method: &'a str,
    pub path: &'a str,
    pub status: u16,
    pub before: &'a str,
    pub after: &'a str,
}

#[derive(Clone)]
pub struct AuditLog {
    path: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl AuditLog {
    pub fn beside(config_path: &Path) -> Self {
        let parent = config_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        Self {
            path: parent.join(".prx-audit.log"),
            lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn append(&self, entry: &AuditEntry<'_>) -> anyhow::Result<()> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| anyhow::anyhow!("audit lock poisoned"))?;
        if fs::metadata(&self.path).is_ok_and(|m| m.len() >= AUDIT_MAX_BYTES) {
            self.rotate();
        }
        let mut line = serde_json::to_vec(entry)?;
        line.push(b'\n');
        private_file(&self.path)?.write_all(&line)?;
        Ok(())
    }

    fn rotate(&self) {
        let numbered = |n: usize| PathBuf::from(format!("{}.{n}", self.path.display()));
        let _ = fs::remove_file(numbered(AUDIT_KEEP));
        for n in (1..AUDIT_KEEP).rev() {
            let _ = fs::rename(numbered(n), numbered(n + 1));
        }
        let _ = fs::rename(&self.path, numbered(1));
    }

    /// The most recent `limit` entries, oldest first, as raw JSON values.
    pub fn tail(&self, limit: usize) -> Vec<serde_json::Value> {
        let text = fs::read_to_string(&self.path).unwrap_or_default();
        let lines: Vec<&str> = text.lines().collect();
        lines[lines.len().saturating_sub(limit)..]
            .iter()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("Prx.toml");
        (dir, config)
    }

    #[test]
    fn three_applies_keep_three_versions_plus_the_original() {
        let (_dir, config) = temp();
        let history = History::beside(&config);
        let originals = [b"a = 1\n".as_slice(), b"a = 2\n", b"a = 3\n"];
        let mut previous = b"a = 0\n".to_vec();
        for text in originals {
            history.record(Some(&previous), text).unwrap();
            previous = text.to_vec();
        }
        let versions = history.list();
        assert_eq!(versions.len(), 4, "{versions:?}");
        // Newest first, and the very first version can be read back.
        assert_eq!(history.read(&versions[0].id).unwrap(), "a = 3\n");
        assert_eq!(history.read(&versions[3].id).unwrap(), "a = 0\n");
    }

    #[test]
    fn identical_content_is_not_stored_twice() {
        let (_dir, config) = temp();
        let history = History::beside(&config);
        history.record(None, b"x\n").unwrap();
        history.record(Some(b"x\n"), b"x\n").unwrap();
        assert_eq!(history.list().len(), 1);
    }

    #[test]
    fn history_is_bounded_and_private() {
        let (_dir, config) = temp();
        let mut history = History::beside(&config);
        history.max = 5;
        for n in 0..12 {
            history
                .record(None, format!("n = {n}\n").as_bytes())
                .unwrap();
        }
        let versions = history.list();
        assert_eq!(versions.len(), 5);
        assert_eq!(history.read(&versions[0].id).unwrap(), "n = 11\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(history.file(&versions[0].id))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
            let dir_mode = fs::metadata(&history.dir).unwrap().permissions().mode();
            assert_eq!(dir_mode & 0o777, 0o700);
        }
    }

    #[test]
    fn a_history_id_cannot_name_another_file() {
        let (_dir, config) = temp();
        let history = History::beside(&config);
        assert!(history.read("../Prx").is_err());
        assert!(history.read("a/b").is_err());
        assert!(history.read("").is_err());
    }

    #[test]
    fn audit_log_rotates_and_keeps_the_tail() {
        let (_dir, config) = temp();
        let log = AuditLog::beside(&config);
        let entry = |n: u16| AuditEntry {
            ts_ms: 1,
            actor: "token",
            method: "PUT",
            path: "/web/config",
            status: n,
            before: "a",
            after: "b",
        };
        for n in 0..5 {
            log.append(&entry(200 + n)).unwrap();
        }
        let tail = log.tail(3);
        assert_eq!(tail.len(), 3);
        assert_eq!(tail[2]["status"], 204);
        // Force a rotation without writing 5 MiB.
        fs::write(&log.path, vec![b'x'; AUDIT_MAX_BYTES as usize]).unwrap();
        log.append(&entry(500)).unwrap();
        assert!(Path::new(&format!("{}.1", log.path.display())).exists());
        assert_eq!(log.tail(10).len(), 1);
    }
}
