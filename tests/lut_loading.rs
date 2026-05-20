use std::path::PathBuf;

use color_lut_tweaks::app;
use color_lut_tweaks::lut::{Channel, GammaRamp, LUT_SIZE};

#[test]
fn identity_ramp_is_generated_in_source_code() {
    let ramp = GammaRamp::identity();

    assert_eq!(ramp.values()[0][0], 0);
    assert_eq!(ramp.values()[0][255], u16::MAX);
    assert_eq!(ramp.values()[1][255], u16::MAX);
    assert_eq!(ramp.values()[2][255], u16::MAX);
    assert_eq!(ramp.channel_summary(Channel::Red).monotonic, true);
}

#[test]
fn loads_known_xiaomi_lut_fixture() {
    let ramp = GammaRamp::from_file(fixture("valid-xiaomi-27i-pro.lut")).unwrap();

    assert_eq!(ramp.values()[0][255], 49816);
    assert_eq!(ramp.values()[1][255], 50320);
    assert_eq!(ramp.values()[2][255], 50787);
}

#[test]
fn rejects_wrong_sized_lut_fixture() {
    let err = GammaRamp::from_file(fixture("invalid-too-small.lut")).unwrap_err();

    assert!(err.to_string().contains("expected 1536 bytes"));
    assert!(err.to_string().contains(&(LUT_SIZE - 2).to_string()));
}

#[test]
fn loads_identity_1d_cube_as_gamma_ramp() {
    let ramp = GammaRamp::from_cube_file(fixture("identity-1d.cube")).unwrap();

    assert_eq!(ramp, GammaRamp::identity());
}

#[test]
fn loads_1d_cube_with_channel_curves() {
    let ramp = GammaRamp::from_cube_file(fixture("red-boost-1d.cube")).unwrap();

    assert_eq!(ramp.values()[0][255], u16::MAX);
    assert_eq!(ramp.values()[1][255], 32768);
    assert_eq!(ramp.values()[2][255], 16384);
}

#[test]
fn loads_3d_cube_as_grayscale_axis_gamma_ramp() {
    let ramp = GammaRamp::from_cube_file(fixture("identity-3d.cube")).unwrap();

    assert_eq!(ramp, GammaRamp::identity());
}

#[test]
fn app_loader_uses_cube_parser_by_extension() {
    let ramp = app::load_lut(fixture("identity-1d.cube")).unwrap();

    assert_eq!(ramp, GammaRamp::identity());
}

#[test]
fn app_loader_uses_cube_parser_for_named_lut_lookup() {
    let ramp = app::load_lut("named-cube-fixture").unwrap();

    assert_eq!(ramp, GammaRamp::identity());
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from("tests").join("fixtures").join(name)
}
