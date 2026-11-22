//! Atomic per-job snapshots. A lifetime lock prevents two processes recovering
//! each other's live jobs when a CLI and gateway share a configuration.
use super::BackgroundJobSnapshot;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;

pub(super) struct JobStore {
    directory: PathBuf,
    _lock: File,
}

impl JobStore {
    pub(super) fn open(directory: PathBuf) -> io::Result<Self> {
        std::fs::create_dir_all(&directory)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join("store.lock"))?;
        // Rust's File::try_lock still returns Unsupported on Android. Use the
        // native flock operation there, matching std's Linux locking semantics.
        #[cfg(target_os = "android")]
        {
            use std::os::fd::AsRawFd;
            // SAFETY: the descriptor remains open for the store's lifetime.
            if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == -1 {
                return Err(io::Error::last_os_error());
            }
        }
        #[cfg(not(target_os = "android"))]
        lock.try_lock().map_err(io::Error::other)?;
        Ok(Self {
            directory,
            _lock: lock,
        })
    }

    pub(super) fn load(&self) -> Vec<BackgroundJobSnapshot> {
        let mut snapshots = Vec::new();
        let Ok(entries) = std::fs::read_dir(&self.directory) else {
            return snapshots;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|v| v.to_str()) != Some("json") {
                continue;
            }
            let result = (|| -> anyhow::Result<BackgroundJobSnapshot> {
                let metadata = std::fs::symlink_metadata(&path)?;
                anyhow::ensure!(
                    metadata.is_file() && metadata.len() <= 64 * 1024 * 1024,
                    "invalid job snapshot file"
                );
                let snapshot: BackgroundJobSnapshot =
                    serde_json::from_slice(&std::fs::read(&path)?)?;
                anyhow::ensure!(
                    uuid::Uuid::parse_str(&snapshot.job_id).is_ok()
                        && path.file_stem().and_then(|v| v.to_str())
                            == Some(snapshot.job_id.as_str()),
                    "invalid job snapshot ID"
                );
                Ok(snapshot)
            })();
            match result {
                Ok(snapshot) => snapshots.push(snapshot),
                Err(error) => tracing::warn!(%error, "Ignoring damaged background job snapshot"),
            }
        }
        snapshots
    }

    pub(super) fn save(&self, snapshot: &BackgroundJobSnapshot) -> io::Result<()> {
        let mut temporary = tempfile::NamedTempFile::new_in(&self.directory)?;
        serde_json::to_writer(temporary.as_file_mut(), snapshot)?;
        temporary.flush()?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(self.directory.join(format!("{}.json", snapshot.job_id)))
            .map_err(|e| e.error)?;
        Ok(())
    }

    pub(super) fn remove(&self, id: &str) {
        if let Err(error) = std::fs::remove_file(self.directory.join(format!("{id}.json")))
            && error.kind() != io::ErrorKind::NotFound
        {
            tracing::warn!(%error, "Could not prune background job snapshot");
        }
    }
}
