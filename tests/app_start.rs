use std::path::PathBuf;

use color_lut_tweaks::app::{ColorMode, TweakOptions};

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
fn single_tweak_config_does_not_parse_as_start_config() {
    let error = TweakOptions::list_from_config_file("tests/fixtures/config-xiaomi.json")
        .expect_err("start config should be a JSON array");

    assert!(error.to_string().contains("failed to parse config"));
}
