use std::cell::RefCell;

use color_lut_tweaks::Result;
use color_lut_tweaks::app::{self, DeviceSelector};
use color_lut_tweaks::lut::GammaRamp;
use color_lut_tweaks::platform::DisplayPlatform;

#[test]
fn reset_applies_identity_ramp_from_source_code() {
    let platform = MockDisplayPlatform::default();

    app::reset_gamma(&platform, Some(&DeviceSelector::Index(2))).unwrap();

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

#[test]
fn reset_accepts_device_name() {
    let platform = MockDisplayPlatform {
        device_names: vec![
            r"\\.\DISPLAY1".to_string(),
            r"\\.\DISPLAY2".to_string(),
            r"\\.\DISPLAY2".to_string(),
        ],
        device_labels: vec![
            "Left Monitor".to_string(),
            "Mi Monitor".to_string(),
            "Mi Monitor".to_string(),
        ],
        ..MockDisplayPlatform::default()
    };

    app::reset_gamma(
        &platform,
        Some(&DeviceSelector::Name("Mi Monitor".to_string())),
    )
    .unwrap();

    assert_eq!(
        platform.applied.borrow().as_slice(),
        &[(1, GammaRamp::identity()), (2, GammaRamp::identity())]
    );
}

struct MockDisplayPlatform {
    applied: RefCell<Vec<(usize, GammaRamp)>>,
    device_names: Vec<String>,
    device_labels: Vec<String>,
}

impl Default for MockDisplayPlatform {
    fn default() -> Self {
        Self {
            applied: RefCell::new(Vec::new()),
            device_names: vec![
                r"\\.\DISPLAY1".to_string(),
                r"\\.\DISPLAY2".to_string(),
                r"\\.\DISPLAY3".to_string(),
            ],
            device_labels: vec![
                "Display 1".to_string(),
                "Display 2".to_string(),
                "Display 3".to_string(),
            ],
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
