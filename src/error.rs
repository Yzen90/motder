use std::fmt;
use std::io;

/// Errors reported to the user.
#[derive(Debug)]
pub enum Error {
    /// An I/O failure, with a description of what was being attempted.
    Io { context: String, source: io::Error },
    /// Any other failure, already phrased for the user.
    Msg(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn msg(msg: impl Into<String>) -> Self {
        Error::Msg(msg.into())
    }

    pub fn is_permission_denied(&self) -> bool {
        matches!(self, Error::Io { source, .. } if source.kind() == io::ErrorKind::PermissionDenied)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io { context, source } => write!(f, "{context}: {source}"),
            Error::Msg(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io { source, .. } => Some(source),
            Error::Msg(_) => None,
        }
    }
}

/// Attach a human-readable description to an I/O result.
pub trait Context<T> {
    fn context<F: FnOnce() -> String>(self, f: F) -> Result<T>;
}

impl<T> Context<T> for io::Result<T> {
    fn context<F: FnOnce() -> String>(self, f: F) -> Result<T> {
        self.map_err(|source| Error::Io {
            context: f(),
            source,
        })
    }
}
