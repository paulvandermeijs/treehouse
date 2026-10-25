//! Embeds the app icon in the Windows executable.

fn main() {
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
