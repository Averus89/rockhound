//! Embeds the labelled readout crops in `data/glyphs/<scale>/<value>-<n>.png`
//! so the app can read a known HUD scale on first run, before anything has
//! been learned.

use std::fmt::Write;
use std::path::Path;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/glyphs");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files: Vec<(u32, String)> = Vec::new();
    for scale in std::fs::read_dir(&root)
        .expect("data/glyphs exists")
        .flatten()
    {
        println!("cargo:rerun-if-changed={}", scale.path().display());
        for file in std::fs::read_dir(scale.path()).expect("readable").flatten() {
            let path = file.path();
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let Some(value) = name
                .strip_suffix(".png")
                .and_then(|stem| stem.split('-').next())
                .and_then(|v| v.parse().ok())
            else {
                panic!(
                    "glyph file must be named <value>-<n>.png: {}",
                    path.display()
                );
            };
            files.push((value, path.display().to_string()));
        }
    }
    // Sorted so builds are reproducible.
    files.sort();
    let mut code = String::from("pub static GLYPHS: &[(u32, &[u8])] = &[\n");
    for (value, path) in &files {
        writeln!(code, "    ({value}, include_bytes!({path:?})),").expect("write to string");
    }
    code.push_str("];\n");
    let out = Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR set")).join("glyphs.rs");
    std::fs::write(out, code).expect("write glyphs.rs");
}
