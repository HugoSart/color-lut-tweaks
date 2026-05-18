use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use crate::error::{Error, Result};
use crate::lut::{Channel, GammaRamp, LUT_SIZE};
use crate::platform::DisplayPlatform;

pub const IDENTITY_LUT: &str = "identity";

pub fn default_config_path() -> Result<PathBuf> {
    let exe_path = std::env::current_exe().map_err(|source| Error::Io { path: None, source })?;
    Ok(exe_path.parent().map_or_else(
        || PathBuf::from("config.json"),
        |path| path.join("config.json"),
    ))
}

pub fn inspect_lut(path: impl AsRef<Path>) -> Result<LutInspection> {
    let path = path.as_ref();
    let ramp = load_lut(path)?;
    Ok(LutInspection {
        path: path.display().to_string(),
        summaries: Channel::ALL.map(|channel| (channel, ramp.channel_summary(channel))),
    })
}

pub fn apply_lut(platform: &impl DisplayPlatform, path: impl AsRef<Path>) -> Result<()> {
    let ramp = load_lut(path)?;
    platform.apply_gamma_ramp(0, &ramp)
}

pub fn load_lut(path: impl AsRef<Path>) -> Result<GammaRamp> {
    let path = path.as_ref();
    if is_identity_lut(path) {
        Ok(GammaRamp::identity())
    } else {
        GammaRamp::from_file(resolve_lut_path(path)?)
    }
}

pub fn resolve_lut_path(path: impl AsRef<Path>) -> Result<PathBuf> {
    let path = path.as_ref();
    if is_named_lut(path) {
        Ok(default_luts_path(path))
    } else {
        Ok(path.to_path_buf())
    }
}

pub fn apply_tweaks(platform: &impl DisplayPlatform, tweaks: &TweakOptions) -> Result<ApplyReport> {
    let mut applied = Vec::new();

    if let Some(path) = &tweaks.lut {
        let device_indices = target_device_indices(platform, tweaks.device)?;
        let ramp = load_lut(path)?;
        let mut applied_lut = false;
        for device_index in &device_indices {
            let mode_matches = if let Some(mode) = tweaks.mode {
                mode.matches_hdr_enabled(platform.hdr_enabled(*device_index)?)
            } else {
                true
            };

            if mode_matches {
                platform.apply_gamma_ramp(*device_index, &ramp)?;
                applied_lut = true;
            }
        }
        if applied_lut {
            applied.push(AppliedTweak::Lut(path.display().to_string()));
        }
    }

    Ok(ApplyReport { applied })
}

pub fn apply_tweak_list(
    platform: &impl DisplayPlatform,
    tweaks: &[TweakOptions],
) -> Result<ApplyReport> {
    let mut applied = Vec::new();

    for options in tweaks {
        applied.extend(apply_tweaks(platform, options)?.applied);
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
        watch_mode(
            platform,
            &device_indices,
            tweaks.mode.unwrap_or(ColorMode::Hdr),
            path,
        )
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

pub fn watch_mode(
    platform: &impl DisplayPlatform,
    device_indices: &[usize],
    mode: ColorMode,
    path: impl AsRef<Path>,
) -> Result<()> {
    let ramp = load_lut(path)?;
    let original_ramps = device_indices
        .iter()
        .map(|device_index| Ok((*device_index, platform.capture_gamma_ramp(*device_index)?)))
        .collect::<Result<Vec<_>>>()?;
    let mut applied = vec![false; device_indices.len()];
    let mut previous_hdr_states = vec![None; device_indices.len()];

    loop {
        for (position, device_index) in device_indices.iter().enumerate() {
            let hdr_enabled = platform.hdr_enabled(*device_index)?;
            let mode_matches = mode.matches_hdr_enabled(hdr_enabled);

            if mode_matches && !applied[position] {
                platform.apply_gamma_ramp(*device_index, &ramp)?;
                applied[position] = true;
                println!(
                    "Device {device_index}: {} mode active; applied LUT",
                    mode.name()
                );
            } else if !mode_matches && applied[position] {
                let (_, original_ramp) = &original_ramps[position];
                platform.apply_gamma_ramp(*device_index, original_ramp)?;
                applied[position] = false;
                println!(
                    "Device {device_index}: {} mode inactive; restored previous gamma ramp",
                    mode.name()
                );
            } else if previous_hdr_states[position] != Some(hdr_enabled) {
                println!(
                    "Device {device_index}: HDR {}; waiting for {} mode",
                    if hdr_enabled { "enabled" } else { "disabled" },
                    mode.name()
                );
            }

            previous_hdr_states[position] = Some(hdr_enabled);
        }

        thread::sleep(Duration::from_secs(2));
    }
}

pub fn start_tweaks(platform: &impl DisplayPlatform, tweaks: &[TweakOptions]) -> Result<()> {
    run_tweaks_until(platform, tweaks, || false)
}

pub fn run_tweaks_until(
    platform: &impl DisplayPlatform,
    tweaks: &[TweakOptions],
    should_stop: impl Fn() -> bool,
) -> Result<()> {
    let rules = start_rules(platform, tweaks)?;
    if rules.is_empty() {
        loop {
            if should_stop() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    let mut runtime = TweakRuntime {
        rules,
        original_ramps: BTreeMap::new(),
        active_rules: BTreeMap::new(),
    };

    runtime.capture_original_ramps(platform)?;
    runtime.run_until(platform, should_stop)
}

struct TweakRuntime {
    rules: Vec<StartRule>,
    original_ramps: BTreeMap<usize, GammaRamp>,
    active_rules: BTreeMap<usize, Option<usize>>,
}

impl TweakRuntime {
    fn capture_original_ramps(&mut self, platform: &impl DisplayPlatform) -> Result<()> {
        for device_index in start_device_indices(&self.rules) {
            self.original_ramps
                .insert(device_index, platform.capture_gamma_ramp(device_index)?);
        }

        Ok(())
    }

    fn run_until(
        &mut self,
        platform: &impl DisplayPlatform,
        should_stop: impl Fn() -> bool,
    ) -> Result<()> {
        let result = loop {
            if should_stop() {
                break Ok(());
            }

            if let Err(err) = self.tick(platform) {
                break Err(err);
            }

            sleep_poll_interval(&should_stop);
        };

        let restore_result = self.restore_original_ramps(platform);
        match (result, restore_result) {
            (Err(err), _) => Err(err),
            (Ok(()), Err(err)) => Err(err),
            (Ok(()), Ok(())) => Ok(()),
        }
    }

    fn tick(&mut self, platform: &impl DisplayPlatform) -> Result<()> {
        for device_index in self.original_ramps.keys().copied().collect::<Vec<_>>() {
            let hdr_enabled = platform.hdr_enabled(device_index)?;
            let active_mode = ColorMode::from_hdr_enabled(hdr_enabled);
            let desired_rule = self
                .rules
                .iter()
                .position(|rule| rule.device_index == device_index && rule.mode == active_mode);
            let active_rule = self.active_rules.get(&device_index).copied().flatten();

            if desired_rule == active_rule {
                continue;
            }

            match desired_rule {
                Some(rule_index) => {
                    let rule = &self.rules[rule_index];
                    platform.apply_gamma_ramp(device_index, &rule.ramp)?;
                    println!(
                        "Device {device_index}: {} mode active; applied {}",
                        rule.mode.name(),
                        rule.path.display()
                    );
                }
                None => {
                    let original_ramp =
                        self.original_ramps.get(&device_index).ok_or_else(|| {
                            Error::Platform(format!(
                                "missing captured gamma ramp for device {device_index}"
                            ))
                        })?;
                    platform.apply_gamma_ramp(device_index, original_ramp)?;
                    println!(
                        "Device {device_index}: no tweak configured for {}; restored previous gamma ramp",
                        active_mode.name()
                    );
                }
            }

            self.active_rules.insert(device_index, desired_rule);
        }

        Ok(())
    }

    fn restore_original_ramps(&self, platform: &impl DisplayPlatform) -> Result<()> {
        for (device_index, ramp) in &self.original_ramps {
            platform.apply_gamma_ramp(*device_index, ramp)?;
        }

        Ok(())
    }
}

fn sleep_poll_interval(should_stop: impl Fn() -> bool) {
    let slice = Duration::from_millis(100);
    for _ in 0..20 {
        if should_stop() {
            return;
        }
        thread::sleep(slice);
    }
}

fn start_rules(platform: &impl DisplayPlatform, tweaks: &[TweakOptions]) -> Result<Vec<StartRule>> {
    let mut rules = Vec::new();

    for (position, options) in tweaks.iter().enumerate() {
        let path = options.lut.as_ref().ok_or_else(|| {
            Error::InvalidArguments(format!(
                "`start` config entry {position} needs a `lut` path"
            ))
        })?;
        let ramp = load_lut(path)?;
        let mode = options.mode.unwrap_or(ColorMode::Hdr);

        for device_index in target_device_indices(platform, options.device)? {
            rules.push(StartRule {
                device_index,
                mode,
                path: path.clone(),
                ramp: ramp.clone(),
            });
        }
    }

    Ok(rules)
}

fn start_device_indices(rules: &[StartRule]) -> Vec<usize> {
    let mut device_indices = Vec::new();
    for rule in rules {
        if !device_indices.contains(&rule.device_index) {
            device_indices.push(rule.device_index);
        }
    }
    device_indices
}

#[derive(Clone, Debug)]
struct StartRule {
    device_index: usize,
    mode: ColorMode,
    path: PathBuf,
    ramp: GammaRamp,
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

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize)]
pub struct TweakOptions {
    #[serde(default)]
    pub device: Option<usize>,
    #[serde(default)]
    pub lut: Option<PathBuf>,
    #[serde(default)]
    pub mode: Option<ColorMode>,
}

impl TweakOptions {
    pub fn from_config_file(path: impl AsRef<Path>) -> Result<Self> {
        let mut options = Self::many_from_config_file(path)?;
        match options.len() {
            1 => Ok(options.remove(0)),
            _ => Err(Error::InvalidArguments(
                "expected config with exactly one tweak entry".to_string(),
            )),
        }
    }

    pub fn many_from_config_file(path: impl AsRef<Path>) -> Result<Vec<Self>> {
        let path = path.as_ref();
        let json = fs::read_to_string(path).map_err(|source| Error::Io {
            path: Some(path.to_path_buf()),
            source,
        })?;

        let mut options = serde_json::from_str::<TweakConfig>(&json)
            .map_err(|source| Error::ConfigJson {
                path: path.to_path_buf(),
                source,
            })?
            .into_options();

        for option in &mut options {
            option.resolve_paths_relative_to(path);
        }

        Ok(options)
    }

    pub fn list_from_config_file(path: impl AsRef<Path>) -> Result<Vec<Self>> {
        let path = path.as_ref();
        let json = fs::read_to_string(path).map_err(|source| Error::Io {
            path: Some(path.to_path_buf()),
            source,
        })?;

        let mut options =
            serde_json::from_str::<Vec<Self>>(&json).map_err(|source| Error::ConfigJson {
                path: path.to_path_buf(),
                source,
            })?;

        for option in &mut options {
            option.resolve_paths_relative_to(path);
        }

        Ok(options)
    }

    pub fn merge_cli_overrides(&mut self, overrides: &Self) {
        if overrides.device.is_some() {
            self.device = overrides.device;
        }
        if overrides.lut.is_some() {
            self.lut = overrides.lut.clone();
        }
        if overrides.mode.is_some() {
            self.mode = overrides.mode;
        }
    }

    fn resolve_paths_relative_to(&mut self, config_path: &Path) {
        if let Some(lut) = &self.lut
            && !is_identity_lut(lut)
            && !is_named_lut(lut)
            && lut.is_relative()
            && let Some(parent) = config_path.parent()
        {
            self.lut = Some(parent.join(lut));
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum TweakConfig {
    Single(TweakOptions),
    Many(Vec<TweakOptions>),
}

impl TweakConfig {
    fn into_options(self) -> Vec<TweakOptions> {
        match self {
            Self::Single(options) => vec![options],
            Self::Many(options) => options,
        }
    }
}

fn is_identity_lut(path: &Path) -> bool {
    path == Path::new(IDENTITY_LUT)
}

fn is_named_lut(path: &Path) -> bool {
    let value = path.as_os_str().to_string_lossy();
    !value.is_empty()
        && !value.contains('/')
        && !value.contains('\\')
        && path.extension().is_none()
        && !is_identity_lut(path)
}

fn default_luts_path(name: &Path) -> PathBuf {
    default_luts_dir().join(name).with_extension("lut")
}

fn default_luts_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join("luts")))
        .unwrap_or_else(|| PathBuf::from("luts"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorMode {
    Hdr,
    Sdr,
}

impl ColorMode {
    pub fn from_hdr_enabled(hdr_enabled: bool) -> Self {
        if hdr_enabled { Self::Hdr } else { Self::Sdr }
    }

    pub fn matches_hdr_enabled(self, hdr_enabled: bool) -> bool {
        match self {
            Self::Hdr => hdr_enabled,
            Self::Sdr => !hdr_enabled,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Hdr => "HDR",
            Self::Sdr => "SDR",
        }
    }
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
