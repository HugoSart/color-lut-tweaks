use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::app::TweakOptions;
use crate::error::{Error, Result};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct ConfigFile {
    #[serde(default)]
    pub device: Option<usize>,
    #[serde(default)]
    pub hdr_lut: Option<PathBuf>,
}

impl ConfigFile {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let json = fs::read_to_string(path).map_err(|source| Error::Io {
            path: Some(path.to_path_buf()),
            source,
        })?;

        let mut config =
            serde_json::from_str::<Self>(&json).map_err(|source| Error::ConfigJson {
                path: path.to_path_buf(),
                source,
            })?;

        if let Some(lut) = &config.hdr_lut
            && lut.is_relative()
            && let Some(parent) = path.parent()
        {
            config.hdr_lut = Some(parent.join(lut));
        }

        Ok(config)
    }

    pub fn into_tweaks(self) -> TweakOptions {
        TweakOptions {
            device: self.device,
            lut: self.hdr_lut,
        }
    }
}
