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

    pub fn from_cube_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(|source| Error::Io {
            path: Some(path.to_path_buf()),
            source,
        })?;
        Self::from_cube_str(&text)
    }

    pub fn from_cube_str(text: &str) -> Result<Self> {
        Cube::parse(text)?.into_gamma_ramp()
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

    pub fn from_values(values: [[u16; ENTRIES]; CHANNELS]) -> Self {
        Self { values }
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

#[derive(Clone, Debug)]
struct Cube {
    kind: CubeKind,
    size: usize,
    domain_min: [f32; CHANNELS],
    domain_max: [f32; CHANNELS],
    samples: Vec<[f32; CHANNELS]>,
}

impl Cube {
    fn parse(text: &str) -> Result<Self> {
        let mut kind = None;
        let mut size = None;
        let mut domain_min = [0.0; CHANNELS];
        let mut domain_max = [1.0; CHANNELS];
        let mut samples = Vec::new();

        for (line_index, raw_line) in text.lines().enumerate() {
            let line = raw_line
                .split_once('#')
                .map_or(raw_line, |(content, _)| content)
                .trim();
            if line.is_empty() {
                continue;
            }

            let mut parts = line.split_whitespace();
            let Some(first) = parts.next() else {
                continue;
            };
            let keyword = first.to_ascii_uppercase();
            match keyword.as_str() {
                "TITLE" => {}
                "DOMAIN_MIN" => {
                    domain_min = parse_cube_triplet(parts, line_index)?;
                }
                "DOMAIN_MAX" => {
                    domain_max = parse_cube_triplet(parts, line_index)?;
                }
                "LUT_1D_SIZE" => {
                    set_cube_kind(&mut kind, CubeKind::OneDimensional)?;
                    size = Some(parse_cube_size(parts, line_index)?);
                }
                "LUT_3D_SIZE" => {
                    set_cube_kind(&mut kind, CubeKind::ThreeDimensional)?;
                    size = Some(parse_cube_size(parts, line_index)?);
                }
                _ => {
                    let sample = parse_cube_sample(line, line_index)?;
                    samples.push(sample);
                }
            }
        }

        let kind = kind.ok_or_else(|| {
            Error::InvalidArguments("missing LUT_1D_SIZE or LUT_3D_SIZE in .cube file".to_string())
        })?;
        let size =
            size.ok_or_else(|| Error::InvalidArguments("missing .cube LUT size".to_string()))?;
        if size < 2 {
            return Err(Error::InvalidArguments(format!(
                ".cube LUT size must be at least 2, got {size}"
            )));
        }

        let expected_samples = match kind {
            CubeKind::OneDimensional => size,
            CubeKind::ThreeDimensional => size * size * size,
        };
        if samples.len() != expected_samples {
            return Err(Error::InvalidArguments(format!(
                ".cube expected {expected_samples} data rows, got {}",
                samples.len()
            )));
        }

        Ok(Self {
            kind,
            size,
            domain_min,
            domain_max,
            samples,
        })
    }

    fn into_gamma_ramp(self) -> Result<GammaRamp> {
        match self.kind {
            CubeKind::OneDimensional => Ok(self.into_1d_gamma_ramp()),
            CubeKind::ThreeDimensional => Ok(self.into_3d_grayscale_gamma_ramp()),
        }
    }

    fn into_1d_gamma_ramp(self) -> GammaRamp {
        let mut values = [[0u16; ENTRIES]; CHANNELS];
        for index in 0..ENTRIES {
            for (channel, values) in values.iter_mut().enumerate() {
                let input = index as f32 / (ENTRIES - 1) as f32;
                let position = self.channel_position(channel, input) * (self.size - 1) as f32;
                let output = interpolate_1d(&self.samples, position, channel);
                values[index] = float_to_u16(output);
            }
        }

        GammaRamp { values }
    }

    fn into_3d_grayscale_gamma_ramp(self) -> GammaRamp {
        let mut values = [[0u16; ENTRIES]; CHANNELS];
        for index in 0..ENTRIES {
            let input = index as f32 / (ENTRIES - 1) as f32;
            let position = [
                self.channel_position(0, input) * (self.size - 1) as f32,
                self.channel_position(1, input) * (self.size - 1) as f32,
                self.channel_position(2, input) * (self.size - 1) as f32,
            ];
            let output = interpolate_3d(&self.samples, self.size, position);
            for channel in 0..CHANNELS {
                values[channel][index] = float_to_u16(output[channel]);
            }
        }

        GammaRamp { values }
    }

    fn channel_position(&self, channel: usize, input: f32) -> f32 {
        let min = self.domain_min[channel];
        let max = self.domain_max[channel];
        if (max - min).abs() <= f32::EPSILON {
            return 0.0;
        }

        ((input - min) / (max - min)).clamp(0.0, 1.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CubeKind {
    OneDimensional,
    ThreeDimensional,
}

fn set_cube_kind(current: &mut Option<CubeKind>, next: CubeKind) -> Result<()> {
    if current.is_some_and(|current| current != next) {
        return Err(Error::InvalidArguments(
            ".cube file cannot contain both LUT_1D_SIZE and LUT_3D_SIZE".to_string(),
        ));
    }

    *current = Some(next);
    Ok(())
}

fn parse_cube_size<'a>(
    mut parts: impl Iterator<Item = &'a str>,
    line_index: usize,
) -> Result<usize> {
    let value = parts.next().ok_or_else(|| {
        Error::InvalidArguments(format!("missing .cube size at line {}", line_index + 1))
    })?;
    value.parse::<usize>().map_err(|_| {
        Error::InvalidArguments(format!(
            "invalid .cube size `{value}` at line {}",
            line_index + 1
        ))
    })
}

fn parse_cube_triplet<'a>(
    mut parts: impl Iterator<Item = &'a str>,
    line_index: usize,
) -> Result<[f32; CHANNELS]> {
    let mut output = [0.0; CHANNELS];
    for value in &mut output {
        let text = parts.next().ok_or_else(|| {
            Error::InvalidArguments(format!("expected 3 values at line {}", line_index + 1))
        })?;
        *value = parse_cube_float(text, line_index)?;
    }

    Ok(output)
}

fn parse_cube_sample(line: &str, line_index: usize) -> Result<[f32; CHANNELS]> {
    let mut parts = line.split_whitespace();
    let sample = parse_cube_triplet(&mut parts, line_index)?;
    if parts.next().is_some() {
        return Err(Error::InvalidArguments(format!(
            "expected 3 values at line {}",
            line_index + 1
        )));
    }

    Ok(sample)
}

fn parse_cube_float(text: &str, line_index: usize) -> Result<f32> {
    text.parse::<f32>().map_err(|_| {
        Error::InvalidArguments(format!(
            "invalid .cube float `{text}` at line {}",
            line_index + 1
        ))
    })
}

fn interpolate_1d(samples: &[[f32; CHANNELS]], position: f32, channel: usize) -> f32 {
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    let amount = position - lower as f32;
    lerp(samples[lower][channel], samples[upper][channel], amount)
}

fn interpolate_3d(
    samples: &[[f32; CHANNELS]],
    size: usize,
    position: [f32; CHANNELS],
) -> [f32; CHANNELS] {
    let lower = position.map(|value| value.floor() as usize);
    let upper = position.map(|value| value.ceil() as usize);
    let amount = [
        position[0] - lower[0] as f32,
        position[1] - lower[1] as f32,
        position[2] - lower[2] as f32,
    ];

    let mut output = [0.0; CHANNELS];
    for red_corner in 0..=1 {
        for green_corner in 0..=1 {
            for blue_corner in 0..=1 {
                let red = if red_corner == 0 { lower[0] } else { upper[0] };
                let green = if green_corner == 0 {
                    lower[1]
                } else {
                    upper[1]
                };
                let blue = if blue_corner == 0 { lower[2] } else { upper[2] };
                let weight = corner_weight(red_corner, amount[0])
                    * corner_weight(green_corner, amount[1])
                    * corner_weight(blue_corner, amount[2]);
                let sample = samples[cube_3d_index(size, red, green, blue)];
                for channel in 0..CHANNELS {
                    output[channel] += sample[channel] * weight;
                }
            }
        }
    }

    output
}

fn cube_3d_index(size: usize, red: usize, green: usize, blue: usize) -> usize {
    red * size * size + green * size + blue
}

fn corner_weight(corner: usize, amount: f32) -> f32 {
    if corner == 0 { 1.0 - amount } else { amount }
}

fn lerp(start: f32, end: f32, amount: f32) -> f32 {
    start + (end - start) * amount
}

fn float_to_u16(value: f32) -> u16 {
    (value.clamp(0.0, 1.0) * u16::MAX as f32).round() as u16
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
