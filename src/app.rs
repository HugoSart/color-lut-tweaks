use std::path::Path;
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
    platform.apply_gamma_ramp(&ramp)
}

pub fn watch_hdr(platform: &impl DisplayPlatform, path: impl AsRef<Path>) -> Result<()> {
    let ramp = GammaRamp::from_file(path)?;
    let original_ramp = platform.capture_gamma_ramp()?;
    let mut applied = false;

    loop {
        let hdr_enabled = platform.hdr_enabled()?;

        if hdr_enabled && !applied {
            platform.apply_gamma_ramp(&ramp)?;
            applied = true;
            println!("HDR enabled; applied LUT");
        } else if !hdr_enabled && applied {
            platform.apply_gamma_ramp(&original_ramp)?;
            applied = false;
            println!("HDR disabled; restored previous gamma ramp");
        }

        thread::sleep(Duration::from_secs(2));
    }
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
