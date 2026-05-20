use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=luts");
    println!("cargo:rerun-if-changed=metadata/icon.ico");
    println!("cargo:rerun-if-changed=configs/identity.config.json");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    embed_windows_icon(&manifest_dir);

    let source_dir = manifest_dir.join("luts");
    if !source_dir.is_dir() {
        return;
    }

    let destination_dir = profile_target_dir().join("luts");
    fs::create_dir_all(&destination_dir).expect("create target luts directory");
    copy_default_config(&manifest_dir, &destination_dir);
    copy_icon(&manifest_dir, &destination_dir);

    for entry in fs::read_dir(&source_dir).expect("read luts directory") {
        let entry = entry.expect("read luts entry");
        let path = entry.path();
        if !is_lut_asset(&path) {
            continue;
        }

        println!("cargo:rerun-if-changed={}", path.display());
        let destination = destination_dir.join(path.file_name().expect("lut file name"));
        fs::copy(&path, &destination).expect("copy LUT into target profile directory");
    }
}

#[cfg(windows)]
fn embed_windows_icon(manifest_dir: &Path) {
    let icon = manifest_dir.join("metadata").join("icon.ico");
    if icon.is_file() {
        winres::WindowsResource::new()
            .set_icon(icon.to_str().expect("icon path should be valid UTF-8"))
            .compile()
            .expect("embed Windows icon resource");
    }
}

#[cfg(not(windows))]
fn embed_windows_icon(_manifest_dir: &Path) {}

fn copy_icon(manifest_dir: &Path, luts_destination_dir: &Path) {
    let source = manifest_dir.join("metadata").join("icon.ico");
    if !source.is_file() {
        return;
    }

    let profile_dir = luts_destination_dir
        .parent()
        .expect("luts destination should have a profile parent directory");
    fs::copy(source, profile_dir.join("icon.ico"))
        .expect("copy icon into target profile directory");
}

fn copy_default_config(manifest_dir: &Path, luts_destination_dir: &Path) {
    let profile_dir = luts_destination_dir
        .parent()
        .expect("luts destination should have a profile parent directory");
    let config_path = profile_dir.join("config.json");
    if config_path.exists() {
        return;
    }

    let source = manifest_dir.join("configs").join("identity.config.json");
    fs::copy(source, config_path).expect("copy identity.config.json into target profile directory");
}

fn profile_target_dir() -> PathBuf {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));

    // OUT_DIR is target/<profile>/build/<metadata-hash>/out.
    out_dir
        .ancestors()
        .nth(3)
        .expect("OUT_DIR should be under target/<profile>/build/<metadata>/out")
        .to_path_buf()
}

fn is_lut_asset(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("lut") || extension.eq_ignore_ascii_case("cube")
            })
}
