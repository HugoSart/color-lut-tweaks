use std::fs;
use std::path::Path;

use crate::error::{Error, Result};

pub const CHANNELS: usize = 3;
pub const ENTRIES: usize = 256;
pub const LUT_SIZE: usize = CHANNELS * ENTRIES * size_of::<u16>();

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GammaRamp {
    values: [[u16; ENTRIES]; CHANNELS],
}

impl GammaRamp {
    pub fn identity() -> Self {
        let mut values = [[0u16; ENTRIES]; CHANNELS];

        for channel in &mut values {
            for (index, value) in channel.iter_mut().enumerate() {
                *value = (index as u32 * 257) as u16;
            }
        }

        Self { values }
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let bytes = fs::read(path).map_err(|source| Error::Io {
            path: Some(path.to_path_buf()),
            source,
        })?;
        Self::from_bytes(&bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != LUT_SIZE {
            return Err(Error::InvalidLutSize {
                expected: LUT_SIZE,
                actual: bytes.len(),
            });
        }

        let mut values = [[0u16; ENTRIES]; CHANNELS];
        for (slot, chunk) in values
            .iter_mut()
            .flatten()
            .zip(bytes.chunks_exact(size_of::<u16>()))
        {
            *slot = u16::from_le_bytes([chunk[0], chunk[1]]);
        }

        Ok(Self { values })
    }

    pub fn values(&self) -> &[[u16; ENTRIES]; CHANNELS] {
        &self.values
    }

    pub fn channel_summary(&self, channel: Channel) -> ChannelSummary {
        let values = &self.values[channel.index()];
        let mut min = values[0];
        let mut max = values[0];
        let mut monotonic = true;

        for pair in values.windows(2) {
            min = min.min(pair[1]);
            max = max.max(pair[1]);
            if pair[1] < pair[0] {
                monotonic = false;
            }
        }

        ChannelSummary {
            first: values[0],
            last: values[ENTRIES - 1],
            min,
            max,
            monotonic,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    Red,
    Green,
    Blue,
}

impl Channel {
    pub const ALL: [Self; CHANNELS] = [Self::Red, Self::Green, Self::Blue];

    pub fn name(self) -> &'static str {
        match self {
            Self::Red => "red",
            Self::Green => "green",
            Self::Blue => "blue",
        }
    }

    pub fn index(self) -> usize {
        match self {
            Self::Red => 0,
            Self::Green => 1,
            Self::Blue => 2,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelSummary {
    pub first: u16,
    pub last: u16,
    pub min: u16,
    pub max: u16,
    pub monotonic: bool,
}
