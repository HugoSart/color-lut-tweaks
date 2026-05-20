use std::path::PathBuf;

use color_lut_tweaks::Result;
use color_lut_tweaks::app::{self, AdjustOptions, ColorMode, RuntimeOptions, TweakOptions};
use color_lut_tweaks::lut::GammaRamp;
use color_lut_tweaks::platform::DisplayPlatform;

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
                adjust: None,
            },
            TweakOptions {
                device: Some(0),
                mode: Some(ColorMode::Sdr),
                lut: Some(PathBuf::from("tests/fixtures/valid-xiaomi-27i-pro.lut")),
                adjust: None,
            },
        ]
    );
}

#[test]
fn empty_tweak_list_is_valid() {
    let platform = EmptyDisplayPlatform;

    app::run_tweaks_until(&platform, &[], RuntimeOptions::default(), || true).unwrap();
}

#[test]
fn runtime_options_default_to_force_enabled() {
    assert!(RuntimeOptions::default().force);
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
                adjust: None,
            },
            TweakOptions {
                device: Some(0),
                mode: Some(ColorMode::Sdr),
                lut: Some(PathBuf::from("tests/fixtures/valid-xiaomi-27i-pro.lut")),
                adjust: None,
            },
        ]
    );
}

#[test]
fn named_lut_resolves_to_luts_folder_next_to_exe() {
    let path = app::resolve_lut_path("xiaomi-27i-pro-hdr-eotf-correction").unwrap();

    assert_eq!(
        path.file_name().unwrap(),
        "xiaomi-27i-pro-hdr-eotf-correction.lut"
    );
    assert_eq!(path.parent().unwrap().file_name().unwrap(), "luts");
}

#[test]
fn named_cube_resolves_to_luts_folder_next_to_exe() {
    let path = app::resolve_lut_path("named-cube-fixture").unwrap();

    assert_eq!(path.file_name().unwrap(), "named-cube-fixture.cube");
    assert_eq!(path.parent().unwrap().file_name().unwrap(), "luts");
}

#[test]
fn named_lut_in_config_is_not_resolved_relative_to_config_file() {
    let tweaks =
        TweakOptions::list_from_config_file("../configs/xiaomi-g-pro-27i.config.json")
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
            adjust: None,
        }]
    );
}

#[test]
fn config_loads_adjust_options() {
    let tweaks = TweakOptions::many_from_config_file("tests/fixtures/config-adjust.json").unwrap();

    assert_eq!(
        tweaks,
        vec![TweakOptions {
            device: Some(0),
            mode: Some(ColorMode::Sdr),
            lut: Some(PathBuf::from("identity")),
            adjust: Some(AdjustOptions {
                contrast: Some(1.05),
                brightness: Some(0.01),
                gamma: Some(1.0),
                gain: Some([1.0, 0.95, 1.0]),
                offset: Some([0.0, 0.0, 0.0]),
            }),
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

struct EmptyDisplayPlatform;

impl DisplayPlatform for EmptyDisplayPlatform {
    fn active_device_count(&self) -> Result<usize> {
        unreachable!("empty tweak list does not enumerate devices")
    }

    fn hdr_enabled(&self, _device_index: usize) -> Result<bool> {
        unreachable!("empty tweak list does not read HDR state")
    }

    fn capture_gamma_ramp(&self, _device_index: usize) -> Result<GammaRamp> {
        unreachable!("empty tweak list does not capture gamma")
    }

    fn apply_gamma_ramp(&self, _device_index: usize, _ramp: &GammaRamp) -> Result<()> {
        unreachable!("empty tweak list does not apply gamma")
    }
}
