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
