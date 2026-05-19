# Color LUT Tweaks

Small Windows CLI for loading raw `.lut` gamma ramps and applying them when HDR or SDR is active. With this application,
you can load separate `.lut` files everytime you switch to HDR or SDR mode.

You can also use this tool to apply EOTF correction for HDR or SDR only.

## Installing

Download the latest [release](https://github.com/HugoSart/color-lut-tweaks/releases) (coming soon), or download the source code of this project and build it using `cargo`:
```shell
cargo build --release
```

This will build the project in `target/release`, where it's ready to be executed. Important files:
- `luts/`: optional pre-built LUTs folder; it's recommended to copy this folder to the same folder as the 
           executable if you decide to move the executable somewhere else.
- `config.json`: edit this file to configure how you want to load the LUTs in your system;
- `color-lut-tweaks.exe`: the main executable;

## Configuration

The configuration file is a JSON array of LUTs to load.

The following example shows a configuration that loads the identity LUT when you are in SDR (i.e. no color correction)
and a custom LUT when you are in HDR:
```json
[
  {
    "device": 0,
    "mode": "sdr",
    "lut": "identity"
  },
  {
    "device": 0,
    "mode": "hdr",
    "lut": "./path-to-my-lut.lut"
  }
]
```

This project also includes a default Xiaomi G Pro 27i HDR EOTF curve correction (because this is what motivated me to 
create this tool). Example:
```json
[
  {
    "device": 0,
    "mode": "sdr",
    "lut": "identity"
  },
  {
    "device": 0,
    "mode": "hdr",
    "lut": "xiaomi-g-pro-27i-hdr-eotf-correction"
  }
]
```

OBS1: If you do not specify file extension or a path like string, the tool will look for the LUTs in the `luts/` folder.
<br>OBS2: Replace `"device": 0` with the device number of your monitor,
<br>OBS3: The `lut` field accepts raw Windows `.lut` files and `.cube` files. `LUT_1D_SIZE` `.cube` files are converted
directly into a Windows gamma ramp. `LUT_3D_SIZE` `.cube` files are approximated by sampling the grayscale axis because
Windows `SetDeviceGammaRamp` cannot apply a true 3D LUT.

## Running
After having your project build and configuration in place, run the executable. It will start running in the background
and will appear in the system tray.

```shell
color-lut-tweaks.exe
```

---
## LUT Format

**LUT** stands for "Look Up Table", and in this context it refers to a set of gamma ramps that can be applied to a color
space.

`.lut` files are expected to be raw Windows gamma ramps:

```text
WORD ramp[3][256]
```

That means:

- `1536` bytes total
- 3 channels: red, green, blue
- 256 `u16` entries per channel
- little-endian encoding
