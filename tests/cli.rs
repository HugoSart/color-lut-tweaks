use std::process::Command;

use color_lut_tweaks::app::{ColorMode, TweakOptions};
use color_lut_tweaks::cli::{self, CliTweakOptions, Command as CliCommand, StartOptions};

#[test]
fn reset_parses_as_command_not_path() {
    let args = vec!["reset".to_string()];

    assert!(matches!(
        cli::parse_command(&args),
        Ok(CliCommand::Reset(_))
    ));
}

#[test]
fn no_args_parse_as_default_tray_launcher() {
    let args = Vec::new();

    assert!(matches!(
        cli::parse_command(&args),
        Ok(CliCommand::LaunchTray)
    ));
}

#[test]
fn lut_path_parses_as_optional_lut_argument() {
    let args = vec![
        "apply".to_string(),
        "--device".to_string(),
        "1".to_string(),
        "--lut".to_string(),
        "tests/fixtures/valid-xiaomi-27i-pro.lut".to_string(),
    ];

    let Ok(CliCommand::Apply(CliTweakOptions {
        config: None,
        tweaks:
            TweakOptions {
                device: Some(1),
                lut: Some(path),
                mode: None,
            },
    })) = cli::parse_command(&args)
    else {
        panic!("expected apply command with optional LUT path");
    };

    assert_eq!(
        path,
        std::path::PathBuf::from("tests/fixtures/valid-xiaomi-27i-pro.lut")
    );
}

#[test]
fn root_lut_equals_argument_parses_as_watch_defaults() {
    let args = vec!["--lut=tests/fixtures/valid-xiaomi-27i-pro.lut".to_string()];

    let Ok(CliCommand::Watch(CliTweakOptions {
        config: None,
        tweaks:
            TweakOptions {
                device: None,
                lut: Some(path),
                mode: None,
            },
    })) = cli::parse_command(&args)
    else {
        panic!("expected root lut option to run watch mode");
    };

    assert_eq!(
        path,
        std::path::PathBuf::from("tests/fixtures/valid-xiaomi-27i-pro.lut")
    );
}

#[test]
fn root_config_equals_argument_parses_as_watch_defaults() {
    let args = vec!["--config=tests/fixtures/config-xiaomi.json".to_string()];

    let Ok(CliCommand::Watch(CliTweakOptions {
        config: Some(config),
        tweaks:
            TweakOptions {
                device: None,
                lut: None,
                mode: None,
            },
    })) = cli::parse_command(&args)
    else {
        panic!("expected root config option to run watch mode");
    };

    assert_eq!(
        config,
        std::path::PathBuf::from("tests/fixtures/config-xiaomi.json")
    );
}

#[test]
fn root_config_and_lut_equals_arguments_parse_as_watch_with_override() {
    let args = vec![
        "--config=tests/fixtures/config-invalid.json".to_string(),
        "--device=1".to_string(),
        "--lut=tests/fixtures/valid-xiaomi-27i-pro.lut".to_string(),
    ];

    let Ok(CliCommand::Watch(CliTweakOptions {
        config: Some(config),
        tweaks:
            TweakOptions {
                device: Some(1),
                lut: Some(path),
                mode: None,
            },
    })) = cli::parse_command(&args)
    else {
        panic!("expected root config and lut options to run watch mode");
    };

    assert_eq!(
        config,
        std::path::PathBuf::from("tests/fixtures/config-invalid.json")
    );
    assert_eq!(
        path,
        std::path::PathBuf::from("tests/fixtures/valid-xiaomi-27i-pro.lut")
    );
}

#[test]
fn start_parses_without_config() {
    let args = vec!["start".to_string()];

    let Ok(CliCommand::Start(StartOptions { config: None })) = cli::parse_command(&args) else {
        panic!("expected start command without explicit config");
    };
}

#[test]
fn start_parses_config_argument() {
    let args = vec![
        "start".to_string(),
        "--config".to_string(),
        "tests/fixtures/start-config.json".to_string(),
    ];

    let Ok(CliCommand::Start(StartOptions {
        config: Some(config),
    })) = cli::parse_command(&args)
    else {
        panic!("expected start command with config path");
    };

    assert_eq!(
        config,
        std::path::PathBuf::from("tests/fixtures/start-config.json")
    );
}

#[test]
fn start_parses_config_equals_argument() {
    let args = vec![
        "start".to_string(),
        "--config=tests/fixtures/start-config.json".to_string(),
    ];

    let Ok(CliCommand::Start(StartOptions {
        config: Some(config),
    })) = cli::parse_command(&args)
    else {
        panic!("expected start command with config path");
    };

    assert_eq!(
        config,
        std::path::PathBuf::from("tests/fixtures/start-config.json")
    );
}

#[test]
fn tray_parses_config_argument() {
    let args = vec![
        "tray".to_string(),
        "--config=tests/fixtures/start-config.json".to_string(),
    ];

    let Ok(CliCommand::Tray(StartOptions {
        config: Some(config),
    })) = cli::parse_command(&args)
    else {
        panic!("expected tray command with config path");
    };

    assert_eq!(
        config,
        std::path::PathBuf::from("tests/fixtures/start-config.json")
    );
}

#[test]
fn tray_worker_parses_config_argument() {
    let args = vec![
        "tray-worker".to_string(),
        "--config=tests/fixtures/start-config.json".to_string(),
    ];

    let Ok(CliCommand::TrayWorker(StartOptions {
        config: Some(config),
    })) = cli::parse_command(&args)
    else {
        panic!("expected tray worker command with config path");
    };

    assert_eq!(
        config,
        std::path::PathBuf::from("tests/fixtures/start-config.json")
    );
}

#[test]
fn identity_lut_parses_as_reserved_lut_value() {
    let args = vec![
        "apply".to_string(),
        "--device".to_string(),
        "1".to_string(),
        "--lut=identity".to_string(),
    ];

    let Ok(CliCommand::Apply(CliTweakOptions {
        config: None,
        tweaks:
            TweakOptions {
                device: Some(1),
                lut: Some(path),
                mode: None,
            },
    })) = cli::parse_command(&args)
    else {
        panic!("expected apply command with identity LUT value");
    };

    assert_eq!(path, std::path::PathBuf::from("identity"));
}

#[test]
fn start_only_accepts_config_argument() {
    let output = Command::new(env!("CARGO_BIN_EXE_color-lut-tweaks"))
        .arg("start")
        .arg("--lut")
        .arg("tests/fixtures/valid-xiaomi-27i-pro.lut")
        .output()
        .unwrap();

    assert!(!output.status.success());

    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("unknown option `--lut`"));
}

#[test]
fn config_path_parses_as_optional_argument() {
    let args = vec![
        "apply".to_string(),
        "--config".to_string(),
        "tests/fixtures/config-xiaomi.json".to_string(),
    ];

    let Ok(CliCommand::Apply(CliTweakOptions {
        config: Some(config),
        tweaks:
            TweakOptions {
                device: None,
                lut: None,
                mode: None,
            },
    })) = cli::parse_command(&args)
    else {
        panic!("expected apply command with config path");
    };

    assert_eq!(
        config,
        std::path::PathBuf::from("tests/fixtures/config-xiaomi.json")
    );
}

#[test]
fn reset_parses_device_option() {
    let args = vec!["reset".to_string(), "--device".to_string(), "1".to_string()];

    let Ok(CliCommand::Reset(CliTweakOptions {
        config: None,
        tweaks:
            TweakOptions {
                device: Some(1),
                lut: None,
                mode: None,
            },
    })) = cli::parse_command(&args)
    else {
        panic!("expected reset command with device option");
    };
}

#[test]
fn mode_parses_as_shared_option() {
    let args = vec![
        "watch".to_string(),
        "--mode=sdr".to_string(),
        "--lut".to_string(),
        "tests/fixtures/valid-xiaomi-27i-pro.lut".to_string(),
    ];

    let Ok(CliCommand::Watch(CliTweakOptions {
        config: None,
        tweaks:
            TweakOptions {
                device: None,
                lut: Some(path),
                mode: Some(ColorMode::Sdr),
            },
    })) = cli::parse_command(&args)
    else {
        panic!("expected watch command with SDR mode");
    };

    assert_eq!(
        path,
        std::path::PathBuf::from("tests/fixtures/valid-xiaomi-27i-pro.lut")
    );
}

#[test]
fn mode_must_be_hdr_or_sdr() {
    let output = Command::new(env!("CARGO_BIN_EXE_color-lut-tweaks"))
        .arg("apply")
        .arg("--mode")
        .arg("auto")
        .output()
        .unwrap();

    assert!(!output.status.success());

    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("`--mode` must be `hdr` or `sdr`"));
}

#[test]
fn device_must_be_zero_based_integer() {
    let output = Command::new(env!("CARGO_BIN_EXE_color-lut-tweaks"))
        .arg("apply")
        .arg("--device")
        .arg("left")
        .output()
        .unwrap();

    assert!(!output.status.success());

    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("`--device` must be a zero-based integer"));
}

#[test]
fn apply_can_run_without_lut_argument() {
    let output = Command::new(env!("CARGO_BIN_EXE_color-lut-tweaks"))
        .arg("apply")
        .output()
        .unwrap();

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("No tweaks configured; nothing to apply"));
}

#[test]
fn inspect_without_lut_argument_fails() {
    let output = Command::new(env!("CARGO_BIN_EXE_color-lut-tweaks"))
        .arg("inspect")
        .output()
        .unwrap();

    assert!(!output.status.success());

    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("`inspect` needs a LUT path"));
}

#[test]
fn inspect_prints_lut_summary() {
    let output = Command::new(env!("CARGO_BIN_EXE_color-lut-tweaks"))
        .arg("inspect")
        .arg("--lut")
        .arg("tests/fixtures/valid-xiaomi-27i-pro.lut")
        .output()
        .unwrap();

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Format: WORD[3][256], little-endian, 1536 bytes"));
    assert!(stdout.contains("  red: first     0, last 49816"));
    assert!(stdout.contains("green: first     0, last 50320"));
    assert!(stdout.contains(" blue: first     0, last 50787"));
}

#[test]
fn inspect_uses_lut_from_config_file() {
    let output = Command::new(env!("CARGO_BIN_EXE_color-lut-tweaks"))
        .arg("inspect")
        .arg("--config")
        .arg("tests/fixtures/config-xiaomi.json")
        .output()
        .unwrap();

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("  red: first     0, last 49816"));
}

#[test]
fn explicit_lut_overrides_config_file_default() {
    let output = Command::new(env!("CARGO_BIN_EXE_color-lut-tweaks"))
        .arg("inspect")
        .arg("--config")
        .arg("tests/fixtures/config-invalid.json")
        .arg("--lut")
        .arg("tests/fixtures/valid-xiaomi-27i-pro.lut")
        .output()
        .unwrap();

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("  red: first     0, last 49816"));
}

#[test]
fn inspect_bad_path_fails() {
    let output = Command::new(env!("CARGO_BIN_EXE_color-lut-tweaks"))
        .arg("inspect")
        .arg("--lut")
        .arg("tests/fixtures/missing.lut")
        .output()
        .unwrap();

    assert!(!output.status.success());

    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("failed to read"));
}

#[test]
fn inspect_malformed_lut_fails() {
    let output = Command::new(env!("CARGO_BIN_EXE_color-lut-tweaks"))
        .arg("inspect")
        .arg("--lut")
        .arg("tests/fixtures/invalid-too-small.lut")
        .output()
        .unwrap();

    assert!(!output.status.success());

    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("expected 1536 bytes"));
}

#[test]
fn bare_lut_path_is_not_accepted() {
    let output = Command::new(env!("CARGO_BIN_EXE_color-lut-tweaks"))
        .arg("tests/fixtures/valid-xiaomi-27i-pro.lut")
        .output()
        .unwrap();

    assert!(!output.status.success());

    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("expected no args to launch the tray"));
}

#[test]
fn old_hdr_lut_flag_is_not_accepted() {
    let output = Command::new(env!("CARGO_BIN_EXE_color-lut-tweaks"))
        .arg("inspect")
        .arg("--hdr-lut")
        .arg("tests/fixtures/valid-xiaomi-27i-pro.lut")
        .output()
        .unwrap();

    assert!(!output.status.success());

    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("unknown option `--hdr-lut`"));
}
