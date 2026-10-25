//! Project trust, shared with Pi through its `trust.json`.
//!
//! The file maps canonical folder paths to `true` (trusted) or `false`; the
//! closest entry at or above a folder decides. Pi reads the same file, so a
//! decision made in Treehouse also holds for `pi` in a terminal. Treehouse
//! passes the decision to Pi explicitly (`--approve` / `--no-approve`) as well,
//! because worktrees can live outside the project folder.

use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trust {
    Trusted,
    /// The user chose read-only.
    NotTrusted,
    /// No decision yet; treated as read-only until the user decides.
    Undecided,
}

impl Trust {
    pub fn is_trusted(self) -> bool {
        self == Trust::Trusted
    }
}

/// The user's choice in the trust dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Trust,
    /// Trust every folder in the project's parent folder.
    TrustParent,
    ReadOnly,
}

pub fn store_path() -> Option<PathBuf> {
    crate::pi::auth::agent_dir().map(|dir| dir.join("trust.json"))
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn read(store: &Path) -> Map<String, Value> {
    std::fs::read_to_string(store)
        .ok()
        .and_then(|json| serde_json::from_str::<Value>(json.trim_start_matches('\u{feff}')).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

/// The decision for a folder: the closest saved entry at or above it.
pub fn lookup_in(store: &Path, folder: &Path) -> Trust {
    let data = read(store);
    let mut current = Some(canonical(folder));
    while let Some(dir) = current {
        match data.get(&dir.display().to_string()) {
            Some(Value::Bool(true)) => return Trust::Trusted,
            Some(Value::Bool(false)) => return Trust::NotTrusted,
            _ => {}
        }
        current = dir.parent().map(Path::to_path_buf);
    }
    Trust::Undecided
}

pub fn lookup(folder: &Path) -> Trust {
    store_path().map_or(Trust::Undecided, |store| lookup_in(&store, folder))
}

/// Saves a decision the way Pi's own trust prompt does.
pub fn save_in(store: &Path, folder: &Path, decision: Decision) -> std::io::Result<()> {
    let folder = canonical(folder);
    let mut data = read(store);
    let key = |path: &Path| path.display().to_string();
    match decision {
        Decision::Trust => {
            data.insert(key(&folder), Value::Bool(true));
        }
        Decision::ReadOnly => {
            data.insert(key(&folder), Value::Bool(false));
        }
        Decision::TrustParent => {
            let parent = folder.parent().unwrap_or(&folder).to_path_buf();
            data.insert(key(&parent), Value::Bool(true));
            data.insert(key(&folder), Value::Null);
        }
    }
    let sorted: std::collections::BTreeMap<_, _> = data.into_iter().collect();
    if let Some(dir) = store.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Pi takes a `proper-lockfile` lock, a `<file>.lock` directory, while it
    // writes; take the same one so the two never interleave.
    let lock = store.with_extension("json.lock");
    let locked = (0..20).any(|_| {
        std::fs::create_dir(&lock).is_ok() || {
            std::thread::sleep(std::time::Duration::from_millis(50));
            false
        }
    });
    let result = std::fs::write(
        store,
        format!("{}\n", serde_json::to_string_pretty(&sorted)?),
    );
    if locked {
        let _ = std::fs::remove_dir(&lock);
    }
    result
}

pub fn save(folder: &Path, decision: Decision) -> std::io::Result<()> {
    let store = store_path().ok_or_else(|| std::io::Error::other("no home folder"))?;
    save_in(&store, folder, decision)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closest_decision_wins() {
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("agent/trust.json");
        let clients = dir.path().join("code/clients");
        let api = clients.join("gadero-api");
        std::fs::create_dir_all(&api).unwrap();

        assert_eq!(lookup_in(&store, &api), Trust::Undecided);
        save_in(&store, &api, Decision::ReadOnly).unwrap();
        assert_eq!(lookup_in(&store, &api), Trust::NotTrusted);

        // Trusting the parent clears the folder's own entry, as Pi does.
        save_in(&store, &api, Decision::TrustParent).unwrap();
        assert_eq!(lookup_in(&store, &api), Trust::Trusted);
        assert_eq!(lookup_in(&store, &clients.join("other")), Trust::Trusted);
        let saved: Value = serde_json::from_str(&std::fs::read_to_string(&store).unwrap()).unwrap();
        assert_eq!(saved[canonical(&api).display().to_string()], Value::Null);
        assert!(!store.with_extension("json.lock").exists());

        save_in(&store, &api, Decision::ReadOnly).unwrap();
        assert_eq!(lookup_in(&store, &api), Trust::NotTrusted);
    }
}
