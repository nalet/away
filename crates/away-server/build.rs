//! Puts the pages into the program: every file of `web/dist`, which `npm run build`
//! makes in `web/`, becomes part of the server, so the image holds one file and the
//! server reads nothing from a disk. Where the pages aren't built, the server holds
//! none and says so on its one page.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn main() {
    let dist = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("the crate's folder")).join("../../web/dist");
    println!("cargo:rerun-if-changed={}", dist.display());
    let mut files = Vec::new();
    collect(&dist, &dist, &mut files);
    files.sort();
    let mut code = String::from("/// The pages' files: the address of each, what it is, and what it holds.\n");
    code.push_str("pub static FILES: &[(&str, &str, &[u8])] = &[\n");
    for (address, path) in &files {
        println!("cargo:rerun-if-changed={}", path.display());
        let _ = writeln!(code, "    ({address:?}, {:?}, include_bytes!({:?})),", kind(path), path.display().to_string());
    }
    code.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("where built files go")).join("pages.rs");
    std::fs::write(out, code).expect("the list of the pages' files is written");
}

/// Every file below a folder, with the address it is served at.
fn collect(root: &Path, folder: &Path, files: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, files);
        } else if let Ok(below) = path.strip_prefix(root) {
            let address = format!("/{}", below.components().map(|part| part.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"));
            files.push((address, path.canonicalize().unwrap_or(path)));
        }
    }
}

/// What a file is, by the end of its name.
fn kind(path: &Path) -> &'static str {
    match path.extension().and_then(|end| end.to_str()).unwrap_or_default() {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "wasm" => "application/wasm",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}
