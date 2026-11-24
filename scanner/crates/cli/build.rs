// Gives ba-key-AMAS.exe the project icon (Windows only; other targets skip it), and hands the add-on signing key from
// the local, uncommitted `.env` to the compiler (read at compile time by `option_env!` in src/upload.rs).
//
// The key is looked for in `BAOCR_ENV_FILE` if set, else in the nearest `.env` above this crate. A variable already
// present in the environment wins, which is how CI passes it. Nothing here prints the value.
use std::path::{Path, PathBuf};

const KEY: &str = "BAOCR_ADDON_SIGNING_KEY";

fn env_file() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("BAOCR_ENV_FILE") {
        let p = PathBuf::from(p);
        return p.is_file().then_some(p);
    }
    let mut dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR")?);
    for _ in 0..6 {
        let candidate = dir.join(".env");
        if candidate.is_file() {
            return Some(candidate);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

/// `NAME=value` lines; `#` comments and blank lines are skipped, one pair of quotes around the value is dropped.
fn value_of(text: &str, name: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (k, v) = line.trim().split_once('=')?;
        if k.trim() != name {
            return None;
        }
        let v = v.trim();
        let v = v.strip_prefix('"').and_then(|s| s.strip_suffix('"')).or_else(|| v.strip_prefix('\'').and_then(|s| s.strip_suffix('\''))).unwrap_or(v);
        Some(v.to_string())
    })
}

fn forward_signing_key() {
    println!("cargo:rerun-if-env-changed={KEY}");
    println!("cargo:rerun-if-env-changed=BAOCR_ENV_FILE");
    if std::env::var_os(KEY).is_some() {
        return;
    }
    let Some(path) = env_file() else { return };
    println!("cargo:rerun-if-changed={}", path.display());
    if let Some(value) = std::fs::read_to_string(Path::new(&path)).ok().and_then(|t| value_of(&t, KEY)).filter(|v| v.len() >= 16) {
        println!("cargo:rustc-env={KEY}={value}");
    }
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/ba-key-amas.ico");
    forward_signing_key();
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/ba-key-amas.ico");
        if let Err(e) = res.compile() {
            println!("cargo:warning=icon not embedded: {e}");
        }
    }
}
