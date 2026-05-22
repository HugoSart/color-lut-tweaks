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
    use std::process::Command;
    use std::ptr;

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
        let script = r#"
$ErrorActionPreference = "Stop"
$devices = @(Get-PnpDevice -Class Monitor -PresentOnly)
if ($devices.Count -eq 0) {
    throw "No present monitor PnP devices were found."
}
foreach ($device in $devices) {
    Disable-PnpDevice -InstanceId $device.InstanceId -Confirm:$false
    Start-Sleep -Seconds 2
    Enable-PnpDevice -InstanceId $device.InstanceId -Confirm:$false
}
"#;

        let status = Command::new("powershell.exe")
            .arg("-NoProfile")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-Command")
            .arg(script)
            .status()
            .map_err(|source| Error::Io { path: None, source })?;

        if status.success() {
            Ok(())
        } else {
            Err(Error::platform(format!(
                "failed to restart monitor devices; powershell exited with {status}"
            )))
        }
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

    type Hkey = *mut c_void;

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
}
