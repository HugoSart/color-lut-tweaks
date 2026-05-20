use std::cell::RefCell;
use std::path::PathBuf;

use color_lut_tweaks::Result;
use color_lut_tweaks::app::{self, AdjustOptions, ColorMode, DeviceSelector, TweakOptions};
use color_lut_tweaks::lut::GammaRamp;
use color_lut_tweaks::platform::DisplayPlatform;

#[test]
fn apply_without_mode_does_not_check_display_mode() {
    let platform = MockDisplayPlatform::default();

    app::apply_tweaks(
        &platform,
        &TweakOptions {
            device: Some(device(0)),
            lut: Some(fixture("valid-xiaomi-27i-pro.lut")),
            mode: None,
            adjust: None,
        },
    )
    .unwrap();

    assert!(platform.hdr_checks.borrow().is_empty());
    assert_eq!(platform.applied.borrow().len(), 1);
}

#[test]
fn apply_with_mode_checks_display_mode() {
    let platform = MockDisplayPlatform::default();

    let report = app::apply_tweaks(
        &platform,
        &TweakOptions {
            device: Some(device(0)),
            lut: Some(fixture("valid-xiaomi-27i-pro.lut")),
            mode: Some(ColorMode::Hdr),
            adjust: None,
        },
    )
    .unwrap();

    assert_eq!(platform.hdr_checks.borrow().as_slice(), &[0]);
    assert_eq!(platform.applied.borrow().len(), 0);
    assert!(report.is_empty());
}

#[test]
fn apply_identity_lut_uses_generated_identity_ramp() {
    let platform = MockDisplayPlatform::default();

    app::apply_tweaks(
        &platform,
        &TweakOptions {
            device: Some(device(0)),
            lut: Some(PathBuf::from("identity")),
            mode: None,
            adjust: None,
        },
    )
    .unwrap();

    assert_eq!(
        platform.applied.borrow().as_slice(),
        &[(0, GammaRamp::identity())]
    );
}

#[test]
fn apply_tweak_list_applies_only_entries_matching_current_mode() {
    let platform = MockDisplayPlatform::default();

    let report = app::apply_tweak_list(
        &platform,
        &[
            TweakOptions {
                device: Some(device(0)),
                lut: Some(PathBuf::from("identity")),
                mode: Some(ColorMode::Sdr),
                adjust: None,
            },
            TweakOptions {
                device: Some(device(0)),
                lut: Some(fixture("valid-xiaomi-27i-pro.lut")),
                mode: Some(ColorMode::Hdr),
                adjust: None,
            },
        ],
    )
    .unwrap();

    assert_eq!(platform.hdr_checks.borrow().as_slice(), &[0, 0]);
    assert_eq!(
        platform.applied.borrow().as_slice(),
        &[(0, GammaRamp::identity())]
    );
    assert_eq!(report.applied.len(), 1);
}

#[test]
fn apply_adjusts_lut_before_applying_gamma_ramp() {
    let platform = MockDisplayPlatform::default();

    app::apply_tweaks(
        &platform,
        &TweakOptions {
            device: Some(device(0)),
            lut: Some(PathBuf::from("identity")),
            mode: None,
            adjust: Some(AdjustOptions {
                gain: Some([1.0, 0.5, 1.0]),
                ..AdjustOptions::default()
            }),
        },
    )
    .unwrap();

    let applied = platform.applied.borrow();
    let ramp = &applied[0].1;
    assert_eq!(ramp.values()[0][255], u16::MAX);
    assert_eq!(ramp.values()[1][255], 32768);
    assert_eq!(ramp.values()[2][255], u16::MAX);
}

#[test]
fn apply_brightness_adjustment_keeps_ramp_valid() {
    let platform = MockDisplayPlatform::default();

    app::apply_tweaks(
        &platform,
        &TweakOptions {
            device: Some(device(0)),
            lut: Some(PathBuf::from("identity")),
            mode: None,
            adjust: Some(AdjustOptions {
                brightness: Some(0.1),
                ..AdjustOptions::default()
            }),
        },
    )
    .unwrap();

    let applied = platform.applied.borrow();
    let ramp = &applied[0].1;
    assert!(ramp.values()[0][0] > 0);
    assert_eq!(ramp.values()[0][255], u16::MAX);
}

#[test]
fn apply_can_target_device_by_name() {
    let platform = MockDisplayPlatform {
        device_names: vec![
            r"\\.\DISPLAY1".to_string(),
            r"\\.\DISPLAY1".to_string(),
            r"\\.\DISPLAY3".to_string(),
        ],
        device_labels: vec![
            "Mi Monitor".to_string(),
            "Mi Monitor".to_string(),
            "Other Monitor".to_string(),
        ],
        ..MockDisplayPlatform::default()
    };

    app::apply_tweaks(
        &platform,
        &TweakOptions {
            device: Some(DeviceSelector::Name("DISPLAY1".to_string())),
            lut: Some(PathBuf::from("identity")),
            mode: None,
            adjust: None,
        },
    )
    .unwrap();

    assert_eq!(
        platform.applied.borrow().as_slice(),
        &[(0, GammaRamp::identity()), (1, GammaRamp::identity())]
    );
}

#[test]
fn apply_can_target_device_by_friendly_name() {
    let platform = MockDisplayPlatform {
        device_names: vec![r"\\.\DISPLAY1".to_string(), r"\\.\DISPLAY2".to_string()],
        device_labels: vec!["Mi Monitor".to_string(), "Other Monitor".to_string()],
        ..MockDisplayPlatform::default()
    };

    app::apply_tweaks(
        &platform,
        &TweakOptions {
            device: Some(DeviceSelector::Name("Mi Monitor".to_string())),
            lut: Some(PathBuf::from("identity")),
            mode: None,
            adjust: None,
        },
    )
    .unwrap();

    assert_eq!(
        platform.applied.borrow().as_slice(),
        &[(0, GammaRamp::identity())]
    );
}

struct MockDisplayPlatform {
    hdr_checks: RefCell<Vec<usize>>,
    applied: RefCell<Vec<(usize, GammaRamp)>>,
    device_names: Vec<String>,
    device_labels: Vec<String>,
}

impl Default for MockDisplayPlatform {
    fn default() -> Self {
        Self {
            hdr_checks: RefCell::new(Vec::new()),
            applied: RefCell::new(Vec::new()),
            device_names: vec![r"\\.\DISPLAY1".to_string()],
            device_labels: vec!["Mi Monitor".to_string()],
        }
    }
}

impl DisplayPlatform for MockDisplayPlatform {
    fn active_device_count(&self) -> Result<usize> {
        Ok(self.device_names.len())
    }

    fn device_name(&self, device_index: usize) -> Result<String> {
        Ok(self.device_names[device_index].clone())
    }

    fn device_label(&self, device_index: usize) -> Result<String> {
        Ok(self.device_labels[device_index].clone())
    }

    fn hdr_enabled(&self, device_index: usize) -> Result<bool> {
        self.hdr_checks.borrow_mut().push(device_index);
        Ok(false)
    }

    fn capture_gamma_ramp(&self, _device_index: usize) -> Result<GammaRamp> {
        unreachable!("apply does not capture gamma")
    }

    fn apply_gamma_ramp(&self, device_index: usize, ramp: &GammaRamp) -> Result<()> {
        self.applied.borrow_mut().push((device_index, ramp.clone()));
        Ok(())
    }
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from("tests").join("fixtures").join(name)
}

fn device(index: usize) -> DeviceSelector {
    DeviceSelector::Index(index)
}
