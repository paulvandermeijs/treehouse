//! Renders the app icon from `assets/icon/treehouse.svg` into the files each
//! platform needs. Run it after changing the SVG:
//!
//!     cargo run --example render-icons
//!
//! - `assets/icon/png/{size}x{size}.png`: full-bleed PNGs, for Linux, the
//!   X11 window icon and cargo-bundle.
//! - `assets/icon/treehouse.icns`: macOS, with Apple's icon grid margin
//!   (an 824px tile on a 1024px canvas).
//! - `assets/icon/treehouse.ico`: Windows.

use resvg::{tiny_skia, usvg};
use std::path::Path;

const SIZES: [u32; 8] = [16, 32, 48, 64, 128, 256, 512, 1024];

fn render(tree: &usvg::Tree, size: u32, margin: f32) -> Vec<u8> {
    let mut pixmap = tiny_skia::Pixmap::new(size, size).expect("a non-empty icon size");
    let inner = size as f32 * (1.0 - 2.0 * margin);
    let scale = inner / tree.size().width().max(tree.size().height());
    let offset = size as f32 * margin;
    let transform = tiny_skia::Transform::from_scale(scale, scale).post_translate(offset, offset);
    resvg::render(tree, transform, &mut pixmap.as_mut());
    pixmap.encode_png().expect("PNG encoding")
}

/// An ICNS file of PNG entries, which macOS reads for every size.
fn icns(entries: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let body: usize = entries.iter().map(|(_, png)| png.len() + 8).sum();
    let mut out = Vec::with_capacity(body + 8);
    out.extend_from_slice(b"icns");
    out.extend_from_slice(&((body + 8) as u32).to_be_bytes());
    for (kind, png) in entries {
        out.extend_from_slice(*kind);
        out.extend_from_slice(&((png.len() + 8) as u32).to_be_bytes());
        out.extend_from_slice(png);
    }
    out
}

/// An ICO file of PNG entries (supported since Windows Vista).
fn ico(entries: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * entries.len();
    for (size, png) in entries {
        let dim = if *size >= 256 { 0 } else { *size as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(png.len() as u32).to_le_bytes());
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += png.len();
    }
    for (_, png) in entries {
        out.extend_from_slice(png);
    }
    out
}

fn main() -> std::io::Result<()> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/icon");
    let svg = std::fs::read(dir.join("treehouse.svg"))?;
    let tree = usvg::Tree::from_data(&svg, &usvg::Options::default()).expect("a valid icon SVG");

    std::fs::create_dir_all(dir.join("png"))?;
    for size in SIZES {
        std::fs::write(
            dir.join(format!("png/{size}x{size}.png")),
            render(&tree, size, 0.0),
        )?;
    }

    // Apple's grid puts an 824px tile on a 1024px canvas.
    let margin = (1024.0 - 824.0) / 2.0 / 1024.0;
    let mac = |size| render(&tree, size, margin);
    let entries: Vec<(&[u8; 4], Vec<u8>)> = vec![
        (b"icp4", mac(16)),
        (b"icp5", mac(32)),
        (b"icp6", mac(64)),
        (b"ic07", mac(128)),
        (b"ic08", mac(256)),
        (b"ic09", mac(512)),
        (b"ic10", mac(1024)),
        (b"ic11", mac(32)),
        (b"ic12", mac(64)),
        (b"ic13", mac(256)),
        (b"ic14", mac(512)),
    ];
    std::fs::write(dir.join("treehouse.icns"), icns(&entries))?;

    let windows: Vec<(u32, Vec<u8>)> = [16, 24, 32, 48, 64, 256]
        .iter()
        .map(|&s| (s, render(&tree, s, 0.0)))
        .collect();
    std::fs::write(dir.join("treehouse.ico"), ico(&windows))?;

    println!("Rendered icons into {}", dir.display());
    Ok(())
}
