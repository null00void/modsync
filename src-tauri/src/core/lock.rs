use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Guards a single profile against a second sync starting while one is
/// already in progress. `create_new(true)` makes acquisition atomic --
/// two processes racing to create the same lock file can only ever have
/// one of them succeed.
pub struct ProfileLock {
    path: PathBuf,
}

/// Locks older than this are assumed to be from a crashed run, not an
/// actually-running sync, and can be taken over.
const STALE_AFTER_SECS: u64 = 10 * 60;

impl ProfileLock {
    pub fn acquire(profile_path: &Path) -> Result<Self, String> {
        let lock_path = profile_path.join(".modsync.lock");

        if lock_path.is_file() {
            let age = fs::metadata(&lock_path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .map(|d| d.as_secs());

            match age {
                Some(secs) if secs > STALE_AFTER_SECS => {
                    // Stale -- a previous run almost certainly crashed
                    // without cleaning up. Safe to take over.
                    let _ = fs::remove_file(&lock_path);
                }
                _ => {
                    return Err(
                        "a sync already appears to be running for this profile (lock file present); wait for it to finish or try again shortly".to_string(),
                    );
                }
            }
        }

        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
            .map_err(|e| format!("could not acquire sync lock: {e}"))?;
        let _ = write!(file, "{}", std::process::id());

        Ok(Self { path: lock_path })
    }
}

impl Drop for ProfileLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
