//! Writing Treehouse's own files safely.

use std::io::Write as _;
use std::path::{Path, PathBuf};

/// Who may read a file `write` creates.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Access {
    /// The usual permissions, or the existing file's when it's replaced.
    Default,
    /// Only the owner (0600 on Unix), whatever the file had before. For
    /// files that hold credentials.
    Private,
}

/// Replaces `path` with `contents` through a temporary file in the same
/// folder and a rename, so a crash leaves the old file or the new one, never
/// half of it. Creates the folder if needed.
pub fn write(path: &Path, contents: impl AsRef<[u8]>, access: Access) -> std::io::Result<()> {
    // Writing through a symlink, such as settings kept in a dotfiles repo,
    // must replace its target rather than the link.
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let dir = match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.to_path_buf(),
        _ => PathBuf::from("."),
    };
    std::fs::create_dir_all(&dir)?;
    let name = path
        .file_name()
        .ok_or_else(|| std::io::Error::other("not a file path"))?
        .to_string_lossy();

    let (temp, mut file) = create_temp(&dir, &name, access)?;
    let result = (|| {
        file.write_all(contents.as_ref())?;
        #[cfg(unix)]
        if access == Access::Default
            && let Ok(existing) = std::fs::metadata(&path)
        {
            file.set_permissions(existing.permissions())?;
        }
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, &path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

/// A new file next to the target, opened exclusively so two writers never
/// share one, and created private from the start when it has to be.
fn create_temp(
    dir: &Path,
    name: &str,
    access: Access,
) -> std::io::Result<(PathBuf, std::fs::File)> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    loop {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let temp = dir.join(format!(".{name}.{}.{n}.tmp", std::process::id()));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(match access {
                Access::Private => 0o600,
                Access::Default => 0o666,
            });
        }
        #[cfg(not(unix))]
        let _ = access;
        match options.open(&temp) {
            Ok(file) => return Ok((temp, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
}

/// Creates a folder only its owner can open (0700 on Unix), and narrows an
/// existing one that's more open than that.
pub fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        builder.mode(0o700);
    }
    builder.create(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        // A symlinked folder is the user's choice; leave its target alone.
        let metadata = std::fs::symlink_metadata(dir)?;
        if metadata.is_dir() && metadata.permissions().mode() & 0o077 != 0 {
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Access, create_private_dir, write};

    #[test]
    fn replaces_the_file_and_leaves_no_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/settings.json");
        write(&path, "one", Access::Default).unwrap();
        write(&path, "two", Access::Default).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "two");
        let names: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["settings.json"]);
    }

    #[cfg(unix)]
    #[test]
    fn writes_through_a_symlink_to_its_target() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target.json");
        std::fs::write(&target, "old").unwrap();
        let link = dir.path().join("link.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        write(&link, "new", Access::Default).unwrap();
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
    }

    #[cfg(unix)]
    #[test]
    fn private_files_are_owner_only_and_others_keep_their_mode() {
        use std::os::unix::fs::PermissionsExt as _;
        let mode =
            |path: &std::path::Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        let dir = tempfile::tempdir().unwrap();

        let auth = dir.path().join("auth.json");
        std::fs::write(&auth, "{}").unwrap();
        std::fs::set_permissions(&auth, std::fs::Permissions::from_mode(0o644)).unwrap();
        write(&auth, "{\"key\":1}", Access::Private).unwrap();
        assert_eq!(mode(&auth), 0o600);

        let shared = dir.path().join("trust.json");
        std::fs::write(&shared, "{}").unwrap();
        std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o640)).unwrap();
        write(&shared, "{}", Access::Default).unwrap();
        assert_eq!(mode(&shared), 0o640);
    }

    #[cfg(unix)]
    #[test]
    fn private_folders_are_created_and_narrowed() {
        use std::os::unix::fs::PermissionsExt as _;
        let mode =
            |path: &std::path::Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        let dir = tempfile::tempdir().unwrap();
        let fresh = dir.path().join("a/treehouse");
        create_private_dir(&fresh).unwrap();
        assert_eq!(mode(&fresh), 0o700);

        let open = dir.path().join("open");
        std::fs::create_dir(&open).unwrap();
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o755)).unwrap();
        create_private_dir(&open).unwrap();
        assert_eq!(mode(&open), 0o700);
    }
}
