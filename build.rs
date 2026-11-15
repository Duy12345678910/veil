//! Renders assets/veil.svg into the Windows .ico (embedded in veil.exe), a raw RGBA
//! window icon, and a 256px PNG preview.

use std::path::{Path, PathBuf};

fn render(tree: &resvg::usvg::Tree, size: u32) -> resvg::tiny_skia::Pixmap {
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size).expect("pixmap");
    let scale = size as f32 / tree.size().width();
    resvg::render(tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    pixmap
}

fn write_ico(path: &Path, pngs: &[(u32, Vec<u8>)]) {
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(pngs.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * pngs.len() as u32;
    for (size, png) in pngs {
        let dim = if *size >= 256 { 0 } else { *size as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0, 1, 0, 32, 0]);
        out.extend_from_slice(&(png.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for (_, png) in pngs {
        out.extend_from_slice(png);
    }
    std::fs::write(path, out).expect("write ico");
}

fn main() {
    println!("cargo:rerun-if-changed=assets/veil.svg");
    println!("cargo:rerun-if-changed=build.rs");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    let svg = std::fs::read("assets/veil.svg").expect("read assets/veil.svg");
    let tree = resvg::usvg::Tree::from_data(&svg, &resvg::usvg::Options::default()).expect("parse svg");

    let pngs: Vec<(u32, Vec<u8>)> = [16, 20, 24, 32, 40, 48, 64, 128, 256]
        .iter()
        .map(|&s| (s, render(&tree, s).encode_png().expect("png")))
        .collect();
    let ico = out.join("veil.ico");
    write_ico(&ico, &pngs);
    std::fs::write(out.join("veil-256.png"), &pngs.last().unwrap().1).unwrap();

    // Straight (non-premultiplied) RGBA for the runtime window icon.
    let pix = render(&tree, 64);
    let rgba: Vec<u8> = pix.pixels().iter().flat_map(|p| {
        let c = p.demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    }).collect();
    std::fs::write(out.join("icon64.rgba"), rgba).unwrap();

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let rc = out.join("veil.rc");
        std::fs::write(&rc, format!("1 ICON \"{}\"\n", ico.display().to_string().replace('\\', "/"))).unwrap();
        embed_resource::compile(&rc, embed_resource::NONE).manifest_optional().unwrap();
    }
}
