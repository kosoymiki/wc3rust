//! Crate error type.

use std::fmt;

/// Failure of a demux/decode operation.
#[derive(Debug)]
pub enum BinkError {
    /// Wrapper for file-system errors.
    Io(std::io::Error),
    /// The RIFF/AVI structure is not readable.
    Avi(crate::avi::AviError),
    /// The bitstream is structurally invalid (truncated, illegal VLC, ...).
    Corrupt(&'static str),
    /// A codec feature this decoder does not implement.
    Unsupported(&'static str),
    /// The requested frame is outside the stream, or not in the ring buffer.
    NoSuchFrame(usize),
    /// No audio track, or the decoder cannot produce PCM for it.
    NoAudio,
}

impl fmt::Display for BinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::Avi(e) => write!(f, "avi error: {e}"),
            Self::Corrupt(what) => write!(f, "corrupt stream: {what}"),
            Self::Unsupported(what) => write!(f, "unsupported feature: {what}"),
            Self::NoSuchFrame(i) => write!(f, "no such frame: {i}"),
            Self::NoAudio => write!(f, "no audio track"),
        }
    }
}

impl std::error::Error for BinkError {}

impl From<std::io::Error> for BinkError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<crate::avi::AviError> for BinkError {
    fn from(e: crate::avi::AviError) -> Self {
        Self::Avi(e)
    }
}
