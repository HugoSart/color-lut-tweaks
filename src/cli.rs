use std::path::PathBuf;

use crate::app::{self, AppliedTweak, ColorMode, TweakOptions};
use crate::error::{Error, Result};
use crate::platform::SystemDisplayPlatform;

pub enum Command {
    Inspect(CliTweakOptions),
    Apply(CliTweakOptions),
    Reset(CliTweakOptions),
    Watch(CliTweakOptions),
    Start(StartOptions),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CliTweakOptions {
    pub config: Option<PathBuf>,
    pub tweaks: TweakOptions,
}

impl CliTweakOptions {
    fn resolve(&self) -> Result<TweakOptions> {
        let mut resolved = if let Some(path) = &self.config {
            TweakOptions::from_config_file(path)?
        } else {
            TweakOptions::default()
        };

        resolved.merge_cli_overrides(&self.tweaks);

        Ok(resolved)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StartOptions {
    pub config: Option<PathBuf>,
}

pub fn run(args: impl IntoIterator<Item = String>) -> Result<()> {
    let command = parse_command(&args.into_iter().collect::<Vec<_>>())?;
    let platform = SystemDisplayPlatform::new();

    match command {
        Command::Inspect(options) => {
            let tweaks = options.resolve()?;
            let path = require_lut(&tweaks, "inspect")?;
            print!("{}", app::inspect_lut(path)?.format());
        }
        Command::Apply(options) => {
            let tweaks = options.resolve()?;
            let report = app::apply_tweaks(&platform, &tweaks)?;
            if report.is_empty() {
                println!("No tweaks configured; nothing to apply");
            } else {
                for applied in report.applied {
                    match applied {
                        AppliedTweak::Lut(path) => match tweaks.mode {
                            Some(mode) => println!(
                                "Applied gamma ramp from {path} where mode is {}",
                                mode.name()
                            ),
                            None => println!("Applied gamma ramp from {path}"),
                        },
                    }
                }
            }
        }
        Command::Reset(options) => {
            let tweaks = options.resolve()?;
            app::reset_gamma(&platform, tweaks.device)?;
            if let Some(device) = tweaks.device {
                println!("Reset gamma ramp to identity on device {device}");
            } else {
                println!("Reset gamma ramp to identity on all devices");
            }
        }
        Command::Watch(options) => {
            let tweaks = options.resolve()?;
            if let Some(path) = &tweaks.lut {
                println!("Watching HDR state for {}", path.display());
            } else {
                println!("Watching HDR state with no LUT configured");
            }
            app::watch_tweaks(&platform, &tweaks)?;
        }
        Command::Start(options) => {
            let config = options.config.unwrap_or(default_start_config_path()?);
            let tweaks = TweakOptions::list_from_config_file(&config)?;
            println!("Starting from {}", config.display());
            app::start_tweaks(&platform, &tweaks)?;
        }
    }

    Ok(())
}

pub fn parse_command(args: &[String]) -> Result<Command> {
    match args {
        [] => Err(Error::InvalidArguments(
            "missing command or LUT path".to_string(),
        )),
        [flag] if flag == "-h" || flag == "--help" => {
            print_usage();
            std::process::exit(0);
        }
        [command, rest @ ..] if command == "reset" => Ok(Command::Reset(parse_options(rest)?)),
        [command, rest @ ..] if command == "inspect" => Ok(Command::Inspect(parse_options(rest)?)),
        [command, rest @ ..] if command == "apply" => Ok(Command::Apply(parse_options(rest)?)),
        [command, rest @ ..] if command == "watch" => Ok(Command::Watch(parse_options(rest)?)),
        [command, rest @ ..] if command == "start" => {
            Ok(Command::Start(parse_start_options(rest)?))
        }
        [first, ..] if first.starts_with('-') => Ok(Command::Watch(parse_options(args)?)),
        _ => Err(Error::InvalidArguments(expected_usage())),
    }
}

pub fn print_usage() {
    eprintln!("Usage:");
    eprintln!("  hdr-tweaks --config=<config.json>");
    eprintln!("  hdr-tweaks --mode=<hdr|sdr> --device=<index> --lut=<path-to-1536-byte-lut>");
    eprintln!(
        "  hdr-tweaks --config=<config.json> --mode=<hdr|sdr> --device=<index> --lut=<path-to-1536-byte-lut>"
    );
    eprintln!(
        "  hdr-tweaks inspect [--config <config.json>] [--device <index>] --lut <path-to-1536-byte-lut>"
    );
    eprintln!(
        "  hdr-tweaks apply [--config <config.json>] [--mode <hdr|sdr>] [--device <index>] [--lut <path-to-1536-byte-lut>]"
    );
    eprintln!("  hdr-tweaks reset [--device <index>]");
    eprintln!(
        "  hdr-tweaks watch [--config <config.json>] [--mode <hdr|sdr>] [--device <index>] [--lut <path-to-1536-byte-lut>]"
    );
    eprintln!("  hdr-tweaks start [--config <config.json>]");
}

fn parse_options(args: &[String]) -> Result<CliTweakOptions> {
    let mut options = CliTweakOptions::default();
    let mut index = 0;

    while index < args.len() {
        let value = args[index].as_str();
        if let Some(path) = value.strip_prefix("--config=") {
            set_config(&mut options, path)?;
            index += 1;
            continue;
        }
        if let Some(path) = value.strip_prefix("--lut=") {
            set_lut(&mut options, path, "--lut")?;
            index += 1;
            continue;
        }
        if let Some(device) = value.strip_prefix("--device=") {
            set_device(&mut options, device)?;
            index += 1;
            continue;
        }
        if let Some(mode) = value.strip_prefix("--mode=") {
            set_mode(&mut options, mode)?;
            index += 1;
            continue;
        }
        match value {
            "--config" => {
                let path = args
                    .get(index + 1)
                    .ok_or_else(|| expected_value("--config"))?;
                set_config(&mut options, path)?;
                index += 2;
            }
            "--lut" => {
                let path = args.get(index + 1).ok_or_else(|| expected_value("--lut"))?;
                set_lut(&mut options, path, "--lut")?;
                index += 2;
            }
            "--device" => {
                let device = args
                    .get(index + 1)
                    .ok_or_else(|| expected_value("--device"))?;
                set_device(&mut options, device)?;
                index += 2;
            }
            "--mode" => {
                let mode = args
                    .get(index + 1)
                    .ok_or_else(|| expected_value("--mode"))?;
                set_mode(&mut options, mode)?;
                index += 2;
            }
            value if value.starts_with('-') => {
                return Err(Error::InvalidArguments(format!("unknown option `{value}`")));
            }
            _ => {
                return Err(Error::InvalidArguments(expected_usage()));
            }
        }
    }

    Ok(options)
}

fn parse_start_options(args: &[String]) -> Result<StartOptions> {
    let mut options = StartOptions::default();
    let mut index = 0;

    while index < args.len() {
        let value = args[index].as_str();
        if let Some(path) = value.strip_prefix("--config=") {
            set_start_config(&mut options, path)?;
            index += 1;
            continue;
        }

        match value {
            "--config" => {
                let path = args
                    .get(index + 1)
                    .ok_or_else(|| expected_value("--config"))?;
                set_start_config(&mut options, path)?;
                index += 2;
            }
            value if value.starts_with('-') => {
                return Err(Error::InvalidArguments(format!("unknown option `{value}`")));
            }
            _ => {
                return Err(Error::InvalidArguments(
                    "`start` accepts only `--config <config.json>`".to_string(),
                ));
            }
        }
    }

    Ok(options)
}

fn set_config(options: &mut CliTweakOptions, path: impl AsRef<str>) -> Result<()> {
    let path = path.as_ref();
    if path.is_empty() {
        return Err(expected_value("--config"));
    }
    if options.config.is_some() {
        return Err(Error::InvalidArguments(
            "`--config` can only be provided once".to_string(),
        ));
    }
    options.config = Some(PathBuf::from(path));
    Ok(())
}

fn set_start_config(options: &mut StartOptions, path: impl AsRef<str>) -> Result<()> {
    let path = path.as_ref();
    if path.is_empty() {
        return Err(expected_value("--config"));
    }
    if options.config.is_some() {
        return Err(Error::InvalidArguments(
            "`--config` can only be provided once".to_string(),
        ));
    }
    options.config = Some(PathBuf::from(path));
    Ok(())
}

fn set_lut(options: &mut CliTweakOptions, path: impl AsRef<str>, flag: &str) -> Result<()> {
    let path = path.as_ref();
    if path.is_empty() {
        return Err(expected_value(flag));
    }
    if options.tweaks.lut.is_some() {
        return Err(Error::InvalidArguments(
            "`--lut` can only be provided once".to_string(),
        ));
    }
    options.tweaks.lut = Some(PathBuf::from(path));
    Ok(())
}

fn set_device(options: &mut CliTweakOptions, device: impl AsRef<str>) -> Result<()> {
    let device = device.as_ref();
    if device.is_empty() {
        return Err(expected_value("--device"));
    }
    if options.tweaks.device.is_some() {
        return Err(Error::InvalidArguments(
            "`--device` can only be provided once".to_string(),
        ));
    }

    let index = device.parse::<usize>().map_err(|_| {
        Error::InvalidArguments(format!(
            "`--device` must be a zero-based integer, got `{device}`"
        ))
    })?;

    options.tweaks.device = Some(index);
    Ok(())
}

fn set_mode(options: &mut CliTweakOptions, mode: impl AsRef<str>) -> Result<()> {
    let mode = mode.as_ref();
    if mode.is_empty() {
        return Err(expected_value("--mode"));
    }
    if options.tweaks.mode.is_some() {
        return Err(Error::InvalidArguments(
            "`--mode` can only be provided once".to_string(),
        ));
    }

    options.tweaks.mode = Some(match mode {
        "hdr" => ColorMode::Hdr,
        "sdr" => ColorMode::Sdr,
        _ => {
            return Err(Error::InvalidArguments(format!(
                "`--mode` must be `hdr` or `sdr`, got `{mode}`"
            )));
        }
    });
    Ok(())
}

fn expected_value(flag: &str) -> Error {
    Error::InvalidArguments(format!("expected a path after `{flag}`"))
}

fn require_lut<'a>(tweaks: &'a TweakOptions, command: &str) -> Result<&'a PathBuf> {
    tweaks.lut.as_ref().ok_or_else(|| {
        Error::InvalidArguments(format!(
            "`{command}` needs a LUT path; pass `--lut <path-to-1536-byte-lut>`"
        ))
    })
}

fn default_start_config_path() -> Result<PathBuf> {
    let exe_path = std::env::current_exe().map_err(|source| Error::Io { path: None, source })?;
    Ok(exe_path.parent().map_or_else(
        || PathBuf::from("config.json"),
        |path| path.join("config.json"),
    ))
}

fn expected_usage() -> String {
    "expected root options `--config=<path>`, `--mode=<hdr|sdr>`, `--device=<index>`, and/or `--lut=<path>`, or `inspect/apply/watch` with the same options, `reset [--device <index>]`, or `start [--config <path>]`"
        .to_string()
}
