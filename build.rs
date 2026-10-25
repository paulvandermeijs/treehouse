//! Embeds the app icon in the Windows executable, and records the commit
//! the build came from for the About window.

fn main() {
    let commit = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    println!("cargo:rustc-env=TREEHOUSE_COMMIT={commit}");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads");

    println!("cargo:rerun-if-changed=assets/icon/treehouse.ico");
    #[cfg(windows)]
    {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("assets/icon/treehouse.ico");
        if let Err(err) = resource.compile() {
            println!("cargo:warning=couldn't embed the app icon: {err}");
        }
    }
}
