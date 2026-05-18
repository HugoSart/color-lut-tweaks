# Color LUT Tweaks

Small Windows CLI for loading raw `.lut` gamma ramps and applying them when HDR or SDR is active. With this application,
you can load separate `.lut` files everytime you switch to HDR or SDR mode.

You can also use this tool to apply EOTF correction for HDR or SDR only.

## Installing

Download the latest [release](https://github.com/HugoSart/color-lut-tweaks/releases) (coming soon), or download the source code of this project and build it using `cargo`:
```shell
cargo build --release
```

## Running

---
## LUT Format

`.lut` files are expected to be raw Windows gamma ramps:

```text
WORD ramp[3][256]
```

That means:

- `1536` bytes total
- 3 channels: red, green, blue
- 256 `u16` entries per channel
- little-endian encoding
