//! Error plumbing shared by the resource-access layer.

use std::fmt;

pub type Result<T> = std::result::Result<T, ReError>;

#[derive(Debug)]
pub enum ReError {
    Io(std::io::Error),
    Mpq(wc3_mpq::MpqError),
    Format(wc3_formats::FormatError),
    Parse(String),
    NotFound(String),
    Unsupported(String),
    Vm(String),
}

impl fmt::Display for ReError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "i/o: {e}"),
            Self::Mpq(e) => write!(f, "mpq: {e}"),
            Self::Format(e) => write!(f, "format: {e}"),
            Self::Parse(e) => write!(f, "parse: {e}"),
            Self::NotFound(p) => write!(f, "not found: {p}"),
            Self::Unsupported(p) => write!(f, "unsupported: {p}"),
            Self::Vm(e) => write!(f, "jass: {e}"),
        }
    }
}

impl std::error::Error for ReError {}

impl From<std::io::Error> for ReError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<wc3_mpq::MpqError> for ReError {
    fn from(value: wc3_mpq::MpqError) -> Self {
        Self::Mpq(value)
    }
}

impl From<wc3_formats::FormatError> for ReError {
    fn from(value: wc3_formats::FormatError) -> Self {
        Self::Format(value)
    }
}
