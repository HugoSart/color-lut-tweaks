use std::ffi::c_void;
use std::mem::size_of;
use std::ptr;

use crate::error::{Error, Result};
use crate::lut::{ENTRIES, GammaRamp};
use crate::platform::DisplayPlatform;

const ERROR_SUCCESS: i32 = 0;
const QDC_ONLY_ACTIVE_PATHS: u32 = 0x00000002;
const DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME: u32 = 1;
const DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME: u32 = 2;
const DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO: u32 = 9;
const DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO_2: u32 = 15;
const DISPLAYCONFIG_ADVANCED_COLOR_MODE_HDR: u32 = 2;
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
    fn active_device_count(&self) -> Result<usize> {
        Ok(active_display_names()?.len())
    }

    fn device_name(&self, device_index: usize) -> Result<String> {
        Ok(display_name_lossy(&active_display_name(device_index)?))
    }

    fn device_label(&self, device_index: usize) -> Result<String> {
        device_label(device_index)
    }

    fn hdr_enabled(&self, device_index: usize) -> Result<bool> {
        hdr_enabled(device_index)
    }

    fn capture_gamma_ramp(&self, device_index: usize) -> Result<GammaRamp> {
        capture_gamma_ramp(device_index)
    }

    fn apply_gamma_ramp(&self, device_index: usize, ramp: &GammaRamp) -> Result<()> {
        apply_gamma_ramp(device_index, ramp)
    }
}

fn capture_gamma_ramp(device_index: usize) -> Result<GammaRamp> {
    let display = active_display_name(device_index)?;
    let hdc = DisplayDc::for_display(&display)?;
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

fn apply_gamma_ramp(device_index: usize, ramp: &GammaRamp) -> Result<()> {
    let display = active_display_name(device_index)?;
    let hdc = DisplayDc::for_display(&display)?;
    let ok = unsafe { SetDeviceGammaRamp(hdc.0, ramp.values().as_ptr().cast::<c_void>()) };

    if ok == 0 {
        return Err(last_os_error(&format!(
            "SetDeviceGammaRamp failed for device {device_index} ({})",
            display_name_lossy(&display)
        )));
    }

    Ok(())
}

fn hdr_enabled(device_index: usize) -> Result<bool> {
    let display = active_display_name(device_index)?;
    let path = active_display_path_for_display_name(&display)?;

    if let Some(enabled) = hdr_enabled_from_advanced_color_info_2(device_index, &path)? {
        return Ok(enabled);
    }

    hdr_enabled_from_advanced_color_info(device_index, &path)
}

fn hdr_enabled_from_advanced_color_info_2(
    device_index: usize,
    path: &DisplayConfigPathInfo,
) -> Result<Option<bool>> {
    let mut info = DisplayConfigGetAdvancedColorInfo2 {
        header: DisplayConfigDeviceInfoHeader {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO_2,
            size: size_of::<DisplayConfigGetAdvancedColorInfo2>() as u32,
            adapter_id: path.target_info.adapter_id,
            id: path.target_info.id,
        },
        value: 0,
        color_encoding: 0,
        bits_per_color_channel: 0,
        active_color_mode: 0,
    };

    let status = unsafe { DisplayConfigGetDeviceInfo((&mut info as *mut _) as *mut c_void) };
    if status == ERROR_SUCCESS {
        return Ok(Some(
            info.active_color_mode == DISPLAYCONFIG_ADVANCED_COLOR_MODE_HDR,
        ));
    }

    // Older Windows builds may not support ADVANCED_COLOR_INFO_2. Fall back to
    // the older query below, but only when the newer query is unavailable.
    const ERROR_INVALID_PARAMETER: i32 = 87;
    if status == ERROR_INVALID_PARAMETER {
        return Ok(None);
    }

    Err(Error::platform(format!(
        "DisplayConfigGetDeviceInfo advanced color info 2 failed for device {device_index} with status {status}"
    )))
}

fn hdr_enabled_from_advanced_color_info(
    device_index: usize,
    path: &DisplayConfigPathInfo,
) -> Result<bool> {
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
        return Err(Error::platform(format!(
            "DisplayConfigGetDeviceInfo failed for device {device_index} with status {status}"
        )));
    }

    Ok(info.value & 0x2 != 0)
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

fn device_label(device_index: usize) -> Result<String> {
    let display = active_display_name(device_index)?;
    if let Ok(path) = active_display_path_for_display_name(&display)
        && let Ok(name) = target_friendly_name(&path)
        && !name.trim().is_empty()
    {
        return Ok(name);
    }

    monitor_device_string(&display).map_or_else(|| Ok(display_name_lossy(&display)), Ok)
}

fn target_friendly_name(path: &DisplayConfigPathInfo) -> Result<String> {
    let mut target_name = DisplayConfigTargetDeviceName {
        header: DisplayConfigDeviceInfoHeader {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
            size: size_of::<DisplayConfigTargetDeviceName>() as u32,
            adapter_id: path.target_info.adapter_id,
            id: path.target_info.id,
        },
        flags: 0,
        output_technology: 0,
        edid_manufacture_id: 0,
        edid_product_code_id: 0,
        connector_instance: 0,
        monitor_friendly_device_name: [0; 64],
        monitor_device_path: [0; 128],
    };

    let status = unsafe { DisplayConfigGetDeviceInfo((&mut target_name as *mut _) as *mut c_void) };
    if status != ERROR_SUCCESS {
        return Err(Error::platform(format!(
            "DisplayConfigGetDeviceInfo target name failed with status {status}"
        )));
    }

    Ok(display_name_lossy(
        &target_name.monitor_friendly_device_name,
    ))
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

    let status = unsafe { DisplayConfigGetDeviceInfo((&mut source_name as *mut _) as *mut c_void) };
    if status != ERROR_SUCCESS {
        return Err(Error::platform(format!(
            "DisplayConfigGetDeviceInfo source name failed with status {status}"
        )));
    }

    Ok(null_terminated_slice(&source_name.view_gdi_device_name))
}

fn monitor_device_string(display_name: &[u16]) -> Option<String> {
    let mut index = 0;
    loop {
        let mut device = DisplayDeviceW::default();
        device.cb = size_of::<DisplayDeviceW>() as u32;

        let ok = unsafe { EnumDisplayDevicesW(display_name.as_ptr(), index, &mut device, 0) };
        if ok == 0 {
            return None;
        }

        if device.state_flags & DISPLAY_DEVICE_ACTIVE != 0 {
            let value = display_name_lossy(&device.device_string);
            if !value.trim().is_empty() {
                return Some(value);
            }
        }

        index += 1;
    }
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

fn active_display_name(device_index: usize) -> Result<Vec<u16>> {
    let names = active_display_names()?;
    names.get(device_index).cloned().ok_or_else(|| {
        Error::platform(format!(
            "device index {device_index} is out of range; {} active display(s) found",
            names.len()
        ))
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

fn last_os_error(context: &str) -> Error {
    let error = std::io::Error::last_os_error();
    Error::platform(format!("{context}: {error}"))
}

struct DisplayDc(Hdc);

impl DisplayDc {
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
struct DisplayConfigGetAdvancedColorInfo2 {
    header: DisplayConfigDeviceInfoHeader,
    value: u32,
    color_encoding: u32,
    bits_per_color_channel: u32,
    active_color_mode: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct DisplayConfigSourceDeviceName {
    header: DisplayConfigDeviceInfoHeader,
    view_gdi_device_name: [u16; 32],
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct DisplayConfigTargetDeviceName {
    header: DisplayConfigDeviceInfoHeader,
    flags: u32,
    output_technology: u32,
    edid_manufacture_id: u16,
    edid_product_code_id: u16,
    connector_instance: u32,
    monitor_friendly_device_name: [u16; 64],
    monitor_device_path: [u16; 128],
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
