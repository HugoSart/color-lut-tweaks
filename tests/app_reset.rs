use std::cell::RefCell;

use color_lut_tweaks::Result;
use color_lut_tweaks::app;
use color_lut_tweaks::lut::GammaRamp;
use color_lut_tweaks::platform::DisplayPlatform;

#[test]
fn reset_applies_identity_ramp_from_source_code() {
    let platform = MockDisplayPlatform::default();

    app::reset_gamma(&platform, Some(2)).unwrap();

    assert_eq!(
        platform.applied.borrow().as_slice(),
        &[(2, GammaRamp::identity())]
    );
}

#[test]
fn reset_without_device_applies_identity_to_all_devices() {
    let platform = MockDisplayPlatform::default();

    app::reset_gamma(&platform, None).unwrap();

    assert_eq!(
        platform.applied.borrow().as_slice(),
        &[
            (0, GammaRamp::identity()),
            (1, GammaRamp::identity()),
            (2, GammaRamp::identity())
        ]
    );
}

#[derive(Default)]
struct MockDisplayPlatform {
    applied: RefCell<Vec<(usize, GammaRamp)>>,
}

impl DisplayPlatform for MockDisplayPlatform {
    fn active_device_count(&self) -> Result<usize> {
        Ok(3)
    }

    fn hdr_enabled(&self, _device_index: usize) -> Result<bool> {
        unreachable!("reset does not read HDR state")
    }

    fn capture_gamma_ramp(&self, _device_index: usize) -> Result<GammaRamp> {
        unreachable!("reset does not capture gamma")
    }

    fn apply_gamma_ramp(&self, device_index: usize, ramp: &GammaRamp) -> Result<()> {
        self.applied.borrow_mut().push((device_index, ramp.clone()));
        Ok(())
    }
}
