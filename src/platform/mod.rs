use crate::error::Result;
use crate::lut::GammaRamp;

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::WindowsDisplayPlatform as SystemDisplayPlatform;

#[cfg(not(windows))]
pub use unsupported::UnsupportedDisplayPlatform as SystemDisplayPlatform;

pub trait DisplayPlatform {
    fn active_device_count(&self) -> Result<usize>;
    fn device_name(&self, device_index: usize) -> Result<String>;
    fn device_hardware_id(&self, device_index: usize) -> Result<String> {
        self.device_name(device_index)
    }
    fn device_label(&self, device_index: usize) -> Result<String> {
        self.device_name(device_index)
    }
    fn hdr_enabled(&self, device_index: usize) -> Result<bool>;
    fn capture_gamma_ramp(&self, device_index: usize) -> Result<GammaRamp>;
    fn apply_gamma_ramp(&self, device_index: usize, ramp: &GammaRamp) -> Result<()>;
}

#[cfg(not(windows))]
mod unsupported {
    use crate::error::{Error, Result};
    use crate::lut::GammaRamp;
    use crate::platform::DisplayPlatform;

    pub struct UnsupportedDisplayPlatform;

    impl UnsupportedDisplayPlatform {
        pub fn new() -> Self {
            Self
        }
    }

    impl DisplayPlatform for UnsupportedDisplayPlatform {
        fn active_device_count(&self) -> Result<usize> {
            Err(Error::platform(
                "enumerating Windows display devices is only supported on Windows",
            ))
        }

        fn device_name(&self, _device_index: usize) -> Result<String> {
            Err(Error::platform(
                "enumerating Windows display devices is only supported on Windows",
            ))
        }

        fn device_hardware_id(&self, _device_index: usize) -> Result<String> {
            Err(Error::platform(
                "enumerating Windows display devices is only supported on Windows",
            ))
        }

        fn device_label(&self, _device_index: usize) -> Result<String> {
            Err(Error::platform(
                "enumerating Windows display devices is only supported on Windows",
            ))
        }

        fn hdr_enabled(&self, _device_index: usize) -> Result<bool> {
            Err(Error::platform(
                "reading Windows HDR state is only supported on Windows",
            ))
        }

        fn capture_gamma_ramp(&self, _device_index: usize) -> Result<GammaRamp> {
            Err(Error::platform(
                "capturing a Windows gamma ramp is only supported on Windows",
            ))
        }

        fn apply_gamma_ramp(&self, _device_index: usize, _ramp: &GammaRamp) -> Result<()> {
            Err(Error::platform(
                "applying a Windows gamma ramp is only supported on Windows",
            ))
        }
    }
}
