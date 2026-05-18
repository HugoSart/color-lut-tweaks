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

Run the coordinated foreground watcher:

```powershell
.\target\release\color-lut-tweaks.exe start --config=.\config.json
```

Run as a background tray app:

```powershell
.\target\release\color-lut-tweaks.exe tray --config=.\config.json
```

If `--config` is omitted, `start` and `tray` look for `config.json` in the same folder as the executable. The tray app keeps a Windows notification-area icon alive, runs the same coordinated LUT runtime in the background, and restores the captured gamma ramps when you choose `Quit`.

Tray menu:

- `Reset gamma`: resets all active displays to the identity gamma ramp.
- `Quit`: stops the background runtime and restores the gamma ramps captured on startup.

Example config:

```json
[
  {
    "device": 0,
    "mode": "hdr",
    "lut": "hdr.lut"
  },
  {
    "device": 0,
    "mode": "sdr",
    "lut": "sdr.lut"
  }
]
```

Use the reserved LUT value `identity` when you want a normal tweak entry to apply the generated identity ramp instead of loading a `.lut` file:

```powershell
.\target\release\color-lut-tweaks.exe apply --device 0 --lut identity
```

You can also pass a plain LUT name with no extension or path separators. Plain names resolve to the `luts` folder beside the running executable:

```powershell
.\target\release\color-lut-tweaks.exe apply --device 0 --lut xiaomi-27i-pro-hdr-eotf-correction
```

This resolves to:

```text
target\release\luts\xiaomi-27i-pro-hdr-eotf-correction.lut
```

When using `cargo run`, Cargo runs `target\debug\color-lut-tweaks.exe`, so plain names resolve under:

```text
target\debug\luts\
```

It also works in config files:

```json
[
  {
    "device": 0,
    "mode": "hdr",
    "lut": "xiaomi-27i-pro-hdr-eotf-correction"
  },
  {
    "device": 0,
    "mode": "sdr",
    "lut": "identity"
  }
]
```

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
