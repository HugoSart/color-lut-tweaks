use std::cell::RefCell;

use hdr_tweaks::Result;
use hdr_tweaks::app;
use hdr_tweaks::lut::GammaRamp;
use hdr_tweaks::platform::DisplayPlatform;

#[test]
fn reset_applies_identity_ramp_from_source_code() {
    let platform = MockDisplayPlatform::default();

    app::reset_gamma(&platform, 2).unwrap();

    assert_eq!(
        platform.applied.borrow().as_ref(),
        Some(&(2, GammaRamp::identity()))
    );
}

#[derive(Default)]
struct MockDisplayPlatform {
    applied: RefCell<Option<(usize, GammaRamp)>>,
}

impl DisplayPlatform for MockDisplayPlatform {
    fn hdr_enabled(&self, _device_index: usize) -> Result<bool> {
        unreachable!("reset does not read HDR state")
    }

    fn capture_gamma_ramp(&self, _device_index: usize) -> Result<GammaRamp> {
        unreachable!("reset does not capture gamma")
    }

    fn apply_gamma_ramp(&self, device_index: usize, ramp: &GammaRamp) -> Result<()> {
        self.applied.replace(Some((device_index, ramp.clone())));
        Ok(())
    }
}
