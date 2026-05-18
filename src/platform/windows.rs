use std::ffi::c_void;
use std::mem::size_of;
use std::ptr;

use crate::error::{Error, Result};
use crate::lut::{ENTRIES, GammaRamp};
use crate::platform::DisplayPlatform;

const ERROR_SUCCESS: i32 = 0;
const QDC_ONLY_ACTIVE_PATHS: u32 = 0x00000002;
const DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO: u32 = 9;
const DISPLAY_DEVICE_ACTIVE: u32 = 0x00000001;
const DISPLAY_DRIVER: &[u16] = &[
    'D' as u16, 'I' as u16, 'S' as u16, 'P' as u16, 'L' as u16, 'A' as u16, 'Y' as u16, 0,
];

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
    let hdc = DisplayDc::primary()?;
    let mut values = [[0u16; ENTRIES]; 3];
    let ok = unsafe { GetDeviceGammaRamp(hdc.0, values.as_mut_ptr().cast::<c_void>()) };

    if ok == 0 {
        return Err(last_os_error("GetDeviceGammaRamp failed"));
    }

    let bytes = values
        .iter()
        .flatten()
        .flat_map(|value| value.to_le_bytes())
        .collect::<Vec<_>>();

    GammaRamp::from_bytes(&bytes)
}

fn apply_gamma_ramp(ramp: &GammaRamp) -> Result<()> {
    let displays = active_display_names()?;
    let mut last_error = None;

    for display in displays {
        let hdc = DisplayDc::for_display(&display)?;
        let ok = unsafe { SetDeviceGammaRamp(hdc.0, ramp.values().as_ptr().cast::<c_void>()) };

        if ok != 0 {
            return Ok(());
        }

        last_error = Some(last_os_error(&format!(
            "SetDeviceGammaRamp failed for {}",
            display_name_lossy(&display)
        )));
    }

    Err(last_error.unwrap_or_else(|| Error::platform("no active display devices found")))
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

fn active_display_names() -> Result<Vec<Vec<u16>>> {
    let mut names = Vec::new();
    let mut index = 0;

    loop {
        let mut device = DisplayDeviceW::default();
        device.cb = size_of::<DisplayDeviceW>() as u32;

        let ok = unsafe { EnumDisplayDevicesW(ptr::null(), index, &mut device, 0) };
        if ok == 0 {
            break;
        }

        if device.state_flags & DISPLAY_DEVICE_ACTIVE != 0 {
            names.push(null_terminated_slice(&device.device_name));
        }

        index += 1;
    }

    if names.is_empty() {
        return Err(Error::platform("no active display devices found"));
    }

    Ok(names)
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

fn last_os_error(context: &str) -> Error {
    let error = std::io::Error::last_os_error();
    Error::platform(format!("{context}: {error}"))
}

struct DisplayDc(Hdc);

impl DisplayDc {
    fn primary() -> Result<Self> {
        let displays = active_display_names()?;
        Self::for_display(&displays[0])
    }

    fn for_display(display_name: &[u16]) -> Result<Self> {
        let hdc = unsafe {
            CreateDCW(
                DISPLAY_DRIVER.as_ptr(),
                display_name.as_ptr(),
                ptr::null(),
                ptr::null(),
            )
        };
        if hdc.is_null() {
            return Err(last_os_error(&format!(
                "CreateDCW failed for {}",
                display_name_lossy(display_name)
            )));
        }

        Ok(Self(hdc))
    }
}

impl Drop for DisplayDc {
    fn drop(&mut self) {
        unsafe {
            DeleteDC(self.0);
        }
    }
}

type Hdc = *mut c_void;

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

#[link(name = "user32")]
unsafe extern "system" {
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
    fn EnumDisplayDevicesW(
        lp_device: *const u16,
        i_dev_num: u32,
        lp_display_device: *mut DisplayDeviceW,
        dw_flags: u32,
    ) -> i32;
}

#[link(name = "gdi32")]
unsafe extern "system" {
    fn CreateDCW(
        driver: *const u16,
        device: *const u16,
        output: *const u16,
        init_data: *const c_void,
    ) -> Hdc;
    fn DeleteDC(hdc: Hdc) -> i32;
    fn GetDeviceGammaRamp(hdc: Hdc, ramp: *mut c_void) -> i32;
    fn SetDeviceGammaRamp(hdc: Hdc, ramp: *const c_void) -> i32;
}
