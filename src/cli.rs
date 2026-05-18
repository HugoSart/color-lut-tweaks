use std::path::PathBuf;

use crate::app;
use crate::error::{Error, Result};
use crate::platform::SystemDisplayPlatform;

pub enum Command {
    Inspect(PathBuf),
    Apply(PathBuf),
    Reset,
    Watch(PathBuf),
}

pub fn run(args: impl IntoIterator<Item = String>) -> Result<()> {
    let command = parse_command(&args.into_iter().collect::<Vec<_>>())?;
    let platform = SystemDisplayPlatform::new();

    match command {
        Command::Inspect(path) => {
            print!("{}", app::inspect_lut(path)?.format());
        }
        Command::Apply(path) => {
            app::apply_lut(&platform, &path)?;
            println!("Applied gamma ramp from {}", path.display());
        }
        Command::Reset => {
            app::reset_gamma(&platform)?;
            println!("Reset gamma ramp to identity");
        }
        Command::Watch(path) => {
            println!("Watching HDR state for {}", path.display());
            app::watch_hdr(&platform, path)?;
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
        [command] if command == "reset" => Ok(Command::Reset),
        [path] => Ok(Command::Inspect(PathBuf::from(path))),
        [command, path] if command == "inspect" => Ok(Command::Inspect(PathBuf::from(path))),
        [command, path] if command == "apply" => Ok(Command::Apply(PathBuf::from(path))),
        [command, path] if command == "watch" => Ok(Command::Watch(PathBuf::from(path))),
        _ => Err(Error::InvalidArguments(
            "expected `inspect <path>`, `apply <path>`, `reset`, or `watch <path>`".to_string(),
        )),
    }
}

pub fn print_usage() {
    eprintln!("Usage:");
    eprintln!("  hdr-tweaks <path-to-1536-byte-lut>");
    eprintln!("  hdr-tweaks inspect <path-to-1536-byte-lut>");
    eprintln!("  hdr-tweaks apply <path-to-1536-byte-lut>");
    eprintln!("  hdr-tweaks reset");
    eprintln!("  hdr-tweaks watch <path-to-1536-byte-lut>");
}
