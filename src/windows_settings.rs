use std::collections::BTreeMap;
use std::path::Path;

use crate::app::{TweakOptions, WindowsColorProfile};
use crate::error::{Error, Result};
use crate::logging;
use crate::platform::{DisplayPlatform, SystemDisplayPlatform};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowsSettingsApply {
    NotConfigured,
    AlreadyApplied,
    Applied,
}

pub fn apply_from_config_file(path: impl AsRef<Path>) -> Result<WindowsSettingsApply> {
    apply_from_tweaks(&TweakOptions::list_from_config_file(path)?)
}

pub fn apply_current_user_from_config_file(path: impl AsRef<Path>) -> Result<WindowsSettingsApply> {
    apply_current_user_from_tweaks(&TweakOptions::list_from_config_file(path)?)
}

pub fn needs_elevated_apply(path: impl AsRef<Path>) -> Result<bool> {
    let settings = desired_settings(&TweakOptions::list_from_config_file(path)?)?;
    let Some(acm) = settings.auto_color_management else {
        return Ok(false);
    };

    Ok(!auto_color_management_matches(acm)?)
}

pub fn apply_from_tweaks(tweaks: &[TweakOptions]) -> Result<WindowsSettingsApply> {
    let profile_result = apply_current_user_from_tweaks(tweaks)?;
    let acm_result = apply_auto_color_management_from_tweaks(tweaks)?;

    Ok(combine_apply_results(profile_result, acm_result))
}

pub fn apply_current_user_from_tweaks(tweaks: &[TweakOptions]) -> Result<WindowsSettingsApply> {
    let platform = SystemDisplayPlatform::new();
    let actions = planned_display_profile_actions(&platform, tweaks)?;
    if actions.is_empty() {
        return Ok(WindowsSettingsApply::NotConfigured);
    }

    for action in &actions {
        logging::info(format!(
            "applying Windows {} color profile on device {}: {}",
            action.kind.name(),
            action.device_index,
            action.profile.label()
        ));
        apply_display_profile_action(action.device_index, action.kind, &action.profile)?;
    }

    Ok(WindowsSettingsApply::Applied)
}

pub fn apply_auto_color_management_from_tweaks(
    tweaks: &[TweakOptions],
) -> Result<WindowsSettingsApply> {
    let settings = desired_settings(tweaks)?;
    let Some(acm) = settings.auto_color_management else {
        return Ok(WindowsSettingsApply::NotConfigured);
    };

    if auto_color_management_matches(acm)? {
        logging::info("recommended Windows settings are already applied");
        return Ok(WindowsSettingsApply::AlreadyApplied);
    }

    logging::info(format!("applying Windows auto color management={acm}"));
    set_auto_color_management(acm)?;
    logging::info("restarting monitor devices after Windows color setting change");
    restart_monitor_devices()?;
    logging::info("recommended Windows settings applied");
    Ok(WindowsSettingsApply::Applied)
}

fn combine_apply_results(
    left: WindowsSettingsApply,
    right: WindowsSettingsApply,
) -> WindowsSettingsApply {
    match (left, right) {
        (WindowsSettingsApply::Applied, _) | (_, WindowsSettingsApply::Applied) => {
            WindowsSettingsApply::Applied
        }
        (WindowsSettingsApply::AlreadyApplied, _) | (_, WindowsSettingsApply::AlreadyApplied) => {
            WindowsSettingsApply::AlreadyApplied
        }
        (WindowsSettingsApply::NotConfigured, WindowsSettingsApply::NotConfigured) => {
            WindowsSettingsApply::NotConfigured
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DisplayProfileKind {
    Sdr,
    Hdr,
}

impl DisplayProfileKind {
    fn is_advanced_color(self) -> bool {
        matches!(self, Self::Hdr)
    }

    fn name(self) -> &'static str {
        match self {
            Self::Sdr => "SDR",
            Self::Hdr => "HDR",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayProfileAction {
    pub device_index: usize,
    pub kind: DisplayProfileKind,
    pub profile: WindowsColorProfile,
}

pub fn planned_display_profile_actions(
    platform: &impl DisplayPlatform,
    tweaks: &[TweakOptions],
) -> Result<Vec<DisplayProfileAction>> {
    let mut desired = BTreeMap::<(usize, DisplayProfileKind), WindowsColorProfile>::new();

    for tweak in tweaks {
        let devices = crate::app::target_device_indices(platform, tweak.device.as_ref())?;
        for device_index in devices {
            push_profile_action(
                &mut desired,
                device_index,
                DisplayProfileKind::Sdr,
                &tweak.windows.sdr_color_profile,
            )?;
            push_profile_action(
                &mut desired,
                device_index,
                DisplayProfileKind::Hdr,
                &tweak.windows.hdr_color_profile,
            )?;
        }
    }

    Ok(desired
        .into_iter()
        .map(|((device_index, kind), profile)| DisplayProfileAction {
            device_index,
            kind,
            profile,
        })
        .collect())
}

fn push_profile_action(
    desired: &mut BTreeMap<(usize, DisplayProfileKind), WindowsColorProfile>,
    device_index: usize,
    kind: DisplayProfileKind,
    profile: &WindowsColorProfile,
) -> Result<()> {
    if profile.is_unset() {
        return Ok(());
    }

    let key = (device_index, kind);
    if let Some(existing) = desired.get(&key) {
        if existing != profile {
            return Err(Error::InvalidArguments(format!(
                "conflicting `windows.{}` values for device {device_index} in config",
                match kind {
                    DisplayProfileKind::Sdr => "sdrColorProfile",
                    DisplayProfileKind::Hdr => "hdrColorProfile",
                }
            )));
        }
        return Ok(());
    }

    desired.insert(key, profile.clone());
    Ok(())
}

fn desired_settings(tweaks: &[TweakOptions]) -> Result<DesiredWindowsSettings> {
    let mut desired = DesiredWindowsSettings::default();

    for tweak in tweaks {
        if let Some(value) = tweak.windows.auto_color_management {
            match desired.auto_color_management {
                Some(existing) if existing != value => {
                    return Err(Error::InvalidArguments(
                        "conflicting `windows.autoColorManagement` values in config".to_string(),
                    ));
                }
                _ => desired.auto_color_management = Some(value),
            }
        }
    }

    Ok(desired)
}

#[derive(Default)]
struct DesiredWindowsSettings {
    auto_color_management: Option<bool>,
}

#[cfg(windows)]
fn auto_color_management_matches(enabled: bool) -> Result<bool> {
    windows::auto_color_management_matches(enabled)
}

#[cfg(not(windows))]
fn auto_color_management_matches(_enabled: bool) -> Result<bool> {
    Err(Error::platform(
        "Windows settings are only supported on Windows",
    ))
}

#[cfg(windows)]
fn set_auto_color_management(enabled: bool) -> Result<()> {
    windows::set_auto_color_management(enabled)
}

#[cfg(not(windows))]
fn set_auto_color_management(_enabled: bool) -> Result<()> {
    Err(Error::platform(
        "Windows settings are only supported on Windows",
    ))
}

#[cfg(windows)]
fn restart_monitor_devices() -> Result<()> {
    windows::restart_monitor_devices()
}

#[cfg(not(windows))]
fn restart_monitor_devices() -> Result<()> {
    Err(Error::platform(
        "Windows settings are only supported on Windows",
    ))
}

#[cfg(windows)]
fn apply_display_profile_action(
    device_index: usize,
    kind: DisplayProfileKind,
    profile: &WindowsColorProfile,
) -> Result<()> {
    windows::apply_display_profile_action(device_index, kind, profile)
}

#[cfg(not(windows))]
fn apply_display_profile_action(
    _device_index: usize,
    _kind: DisplayProfileKind,
    _profile: &WindowsColorProfile,
) -> Result<()> {
    Err(Error::platform(
        "Windows color profiles are only supported on Windows",
    ))
}

#[cfg(windows)]
mod windows {
    use std::ffi::{OsStr, c_void};
    use std::fs;
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Path, PathBuf};
    use std::ptr;
    use std::slice;
    use std::thread;
    use std::time::Duration;

    use crate::app::WindowsColorProfile;
    use crate::error::{Error, Result};
    use crate::logging;
    use crate::windows_settings::DisplayProfileKind;

    const ERROR_NO_MORE_ITEMS: i32 = 259;
    const ERROR_SUCCESS: i32 = 0;
    const QDC_ONLY_ACTIVE_PATHS: u32 = 0x00000002;
    const DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME: u32 = 1;
    const HKEY_LOCAL_MACHINE: Hkey = 0x80000002usize as Hkey;
    const WCS_PROFILE_MANAGEMENT_SCOPE_CURRENT_USER: i32 = 1;
    const S_OK: i32 = 0;
    const HRESULT_FILE_NOT_FOUND: i32 = 0x80070002u32 as i32;
    const HRESULT_NOT_FOUND: i32 = 0x80070490u32 as i32;
    const KEY_QUERY_VALUE: u32 = 0x0001;
    const KEY_SET_VALUE: u32 = 0x0002;
    const KEY_ENUMERATE_SUB_KEYS: u32 = 0x0008;
    const KEY_WOW64_64KEY: u32 = 0x0100;
    const REG_DWORD: u32 = 4;
    const ACM_VALUE_NAME: &str = "AutoColorManagementEnabled";
    const MONITOR_DATA_STORE: &str =
        r"SYSTEM\CurrentControlSet\Control\GraphicsDrivers\MonitorDataStore";
    const GUID_DEVCLASS_MONITOR: Guid = Guid {
        data1: 0x4d36e96e,
        data2: 0xe325,
        data3: 0x11ce,
        data4: [0xbf, 0xc1, 0x08, 0x00, 0x2b, 0xe1, 0x03, 0x18],
    };
    const DIGCF_PRESENT: u32 = 0x00000002;
    const DIF_PROPERTYCHANGE: u32 = 0x00000012;
    const DICS_ENABLE: u32 = 0x00000001;
    const DICS_DISABLE: u32 = 0x00000002;
    const DICS_FLAG_GLOBAL: u32 = 0x00000001;
    const MONITOR_RESTART_DELAY: Duration = Duration::from_secs(2);

    pub fn auto_color_management_matches(enabled: bool) -> Result<bool> {
        let desired = u32::from(enabled);
        let states = read_monitor_acm_states()?;
        Ok(states.iter().all(|state| state.unwrap_or(0) == desired))
    }

    pub fn set_auto_color_management(enabled: bool) -> Result<()> {
        let desired = u32::from(enabled);
        let root = RegKey::open(
            HKEY_LOCAL_MACHINE,
            MONITOR_DATA_STORE,
            KEY_ENUMERATE_SUB_KEYS | KEY_WOW64_64KEY,
        )?;
        for monitor_key in root.subkey_names()? {
            let monitor = RegKey::open(
                root.0,
                &monitor_key,
                KEY_SET_VALUE | KEY_QUERY_VALUE | KEY_WOW64_64KEY,
            )?;
            monitor.set_dword(ACM_VALUE_NAME, desired)?;
        }

        Ok(())
    }

    pub fn restart_monitor_devices() -> Result<()> {
        let devices = MonitorDevices::present()?;
        let mut restarted = 0;
        let mut index = 0;

        while let Some(mut device) = devices.device_at(index)? {
            set_device_enabled(devices.handle, &mut device, false)?;
            thread::sleep(MONITOR_RESTART_DELAY);
            set_device_enabled(devices.handle, &mut device, true)?;
            restarted += 1;
            index += 1;
        }

        if restarted == 0 {
            return Err(Error::platform("no present monitor PnP devices were found"));
        }

        Ok(())
    }

    pub fn apply_display_profile_action(
        device_index: usize,
        kind: DisplayProfileKind,
        profile: &WindowsColorProfile,
    ) -> Result<()> {
        let target = display_profile_target(device_index)?;
        match profile {
            WindowsColorProfile::Unset => Ok(()),
            WindowsColorProfile::Clear => clear_display_profiles(&target, kind),
            WindowsColorProfile::Set(path) => set_display_profile(&target, kind, path),
        }
    }

    fn set_display_profile(
        target: &DisplayProfileTarget,
        kind: DisplayProfileKind,
        path: &Path,
    ) -> Result<()> {
        let full_path = fully_qualified_profile_path(path)?;
        let ok = unsafe { InstallColorProfileW(ptr::null(), wide_path(&full_path).as_ptr()) };
        if ok == 0 {
            return Err(last_error(&format!(
                "InstallColorProfileW failed for {}",
                full_path.display()
            )));
        }

        let profile_name = full_path
            .file_name()
            .ok_or_else(|| {
                Error::InvalidArguments(format!(
                    "color profile path has no file name: {}",
                    full_path.display()
                ))
            })?
            .to_string_lossy();
        let status = unsafe {
            ColorProfileAddDisplayAssociation(
                WCS_PROFILE_MANAGEMENT_SCOPE_CURRENT_USER,
                wide(&profile_name).as_ptr(),
                target.target_adapter_id,
                target.source_id,
                1,
                bool_to_win32(kind.is_advanced_color()),
            )
        };

        if hresult_failed(status) {
            return Err(Error::platform(format!(
                "ColorProfileAddDisplayAssociation failed for {} on device {} ({}) with HRESULT 0x{:08X}",
                full_path.display(),
                target.device_index,
                kind.name(),
                status as u32
            )));
        }

        Ok(())
    }

    fn clear_display_profiles(
        target: &DisplayProfileTarget,
        kind: DisplayProfileKind,
    ) -> Result<()> {
        let mut profile_list = ptr::null_mut();
        let mut profile_count = 0u32;
        let status = unsafe {
            ColorProfileGetDisplayList(
                WCS_PROFILE_MANAGEMENT_SCOPE_CURRENT_USER,
                target.target_adapter_id,
                target.source_id,
                &mut profile_list,
                &mut profile_count,
            )
        };
        if hresult_failed(status) {
            return Err(Error::platform(format!(
                "ColorProfileGetDisplayList failed for device {} ({}) with HRESULT 0x{:08X}",
                target.device_index,
                kind.name(),
                status as u32
            )));
        }

        let _profile_list = ProfileList(profile_list);
        if profile_list.is_null() || profile_count == 0 {
            return Ok(());
        }

        let profiles = unsafe { slice::from_raw_parts(profile_list, profile_count as usize) };
        for profile in profiles
            .iter()
            .copied()
            .filter(|profile| !profile.is_null())
        {
            let status = unsafe {
                ColorProfileRemoveDisplayAssociation(
                    WCS_PROFILE_MANAGEMENT_SCOPE_CURRENT_USER,
                    profile.cast_const(),
                    target.target_adapter_id,
                    target.source_id,
                    bool_to_win32(kind.is_advanced_color()),
                )
            };
            if hresult_failed(status) {
                if profile_remove_can_be_ignored(status) {
                    logging::warn(format!(
                        "skipping color profile association removal for {} on device {} ({}) because it is not associated in that mode",
                        wide_ptr_to_string(profile),
                        target.device_index,
                        kind.name()
                    ));
                    continue;
                }

                let name = wide_ptr_to_string(profile);
                return Err(Error::platform(format!(
                    "ColorProfileRemoveDisplayAssociation failed for {name} on device {} ({}) with HRESULT 0x{:08X}",
                    target.device_index,
                    kind.name(),
                    status as u32
                )));
            }
        }

        Ok(())
    }

    fn display_profile_target(device_index: usize) -> Result<DisplayProfileTarget> {
        let display = active_display_name(device_index)?;
        let path = active_display_path_for_display_name(&display)?;
        Ok(DisplayProfileTarget {
            device_index,
            target_adapter_id: path.target_info.adapter_id,
            source_id: path.source_info.id,
        })
    }

    fn active_display_path_for_display_name(display_name: &[u16]) -> Result<DisplayConfigPathInfo> {
        for path in active_display_paths()? {
            let source_name = source_display_name(&path)?;
            if source_name == display_name {
                return Ok(path);
            }
        }

        Err(Error::platform(format!(
            "could not match {} to an active display path",
            display_name_lossy(display_name)
        )))
    }

    fn active_display_paths() -> Result<Vec<DisplayConfigPathInfo>> {
        let mut path_count = 0;
        let mut mode_count = 0;
        let status = unsafe {
            GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count)
        };

        if status != ERROR_SUCCESS {
            return Err(Error::platform(format!(
                "GetDisplayConfigBufferSizes failed with status {status}"
            )));
        }

        let mut paths = vec![DisplayConfigPathInfo::default(); path_count as usize];
        let mut modes = vec![DisplayConfigModeInfo::default(); mode_count as usize];
        let status = unsafe {
            QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut path_count,
                paths.as_mut_ptr(),
                &mut mode_count,
                modes.as_mut_ptr(),
                ptr::null_mut(),
            )
        };

        if status != ERROR_SUCCESS {
            return Err(Error::platform(format!(
                "QueryDisplayConfig failed with status {status}"
            )));
        }

        paths.truncate(path_count as usize);
        Ok(paths)
    }

    fn source_display_name(path: &DisplayConfigPathInfo) -> Result<Vec<u16>> {
        let mut source_name = DisplayConfigSourceDeviceName {
            header: DisplayConfigDeviceInfoHeader {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                size: size_of::<DisplayConfigSourceDeviceName>() as u32,
                adapter_id: path.source_info.adapter_id,
                id: path.source_info.id,
            },
            view_gdi_device_name: [0; 32],
        };

        let status =
            unsafe { DisplayConfigGetDeviceInfo((&mut source_name as *mut _) as *mut c_void) };
        if status != ERROR_SUCCESS {
            return Err(Error::platform(format!(
                "DisplayConfigGetDeviceInfo source name failed with status {status}"
            )));
        }

        Ok(null_terminated_slice(&source_name.view_gdi_device_name))
    }

    fn active_display_name(device_index: usize) -> Result<Vec<u16>> {
        let names = active_display_names()?;
        names.get(device_index).cloned().ok_or_else(|| {
            Error::platform(format!(
                "device index {device_index} is out of range; {} active display(s) found",
                names.len()
            ))
        })
    }

    fn active_display_names() -> Result<Vec<Vec<u16>>> {
        let mut names = Vec::new();
        let mut index = 0;

        loop {
            let mut device = DisplayDeviceW {
                cb: size_of::<DisplayDeviceW>() as u32,
                ..Default::default()
            };

            let ok = unsafe { EnumDisplayDevicesW(ptr::null(), index, &mut device, 0) };
            if ok == 0 {
                break;
            }

            if device.state_flags & 0x00000001 != 0 {
                names.push(null_terminated_slice(&device.device_name));
            }

            index += 1;
        }

        if names.is_empty() {
            return Err(Error::platform("no active display devices found"));
        }

        Ok(names)
    }

    fn fully_qualified_profile_path(path: &Path) -> Result<PathBuf> {
        fs::canonicalize(path).map_err(|source| Error::Io {
            path: Some(path.to_path_buf()),
            source,
        })
    }

    fn null_terminated_slice(value: &[u16]) -> Vec<u16> {
        let len = value
            .iter()
            .position(|char| *char == 0)
            .unwrap_or(value.len());
        let mut output = value[..len].to_vec();
        output.push(0);
        output
    }

    fn display_name_lossy(value: &[u16]) -> String {
        let len = value
            .iter()
            .position(|char| *char == 0)
            .unwrap_or(value.len());
        String::from_utf16_lossy(&value[..len])
    }

    fn wide_ptr_to_string(value: *mut u16) -> String {
        if value.is_null() {
            return String::new();
        }

        let mut len = 0usize;
        unsafe {
            while *value.add(len) != 0 {
                len += 1;
            }
            String::from_utf16_lossy(slice::from_raw_parts(value, len))
        }
    }

    fn set_device_enabled(
        devices: Hdevinfo,
        device: &mut SpDevinfoData,
        enabled: bool,
    ) -> Result<()> {
        let state_change = if enabled { DICS_ENABLE } else { DICS_DISABLE };
        let action = if enabled { "enable" } else { "disable" };
        let mut params = SpPropchangeParams {
            class_install_header: SpClassinstallHeader {
                cb_size: size_of::<SpClassinstallHeader>() as u32,
                install_function: DIF_PROPERTYCHANGE,
            },
            state_change,
            scope: DICS_FLAG_GLOBAL,
            hw_profile: 0,
        };

        let ok = unsafe {
            SetupDiSetClassInstallParamsW(
                devices,
                device,
                (&mut params as *mut SpPropchangeParams).cast::<SpClassinstallHeader>(),
                size_of::<SpPropchangeParams>() as u32,
            )
        };
        if ok == 0 {
            return Err(last_error(&format!(
                "SetupDiSetClassInstallParamsW failed while trying to {action} monitor device"
            )));
        }

        let ok = unsafe { SetupDiCallClassInstaller(DIF_PROPERTYCHANGE, devices, device) };
        if ok == 0 {
            return Err(last_error(&format!(
                "SetupDiCallClassInstaller failed while trying to {action} monitor device"
            )));
        }

        Ok(())
    }

    fn read_monitor_acm_states() -> Result<Vec<Option<u32>>> {
        let root = RegKey::open(
            HKEY_LOCAL_MACHINE,
            MONITOR_DATA_STORE,
            KEY_ENUMERATE_SUB_KEYS | KEY_WOW64_64KEY,
        )?;
        let mut states = Vec::new();
        for monitor_key in root.subkey_names()? {
            let monitor = RegKey::open(root.0, &monitor_key, KEY_QUERY_VALUE | KEY_WOW64_64KEY)?;
            states.push(monitor.query_dword(ACM_VALUE_NAME)?);
        }
        Ok(states)
    }

    struct MonitorDevices {
        handle: Hdevinfo,
    }

    impl MonitorDevices {
        fn present() -> Result<Self> {
            let handle = unsafe {
                SetupDiGetClassDevsW(
                    &GUID_DEVCLASS_MONITOR,
                    ptr::null(),
                    ptr::null_mut(),
                    DIGCF_PRESENT,
                )
            };

            if handle == INVALID_HANDLE_VALUE {
                return Err(last_error(
                    "SetupDiGetClassDevsW failed for monitor devices",
                ));
            }

            Ok(Self { handle })
        }

        fn device_at(&self, index: u32) -> Result<Option<SpDevinfoData>> {
            let mut device = SpDevinfoData {
                cb_size: size_of::<SpDevinfoData>() as u32,
                ..Default::default()
            };
            let ok = unsafe { SetupDiEnumDeviceInfo(self.handle, index, &mut device) };
            if ok != 0 {
                return Ok(Some(device));
            }

            let error = unsafe { GetLastError() } as i32;
            if error == ERROR_NO_MORE_ITEMS {
                Ok(None)
            } else {
                Err(Error::platform(format!(
                    "SetupDiEnumDeviceInfo failed for monitor device {index} with status {error}"
                )))
            }
        }
    }

    impl Drop for MonitorDevices {
        fn drop(&mut self) {
            unsafe {
                SetupDiDestroyDeviceInfoList(self.handle);
            }
        }
    }

    struct RegKey(Hkey);

    impl RegKey {
        fn open(parent: Hkey, path: &str, access: u32) -> Result<Self> {
            let mut key = ptr::null_mut();
            let status = unsafe { RegOpenKeyExW(parent, wide(path).as_ptr(), 0, access, &mut key) };
            if status != 0 {
                return Err(Error::platform(format!(
                    "RegOpenKeyExW failed for {path} with status {status}"
                )));
            }

            Ok(Self(key))
        }

        fn subkey_names(&self) -> Result<Vec<String>> {
            let mut names = Vec::new();
            let mut index = 0;
            loop {
                let mut buffer = [0u16; 256];
                let mut len = buffer.len() as u32;
                let status = unsafe {
                    RegEnumKeyExW(
                        self.0,
                        index,
                        buffer.as_mut_ptr(),
                        &mut len,
                        ptr::null_mut(),
                        ptr::null_mut(),
                        ptr::null_mut(),
                        ptr::null_mut(),
                    )
                };

                if status == ERROR_NO_MORE_ITEMS {
                    break;
                }
                if status != 0 {
                    return Err(Error::platform(format!(
                        "RegEnumKeyExW failed with status {status}"
                    )));
                }

                names.push(String::from_utf16_lossy(&buffer[..len as usize]));
                index += 1;
            }

            Ok(names)
        }

        fn query_dword(&self, name: &str) -> Result<Option<u32>> {
            let mut value = 0u32;
            let mut value_type = 0u32;
            let mut size = size_of::<u32>() as u32;
            let status = unsafe {
                RegQueryValueExW(
                    self.0,
                    wide(name).as_ptr(),
                    ptr::null_mut(),
                    &mut value_type,
                    (&mut value as *mut u32).cast::<u8>(),
                    &mut size,
                )
            };

            if status == 2 {
                return Ok(None);
            }
            if status != 0 {
                return Err(Error::platform(format!(
                    "RegQueryValueExW failed for {name} with status {status}"
                )));
            }
            if value_type != REG_DWORD {
                return Err(Error::platform(format!(
                    "{name} has unexpected registry type {value_type}"
                )));
            }

            Ok(Some(value))
        }

        fn set_dword(&self, name: &str, value: u32) -> Result<()> {
            let status = unsafe {
                RegSetValueExW(
                    self.0,
                    wide(name).as_ptr(),
                    0,
                    REG_DWORD,
                    (&value as *const u32).cast::<u8>(),
                    size_of::<u32>() as u32,
                )
            };

            if status == 0 {
                Ok(())
            } else {
                Err(Error::platform(format!(
                    "RegSetValueExW failed for {name} with status {status}"
                )))
            }
        }
    }

    impl Drop for RegKey {
        fn drop(&mut self) {
            unsafe {
                RegCloseKey(self.0);
            }
        }
    }

    struct ProfileList(*mut *mut u16);

    impl Drop for ProfileList {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    LocalFree(self.0.cast::<c_void>());
                }
            }
        }
    }

    fn hresult_failed(status: i32) -> bool {
        status < S_OK
    }

    fn profile_remove_can_be_ignored(status: i32) -> bool {
        matches!(status, HRESULT_FILE_NOT_FOUND | HRESULT_NOT_FOUND)
    }

    fn bool_to_win32(value: bool) -> i32 {
        i32::from(value)
    }

    fn wide(value: impl AsRef<str>) -> Vec<u16> {
        OsStr::new(value.as_ref())
            .encode_wide()
            .chain([0])
            .collect()
    }

    fn wide_path(value: &Path) -> Vec<u16> {
        value.as_os_str().encode_wide().chain([0]).collect()
    }

    fn last_error(context: &str) -> Error {
        Error::platform(format!("{context} with status {}", unsafe {
            GetLastError()
        }))
    }

    type Hkey = *mut c_void;
    type Hdevinfo = *mut c_void;
    const INVALID_HANDLE_VALUE: Hdevinfo = -1isize as Hdevinfo;

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
    struct Luid {
        low_part: u32,
        high_part: i32,
    }

    #[derive(Clone, Copy, Debug)]
    struct DisplayProfileTarget {
        device_index: usize,
        target_adapter_id: Luid,
        source_id: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Guid {
        data1: u32,
        data2: u16,
        data3: u16,
        data4: [u8; 8],
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct SpDevinfoData {
        cb_size: u32,
        class_guid: Guid,
        dev_inst: u32,
        reserved: usize,
    }

    #[repr(C)]
    struct SpClassinstallHeader {
        cb_size: u32,
        install_function: u32,
    }

    #[repr(C)]
    struct SpPropchangeParams {
        class_install_header: SpClassinstallHeader,
        state_change: u32,
        scope: u32,
        hw_profile: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    struct DisplayConfigPathInfo {
        source_info: DisplayConfigPathSourceInfo,
        target_info: DisplayConfigPathTargetInfo,
        flags: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    struct DisplayConfigPathSourceInfo {
        adapter_id: Luid,
        id: u32,
        mode_info_idx: u32,
        status_flags: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    struct DisplayConfigPathTargetInfo {
        adapter_id: Luid,
        id: u32,
        mode_info_idx: u32,
        output_technology: u32,
        rotation: u32,
        scaling: u32,
        refresh_rate: DisplayConfigRational,
        scan_line_ordering: u32,
        target_available: i32,
        status_flags: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    struct DisplayConfigRational {
        numerator: u32,
        denominator: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    struct DisplayConfigModeInfo {
        info_type: u32,
        id: u32,
        adapter_id: Luid,
        payload: [u8; 56],
    }

    impl Default for DisplayConfigModeInfo {
        fn default() -> Self {
            Self {
                info_type: 0,
                id: 0,
                adapter_id: Luid::default(),
                payload: [0; 56],
            }
        }
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    struct DisplayConfigDeviceInfoHeader {
        r#type: u32,
        size: u32,
        adapter_id: Luid,
        id: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    struct DisplayConfigSourceDeviceName {
        header: DisplayConfigDeviceInfoHeader,
        view_gdi_device_name: [u16; 32],
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    struct DisplayDeviceW {
        cb: u32,
        device_name: [u16; 32],
        device_string: [u16; 128],
        state_flags: u32,
        device_id: [u16; 128],
        device_key: [u16; 128],
    }

    impl Default for DisplayDeviceW {
        fn default() -> Self {
            Self {
                cb: 0,
                device_name: [0; 32],
                device_string: [0; 128],
                state_flags: 0,
                device_id: [0; 128],
                device_key: [0; 128],
            }
        }
    }

    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn RegOpenKeyExW(
            key: Hkey,
            sub_key: *const u16,
            options: u32,
            desired: u32,
            result: *mut Hkey,
        ) -> i32;
        fn RegEnumKeyExW(
            key: Hkey,
            index: u32,
            name: *mut u16,
            name_len: *mut u32,
            reserved: *mut u32,
            class: *mut u16,
            class_len: *mut u32,
            last_write_time: *mut c_void,
        ) -> i32;
        fn RegQueryValueExW(
            key: Hkey,
            value_name: *const u16,
            reserved: *mut u32,
            value_type: *mut u32,
            data: *mut u8,
            data_size: *mut u32,
        ) -> i32;
        fn RegSetValueExW(
            key: Hkey,
            value_name: *const u16,
            reserved: u32,
            value_type: u32,
            data: *const u8,
            data_size: u32,
        ) -> i32;
        fn RegCloseKey(key: Hkey) -> i32;
    }

    #[link(name = "setupapi")]
    unsafe extern "system" {
        fn SetupDiGetClassDevsW(
            class_guid: *const Guid,
            enumerator: *const u16,
            hwnd_parent: *mut c_void,
            flags: u32,
        ) -> Hdevinfo;
        fn SetupDiEnumDeviceInfo(
            device_info_set: Hdevinfo,
            member_index: u32,
            device_info_data: *mut SpDevinfoData,
        ) -> i32;
        fn SetupDiSetClassInstallParamsW(
            device_info_set: Hdevinfo,
            device_info_data: *mut SpDevinfoData,
            class_install_params: *mut SpClassinstallHeader,
            class_install_params_size: u32,
        ) -> i32;
        fn SetupDiCallClassInstaller(
            install_function: u32,
            device_info_set: Hdevinfo,
            device_info_data: *mut SpDevinfoData,
        ) -> i32;
        fn SetupDiDestroyDeviceInfoList(device_info_set: Hdevinfo) -> i32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLastError() -> u32;
        fn LocalFree(memory: *mut c_void) -> *mut c_void;
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetDisplayConfigBufferSizes(
            flags: u32,
            path_count: *mut u32,
            mode_count: *mut u32,
        ) -> i32;
        fn QueryDisplayConfig(
            flags: u32,
            path_count: *mut u32,
            paths: *mut DisplayConfigPathInfo,
            mode_count: *mut u32,
            modes: *mut DisplayConfigModeInfo,
            current_topology_id: *mut u32,
        ) -> i32;
        fn DisplayConfigGetDeviceInfo(request_packet: *mut c_void) -> i32;
        fn EnumDisplayDevicesW(
            lp_device: *const u16,
            i_dev_num: u32,
            lp_display_device: *mut DisplayDeviceW,
            dw_flags: u32,
        ) -> i32;
    }

    #[link(name = "mscms")]
    unsafe extern "system" {
        fn InstallColorProfileW(machine_name: *const u16, profile_name: *const u16) -> i32;
        fn ColorProfileAddDisplayAssociation(
            scope: i32,
            profile_name: *const u16,
            target_adapter_id: Luid,
            source_id: u32,
            set_as_default: i32,
            associate_as_advanced_color: i32,
        ) -> i32;
        fn ColorProfileGetDisplayList(
            scope: i32,
            target_adapter_id: Luid,
            source_id: u32,
            profile_list: *mut *mut *mut u16,
            profile_count: *mut u32,
        ) -> i32;
        fn ColorProfileRemoveDisplayAssociation(
            scope: i32,
            profile_name: *const u16,
            target_adapter_id: Luid,
            source_id: u32,
            dissociate_advanced_color: i32,
        ) -> i32;
    }
}
