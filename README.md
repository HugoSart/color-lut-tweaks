# Color LUT Tweaks

Small Windows CLI for loading raw `.lut` gamma ramps and applying them when HDR or SDR is active.

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

## Commands

Inspect a LUT:

```powershell
cargo run -- inspect --device 0 --lut "C:\path\to\file.lut"
```

Apply a LUT immediately:

```powershell
cargo run -- apply --device 0 --lut "C:\path\to\file.lut"
cargo run -- apply --mode hdr --device 0 --lut "C:\path\to\file.lut"
```

Run in watch mode using root-level options:

```powershell
.\target\debug\color-lut-tweaks.exe --config=.\color-lut-tweaks.json
.\target\debug\color-lut-tweaks.exe --mode=hdr --device=0 --lut=.\tests\fixtures\xiaomi-27i-pro-eotf-correction.lut
.\target\debug\color-lut-tweaks.exe --config=.\color-lut-tweaks.json --mode=sdr --device=0 --lut=.\tests\fixtures\xiaomi-27i-pro-eotf-correction.lut
```

Start the full configured runtime:

```powershell
.\target\debug\color-lut-tweaks.exe start
.\target\debug\color-lut-tweaks.exe start --config=.\config.json
```

When `--config` is omitted, `start` looks for `config.json` in the same folder as `color-lut-tweaks.exe`.

Use a config file for defaults with an explicit command:

```powershell
cargo run -- apply --config ".\color-lut-tweaks.json"
```

Example config:

```json
{
  "device": 0,
  "mode": "hdr",
  "lut": "C:\\path\\to\\file.lut"
}
```

`device` is a zero-based active display index. If omitted, apply/reset/watch target all active devices. `mode` is either `hdr` or `sdr`; for `apply`, mode is only checked when specified. For `watch`, omitted mode defaults to `hdr`. Relative paths in the config are resolved relative to the config file. Explicit CLI options override config defaults:

```powershell
cargo run -- apply --config ".\color-lut-tweaks.json" --mode sdr --device 1 --lut "C:\other\file.lut"
```

Run apply without a LUT:

```powershell
cargo run -- apply
```

Reset gamma to an in-code identity ramp:

```powershell
cargo run -- reset
cargo run -- reset --device 0
```

Watch Windows HDR state and apply/restore automatically:

```powershell
cargo run -- watch --config ".\color-lut-tweaks.json"
```

`watch` captures the current gamma ramp on startup, polls HDR state, applies the LUT when the selected mode matches, and restores the captured ramp when the selected mode no longer matches.

You can also pass the LUT directly:

```powershell
cargo run -- watch --mode hdr --device 0 --lut "C:\path\to\file.lut"
cargo run -- watch --mode sdr --device 0 --lut "C:\path\to\file.lut"
```

`start` uses an array of tweak entries. Each entry behaves like a coordinated watch rule. For example, this applies `sdr.lut` while device 0 is in SDR, switches to `hdr.lut` when device 0 enters HDR, and switches back to `sdr.lut` when device 0 leaves HDR:

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

If a `start` entry omits `device`, it applies to all active devices. If it omits `mode`, it defaults to `hdr`, matching `watch`.

## Build

```powershell
cargo build
```

The executable will be created under:

```text
target\debug\color-lut-tweaks.exe
```

Run it directly:

```powershell
.\target\debug\color-lut-tweaks.exe inspect --device 0 --lut "C:\path\to\file.lut"
```

## Tests

This project uses integration and CLI tests only. There are no unit tests inside `src`.

Run all tests:

```powershell
cargo test
```

If Windows or the IDE holds locks in `target`, use a separate target directory:

```powershell
$env:CARGO_INCREMENTAL='0'
$env:CARGO_TARGET_DIR='target-test'
cargo test
```

Test fixtures live in:

```text
tests\fixtures\
```

Current fixtures:

```text
config-invalid.json
config-xiaomi.json
start-config.json
valid-xiaomi-27i-pro.lut
invalid-too-small.lut
```

## Project Layout

```text
src/
  main.rs              # tiny binary entrypoint
  lib.rs               # public library exports
  cli.rs               # CLI parsing and command dispatch
  app.rs               # app-level inspect/apply/watch orchestration
  lut.rs               # LUT parsing and summaries
  error.rs             # shared error type
  platform/
    mod.rs             # platform abstraction
    windows.rs         # Windows HDR/gamma FFI

tests/
  cli.rs               # binary-level CLI tests
  lut_loading.rs       # fixture-based LUT loading tests
  fixtures/            # sample .lut files
```

## Notes

- Applying and watching HDR state are Windows-only.
- Root-level `--config`, `--mode`, `--device`, and `--lut` run the default watch behavior.
- `apply` ignores display mode unless `--mode` is specified.
- `reset` always ignores display mode.
- `watch` defaults to `--mode hdr` when mode is omitted.
- `start` loads an array config and keeps one matching LUT active per device based on current HDR/SDR state.
- When `--device` is omitted, apply/reset/watch target all active devices.
- `reset` uses an identity ramp generated in source code, not a fixture file.
- `inspect` and LUT parsing are platform-neutral.
- `watch` currently polls HDR state every 2 seconds.
- Future tray/background behavior should reuse `src/app.rs` and `src/platform/`.
