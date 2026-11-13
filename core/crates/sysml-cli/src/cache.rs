//! A memo of the last `check`, so an unchanged project is not re-checked.
//!
//! What this is: if every input is byte-identical to the last run, the answer is too, so the run
//! prints the stored one instead of computing it again. What this is not: incrementality. One file
//! changes and the whole project is re-checked.
//!
//! That is a deliberate limit, not a shortcut. The measured split at twelve thousand elements is
//! 126ms to parse and lower, 564ms to resolve, 1846ms for `check` — so a per-file parse cache could
//! save at most five percent, while the rule pass over the merged model is sixty-nine. There is no
//! per-file answer to cache: one file's rules depend on every other file's names.
//!
//! The key covers every input that can change the answer: the sources and the library by path and
//! content, the ledger, and a format version. Miss one and the memo reports yesterday's model.
//!
//! The digest is FNV-1a over those bytes. It detects change; it does not resist an attacker. An
//! attacker who can rewrite the model can also rewrite the memo, and neither is a boundary here.

use std::path::Path;

/// Bump when the report's text or the key's inputs change. A stale memo is a wrong answer, and a
/// wrong answer about a model is the one thing this tool exists to prevent.
const FORMAT: u32 = 1;

fn fnv(bytes: &[u8], seed: u64) -> u64 {
    let mut hash = seed;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Fold one file into the digest: its path, its length, and its bytes.
fn fold(hash: u64, path: &Path, bytes: Option<&[u8]>) -> u64 {
    let name = path.to_string_lossy();
    let mut hash = fnv(name.as_bytes(), hash);
    match bytes {
        Some(bytes) => {
            hash = fnv(&(bytes.len() as u64).to_le_bytes(), hash);
            fnv(bytes, hash)
        }
        // A file that was there and is not now is a different input from one that never was.
        None => fnv(b"absent", hash),
    }
}

/// The digest of everything a `check` run reads, plus which output it asked for.
pub fn key(
    sources: &[std::path::PathBuf],
    library: &[std::path::PathBuf],
    ledger: &Path,
    json: bool,
) -> u64 {
    let mut hash = fnv(&FORMAT.to_le_bytes(), 0xcbf2_9ce4_8422_2325);
    hash = fnv(if json { b"json" } else { b"text" }, hash);
    for path in sources.iter().chain(library.iter()) {
        let bytes = std::fs::read(path).ok();
        hash = fold(hash, path, bytes.as_deref());
    }
    hash = fold(hash, ledger, std::fs::read(ledger).ok().as_deref());
    hash
}

fn path_in(root: &Path) -> std::path::PathBuf {
    root.join(".sysml-cache").join("check.json")
}

/// What the last run printed, when the key matches.
pub fn lookup(root: &Path, key: u64) -> Option<(String, u8)> {
    let text = std::fs::read_to_string(path_in(root)).ok()?;
    let doc: serde_json::Value = serde_json::from_str(&text).ok()?;
    if doc["key"].as_u64()? != key {
        return None;
    }
    let exit = doc["exit"].as_u64()? as u8;
    Some((doc["report"].as_str()?.to_string(), exit))
}

/// Remember this run's answer.
///
/// A cache that cannot be written is not a failure: the answer was already computed and printed, and
/// refusing to finish because a dot-directory could not be created would trade a working check for a
/// tidy one.
pub fn store(root: &Path, key: u64, report: &str, exit: u8) {
    let path = path_in(root);
    let Some(parent) = path.parent() else { return };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let doc = serde_json::json!({ "key": key, "exit": exit, "report": report });
    let _ = std::fs::write(path, doc.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Scratch {
            let path =
                std::env::temp_dir().join(format!("sysml-cache-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("scratch");
            Scratch(path)
        }

        fn write(&self, name: &str, text: &str) -> PathBuf {
            let path = self.0.join(name);
            std::fs::write(&path, text).expect("write");
            path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn key_of(source: &PathBuf, ledger: &PathBuf) -> u64 {
        key(&[source.clone()], &[], ledger, true)
    }

    #[test]
    fn changing_a_source_changes_the_key() {
        let scratch = Scratch::new("source");
        let ledger = scratch.0.join("m.ledger.json");
        let source = scratch.write("m.sysml", "package A {}");
        let before = key_of(&source, &ledger);
        scratch.write("m.sysml", "package A { part p; }");
        assert_ne!(
            before,
            key_of(&source, &ledger),
            "a changed model must miss"
        );
    }

    #[test]
    fn changing_the_ledger_changes_the_key() {
        // The report quotes the ledger's own state — "its ledger has not been started" becomes a
        // verdict once someone records a decision. A key over the model alone would replay the old
        // sentence after the ledger moved.
        let scratch = Scratch::new("ledger");
        let source = scratch.write("m.sysml", "package A {}");
        let ledger = scratch.write("m.ledger.json", "{}");
        let before = key_of(&source, &ledger);
        scratch.write("m.ledger.json", r#"{"decisions": []}"#);
        assert_ne!(before, key_of(&source, &ledger));
    }

    #[test]
    fn the_machine_answer_and_the_human_one_are_different_entries() {
        let scratch = Scratch::new("variant");
        let source = scratch.write("m.sysml", "package A {}");
        let ledger = scratch.0.join("m.ledger.json");
        assert_ne!(
            key(&[source.clone()], &[], &ledger, true),
            key(&[source], &[], &ledger, false)
        );
    }

    #[test]
    fn a_stored_answer_comes_back_with_its_exit_code() {
        let scratch = Scratch::new("roundtrip");
        store(&scratch.0, 42, "report text", 1);
        assert_eq!(lookup(&scratch.0, 42), Some(("report text".to_string(), 1)));
    }

    #[test]
    fn a_different_key_does_not_match() {
        let scratch = Scratch::new("miss");
        store(&scratch.0, 42, "report text", 0);
        assert_eq!(lookup(&scratch.0, 43), None);
    }

    #[test]
    fn an_unwritable_root_is_not_a_failure() {
        // The answer was computed and about to be printed; refusing to finish because a
        // dot-directory could not be created would trade a working check for a tidy one. A file
        // where the directory should go is the portable way to make the write fail.
        let scratch = Scratch::new("blocked");
        let blocked = scratch.write("blocked", "not a directory");
        store(&blocked, 1, "report", 0);
        assert_eq!(lookup(&blocked, 1), None);
    }
}
