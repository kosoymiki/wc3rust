//! Filesystem transactions for the Android checkpoint path (I-006/I-007/I-008).
//!
//! The host used to (a) pick the first checkpoint with readable metadata and
//! give up when that one failed later, (b) call a migration path that only
//! looked at the *current* package directory while calling it the old package's
//! sandbox, and (c) mark the background save as done even when the write
//! failed. This module holds the pure-std pieces so the behaviour is identical
//! in the app and in the host-side probe
//! (`analysis/full-audit-2026-11-18/probes/I-006/`), with no Android or game
//! types in the way.
//!
//! Invariants:
//! * a checkpoint pair (payload + meta) is written through temp files, fsynced,
//!   then renamed; a failure leaves the previous pair untouched;
//! * restore tries every candidate and reports each rejection with its path and
//!   reason, accepted only after metadata + verdict + digest + deserialize;
//! * migration reports an unreadable legacy source (another applicationId's
//!   private sandbox) instead of silently treating it as absent.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Result of a save attempt. `Failed` must leave the retry available: only
/// `Saved` and an explicit `Skipped` may suppress the next attempt.
#[derive(Debug)]
pub enum SaveOutcome {
    Saved,
    Skipped(&'static str),
    Failed(String),
}

impl SaveOutcome {
    /// Only a committed save or an explicit skip may suppress the retry.
    #[allow(dead_code)]
    pub fn suppress_retry(&self) -> bool {
        matches!(self, SaveOutcome::Saved | SaveOutcome::Skipped(_))
    }
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Temp sibling in the target directory (same filesystem, so `rename` is
/// atomic). Never named like a real checkpoint: resume candidates use exact
/// file names.
pub fn temp_sibling(path: &Path) -> PathBuf {
    let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".tmp-{}-{seq}", std::process::id()));
    PathBuf::from(name)
}

/// `<save>` plus the sidecar suffix (e.g. `.meta`).
pub fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// fsync a directory so a rename survives a power loss.
pub fn sync_dir(dir: &Path) -> io::Result<()> {
    fs::File::open(dir)?.sync_all()
}

fn discard(path: &Path) {
    let _ = fs::remove_file(path);
}

/// Write `bytes` to `path` atomically: temp file + fsync + rename + dir fsync.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let tmp = temp_sibling(path);
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    if let Err(error) = fs::rename(&tmp, path) {
        discard(&tmp);
        return Err(error);
    }
    sync_dir(parent)
}

/// Publish a checkpoint pair. `write_payload` writes the serialized sim into
/// the temp path; the meta is written from `meta_bytes`. Both are fsynced
/// before either rename; a failure removes the temp files and keeps the old
/// pair intact.
pub fn publish_pair(
    save: &Path,
    meta_bytes: &[u8],
    write_payload: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    let parent = save.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| format!("mkdir {}: {error}", parent.display()))?;
    let meta = sidecar(save, ".meta");
    let save_tmp = temp_sibling(save);
    let meta_tmp = temp_sibling(&meta);
    if let Err(error) = write_payload(&save_tmp) {
        discard(&save_tmp);
        return Err(format!("serialize: {error}"));
    }
    let staged = (|| -> io::Result<()> {
        fs::File::open(&save_tmp)?.sync_all()?;
        fs::write(&meta_tmp, meta_bytes)?;
        fs::File::open(&meta_tmp)?.sync_all()?;
        Ok(())
    })();
    if let Err(error) = staged {
        discard(&save_tmp);
        discard(&meta_tmp);
        return Err(format!("io: {error}"));
    }
    if let Err(error) = fs::rename(&save_tmp, save) {
        discard(&save_tmp);
        discard(&meta_tmp);
        return Err(format!("rename {}: {error}", save.display()));
    }
    if let Err(error) = fs::rename(&meta_tmp, &meta) {
        discard(&meta_tmp);
        return Err(format!("rename {}: {error}", meta.display()));
    }
    sync_dir(parent).map_err(|error| format!("fsync {}: {error}", parent.display()))
}

/// Try every candidate transactionally: read metadata, then let `accept` run
/// verdict + digest + deserialize and commit on success. A rejection logs
/// `GAME_CHECKPOINT_REJECT path=<p> reason=<r>` and moves to the next file.
/// Returns the accepted candidate, its metadata and the tick reported by
/// `accept` (the restored sim tick).
pub fn restore_candidates<M>(
    candidates: &[PathBuf],
    mut read_meta: impl FnMut(&Path) -> Result<M, String>,
    mut accept: impl FnMut(&Path, &M) -> Result<u64, String>,
    log: &mut impl FnMut(&str),
) -> Option<(PathBuf, M, u64)> {
    let mut rejected = 0usize;
    for path in candidates {
        if !path.is_file() {
            continue;
        }
        let meta = match read_meta(path) {
            Ok(meta) => meta,
            Err(reason) => {
                rejected += 1;
                log(&format!(
                    "GAME_CHECKPOINT_REJECT path={} reason={reason}",
                    path.display()
                ));
                continue;
            }
        };
        match accept(path, &meta) {
            Ok(tick) => return Some((path.clone(), meta, tick)),
            Err(reason) => {
                rejected += 1;
                log(&format!(
                    "GAME_CHECKPOINT_REJECT path={} reason={reason}",
                    path.display()
                ));
            }
        }
    }
    if rejected > 0 {
        log(&format!("GAME_CHECKPOINT_REJECT_ALL count={rejected}"));
    }
    None
}

/// Copy a legacy checkpoint pair into `target` when the target does not exist.
///
/// `sources` is ordered by trust: files this package already owns, then the
/// old package's external files dir, then its private sandbox (which Android
/// denies to another applicationId - reported as unreadable, not absent).
/// Returns the migrated source path.
pub fn migrate_pair(
    target: &Path,
    sources: &[PathBuf],
    log: &mut impl FnMut(&str),
) -> Option<PathBuf> {
    if target.is_file() {
        return None;
    }
    let target_meta = sidecar(target, ".meta");
    for source in sources {
        let payload = match fs::read(source) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                log(&format!(
                    "GAME_CHECKPOINT_MIGRATE_UNREADABLE path={} err={error}",
                    source.display()
                ));
                continue;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => {
                log(&format!(
                    "GAME_CHECKPOINT_MIGRATE_UNREADABLE path={} err={error}",
                    source.display()
                ));
                continue;
            }
        };
        if let Err(error) = write_atomic(target, &payload) {
            log(&format!(
                "GAME_CHECKPOINT_MIGRATE_FAILED from={} err={error}",
                source.display()
            ));
            continue;
        }
        let source_meta = sidecar(source, ".meta");
        if let Ok(meta) = fs::read(&source_meta) {
            if let Err(error) = write_atomic(&target_meta, &meta) {
                log(&format!(
                    "GAME_CHECKPOINT_MIGRATE_FAILED meta_from={} err={error}",
                    source_meta.display()
                ));
                continue;
            }
        } else {
            log(&format!(
                "GAME_CHECKPOINT_MIGRATE_NO_META from={}",
                source.display()
            ));
        }
        log(&format!(
            "GAME_CHECKPOINT_MIGRATED from={} to={}",
            source.display(),
            target.display()
        ));
        return Some(source.clone());
    }
    None
}
