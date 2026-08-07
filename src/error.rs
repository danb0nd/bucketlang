use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Message(String),
    #[error("{kind} at {line}:{col}: {message}")]
    Located {
        kind: &'static str,
        line: usize,
        col: usize,
        message: String,
    },
}

impl Error {
    pub fn msg(m: impl Into<String>) -> Self {
        Error::Message(m.into())
    }

    pub fn at(kind: &'static str, line: usize, col: usize, message: impl Into<String>) -> Self {
        Error::Located {
            kind,
            line,
            col,
            message: message.into(),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
