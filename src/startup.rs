#[cfg(windows)]
mod windows_startup {
    use std::ffi::{OsStr, c_void};
    use std::os::windows::ffi::OsStrExt;
    use std::path::PathBuf;
    use std::ptr;

    use crate::error::{Error, Result};

    const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    const VALUE_NAME: &str = "ColorLutTweaks";

    const ERROR_SUCCESS: i32 = 0;
    const ERROR_FILE_NOT_FOUND: i32 = 2;
    const REG_SZ: u32 = 1;
    const KEY_QUERY_VALUE: u32 = 0x0001;
    const KEY_SET_VALUE: u32 = 0x0002;
    const HKEY_CURRENT_USER: Hkey = 0x80000001usize as Hkey;

    pub fn enable() -> Result<()> {
        let command = startup_command()?;
        let key = RegistryKey::create_current_user_run(KEY_SET_VALUE)?;
        let name = wide(VALUE_NAME);
        let value = wide(&command);
        let bytes = value.len() * size_of::<u16>();
        let status = unsafe {
            RegSetValueExW(
                key.0,
                name.as_ptr(),
                0,
                REG_SZ,
                value.as_ptr().cast::<u8>(),
                bytes as u32,
            )
        };

        if status != ERROR_SUCCESS {
            return Err(Error::platform(format!(
                "RegSetValueExW failed with status {status}"
            )));
        }

        Ok(())
    }

    pub fn disable() -> Result<()> {
        let Some(key) = RegistryKey::open_current_user_run(KEY_SET_VALUE)? else {
            return Ok(());
        };
        let name = wide(VALUE_NAME);
        let status = unsafe { RegDeleteValueW(key.0, name.as_ptr()) };
        match status {
            ERROR_SUCCESS | ERROR_FILE_NOT_FOUND => Ok(()),
            status => Err(Error::platform(format!(
                "RegDeleteValueW failed with status {status}"
            ))),
        }
    }

    pub fn enabled() -> Result<bool> {
        let Some(value) = read_startup_value()? else {
            return Ok(false);
        };

        Ok(value == startup_command()?)
    }

    fn read_startup_value() -> Result<Option<String>> {
        let Some(key) = RegistryKey::open_current_user_run(KEY_QUERY_VALUE)? else {
            return Ok(None);
        };
        let name = wide(VALUE_NAME);
        let mut value_type = 0;
        let mut byte_len = 0;
        let status = unsafe {
            RegQueryValueExW(
                key.0,
                name.as_ptr(),
                ptr::null_mut(),
                &mut value_type,
                ptr::null_mut(),
                &mut byte_len,
            )
        };

        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if status != ERROR_SUCCESS {
            return Err(Error::platform(format!(
                "RegQueryValueExW failed with status {status}"
            )));
        }
        if value_type != REG_SZ {
            return Ok(None);
        }

        let mut value = vec![0u16; byte_len as usize / size_of::<u16>()];
        let status = unsafe {
            RegQueryValueExW(
                key.0,
                name.as_ptr(),
                ptr::null_mut(),
                &mut value_type,
                value.as_mut_ptr().cast::<u8>(),
                &mut byte_len,
            )
        };

        if status != ERROR_SUCCESS {
            return Err(Error::platform(format!(
                "RegQueryValueExW failed with status {status}"
            )));
        }

        if let Some(null_index) = value.iter().position(|value| *value == 0) {
            value.truncate(null_index);
        }

        Ok(Some(String::from_utf16_lossy(&value)))
    }

    fn startup_command() -> Result<String> {
        let exe = std::env::current_exe().map_err(|source| Error::Io { path: None, source })?;
        Ok(format!("\"{}\"", display_path(exe)))
    }

    fn display_path(path: PathBuf) -> String {
        path.to_string_lossy().into_owned()
    }

    struct RegistryKey(Hkey);

    impl RegistryKey {
        fn create_current_user_run(access: u32) -> Result<Self> {
            let path = wide(RUN_KEY);
            let mut key = ptr::null_mut();
            let status = unsafe {
                RegCreateKeyExW(
                    HKEY_CURRENT_USER,
                    path.as_ptr(),
                    0,
                    ptr::null_mut(),
                    0,
                    access,
                    ptr::null_mut(),
                    &mut key,
                    ptr::null_mut(),
                )
            };

            if status != ERROR_SUCCESS {
                return Err(Error::platform(format!(
                    "RegCreateKeyExW failed with status {status}"
                )));
            }

            Ok(Self(key))
        }

        fn open_current_user_run(access: u32) -> Result<Option<Self>> {
            let path = wide(RUN_KEY);
            let mut key = ptr::null_mut();
            let status =
                unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, access, &mut key) };

            if status == ERROR_FILE_NOT_FOUND {
                return Ok(None);
            }
            if status != ERROR_SUCCESS {
                return Err(Error::platform(format!(
                    "RegOpenKeyExW failed with status {status}"
                )));
            }

            Ok(Some(Self(key)))
        }
    }

    impl Drop for RegistryKey {
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
        fn RegCreateKeyExW(
            key: Hkey,
            sub_key: *const u16,
            reserved: u32,
            class: *mut u16,
            options: u32,
            sam_desired: u32,
            security_attributes: *mut c_void,
            result: *mut Hkey,
            disposition: *mut u32,
        ) -> i32;
        fn RegOpenKeyExW(
            key: Hkey,
            sub_key: *const u16,
            options: u32,
            sam_desired: u32,
            result: *mut Hkey,
        ) -> i32;
        fn RegSetValueExW(
            key: Hkey,
            value_name: *const u16,
            reserved: u32,
            value_type: u32,
            data: *const u8,
            data_size: u32,
        ) -> i32;
        fn RegQueryValueExW(
            key: Hkey,
            value_name: *const u16,
            reserved: *mut u32,
            value_type: *mut u32,
            data: *mut u8,
            data_size: *mut u32,
        ) -> i32;
        fn RegDeleteValueW(key: Hkey, value_name: *const u16) -> i32;
        fn RegCloseKey(key: Hkey) -> i32;
    }
}

#[cfg(windows)]
pub use windows_startup::{disable, enable, enabled};

#[cfg(not(windows))]
pub fn enable() -> crate::Result<()> {
    Err(crate::Error::platform(
        "start with Windows is only supported on Windows",
    ))
}

#[cfg(not(windows))]
pub fn disable() -> crate::Result<()> {
    Err(crate::Error::platform(
        "start with Windows is only supported on Windows",
    ))
}

#[cfg(not(windows))]
pub fn enabled() -> crate::Result<bool> {
    Ok(false)
}
