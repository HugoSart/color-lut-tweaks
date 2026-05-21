use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=luts");
    println!("cargo:rerun-if-changed=configs");
    println!("cargo:rerun-if-changed=profiles");
    println!("cargo:rerun-if-changed=icon.ico");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    embed_windows_icon(&manifest_dir);

    let profile_dir = profile_target_dir();
    copy_configs(&manifest_dir, &profile_dir);
    copy_icon(&manifest_dir, &profile_dir);
    copy_luts(&manifest_dir, &profile_dir);
    copy_profiles(&manifest_dir, &profile_dir);
}

fn copy_luts(manifest_dir: &Path, profile_dir: &Path) {
    let source_dir = manifest_dir.join("luts");
    if !source_dir.is_dir() {
        return;
    }

    let destination_dir = profile_dir.join("luts");
    fs::create_dir_all(&destination_dir).expect("create target luts directory");
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
    let icon = manifest_dir.join("icon.ico");
    if icon.is_file() {
        winres::WindowsResource::new()
            .set_icon(icon.to_str().expect("icon path should be valid UTF-8"))
            .compile()
            .expect("embed Windows icon resource");
    }
}

#[cfg(not(windows))]
fn embed_windows_icon(_manifest_dir: &Path) {}

fn copy_icon(manifest_dir: &Path, profile_dir: &Path) {
    let source = manifest_dir.join("icon.ico");
    if !source.is_file() {
        return;
    }

    fs::copy(source, profile_dir.join("icon.ico"))
        .expect("copy icon into target profile directory");
}

fn copy_configs(manifest_dir: &Path, profile_dir: &Path) {
    let source_dir = manifest_dir.join("configs");
    if !source_dir.is_dir() {
        return;
    }

    let destination_dir = profile_dir.join("configs");
    fs::create_dir_all(&destination_dir).expect("create target configs directory");
    for entry in fs::read_dir(&source_dir).expect("read configs directory") {
        let entry = entry.expect("read configs entry");
        let path = entry.path();
        if !is_config_asset(&path) {
            continue;
        }

        println!("cargo:rerun-if-changed={}", path.display());
        let destination = destination_dir.join(path.file_name().expect("config file name"));
        fs::copy(&path, &destination).expect("copy config into target profile directory");
    }
}

fn copy_profiles(manifest_dir: &Path, profile_dir: &Path) {
    let source_dir = manifest_dir.join("profiles");
    if !source_dir.is_dir() {
        return;
    }

    let destination_dir = profile_dir.join("profiles");
    fs::create_dir_all(&destination_dir).expect("create target profiles directory");
    for entry in fs::read_dir(&source_dir).expect("read profiles directory") {
        let entry = entry.expect("read profiles entry");
        let path = entry.path();
        if !is_profile_asset(&path) {
            continue;
        }

        println!("cargo:rerun-if-changed={}", path.display());
        let destination = destination_dir.join(path.file_name().expect("profile file name"));
        fs::copy(&path, &destination).expect("copy profile into target profile directory");
    }
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

fn is_config_asset(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
}

fn is_profile_asset(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("icc") || extension.eq_ignore_ascii_case("icm")
            })
}
