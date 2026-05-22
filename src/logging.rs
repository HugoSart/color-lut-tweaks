use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

const MAX_LOG_SIZE: u64 = 1024 * 1024;
const MAX_LOG_FILES: usize = 5;
const LOG_FILE_NAME: &str = "color-lut-tweaks.log";

static LOGGER: OnceLock<Mutex<Logger>> = OnceLock::new();

pub fn init() {
    LOGGER.get_or_init(|| Mutex::new(Logger::new(default_log_path())));
}

pub fn info(message: impl fmt::Display) {
    write(Level::Info, format_args!("{message}"));
}

pub fn warn(message: impl fmt::Display) {
    write(Level::Warn, format_args!("{message}"));
}

pub fn error(message: impl fmt::Display) {
    write(Level::Error, format_args!("{message}"));
}

fn write(level: Level, args: fmt::Arguments<'_>) {
    let logger = LOGGER.get_or_init(|| Mutex::new(Logger::new(default_log_path())));
    match logger.lock() {
        Ok(mut logger) => logger.write(level, args),
        Err(_) => eprintln!("ERROR logging mutex is poisoned; lost log entry: {args}"),
    }
}

fn default_log_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join("logs")))
        .unwrap_or_else(|| PathBuf::from("logs"))
        .join(LOG_FILE_NAME)
}

#[derive(Clone, Copy)]
enum Level {
    Info,
    Warn,
    Error,
}

impl Level {
    fn label(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }
}

struct Logger {
    path: PathBuf,
}

impl Logger {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn write(&mut self, level: Level, args: fmt::Arguments<'_>) {
        if let Err(err) = self.try_write(level, args) {
            eprintln!("ERROR failed to write log entry: {err}");
        }
    }

    fn try_write(&mut self, level: Level, args: fmt::Arguments<'_>) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        self.rotate_if_needed()?;

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;

        writeln!(file, "{} | {:<5} | {}", timestamp(), level.label(), args)
    }

    fn rotate_if_needed(&self) -> std::io::Result<()> {
        let Ok(metadata) = fs::metadata(&self.path) else {
            return Ok(());
        };
        if metadata.len() < MAX_LOG_SIZE {
            return Ok(());
        }

        let oldest = rotated_path(&self.path, MAX_LOG_FILES - 1);
        if oldest.exists() {
            fs::remove_file(oldest)?;
        }

        for index in (1..MAX_LOG_FILES - 1).rev() {
            let source = rotated_path(&self.path, index);
            if source.exists() {
                fs::rename(source, rotated_path(&self.path, index + 1))?;
            }
        }

        fs::rename(&self.path, rotated_path(&self.path, 1))?;
        Ok(())
    }
}

fn rotated_path(path: &std::path::Path, index: usize) -> PathBuf {
    let file_name = format!("color-lut-tweaks.{index}.log");
    path.with_file_name(file_name)
}

#[cfg(windows)]
fn timestamp() -> String {
    let mut time = SystemTimeFields::default();
    unsafe {
        GetLocalTime(&mut time);
    }

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        time.year, time.month, time.day, time.hour, time.minute, time.second, time.milliseconds
    )
}

#[cfg(not(windows))]
fn timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("unix-ms:{}", duration.as_millis())
}

#[cfg(windows)]
#[repr(C)]
#[derive(Default)]
struct SystemTimeFields {
    year: u16,
    month: u16,
    day_of_week: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
    milliseconds: u16,
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetLocalTime(system_time: *mut SystemTimeFields);
}
