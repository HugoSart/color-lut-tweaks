use color_lut_tweaks::updates::is_newer_version;

#[test]
fn newer_version_detects_plain_and_tagged_versions() {
    assert!(is_newer_version("0.2.1", "0.2.0"));
    assert!(is_newer_version("v0.3.0", "0.2.9"));
}

#[test]
fn newer_version_rejects_same_older_and_invalid_versions() {
    assert!(!is_newer_version("v0.2.0", "0.2.0"));
    assert!(!is_newer_version("0.1.9", "0.2.0"));
    assert!(!is_newer_version("not-a-version", "0.2.0"));
}
