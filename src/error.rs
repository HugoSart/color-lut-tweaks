use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Io {
        path: Option<std::path::PathBuf>,
        source: std::io::Error,
    },
    ConfigJson {
        path: std::path::PathBuf,
        source: serde_json::Error,
    },
    InvalidLutSize {
        expected: usize,
        actual: usize,
    },
    InvalidArguments(String),
    Platform(String),
}

impl Error {
    pub fn platform(message: impl Into<String>) -> Self {
        Self::Platform(message.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                if let Some(path) = path {
                    write!(f, "failed to read {}: {source}", path.display())
                } else {
                    write!(f, "I/O error: {source}")
                }
            }
            Self::ConfigJson { path, source } => {
                write!(f, "failed to parse config {}: {source}", path.display())
            }
            Self::InvalidLutSize { expected, actual } => {
                write!(
                    f,
                    "expected {expected} bytes for WORD[3][256], got {actual} bytes"
                )
            }
            Self::InvalidArguments(message) => f.write_str(message),
            Self::Platform(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::ConfigJson { source, .. } => Some(source),
            _ => None,
        }
    }
}
