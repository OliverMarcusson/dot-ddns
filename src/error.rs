use thiserror::Error;

#[derive(Debug, Clone, Copy)]
pub enum ExitCode {
    Ok = 0,
    Changed = 10,
    Config = 20,
    BackendUnavailable = 21,
    ResolutionFailure = 22,
    ApplyFailure = 23,
    Permission = 24,
    StateIo = 25,
}

impl ExitCode {
    pub fn as_i32(self) -> i32 {
        self as i32
    }
}

#[derive(Debug, Error)]
pub enum DotDdnsError {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("backend unavailable: {0}")]
    Backend(String),
    #[error("resolution failure: {0}")]
    Resolution(String),
    #[error("apply failure: {0}")]
    Apply(String),
    #[error("permission error: {0}")]
    Permission(String),
    #[error("state I/O error: {0}")]
    StateIo(String),
}

impl DotDdnsError {
    pub fn exit_code(&self) -> ExitCode {
        match self {
            Self::Config(_) => ExitCode::Config,
            Self::Backend(_) => ExitCode::BackendUnavailable,
            Self::Resolution(_) => ExitCode::ResolutionFailure,
            Self::Apply(_) => ExitCode::ApplyFailure,
            Self::Permission(_) => ExitCode::Permission,
            Self::StateIo(_) => ExitCode::StateIo,
        }
    }
}

pub type Result<T> = std::result::Result<T, DotDdnsError>;
