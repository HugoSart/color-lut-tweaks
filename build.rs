use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=luts");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let source_dir = manifest_dir.join("luts");
    if !source_dir.is_dir() {
        return;
    }

    let destination_dir = profile_target_dir().join("luts");
    fs::create_dir_all(&destination_dir).expect("create target luts directory");
    create_default_config(&destination_dir);

    for entry in fs::read_dir(&source_dir).expect("read luts directory") {
        let entry = entry.expect("read luts entry");
        let path = entry.path();
        if !is_lut_file(&path) {
            continue;
        }

        println!("cargo:rerun-if-changed={}", path.display());
        let destination = destination_dir.join(path.file_name().expect("lut file name"));
        fs::copy(&path, &destination).expect("copy LUT into target profile directory");
    }
}

fn create_default_config(luts_destination_dir: &Path) {
    let profile_dir = luts_destination_dir
        .parent()
        .expect("luts destination should have a profile parent directory");
    let config_path = profile_dir.join("config.json");
    if config_path.exists() {
        return;
    }

    fs::write(config_path, "[]\n").expect("write default config.json");
}

fn profile_target_dir() -> PathBuf {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));

    // OUT_DIR is target/<profile>/build/<package-hash>/out.
    out_dir
        .ancestors()
        .nth(3)
        .expect("OUT_DIR should be under target/<profile>/build/<package>/out")
        .to_path_buf()
}

fn is_lut_file(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("lut"))
}
