use std::cell::RefCell;

use hdr_tweaks::Result;
use hdr_tweaks::app;
use hdr_tweaks::lut::GammaRamp;
use hdr_tweaks::platform::DisplayPlatform;

#[test]
fn reset_applies_identity_ramp_from_source_code() {
    let platform = MockDisplayPlatform::default();

    app::reset_gamma(&platform).unwrap();

    assert_eq!(
        platform.applied.borrow().as_ref(),
        Some(&GammaRamp::identity())
    );
}

#[derive(Default)]
struct MockDisplayPlatform {
    applied: RefCell<Option<GammaRamp>>,
}

impl DisplayPlatform for MockDisplayPlatform {
    fn hdr_enabled(&self) -> Result<bool> {
        unreachable!("reset does not read HDR state")
    }

    fn capture_gamma_ramp(&self) -> Result<GammaRamp> {
        unreachable!("reset does not capture gamma")
    }

    fn apply_gamma_ramp(&self, ramp: &GammaRamp) -> Result<()> {
        self.applied.replace(Some(ramp.clone()));
        Ok(())
    }
}
