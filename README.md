# hdr-tweaks

Small Windows CLI for loading raw `.lut` gamma ramps and applying them when HDR is active.

The project currently starts as a CLI, but the code is organized library-first so the same core can later be reused by a background/system-tray app.

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
cargo run -- inspect "C:\path\to\file.lut"
```

Apply a LUT immediately:

```powershell
cargo run -- apply "C:\path\to\file.lut"
```

Watch Windows HDR state and apply/restore automatically:

```powershell
cargo run -- watch "C:\path\to\file.lut"
```

`watch` captures the current gamma ramp on startup, polls HDR state, applies the LUT when HDR is enabled, and restores the captured ramp when HDR is disabled.

## Build

```powershell
cargo build
```

The executable will be created under:

```text
target\debug\hdr-tweaks.exe
```

Run it directly:

```powershell
.\target\debug\hdr-tweaks.exe inspect "C:\path\to\file.lut"
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
valid-linear.lut
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
- `inspect` and LUT parsing are platform-neutral.
- `watch` currently polls HDR state every 2 seconds.
- Future tray/background behavior should reuse `src/app.rs` and `src/platform/`.
