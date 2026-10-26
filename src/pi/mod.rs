pub mod auth;
pub mod rpc;

/// The bridge extension Treehouse loads into every Pi session.
pub const EXTENSION: &str = include_str!("../../assets/pi/treehouse.ts");

/// Writes the bridge extension where Pi can load it and returns its path.
pub fn extension_path() -> std::io::Result<std::path::PathBuf> {
    let dir = crate::config::Config::data_dir().join("pi");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("treehouse.ts");
    if std::fs::read_to_string(&path).ok().as_deref() != Some(EXTENSION) {
        std::fs::write(&path, EXTENSION)?;
    }
    Ok(path)
}

/// The built-in skills behind `/commit`, `/sync` and `/finish-branch`. The
/// `treehouse-` prefix keeps them from hiding a project's own skill of the
/// same name, since Pi keeps the first one it finds.
pub const SKILLS: [(&str, &str); 3] = [
    (
        "treehouse-commit",
        include_str!("../../assets/pi/skills/treehouse-commit/SKILL.md"),
    ),
    (
        "treehouse-sync",
        include_str!("../../assets/pi/skills/treehouse-sync/SKILL.md"),
    ),
    (
        "treehouse-finish-branch",
        include_str!("../../assets/pi/skills/treehouse-finish-branch/SKILL.md"),
    ),
];

/// Writes the built-in skills where Pi can load them and returns their
/// folders, for `--skill`.
pub fn skill_paths() -> std::io::Result<Vec<std::path::PathBuf>> {
    let root = crate::config::Config::data_dir().join("pi").join("skills");
    SKILLS
        .iter()
        .map(|(name, source)| {
            let dir = root.join(name);
            std::fs::create_dir_all(&dir)?;
            let path = dir.join("SKILL.md");
            if std::fs::read_to_string(&path).ok().as_deref() != Some(*source) {
                std::fs::write(&path, source)?;
            }
            Ok(dir)
        })
        .collect()
}
