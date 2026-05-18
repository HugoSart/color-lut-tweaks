use std::cell::RefCell;
use std::path::PathBuf;

use hdr_tweaks::Result;
use hdr_tweaks::app::{self, ColorMode, TweakOptions};
use hdr_tweaks::lut::GammaRamp;
use hdr_tweaks::platform::DisplayPlatform;

#[test]
fn apply_without_mode_does_not_check_display_mode() {
    let platform = MockDisplayPlatform::default();

    app::apply_tweaks(
        &platform,
        &TweakOptions {
            device: Some(0),
            lut: Some(fixture("valid-xiaomi-27i-pro.lut")),
            mode: None,
        },
    )
    .unwrap();

    assert!(platform.hdr_checks.borrow().is_empty());
    assert_eq!(platform.applied.borrow().len(), 1);
}

#[test]
fn apply_with_mode_checks_display_mode() {
    let platform = MockDisplayPlatform::default();

    app::apply_tweaks(
        &platform,
        &TweakOptions {
            device: Some(0),
            lut: Some(fixture("valid-xiaomi-27i-pro.lut")),
            mode: Some(ColorMode::Hdr),
        },
    )
    .unwrap();

    assert_eq!(platform.hdr_checks.borrow().as_slice(), &[0]);
    assert_eq!(platform.applied.borrow().len(), 0);
}

#[derive(Default)]
struct MockDisplayPlatform {
    hdr_checks: RefCell<Vec<usize>>,
    applied: RefCell<Vec<(usize, GammaRamp)>>,
}

impl DisplayPlatform for MockDisplayPlatform {
    fn active_device_count(&self) -> Result<usize> {
        Ok(1)
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
