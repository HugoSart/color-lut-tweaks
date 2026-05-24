use std::path::PathBuf;

use color_lut_tweaks::Result;
use color_lut_tweaks::app::{
    DeviceSelector, TweakOptions, WindowsColorProfile, WindowsTweakOptions,
};
use color_lut_tweaks::lut::GammaRamp;
use color_lut_tweaks::platform::DisplayPlatform;
use color_lut_tweaks::windows_settings::{DisplayProfileKind, planned_display_profile_actions};

#[test]
fn profile_planning_accepts_duplicate_same_desired_profile() {
    let platform = MockDisplayPlatform::two_mi_monitors();
    let tweaks = vec![
        hdr_profile_tweak(0, "hdr.icm"),
        hdr_profile_tweak(0, "hdr.icm"),
    ];

    let actions = planned_display_profile_actions(&platform, &tweaks).unwrap();

    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].device_index, 0);
    assert_eq!(actions[0].kind, DisplayProfileKind::Hdr);
    assert_eq!(
        actions[0].profile,
        WindowsColorProfile::Set(PathBuf::from("hdr.icm"))
    );
}

#[test]
fn profile_planning_rejects_conflicting_profiles_for_same_display_and_kind() {
    let platform = MockDisplayPlatform::two_mi_monitors();
    let tweaks = vec![
        hdr_profile_tweak(0, "hdr-a.icm"),
        hdr_profile_tweak(0, "hdr-b.icm"),
    ];

    let error = planned_display_profile_actions(&platform, &tweaks).unwrap_err();

    assert!(error.to_string().contains("conflicting"));
    assert!(error.to_string().contains("hdrColorProfile"));
}

#[test]
fn profile_planning_applies_name_selector_to_all_matching_devices() {
    let platform = MockDisplayPlatform::two_mi_monitors();
    let tweaks = vec![TweakOptions {
        device: Some(DeviceSelector::Name("Mi Monitor".to_string())),
        windows: WindowsTweakOptions {
            sdr_color_profile: WindowsColorProfile::Clear,
            ..Default::default()
        },
        ..Default::default()
    }];

    let actions = planned_display_profile_actions(&platform, &tweaks).unwrap();

    assert_eq!(
        actions
            .iter()
            .map(|action| (action.device_index, action.kind, action.profile.clone()))
            .collect::<Vec<_>>(),
        vec![
            (0, DisplayProfileKind::Sdr, WindowsColorProfile::Clear),
            (1, DisplayProfileKind::Sdr, WindowsColorProfile::Clear),
        ]
    );
}

#[test]
fn profile_planning_ignores_missing_profile_fields() {
    let platform = MockDisplayPlatform::two_mi_monitors();

    let actions = planned_display_profile_actions(&platform, &[TweakOptions::default()]).unwrap();

    assert!(actions.is_empty());
}

struct MockDisplayPlatform {
    names: Vec<&'static str>,
    labels: Vec<&'static str>,
}

impl MockDisplayPlatform {
    fn two_mi_monitors() -> Self {
        Self {
            names: vec![r"\\.\DISPLAY1", r"\\.\DISPLAY2"],
            labels: vec!["Mi Monitor", "Mi Monitor"],
        }
    }
}

impl DisplayPlatform for MockDisplayPlatform {
    fn active_device_count(&self) -> Result<usize> {
        Ok(self.names.len())
    }

    fn device_name(&self, device_index: usize) -> Result<String> {
        Ok(self.names[device_index].to_string())
    }

    fn device_label(&self, device_index: usize) -> Result<String> {
        Ok(self.labels[device_index].to_string())
    }

    fn hdr_enabled(&self, _device_index: usize) -> Result<bool> {
        unreachable!("profile planning does not read HDR state")
    }

    fn capture_gamma_ramp(&self, _device_index: usize) -> Result<GammaRamp> {
        unreachable!("profile planning does not capture gamma")
    }

    fn apply_gamma_ramp(&self, _device_index: usize, _ramp: &GammaRamp) -> Result<()> {
        unreachable!("profile planning does not apply gamma")
    }
}

fn hdr_profile_tweak(device_index: usize, path: &str) -> TweakOptions {
    TweakOptions {
        device: Some(DeviceSelector::Index(device_index)),
        windows: WindowsTweakOptions {
            hdr_color_profile: WindowsColorProfile::Set(PathBuf::from(path)),
            ..Default::default()
        },
        ..Default::default()
    }
}
