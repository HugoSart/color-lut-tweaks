use std::path::PathBuf;

use color_lut_tweaks::app::{self, ColorMode, TweakOptions};
use color_lut_tweaks::lut::GammaRamp;

#[test]
fn start_config_loads_tweak_options_list() {
    let tweaks = TweakOptions::list_from_config_file("tests/fixtures/start-config.json").unwrap();

    assert_eq!(
        tweaks,
        vec![
            TweakOptions {
                device: Some(0),
                mode: Some(ColorMode::Hdr),
                lut: Some(PathBuf::from("tests/fixtures/valid-xiaomi-27i-pro.lut")),
            },
            TweakOptions {
                device: Some(0),
                mode: Some(ColorMode::Sdr),
                lut: Some(PathBuf::from("tests/fixtures/valid-xiaomi-27i-pro.lut")),
            },
        ]
    );
}

#[test]
fn identity_lut_is_loaded_from_reserved_name() {
    let ramp = app::load_lut("identity").unwrap();

    assert_eq!(ramp, GammaRamp::identity());
}

#[test]
fn identity_lut_in_config_is_not_resolved_as_relative_path() {
    let tweaks =
        TweakOptions::list_from_config_file("tests/fixtures/start-identity-config.json").unwrap();

    assert_eq!(
        tweaks,
        vec![
            TweakOptions {
                device: Some(0),
                mode: Some(ColorMode::Hdr),
                lut: Some(PathBuf::from("identity")),
            },
            TweakOptions {
                device: Some(0),
                mode: Some(ColorMode::Sdr),
                lut: Some(PathBuf::from("tests/fixtures/valid-xiaomi-27i-pro.lut")),
            },
        ]
    );
}

#[test]
fn named_lut_resolves_to_luts_folder_next_to_exe() {
    let path = app::resolve_lut_path("xiaomi-27i-pro-hdr-eotf-correction").unwrap();

    assert_eq!(
        path.file_name().unwrap(),
        "xiaomi-g-pro-27i-hdr-eotf-correction.lut"
    );
    assert_eq!(path.parent().unwrap().file_name().unwrap(), "luts");
}

#[test]
fn named_lut_in_config_is_not_resolved_relative_to_config_file() {
    let tweaks =
        TweakOptions::list_from_config_file("configs/xiaomi-27i-pro-hdr-eotf-correction.json")
            .unwrap();

    assert_eq!(
        tweaks[1].lut,
        Some(PathBuf::from("xiaomi-27i-pro-hdr-eotf-correction"))
    );
}

#[test]
fn many_config_loader_accepts_single_tweak_object() {
    let tweaks = TweakOptions::many_from_config_file("tests/fixtures/config-xiaomi.json").unwrap();

    assert_eq!(
        tweaks,
        vec![TweakOptions {
            device: Some(0),
            mode: Some(ColorMode::Hdr),
            lut: Some(PathBuf::from("tests/fixtures/valid-xiaomi-27i-pro.lut")),
        }]
    );
}

#[test]
fn single_config_loader_rejects_multi_entry_config() {
    let error = TweakOptions::from_config_file("tests/fixtures/start-config.json")
        .expect_err("single-entry config should reject multi-entry array");

    assert!(error.to_string().contains("exactly one tweak entry"));
}

#[test]
fn single_tweak_config_does_not_parse_as_start_config() {
    let error = TweakOptions::list_from_config_file("tests/fixtures/config-xiaomi.json")
        .expect_err("start config should be a JSON array");

    assert!(error.to_string().contains("failed to parse config"));
}
