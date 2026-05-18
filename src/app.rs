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

    if let Some(path) = &tweaks.lut {
        let device_indices = target_device_indices(platform, tweaks.device)?;
        let ramp = GammaRamp::from_file(path)?;
        for device_index in &device_indices {
            platform.apply_gamma_ramp(*device_index, &ramp)?;
        }
        applied.push(AppliedTweak::Lut(path.display().to_string()));
    }

    Ok(ApplyReport { applied })
}

pub fn reset_gamma(platform: &impl DisplayPlatform, device: Option<usize>) -> Result<()> {
    let ramp = GammaRamp::identity();
    for device_index in target_device_indices(platform, device)? {
        platform.apply_gamma_ramp(device_index, &ramp)?;
    }
    Ok(())
}

pub fn watch_tweaks(platform: &impl DisplayPlatform, tweaks: &TweakOptions) -> Result<()> {
    let device_indices = target_device_indices(platform, tweaks.device)?;

    if let Some(path) = &tweaks.lut {
        watch_hdr(platform, &device_indices, path)
    } else {
        let mut previous_states = vec![None; device_indices.len()];
        loop {
            for (position, device_index) in device_indices.iter().enumerate() {
                let hdr_enabled = platform.hdr_enabled(*device_index)?;
                if previous_states[position] != Some(hdr_enabled) {
                    println!(
                        "Device {device_index}: HDR {}; no LUT configured",
                        if hdr_enabled { "enabled" } else { "disabled" }
                    );
                    previous_states[position] = Some(hdr_enabled);
                }
            }
            thread::sleep(Duration::from_secs(2));
        }
    }
}

pub fn watch_hdr(
    platform: &impl DisplayPlatform,
    device_indices: &[usize],
    path: impl AsRef<Path>,
) -> Result<()> {
    let ramp = GammaRamp::from_file(path)?;
    let original_ramps = device_indices
        .iter()
        .map(|device_index| Ok((*device_index, platform.capture_gamma_ramp(*device_index)?)))
        .collect::<Result<Vec<_>>>()?;
    let mut applied = vec![false; device_indices.len()];
    let mut previous_hdr_states = vec![None; device_indices.len()];

    loop {
        for (position, device_index) in device_indices.iter().enumerate() {
            let hdr_enabled = platform.hdr_enabled(*device_index)?;

            if hdr_enabled && !applied[position] {
                platform.apply_gamma_ramp(*device_index, &ramp)?;
                applied[position] = true;
                println!("Device {device_index}: HDR enabled; applied LUT");
            } else if !hdr_enabled && applied[position] {
                let (_, original_ramp) = &original_ramps[position];
                platform.apply_gamma_ramp(*device_index, original_ramp)?;
                applied[position] = false;
                println!("Device {device_index}: HDR disabled; restored previous gamma ramp");
            } else if previous_hdr_states[position] != Some(hdr_enabled) {
                println!(
                    "Device {device_index}: HDR {}; {}",
                    if hdr_enabled { "enabled" } else { "disabled" },
                    if applied[position] {
                        "LUT already applied"
                    } else {
                        "waiting"
                    }
                );
            }

            previous_hdr_states[position] = Some(hdr_enabled);
        }

        thread::sleep(Duration::from_secs(2));
    }
}

fn target_device_indices(
    platform: &impl DisplayPlatform,
    device: Option<usize>,
) -> Result<Vec<usize>> {
    if let Some(device) = device {
        return Ok(vec![device]);
    }

    Ok((0..platform.active_device_count()?).collect())
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
