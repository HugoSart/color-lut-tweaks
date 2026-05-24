use std::path::PathBuf;

use crate::app::{self, AppliedTweak, ColorMode, DeviceSelector, TweakOptions};
use crate::error::{Error, Result};
use crate::logging;
use crate::platform::SystemDisplayPlatform;

pub enum Command {
    LaunchTray,
    Inspect(CliTweakOptions),
    Apply(CliTweakOptions),
    Reset(CliTweakOptions),
    Watch(CliTweakOptions),
    Start(StartOptions),
    Tray(StartOptions),
    TrayWorker(StartOptions),
    ApplyWindowsSettings(StartOptions),
    ApplyAutoColorManagementSettings(StartOptions),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CliTweakOptions {
    pub config: Option<PathBuf>,
    pub tweaks: TweakOptions,
}

impl CliTweakOptions {
    fn resolve(&self) -> Result<TweakOptions> {
        let mut resolved = self.resolve_many()?;
        match resolved.len() {
            1 => Ok(resolved.remove(0)),
            _ => Err(Error::InvalidArguments(
                "this command needs exactly one tweak entry; pass `--lut` directly or use a single-entry config"
                    .to_string(),
            )),
        }
    }

    fn resolve_many(&self) -> Result<Vec<TweakOptions>> {
        let mut resolved = self
            .config
            .as_ref()
            .map(TweakOptions::many_from_config_file)
            .transpose()?
            .unwrap_or_else(|| vec![TweakOptions::default()]);

        for options in &mut resolved {
            options.merge_cli_overrides(&self.tweaks);
        }

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
        Command::LaunchTray => {
            crate::tray::launch(None)?;
            print_info("Started color-lut-tweaks tray");
        }
        Command::Inspect(options) => {
            let tweaks = options.resolve()?;
            let path = require_lut(&tweaks, "inspect")?;
            print!("{}", app::inspect_lut(path)?.format());
        }
        Command::Apply(options) => {
            let tweaks = options.resolve_many()?;
            let report = app::apply_tweak_list(&platform, &tweaks)?;
            if report.is_empty() {
                print_info("No tweaks configured; nothing to apply");
            } else {
                for applied in report.applied {
                    match applied {
                        AppliedTweak::Lut(path) => {
                            print_info(format!("Applied gamma ramp from {path}"))
                        }
                    }
                }
            }
        }
        Command::Reset(options) => {
            let tweaks = options.resolve()?;
            app::reset_gamma(&platform, tweaks.device.as_ref())?;
            if let Some(device) = tweaks.device {
                print_info(format!(
                    "Reset gamma ramp to identity on device {}",
                    device.label()
                ));
            } else {
                print_info("Reset gamma ramp to identity on all devices");
            }
        }
        Command::Watch(options) => {
            let tweaks = options.resolve_many()?;
            match tweaks.as_slice() {
                [tweak] => {
                    if let Some(path) = &tweak.lut {
                        print_info(format!("Watching HDR state for {}", path.display()));
                    } else {
                        print_info("Watching HDR state with no LUT configured");
                    }
                    app::watch_tweaks(&platform, tweak)?;
                }
                _ => {
                    print_info(format!(
                        "Watching HDR state from {} configured tweaks",
                        tweaks.len()
                    ));
                    app::start_tweaks(&platform, &tweaks)?;
                }
            }
        }
        Command::Start(options) => {
            let config = options.config.unwrap_or(app::default_config_path()?);
            let tweaks = TweakOptions::list_from_config_file(&config)?;
            print_info(format!("Starting from {}", config.display()));
            app::start_tweaks(&platform, &tweaks)?;
        }
        Command::Tray(options) => {
            crate::tray::run(options.config)?;
        }
        Command::TrayWorker(options) => {
            crate::tray::run(options.config)?;
        }
        Command::ApplyWindowsSettings(options) => {
            let config = options.config.unwrap_or(app::default_config_path()?);
            match crate::windows_settings::apply_from_config_file(&config)? {
                crate::windows_settings::WindowsSettingsApply::NotConfigured => {
                    print_info("No recommended Windows settings configured")
                }
                crate::windows_settings::WindowsSettingsApply::AlreadyApplied => {
                    print_info("Recommended Windows settings already applied")
                }
                crate::windows_settings::WindowsSettingsApply::Applied => {
                    print_info("Applied recommended Windows settings")
                }
            }
        }
        Command::ApplyAutoColorManagementSettings(options) => {
            let config = options.config.unwrap_or(app::default_config_path()?);
            let tweaks = TweakOptions::list_from_config_file(&config)?;
            match crate::windows_settings::apply_auto_color_management_from_tweaks(&tweaks)? {
                crate::windows_settings::WindowsSettingsApply::NotConfigured => {
                    print_info("No Windows auto color management setting configured")
                }
                crate::windows_settings::WindowsSettingsApply::AlreadyApplied => {
                    print_info("Windows auto color management already applied")
                }
                crate::windows_settings::WindowsSettingsApply::Applied => {
                    print_info("Applied Windows auto color management")
                }
            }
        }
    }

    Ok(())
}

fn print_info(message: impl std::fmt::Display) {
    let message = message.to_string();
    logging::info(&message);
    println!("{message}");
}

pub fn parse_command(args: &[String]) -> Result<Command> {
    match args {
        [] => Ok(Command::LaunchTray),
        [flag] if flag == "-h" || flag == "--help" => {
            print_usage();
            std::process::exit(0);
        }
        [command, rest @ ..] if command == "reset" => Ok(Command::Reset(parse_options(rest)?)),
        [command, rest @ ..] if command == "inspect" => Ok(Command::Inspect(parse_options(rest)?)),
        [command, rest @ ..] if command == "apply" => Ok(Command::Apply(parse_options(rest)?)),
        [command, rest @ ..] if command == "watch" => Ok(Command::Watch(parse_options(rest)?)),
        [command, rest @ ..] if command == "start" => {
            Ok(Command::Start(parse_start_options("start", rest)?))
        }
        [command, rest @ ..] if command == "tray" => {
            Ok(Command::Tray(parse_start_options("tray", rest)?))
        }
        [command, rest @ ..] if command == "tray-worker" => Ok(Command::TrayWorker(
            parse_start_options("tray-worker", rest)?,
        )),
        [command, rest @ ..] if command == "apply-windows-settings" => Ok(
            Command::ApplyWindowsSettings(parse_start_options("apply-windows-settings", rest)?),
        ),
        [command, rest @ ..] if command == "apply-auto-color-management-settings" => {
            Ok(Command::ApplyAutoColorManagementSettings(
                parse_start_options("apply-auto-color-management-settings", rest)?,
            ))
        }
        [first, ..] if first.starts_with('-') => Ok(Command::Watch(parse_options(args)?)),
        _ => Err(Error::InvalidArguments(expected_usage())),
    }
}

pub fn print_usage() {
    eprintln!("Usage:");
    eprintln!("  color-lut-tweaks --config=<configs/Default.config.json>");
    eprintln!(
        "  color-lut-tweaks --mode=<hdr|sdr> --device=<index|name> --lut=<path-to-1536-byte-lut|identity>"
    );
    eprintln!(
        "  color-lut-tweaks --config=<configs/Default.config.json> --mode=<hdr|sdr> --device=<index|name> --lut=<path-to-1536-byte-lut|identity>"
    );
    eprintln!(
        "  color-lut-tweaks inspect [--config <configs/Default.config.json>] [--device <index|name>] --lut <path-to-1536-byte-lut|identity>"
    );
    eprintln!(
        "  color-lut-tweaks apply [--config <configs/Default.config.json>] [--mode <hdr|sdr>] [--device <index|name>] [--lut <path-to-1536-byte-lut|identity>]"
    );
    eprintln!("  color-lut-tweaks reset [--device <index|name>]");
    eprintln!(
        "  color-lut-tweaks watch [--config <configs/Default.config.json>] [--mode <hdr|sdr>] [--device <index|name>] [--lut <path-to-1536-byte-lut|identity>]"
    );
    eprintln!("  color-lut-tweaks start [--config <configs/Default.config.json>]");
    eprintln!("  color-lut-tweaks tray [--config <configs/Default.config.json>]");
    eprintln!("  color-lut-tweaks apply-windows-settings [--config <configs/Default.config.json>]");
    eprintln!("  color-lut-tweaks");
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

fn parse_start_options(command: &str, args: &[String]) -> Result<StartOptions> {
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
                return Err(Error::InvalidArguments(format!(
                    "`{command}` accepts only `--config <configs/Default.config.json>`"
                )));
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

    options.tweaks.device = Some(device.parse::<usize>().map_or_else(
        |_| DeviceSelector::Name(device.to_string()),
        DeviceSelector::Index,
    ));
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
            "`{command}` needs a LUT path; pass `--lut <path-to-1536-byte-lut|identity>`"
        ))
    })
}

fn expected_usage() -> String {
    "expected no args to launch the tray, root options `--config=<path>`, `--mode=<hdr|sdr>`, `--device=<index|name>`, and/or `--lut=<path>`, or `inspect/apply/watch` with the same options, `reset [--device <index|name>]`, `start [--config <path>]`, or `tray [--config <path>]`"
        .to_string()
}
