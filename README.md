# Color LUT Tweaks

Small Windows CLI for loading raw `.lut` gamma ramps and applying them when HDR or SDR is active. With this application,
you can load separate `.lut` files everytime you switch to HDR or SDR mode.

You can also use this tool to apply EOTF correction for HDR or SDR only.

## Roadmap

### MVP
- [x] ~~Loading LUT files in SDR and HDR.~~
- [x] ~~Support for configuration presets.~~
- [x] ~~Preset for Xiaomi G Pro 27i.~~
- [ ] Automatic Windows configuration of ICC profiles and recommended settings.
- [ ] Automatic NVIDIA configuration of system color settings.
- [ ] Automatic monitor configuration using DDC/CI.
- [ ] Improve initial loading performance for faster auto start.

### Future
- [ ] Automatic AMD configuration of system color settings.
- [ ] Automatic Intel Graphics configuration of system color settings.
- [ ] Create graphical user interface.
- [ ] Improve command line interface.
- [ ] MacOS support.
- [ ] Linux support.

## Installing

Download the latest [release](https://github.com/HugoSart/color-lut-tweaks/releases), or download the source code of this project and build it using `cargo`:
```shell
cargo build --release
```

This will build the project in `target/release`, where it's ready to be executed. Important files:
- `luts/`: optional pre-built LUTs folder; it's recommended to copy this folder to the same folder as the 
           executable if you decide to move the executable somewhere else.
- `configs/`: pre-built configurations files (shown in "Presets" menu);
- `profiles/`: bundled ICC/ICM color profiles;
- `color-lut-tweaks.exe`: the main executable;


## Running
After having your project build and configuration in place, run the executable. It will start running in the background
and will appear in the system tray.

```shell
color-lut-tweaks.exe
```

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
    "lut": "./path-to-my-lut.lut",
    "adjust": {
      "contrast": 1.00,
      "brightness": 0.0,
      "gamma": 1.0,
      "gain": [1.0, 1.0, 1.0],
      "offset": [0.0, 0.0, 0.0]
    }
  }
]
```

### Xiaomi G Pro 27i Users
This project also includes a default Xiaomi G Pro 27i HDR EOTF curve correction (because this is what motivated me to 
create this tool) and Native to sRGB lut. You can use it by simply starting the application and selecting the desired 
preset. If the monitor device id is not 0, click on the "Edit" button and manually edit the device number.

Check the [Xiaomi G Pro 27i Guide](./docs/guide-xiaomi-g-pro-27i.md) docs for more details.

---
# Contributing

## Requirements

Required tools for development:
- [cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html): Required to build and run the project in
  development mode.
- [prek](https://github.com/j178/prek): Used for development quality checks.

## Development Commands

Useful commands for development:
- `cargo build`: Build the project.
- `cargo run`: Run the project in system tray mode.
- `cargo run -- <args>`: Run the project in CLI mode.
- `prek run --all-files`: Run code quality checks.

---
## Resources
### LUT Format

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

3D LUT files `.cube` are also supported, but they are converted to `.lut` files on the fly. So, if you have a 3D LUT
file that is complex, the result might be a bit different from the original.