use std::path::Path;

use crate::app::TweakOptions;
use crate::error::{Error, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowsSettingsApply {
    NotConfigured,
    AlreadyApplied,
    Applied,
}

pub fn apply_from_config_file(path: impl AsRef<Path>) -> Result<WindowsSettingsApply> {
    apply_from_tweaks(&TweakOptions::list_from_config_file(path)?)
}

pub fn needs_elevated_apply(path: impl AsRef<Path>) -> Result<bool> {
    let settings = desired_settings(&TweakOptions::list_from_config_file(path)?)?;
    let Some(acm) = settings.auto_color_management else {
        return Ok(false);
    };

    Ok(!auto_color_management_matches(acm)?)
}

pub fn apply_from_tweaks(tweaks: &[TweakOptions]) -> Result<WindowsSettingsApply> {
    let settings = desired_settings(tweaks)?;
    let Some(acm) = settings.auto_color_management else {
        return Ok(WindowsSettingsApply::NotConfigured);
    };

    if auto_color_management_matches(acm)? {
        return Ok(WindowsSettingsApply::AlreadyApplied);
    }

    set_auto_color_management(acm)?;
    restart_monitor_devices()?;
    Ok(WindowsSettingsApply::Applied)
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
mod windows {
    use std::ffi::{OsStr, c_void};
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;
    use std::thread;
    use std::time::Duration;

    use crate::error::{Error, Result};

    const ERROR_NO_MORE_ITEMS: i32 = 259;
    const HKEY_LOCAL_MACHINE: Hkey = 0x80000002usize as Hkey;
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

    fn wide(value: impl AsRef<str>) -> Vec<u16> {
        OsStr::new(value.as_ref())
            .encode_wide()
            .chain([0])
            .collect()
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
    }
}
