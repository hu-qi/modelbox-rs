use std::fmt;

#[derive(Debug)]
pub enum FlowError {
    InvalidGraph(String),
    UnknownUnit(String),
    Execution(String),
    Io(std::io::Error),
    Json(serde_json::Error),
}

impl fmt::Display for FlowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidGraph(message) => write!(f, "invalid graph: {message}"),
            Self::UnknownUnit(message) => write!(f, "unknown flow unit: {message}"),
            Self::Execution(message) => write!(f, "execution error: {message}"),
            Self::Io(error) => write!(f, "I/O error: {error}"),
            Self::Json(error) => write!(f, "JSON error: {error}"),
        }
    }
}

impl std::error::Error for FlowError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for FlowError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for FlowError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}
