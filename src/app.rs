use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use crate::error::Result;
use crate::lut::{Channel, GammaRamp, LUT_SIZE};
use crate::platform::DisplayPlatform;

pub fn inspect_lut(path: impl AsRef<Path>) -> Result<LutInspection> {
    let path = path.as_ref();
    let ramp = GammaRamp::from_file(path)?;
    Ok(LutInspection {
        path: path.display().to_string(),
        summaries: Channel::ALL.map(|channel| (channel, ramp.channel_summary(channel))),
    })
}

pub fn apply_lut(platform: &impl DisplayPlatform, path: impl AsRef<Path>) -> Result<()> {
    let ramp = GammaRamp::from_file(path)?;
    platform.apply_gamma_ramp(0, &ramp)
}

pub fn apply_tweaks(platform: &impl DisplayPlatform, tweaks: &TweakOptions) -> Result<ApplyReport> {
    let mut applied = Vec::new();
    let device_index = tweaks.device.unwrap_or(0);

    if let Some(path) = &tweaks.lut {
        let ramp = GammaRamp::from_file(path)?;
        platform.apply_gamma_ramp(device_index, &ramp)?;
        applied.push(AppliedTweak::Lut(path.display().to_string()));
    }

    Ok(ApplyReport { applied })
}

pub fn reset_gamma(platform: &impl DisplayPlatform, device_index: usize) -> Result<()> {
    platform.apply_gamma_ramp(device_index, &GammaRamp::identity())
}

pub fn watch_tweaks(platform: &impl DisplayPlatform, tweaks: &TweakOptions) -> Result<()> {
    let device_index = tweaks.device.unwrap_or(0);

    if let Some(path) = &tweaks.lut {
        watch_hdr(platform, device_index, path)
    } else {
        loop {
            let hdr_enabled = platform.hdr_enabled(device_index)?;
            println!(
                "HDR {}; no LUT configured",
                if hdr_enabled { "enabled" } else { "disabled" }
            );
            thread::sleep(Duration::from_secs(2));
        }
    }
}

pub fn watch_hdr(
    platform: &impl DisplayPlatform,
    device_index: usize,
    path: impl AsRef<Path>,
) -> Result<()> {
    let ramp = GammaRamp::from_file(path)?;
    let original_ramp = platform.capture_gamma_ramp(device_index)?;
    let mut applied = false;
    let mut previous_hdr_state = None;

    loop {
        let hdr_enabled = platform.hdr_enabled(device_index)?;

        if hdr_enabled && !applied {
            platform.apply_gamma_ramp(device_index, &ramp)?;
            applied = true;
            println!("HDR enabled; applied LUT");
        } else if !hdr_enabled && applied {
            platform.apply_gamma_ramp(device_index, &original_ramp)?;
            applied = false;
            println!("HDR disabled; restored previous gamma ramp");
        } else if previous_hdr_state != Some(hdr_enabled) {
            println!(
                "HDR {}; {}",
                if hdr_enabled { "enabled" } else { "disabled" },
                if applied {
                    "LUT already applied"
                } else {
                    "waiting"
                }
            );
        }

        previous_hdr_state = Some(hdr_enabled);
        thread::sleep(Duration::from_secs(2));
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TweakOptions {
    pub device: Option<usize>,
    pub lut: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplyReport {
    pub applied: Vec<AppliedTweak>,
}

impl ApplyReport {
    pub fn is_empty(&self) -> bool {
        self.applied.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppliedTweak {
    Lut(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LutInspection {
    pub path: String,
    pub summaries: [(Channel, crate::lut::ChannelSummary); crate::lut::CHANNELS],
}

impl LutInspection {
    pub fn format(&self) -> String {
        let mut output = String::new();
        output.push_str(&format!("Loaded {}\n", self.path));
        output.push_str(&format!(
            "Format: WORD[3][256], little-endian, {LUT_SIZE} bytes\n"
        ));

        for (channel, summary) in &self.summaries {
            output.push_str(&format!(
                "{:>5}: first {:>5}, last {:>5}, min {:>5}, max {:>5}, monotonic {}\n",
                channel.name(),
                summary.first,
                summary.last,
                summary.min,
                summary.max,
                if summary.monotonic { "yes" } else { "no" }
            ));
        }

        output
    }
}
