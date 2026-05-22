use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use crate::error::{Error, Result};
use crate::lut::{CHANNELS, Channel, ENTRIES, GammaRamp, LUT_SIZE};
use crate::platform::DisplayPlatform;

pub const IDENTITY_LUT: &str = "identity";

pub fn default_config_path() -> Result<PathBuf> {
    let exe_path = std::env::current_exe().map_err(|source| Error::Io { path: None, source })?;
    Ok(exe_path.parent().map_or_else(
        || PathBuf::from("configs").join("Default.config.json"),
        |path| path.join("configs").join("Default.config.json"),
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
        let path = resolve_lut_path(path)?;
        if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("cube"))
        {
            GammaRamp::from_cube_file(path)
        } else {
            GammaRamp::from_file(path)
        }
    }
}

pub fn load_adjusted_lut(
    path: impl AsRef<Path>,
    adjust: Option<&AdjustOptions>,
) -> Result<GammaRamp> {
    let ramp = load_lut(path)?;
    match adjust {
        Some(adjust) => adjust.apply_to(&ramp),
        None => Ok(ramp),
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
        let device_indices = target_device_indices(platform, tweaks.device.as_ref())?;
        let ramp = load_adjusted_lut(path, tweaks.adjust.as_ref())?;
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

pub fn reset_gamma(platform: &impl DisplayPlatform, device: Option<&DeviceSelector>) -> Result<()> {
    let ramp = GammaRamp::identity();
    for device_index in target_device_indices(platform, device)? {
        platform.apply_gamma_ramp(device_index, &ramp)?;
    }
    Ok(())
}

pub fn watch_tweaks(platform: &impl DisplayPlatform, tweaks: &TweakOptions) -> Result<()> {
    let device_indices = target_device_indices(platform, tweaks.device.as_ref())?;

    if let Some(path) = &tweaks.lut {
        watch_mode(
            platform,
            &device_indices,
            tweaks.mode.unwrap_or(ColorMode::Hdr),
            path,
            tweaks.adjust.as_ref(),
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
    adjust: Option<&AdjustOptions>,
) -> Result<()> {
    let ramp = load_adjusted_lut(path, adjust)?;
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
    run_tweaks_until(platform, tweaks, RuntimeOptions::default(), || false)
}

pub fn run_tweaks_until(
    platform: &impl DisplayPlatform,
    tweaks: &[TweakOptions],
    options: RuntimeOptions,
    should_stop: impl Fn() -> bool,
) -> Result<()> {
    let rules = start_rules(tweaks)?;
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
        options,
    };

    runtime.capture_original_ramps(platform)?;
    runtime.run_until(platform, should_stop)
}

struct TweakRuntime {
    rules: Vec<StartRule>,
    original_ramps: BTreeMap<usize, GammaRamp>,
    active_rules: BTreeMap<usize, Option<usize>>,
    options: RuntimeOptions,
}

impl TweakRuntime {
    fn capture_original_ramps(&mut self, platform: &impl DisplayPlatform) -> Result<()> {
        self.capture_available_original_ramps(platform)
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
        self.capture_available_original_ramps(platform)?;

        for device_index in self.original_ramps.keys().copied().collect::<Vec<_>>() {
            if !device_index_available(platform, device_index)? {
                self.active_rules.remove(&device_index);
                continue;
            }

            let hdr_enabled = platform.hdr_enabled(device_index)?;
            let active_mode = ColorMode::from_hdr_enabled(hdr_enabled);
            let desired_rule = self.rules.iter().position(|rule| {
                rule.mode == active_mode
                    && rule
                        .active_device_indices(platform)
                        .is_ok_and(|indices| indices.contains(&device_index))
            });
            let active_rule = self.active_rules.get(&device_index).copied().flatten();

            if desired_rule == active_rule {
                if let Err(err) = self.reapply_if_needed(platform, device_index, desired_rule) {
                    println!("Device {device_index}: could not reapply tweak yet: {err}");
                }
                continue;
            }

            match desired_rule {
                Some(rule_index) => {
                    let rule = &self.rules[rule_index];
                    if let Err(err) = platform.apply_gamma_ramp(device_index, &rule.ramp) {
                        println!(
                            "Device {device_index}: could not apply {} yet: {err}",
                            rule.path.display()
                        );
                        continue;
                    }
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
                    if let Err(err) = platform.apply_gamma_ramp(device_index, original_ramp) {
                        println!(
                            "Device {device_index}: could not restore previous gamma ramp yet: {err}"
                        );
                        continue;
                    }
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

    fn capture_available_original_ramps(&mut self, platform: &impl DisplayPlatform) -> Result<()> {
        for rule in &self.rules {
            for device_index in rule.active_device_indices(platform)? {
                if self.original_ramps.contains_key(&device_index) {
                    continue;
                }

                let Ok(original_ramp) = platform.capture_gamma_ramp(device_index) else {
                    println!(
                        "Device {device_index}: monitor detected but gamma ramp is not available yet"
                    );
                    continue;
                };

                self.original_ramps.insert(device_index, original_ramp);
                println!("Device {device_index}: monitor available; captured original gamma ramp");
            }
        }

        Ok(())
    }

    fn reapply_if_needed(
        &self,
        platform: &impl DisplayPlatform,
        device_index: usize,
        desired_rule: Option<usize>,
    ) -> Result<()> {
        if !self.options.force {
            return Ok(());
        }

        let Some(rule_index) = desired_rule else {
            return Ok(());
        };
        let rule = &self.rules[rule_index];
        let current_ramp = platform.capture_gamma_ramp(device_index)?;
        if current_ramp != rule.ramp {
            platform.apply_gamma_ramp(device_index, &rule.ramp)?;
            println!(
                "Device {device_index}: reapplied {} after gamma ramp changed",
                rule.path.display()
            );
        }

        Ok(())
    }

    fn restore_original_ramps(&self, platform: &impl DisplayPlatform) -> Result<()> {
        for (device_index, ramp) in &self.original_ramps {
            if !device_index_available(platform, *device_index)? {
                continue;
            }

            platform.apply_gamma_ramp(*device_index, ramp)?;
        }

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuntimeOptions {
    pub force: bool,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self { force: true }
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

fn start_rules(tweaks: &[TweakOptions]) -> Result<Vec<StartRule>> {
    let mut rules = Vec::new();

    for (position, options) in tweaks.iter().enumerate() {
        let path = options.lut.as_ref().ok_or_else(|| {
            Error::InvalidArguments(format!(
                "`start` config entry {position} needs a `lut` path"
            ))
        })?;
        let ramp = load_adjusted_lut(path, options.adjust.as_ref())?;
        let mode = options.mode.unwrap_or(ColorMode::Hdr);

        rules.push(StartRule {
            device: options.device.clone(),
            mode,
            path: path.clone(),
            ramp,
        });
    }

    Ok(rules)
}

#[derive(Clone, Debug)]
struct StartRule {
    device: Option<DeviceSelector>,
    mode: ColorMode,
    path: PathBuf,
    ramp: GammaRamp,
}

impl StartRule {
    fn active_device_indices(&self, platform: &impl DisplayPlatform) -> Result<Vec<usize>> {
        available_target_device_indices(platform, self.device.as_ref())
    }
}

fn target_device_indices(
    platform: &impl DisplayPlatform,
    device: Option<&DeviceSelector>,
) -> Result<Vec<usize>> {
    match device {
        Some(DeviceSelector::Index(index)) => Ok(vec![*index]),
        Some(DeviceSelector::Name(name)) => {
            let count = platform.active_device_count()?;
            let mut indices = Vec::new();
            for index in 0..count {
                let candidate = platform.device_name(index)?;
                let label = platform.device_label(index)?;
                if device_names_match(&candidate, name) || device_names_match(&label, name) {
                    indices.push(index);
                }
            }

            if indices.is_empty() {
                Err(Error::InvalidArguments(format!(
                    "device `{name}` was not found among {count} active display(s)"
                )))
            } else {
                Ok(indices)
            }
        }
        None => Ok((0..platform.active_device_count()?).collect()),
    }
}

fn available_target_device_indices(
    platform: &impl DisplayPlatform,
    device: Option<&DeviceSelector>,
) -> Result<Vec<usize>> {
    match device {
        Some(DeviceSelector::Index(index)) => {
            if device_index_available(platform, *index)? {
                Ok(vec![*index])
            } else {
                Ok(Vec::new())
            }
        }
        Some(DeviceSelector::Name(name)) => {
            let Ok(count) = platform.active_device_count() else {
                return Ok(Vec::new());
            };
            let mut indices = Vec::new();
            for index in 0..count {
                let candidate = platform.device_name(index)?;
                let label = platform.device_label(index)?;
                if device_names_match(&candidate, name) || device_names_match(&label, name) {
                    indices.push(index);
                }
            }
            Ok(indices)
        }
        None => {
            let Ok(count) = platform.active_device_count() else {
                return Ok(Vec::new());
            };
            Ok((0..count).collect())
        }
    }
}

fn device_index_available(platform: &impl DisplayPlatform, device_index: usize) -> Result<bool> {
    Ok(device_index < platform.active_device_count().unwrap_or(0))
}

fn device_names_match(candidate: &str, requested: &str) -> bool {
    candidate.eq_ignore_ascii_case(requested)
        || candidate
            .strip_prefix(r"\\.\")
            .is_some_and(|candidate| candidate.eq_ignore_ascii_case(requested))
}

#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize)]
pub struct TweakOptions {
    #[serde(default)]
    pub device: Option<DeviceSelector>,
    #[serde(default)]
    pub lut: Option<PathBuf>,
    #[serde(default)]
    pub mode: Option<ColorMode>,
    #[serde(default)]
    pub adjust: Option<AdjustOptions>,
    #[serde(default)]
    pub windows: WindowsTweakOptions,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(untagged)]
pub enum DeviceSelector {
    Index(usize),
    Name(String),
}

impl DeviceSelector {
    pub fn label(&self) -> String {
        match self {
            Self::Index(index) => index.to_string(),
            Self::Name(name) => name.clone(),
        }
    }
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
            self.device = overrides.device.clone();
        }
        if overrides.lut.is_some() {
            self.lut = overrides.lut.clone();
        }
        if overrides.mode.is_some() {
            self.mode = overrides.mode;
        }
        if overrides.adjust.is_some() {
            self.adjust = overrides.adjust.clone();
        }
        if overrides.windows.auto_color_management.is_some() {
            self.windows = overrides.windows.clone();
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

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize)]
pub struct WindowsTweakOptions {
    #[serde(default, rename = "autoColorManagement")]
    pub auto_color_management: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize)]
pub struct AdjustOptions {
    #[serde(default)]
    pub contrast: Option<f32>,
    #[serde(default)]
    pub brightness: Option<f32>,
    #[serde(default)]
    pub gamma: Option<f32>,
    #[serde(default)]
    pub gain: Option<[f32; CHANNELS]>,
    #[serde(default)]
    pub offset: Option<[f32; CHANNELS]>,
}

impl AdjustOptions {
    pub fn apply_to(&self, ramp: &GammaRamp) -> Result<GammaRamp> {
        let contrast = self.finite_or_default(self.contrast, 1.0, "contrast")?;
        let brightness = self.finite_or_default(self.brightness, 0.0, "brightness")?;
        let gamma = self.finite_or_default(self.gamma, 1.0, "gamma")?;
        if gamma <= 0.0 {
            return Err(Error::InvalidArguments(
                "`adjust.gamma` must be greater than 0".to_string(),
            ));
        }

        let gain = self.finite_array_or_default(self.gain, [1.0; CHANNELS], "gain")?;
        let offset = self.finite_array_or_default(self.offset, [0.0; CHANNELS], "offset")?;

        let mut values = [[0u16; ENTRIES]; CHANNELS];
        for (channel, channel_values) in values.iter_mut().enumerate() {
            for (index, entry) in channel_values.iter_mut().enumerate() {
                let mut value = ramp.values()[channel][index] as f32 / u16::MAX as f32;
                value = (value + brightness).clamp(0.0, 1.0);

                // HDR-safe soft contrast
                let c = contrast.max(0.01);
                let x = value - 0.5;
                value = 0.5 + (x * c) / (1.0 + x.abs() * (c - 1.0) * 2.0);

                value = value.clamp(0.0, 1.0).powf(1.0 / gamma);
                value = value * gain[channel] + offset[channel];
                if !value.is_finite() {
                    return Err(Error::InvalidArguments(
                        "`adjust` produced a non-finite gamma ramp value".to_string(),
                    ));
                }
                *entry = normalized_to_u16(value);
            }
        }

        Ok(GammaRamp::from_values(values))
    }

    fn finite_or_default(
        &self,
        value: Option<f32>,
        default: f32,
        name: &'static str,
    ) -> Result<f32> {
        let value = value.unwrap_or(default);
        if value.is_finite() {
            Ok(value)
        } else {
            Err(Error::InvalidArguments(format!(
                "`adjust.{name}` must be finite"
            )))
        }
    }

    fn finite_array_or_default(
        &self,
        value: Option<[f32; CHANNELS]>,
        default: [f32; CHANNELS],
        name: &'static str,
    ) -> Result<[f32; CHANNELS]> {
        let value = value.unwrap_or(default);
        if value.iter().all(|item| item.is_finite()) {
            Ok(value)
        } else {
            Err(Error::InvalidArguments(format!(
                "`adjust.{name}` values must be finite"
            )))
        }
    }
}

fn normalized_to_u16(value: f32) -> u16 {
    (value.clamp(0.0, 1.0) * u16::MAX as f32).round() as u16
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
    let directory = default_luts_dir();
    let lut_path = directory.join(name).with_extension("lut");
    let cube_path = directory.join(name).with_extension("cube");

    match (lut_path.exists(), cube_path.exists()) {
        (false, true) => cube_path,
        _ => lut_path,
    }
}

fn default_luts_dir() -> PathBuf {
    let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
    else {
        return PathBuf::from("luts");
    };

    if exe_dir.file_name().is_some_and(|name| name == "deps")
        && let Some(profile_dir) = exe_dir.parent()
    {
        return profile_dir.join("luts");
    }

    exe_dir.join("luts")
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
