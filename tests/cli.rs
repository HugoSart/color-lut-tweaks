use std::process::Command;

#[test]
fn inspect_prints_lut_summary() {
    let output = Command::new(env!("CARGO_BIN_EXE_hdr-tweaks"))
        .arg("inspect")
        .arg("tests/fixtures/valid-xiaomi-27i-pro.lut")
        .output()
        .unwrap();

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Format: WORD[3][256], little-endian, 1536 bytes"));
    assert!(stdout.contains("  red: first     0, last 49816"));
    assert!(stdout.contains("green: first     0, last 50320"));
    assert!(stdout.contains(" blue: first     0, last 50787"));
}

#[test]
fn inspect_bad_path_fails() {
    let output = Command::new(env!("CARGO_BIN_EXE_hdr-tweaks"))
        .arg("inspect")
        .arg("tests/fixtures/missing.lut")
        .output()
        .unwrap();

    assert!(!output.status.success());

    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("failed to read"));
}

#[test]
fn inspect_malformed_lut_fails() {
    let output = Command::new(env!("CARGO_BIN_EXE_hdr-tweaks"))
        .arg("inspect")
        .arg("tests/fixtures/invalid-too-small.lut")
        .output()
        .unwrap();

    assert!(!output.status.success());

    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("expected 1536 bytes"));
}
