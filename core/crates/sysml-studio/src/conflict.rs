//! A digest of a source buffer, used to detect an edit made outside the canvas.
//!
//! The canvas holds the text it read, edits that text in memory, verifies the result by parsing,
//! and writes it back. Between the read and the write, a person may have saved the same file in
//! their own editor. Writing then replaces their work with a buffer that never saw it, and the
//! loss is silent — the file is valid SysML, so nothing downstream complains.
//!
//! The guard is a digest taken at read time and re-taken immediately before the write. Equal
//! digests mean nobody else touched the file and the write is safe; a difference means the
//! in-memory buffer is stale and the caller has to re-read and redo the edit against the new text.
//!
//! This is deliberately not a file mtime. Modification times have one-second granularity on some
//! filesystems and are updated by tools that only touch a file, so a same-second edit would slip
//! through — and the cost of missing a conflict is a silently reverted change, which is exactly
//! what this exists to prevent.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// What a buffer looked like when it was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Digest(pub u64);

impl Digest {
    pub fn of(text: &str) -> Self {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        Digest(hasher.finish())
    }
}

/// One file and the digest its content had when this buffer was read.
#[derive(Debug, Clone)]
pub struct Fingerprint {
    pub path: PathBuf,
    pub digest: Digest,
}

impl Fingerprint {
    /// Take the digest of what is on disk right now.
    ///
    /// A file that cannot be read yields `None` rather than an error: a deleted file is also a
    /// conflict, and the caller reports it the same way.
    pub fn now(path: &Path) -> Option<Digest> {
        std::fs::read_to_string(path).ok().map(|t| Digest::of(&t))
    }
}

/// The result of checking a buffer against the disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conflict {
    /// The file changed under us: someone else saved it after this buffer was read.
    Changed { path: PathBuf },
    /// The file is gone.
    Missing { path: PathBuf },
}

impl Conflict {
    /// The message a person reads. Says what happened and what to do, in that order — a conflict
    /// report that only says "stale" leaves the person to guess whether their edit was applied.
    pub fn message(&self) -> String {
        match self {
            Conflict::Changed { path } => format!(
                "{} changed on disk after the canvas read it. Nothing was written; \
                 the change is not lost, but this edit has to be made again against the new text.",
                path.display()
            ),
            Conflict::Missing { path } => format!(
                "{} is gone. Nothing was written; the model file has to exist before the canvas \
                 can edit it.",
                path.display()
            ),
        }
    }
}
