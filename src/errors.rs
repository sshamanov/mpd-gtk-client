//! Error types — UserFacingError trait, ErrorSinkEvent, ErrorLevel. Thread: any.

pub trait UserFacingError {
    fn user_facing_message(&self) -> String;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorLevel {
    Error,
    Warn,
    Info,
}

#[derive(Debug)]
pub enum ErrorSinkEvent {
    Recoverable(String, ErrorLevel),
    Retryable(String),
    Fatal(String),
}
