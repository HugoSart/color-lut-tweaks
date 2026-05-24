use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use crate::error::{Error, Result};
use crate::logging;
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
            let mode_matches = if let Some(mode) = &tweaks.mode {
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
            tweaks.mode.clone().unwrap_or(ColorMode::Hdr),
            path,
            tweaks.adjust.as_ref(),
        )
    } else {
        let mut previous_states = vec![None; device_indices.len()];
        loop {
            for (position, device_index) in device_indices.iter().enumerate() {
                let hdr_enabled = platform.hdr_enabled(*device_index)?;
                if previous_states[position] != Some(hdr_enabled) {
                    report_info(format_args!(
                        "Device {device_index}: HDR {}; no LUT configured",
                        if hdr_enabled { "enabled" } else { "disabled" }
                    ));
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
                report_info(format_args!(
                    "Device {device_index}: {} mode active; applied LUT",
                    mode.name()
                ));
            } else if !mode_matches && applied[position] {
                let (_, original_ramp) = &original_ramps[position];
                platform.apply_gamma_ramp(*device_index, original_ramp)?;
                applied[position] = false;
                report_info(format_args!(
                    "Device {device_index}: {} mode inactive; restored previous gamma ramp",
                    mode.name()
                ));
            } else if previous_hdr_states[position] != Some(hdr_enabled) {
                report_info(format_args!(
                    "Device {device_index}: HDR {}; waiting for {} mode",
                    if hdr_enabled { "enabled" } else { "disabled" },
                    mode.name()
                ));
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
                    report_warn(format_args!(
                        "Device {device_index}: could not reapply tweak yet: {err}"
                    ));
                }
                continue;
            }

            match desired_rule {
                Some(rule_index) => {
                    let rule = &self.rules[rule_index];
                    if let Err(err) = platform.apply_gamma_ramp(device_index, &rule.ramp) {
                        report_warn(format_args!(
                            "Device {device_index}: could not apply {} yet: {err}",
                            rule.path.display()
                        ));
                        continue;
                    }
                    report_info(format_args!(
                        "Device {device_index}: {} mode active; applied {}",
                        rule.mode.name(),
                        rule.path.display()
                    ));
                }
                None => {
                    let original_ramp =
                        self.original_ramps.get(&device_index).ok_or_else(|| {
                            Error::Platform(format!(
                                "missing captured gamma ramp for device {device_index}"
                            ))
                        })?;
                    if let Err(err) = platform.apply_gamma_ramp(device_index, original_ramp) {
                        report_warn(format_args!(
                            "Device {device_index}: could not restore previous gamma ramp yet: {err}"
                        ));
                        continue;
                    }
                    report_info(format_args!(
                        "Device {device_index}: no tweak configured for {}; restored previous gamma ramp",
                        active_mode.name()
                    ));
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
                    report_warn(format_args!(
                        "Device {device_index}: monitor detected but gamma ramp is not available yet"
                    ));
                    continue;
                };

                self.original_ramps.insert(device_index, original_ramp);
                report_info(format_args!(
                    "Device {device_index}: monitor available; captured original gamma ramp"
                ));
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
            report_info(format_args!(
                "Device {device_index}: reapplied {} after gamma ramp changed",
                rule.path.display()
            ));
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TweakModeFilter {
    pub ignore_sdr_adjustments: bool,
    pub ignore_hdr_adjustments: bool,
}

impl TweakModeFilter {
    pub fn apply_to(self, tweaks: &[TweakOptions]) -> Vec<TweakOptions> {
        tweaks
            .iter()
            .filter_map(|tweak| {
                let mut tweak = tweak.clone();
                tweak.mode = self.filtered_mode(tweak.mode.as_ref())?;
                Some(tweak)
            })
            .collect()
    }

    pub fn includes(self, tweak: &TweakOptions) -> bool {
        self.filtered_mode(tweak.mode.as_ref()).is_some()
    }

    fn filtered_mode(self, mode: Option<&ColorMode>) -> Option<Option<ColorMode>> {
        let defaulted = mode.cloned().unwrap_or(ColorMode::Hdr);
        let filtered = defaulted.filter_modes(|mode| match mode {
            ColorMode::Hdr => !self.ignore_hdr_adjustments,
            ColorMode::Sdr => !self.ignore_sdr_adjustments,
            ColorMode::Any(_) => false,
        })?;

        if mode.is_none() && filtered == ColorMode::Hdr {
            Some(None)
        } else {
            Some(Some(filtered))
        }
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
        for mode in options
            .mode
            .clone()
            .unwrap_or(ColorMode::Hdr)
            .concrete_modes()
        {
            rules.push(StartRule {
                device: options.device.clone(),
                mode,
                path: path.clone(),
                ramp: ramp.clone(),
            });
        }
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

pub(crate) fn target_device_indices(
    platform: &impl DisplayPlatform,
    device: Option<&DeviceSelector>,
) -> Result<Vec<usize>> {
    match device {
        Some(device) => target_device_indices_for_selector(platform, device, true),
        None => Ok((0..platform.active_device_count()?).collect()),
    }
}

fn available_target_device_indices(
    platform: &impl DisplayPlatform,
    device: Option<&DeviceSelector>,
) -> Result<Vec<usize>> {
    match device {
        Some(device) => target_device_indices_for_selector(platform, device, false),
        None => {
            let Ok(count) = platform.active_device_count() else {
                return Ok(Vec::new());
            };
            Ok((0..count).collect())
        }
    }
}

fn target_device_indices_for_selector(
    platform: &impl DisplayPlatform,
    device: &DeviceSelector,
    strict: bool,
) -> Result<Vec<usize>> {
    match device {
        DeviceSelector::Index(index) => {
            if strict || device_index_available(platform, *index)? {
                Ok(vec![*index])
            } else {
                Ok(Vec::new())
            }
        }
        DeviceSelector::Name(name) => target_device_indices_for_name(platform, name, strict),
        DeviceSelector::Any(selectors) => {
            let mut indices = Vec::new();
            for selector in selectors {
                for index in target_device_indices_for_selector(platform, selector, false)? {
                    if !indices.contains(&index) {
                        indices.push(index);
                    }
                }
            }
            if strict && indices.is_empty() {
                Err(Error::InvalidArguments(format!(
                    "device `{}` was not found among active displays",
                    device.label()
                )))
            } else {
                Ok(indices)
            }
        }
    }
}

fn target_device_indices_for_name(
    platform: &impl DisplayPlatform,
    name: &str,
    strict: bool,
) -> Result<Vec<usize>> {
    let count = match platform.active_device_count() {
        Ok(count) => count,
        Err(err) if !strict => {
            logging::warn(format!(
                "could not read active display count while matching device `{name}`: {err}"
            ));
            return Ok(Vec::new());
        }
        Err(err) => return Err(err),
    };

    for match_kind in [
        DeviceStringMatch::HardwareId,
        DeviceStringMatch::ReadableName,
        DeviceStringMatch::DisplayName,
    ] {
        let mut indices = Vec::new();
        for index in 0..count {
            if device_string_matches(platform, index, name, match_kind)? {
                indices.push(index);
            }
        }

        if !indices.is_empty() {
            return Ok(indices);
        }
    }

    if strict {
        Err(Error::InvalidArguments(format!(
            "device `{name}` was not found among {count} active display(s)"
        )))
    } else {
        Ok(Vec::new())
    }
}

#[derive(Clone, Copy)]
enum DeviceStringMatch {
    HardwareId,
    ReadableName,
    DisplayName,
}

fn device_string_matches(
    platform: &impl DisplayPlatform,
    device_index: usize,
    requested: &str,
    match_kind: DeviceStringMatch,
) -> Result<bool> {
    let candidate = match match_kind {
        DeviceStringMatch::HardwareId => platform.device_hardware_id(device_index)?,
        DeviceStringMatch::ReadableName => platform.device_label(device_index)?,
        DeviceStringMatch::DisplayName => platform.device_name(device_index)?,
    };

    Ok(device_names_match(&candidate, requested))
}

fn device_index_available(platform: &impl DisplayPlatform, device_index: usize) -> Result<bool> {
    match platform.active_device_count() {
        Ok(count) => Ok(device_index < count),
        Err(err) => {
            logging::warn(format!(
                "could not read active display count while checking device {device_index}: {err}"
            ));
            Ok(false)
        }
    }
}

fn device_names_match(candidate: &str, requested: &str) -> bool {
    candidate.eq_ignore_ascii_case(requested)
        || candidate
            .strip_prefix(r"\\.\")
            .is_some_and(|candidate| candidate.eq_ignore_ascii_case(requested))
}

fn report_info(message: fmt::Arguments<'_>) {
    let message = message.to_string();
    logging::info(&message);
    println!("{message}");
}

fn report_warn(message: fmt::Arguments<'_>) {
    let message = message.to_string();
    logging::warn(&message);
    println!("WARN  {message}");
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceSelector {
    Index(usize),
    Name(String),
    Any(Vec<DeviceSelector>),
}

impl DeviceSelector {
    pub fn label(&self) -> String {
        match self {
            Self::Index(index) => index.to_string(),
            Self::Name(name) => name.clone(),
            Self::Any(selectors) => selectors
                .iter()
                .map(Self::label)
                .collect::<Vec<_>>()
                .join(","),
        }
    }
}

impl<'de> serde::Deserialize<'de> for DeviceSelector {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum DeviceSelectorConfig {
            Index(usize),
            Name(String),
            Any(Vec<DeviceSelectorConfig>),
        }

        fn from_config(config: DeviceSelectorConfig) -> DeviceSelector {
            match config {
                DeviceSelectorConfig::Index(index) => DeviceSelector::Index(index),
                DeviceSelectorConfig::Name(name) => DeviceSelector::Name(name),
                DeviceSelectorConfig::Any(selectors) => {
                    DeviceSelector::Any(selectors.into_iter().map(from_config).collect())
                }
            }
        }

        DeviceSelectorConfig::deserialize(deserializer).map(from_config)
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
            self.mode = overrides.mode.clone();
        }
        if overrides.adjust.is_some() {
            self.adjust = overrides.adjust.clone();
        }
        if overrides.windows.auto_color_management.is_some() {
            self.windows = overrides.windows.clone();
        }
        if !overrides.windows.sdr_color_profile.is_unset() {
            self.windows.sdr_color_profile = overrides.windows.sdr_color_profile.clone();
        }
        if !overrides.windows.hdr_color_profile.is_unset() {
            self.windows.hdr_color_profile = overrides.windows.hdr_color_profile.clone();
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
        self.windows.resolve_paths_relative_to(config_path);
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize)]
pub struct WindowsTweakOptions {
    #[serde(default, rename = "autoColorManagement")]
    pub auto_color_management: Option<bool>,
    #[serde(
        default,
        rename = "sdrColorProfile",
        deserialize_with = "deserialize_windows_color_profile"
    )]
    pub sdr_color_profile: WindowsColorProfile,
    #[serde(
        default,
        rename = "hdrColorProfile",
        deserialize_with = "deserialize_windows_color_profile"
    )]
    pub hdr_color_profile: WindowsColorProfile,
}

impl WindowsTweakOptions {
    fn resolve_paths_relative_to(&mut self, config_path: &Path) {
        self.sdr_color_profile
            .resolve_paths_relative_to(config_path);
        self.hdr_color_profile
            .resolve_paths_relative_to(config_path);
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum WindowsColorProfile {
    #[default]
    Unset,
    Clear,
    Set(PathBuf),
}

impl WindowsColorProfile {
    pub fn is_unset(&self) -> bool {
        matches!(self, Self::Unset)
    }

    pub fn label(&self) -> String {
        match self {
            Self::Unset => "unset".to_string(),
            Self::Clear => "clear".to_string(),
            Self::Set(path) => path.display().to_string(),
        }
    }

    fn resolve_paths_relative_to(&mut self, config_path: &Path) {
        let Self::Set(profile) = self else {
            return;
        };
        if is_named_profile(profile) {
            *profile = default_profiles_path(profile);
        } else if profile.is_relative()
            && let Some(parent) = config_path.parent()
        {
            *profile = parent.join(&profile);
        }
    }
}

fn deserialize_windows_color_profile<'de, D>(
    deserializer: D,
) -> std::result::Result<WindowsColorProfile, D::Error>
where
    D: serde::Deserializer<'de>,
{
    <Option<PathBuf> as serde::Deserialize>::deserialize(deserializer).map(
        |profile| match profile {
            Some(profile) => WindowsColorProfile::Set(profile),
            None => WindowsColorProfile::Clear,
        },
    )
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

fn is_named_profile(path: &Path) -> bool {
    let value = path.as_os_str().to_string_lossy();
    !value.is_empty() && !value.contains('/') && !value.contains('\\') && path.extension().is_none()
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

fn default_profiles_path(name: &Path) -> PathBuf {
    let directory = default_profiles_dir();
    let icm_path = directory.join(name).with_extension("icm");
    let icc_path = directory.join(name).with_extension("icc");

    match (icm_path.exists(), icc_path.exists()) {
        (false, true) => icc_path,
        _ => icm_path,
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

fn default_profiles_dir() -> PathBuf {
    let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
    else {
        return PathBuf::from("profiles");
    };

    if exe_dir.file_name().is_some_and(|name| name == "deps")
        && let Some(profile_dir) = exe_dir.parent()
    {
        return profile_dir.join("profiles");
    }

    exe_dir.join("profiles")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ColorMode {
    Hdr,
    Sdr,
    Any(Vec<ColorMode>),
}

impl ColorMode {
    pub fn from_hdr_enabled(hdr_enabled: bool) -> Self {
        if hdr_enabled { Self::Hdr } else { Self::Sdr }
    }

    pub fn matches_hdr_enabled(&self, hdr_enabled: bool) -> bool {
        match self {
            Self::Hdr => hdr_enabled,
            Self::Sdr => !hdr_enabled,
            Self::Any(modes) => modes
                .iter()
                .any(|mode| mode.matches_hdr_enabled(hdr_enabled)),
        }
    }

    pub fn name(&self) -> String {
        match self {
            Self::Hdr => "HDR".to_string(),
            Self::Sdr => "SDR".to_string(),
            Self::Any(modes) => modes.iter().map(Self::name).collect::<Vec<_>>().join("/"),
        }
    }

    fn concrete_modes(self) -> Vec<Self> {
        match self {
            Self::Hdr => vec![Self::Hdr],
            Self::Sdr => vec![Self::Sdr],
            Self::Any(modes) => {
                let mut concrete = Vec::new();
                for mode in modes.into_iter().flat_map(Self::concrete_modes) {
                    if !concrete.contains(&mode) {
                        concrete.push(mode);
                    }
                }
                concrete
            }
        }
    }

    fn filter_modes(self, include: impl Fn(&ColorMode) -> bool) -> Option<Self> {
        let modes = self
            .concrete_modes()
            .into_iter()
            .filter(include)
            .collect::<Vec<_>>();

        match modes.as_slice() {
            [] => None,
            [mode] => Some(mode.clone()),
            _ => Some(Self::Any(modes)),
        }
    }
}

impl<'de> serde::Deserialize<'de> for ColorMode {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "lowercase")]
        enum ColorModeValue {
            Hdr,
            Sdr,
        }

        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum ColorModeConfig {
            Single(ColorModeValue),
            Any(Vec<ColorModeValue>),
        }

        fn from_value(value: ColorModeValue) -> ColorMode {
            match value {
                ColorModeValue::Hdr => ColorMode::Hdr,
                ColorModeValue::Sdr => ColorMode::Sdr,
            }
        }

        ColorModeConfig::deserialize(deserializer).map(|config| match config {
            ColorModeConfig::Single(value) => from_value(value),
            ColorModeConfig::Any(values) => {
                ColorMode::Any(values.into_iter().map(from_value).collect())
            }
        })
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
