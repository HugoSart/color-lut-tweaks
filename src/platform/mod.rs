use crate::error::Result;
use crate::lut::GammaRamp;

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::WindowsDisplayPlatform as SystemDisplayPlatform;

#[cfg(not(windows))]
pub use unsupported::UnsupportedDisplayPlatform as SystemDisplayPlatform;

pub trait DisplayPlatform {
    fn hdr_enabled(&self) -> Result<bool>;
    fn capture_gamma_ramp(&self) -> Result<GammaRamp>;
    fn apply_gamma_ramp(&self, ramp: &GammaRamp) -> Result<()>;
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
        fn hdr_enabled(&self) -> Result<bool> {
            Err(Error::platform(
                "reading Windows HDR state is only supported on Windows",
            ))
        }

        fn capture_gamma_ramp(&self) -> Result<GammaRamp> {
            Err(Error::platform(
                "capturing a Windows gamma ramp is only supported on Windows",
            ))
        }

        fn apply_gamma_ramp(&self, _ramp: &GammaRamp) -> Result<()> {
            Err(Error::platform(
                "applying a Windows gamma ramp is only supported on Windows",
            ))
        }
    }
}
