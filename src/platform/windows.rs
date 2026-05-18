use std::ffi::c_void;
use std::mem::size_of;
use std::ptr;

use crate::error::{Error, Result};
use crate::lut::{ENTRIES, GammaRamp};
use crate::platform::DisplayPlatform;

const ERROR_SUCCESS: i32 = 0;
const QDC_ONLY_ACTIVE_PATHS: u32 = 0x00000002;
const DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO: u32 = 9;

pub struct WindowsDisplayPlatform;

impl WindowsDisplayPlatform {
    pub fn new() -> Self {
        Self
    }
}

impl DisplayPlatform for WindowsDisplayPlatform {
    fn hdr_enabled(&self) -> Result<bool> {
        hdr_enabled()
    }

    fn capture_gamma_ramp(&self) -> Result<GammaRamp> {
        capture_gamma_ramp()
    }

    fn apply_gamma_ramp(&self, ramp: &GammaRamp) -> Result<()> {
        apply_gamma_ramp(ramp)
    }
}

fn capture_gamma_ramp() -> Result<GammaRamp> {
    let hdc = ScreenDc::get()?;
    let mut values = [[0u16; ENTRIES]; 3];
    let ok = unsafe { GetDeviceGammaRamp(hdc.0, values.as_mut_ptr().cast::<c_void>()) };

    if ok == 0 {
        return Err(Error::platform("GetDeviceGammaRamp failed"));
    }

    let bytes = values
        .iter()
        .flatten()
        .flat_map(|value| value.to_le_bytes())
        .collect::<Vec<_>>();

    GammaRamp::from_bytes(&bytes)
}

fn apply_gamma_ramp(ramp: &GammaRamp) -> Result<()> {
    let hdc = ScreenDc::get()?;
    let ok = unsafe { SetDeviceGammaRamp(hdc.0, ramp.values().as_ptr().cast::<c_void>()) };

    if ok == 0 {
        return Err(Error::platform("SetDeviceGammaRamp failed"));
    }

    Ok(())
}

fn hdr_enabled() -> Result<bool> {
    let paths = active_display_paths()?;

    for path in paths {
        let mut info = DisplayConfigGetAdvancedColorInfo {
            header: DisplayConfigDeviceInfoHeader {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO,
                size: size_of::<DisplayConfigGetAdvancedColorInfo>() as u32,
                adapter_id: path.target_info.adapter_id,
                id: path.target_info.id,
            },
            value: 0,
            color_encoding: 0,
            bits_per_color_channel: 0,
        };

        let status = unsafe { DisplayConfigGetDeviceInfo((&mut info as *mut _) as *mut c_void) };
        if status != ERROR_SUCCESS {
            continue;
        }

        if info.value & 0x2 != 0 {
            return Ok(true);
        }
    }

    Ok(false)
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

struct ScreenDc(Hdc);

impl ScreenDc {
    fn get() -> Result<Self> {
        let hdc = unsafe { GetDC(ptr::null_mut()) };
        if hdc.is_null() {
            return Err(Error::platform("GetDC(NULL) failed"));
        }

        Ok(Self(hdc))
    }
}

impl Drop for ScreenDc {
    fn drop(&mut self) {
        unsafe {
            ReleaseDC(ptr::null_mut(), self.0);
        }
    }
}

type Hdc = *mut c_void;
type Hwnd = *mut c_void;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct Luid {
    low_part: u32,
    high_part: i32,
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
struct DisplayConfigGetAdvancedColorInfo {
    header: DisplayConfigDeviceInfoHeader,
    value: u32,
    color_encoding: u32,
    bits_per_color_channel: u32,
}

#[link(name = "user32")]
unsafe extern "system" {
    fn GetDC(hwnd: Hwnd) -> Hdc;
    fn ReleaseDC(hwnd: Hwnd, hdc: Hdc) -> i32;
    fn GetDisplayConfigBufferSizes(flags: u32, path_count: *mut u32, mode_count: *mut u32) -> i32;
    fn QueryDisplayConfig(
        flags: u32,
        path_count: *mut u32,
        paths: *mut DisplayConfigPathInfo,
        mode_count: *mut u32,
        modes: *mut DisplayConfigModeInfo,
        current_topology_id: *mut u32,
    ) -> i32;
    fn DisplayConfigGetDeviceInfo(request_packet: *mut c_void) -> i32;
}

#[link(name = "gdi32")]
unsafe extern "system" {
    fn GetDeviceGammaRamp(hdc: Hdc, ramp: *mut c_void) -> i32;
    fn SetDeviceGammaRamp(hdc: Hdc, ramp: *const c_void) -> i32;
}
