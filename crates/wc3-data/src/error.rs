//! Error types for the typed WC3 data layer.

use std::fmt;
use std::path::PathBuf;

use wc3_formats::gamedata::GamedataError;

/// Anything that can go wrong while loading or querying game data.
#[derive(Debug)]
pub enum DataError {
    /// A resource was not available from the source.
    Missing {
        path: String,
        root: PathBuf,
    },
    /// Resource bytes could not be read.
    Io {
        path: String,
        source: std::io::Error,
    },
    /// Resource bytes are not a valid WC3 data file.
    Format {
        path: String,
        source: GamedataError,
    },
    /// A table does not have the column a schema needs.
    MissingColumn {
        table: String,
        column: String,
    },
    /// A cell could not be converted.
    BadCell {
        table: String,
        row: String,
        column: String,
        value: String,
    },
    /// Object data referenced a file the caller did not supply.
    MissingObjectData {
        kind: wc3_formats::gamedata::ObjectKind,
    },
    /// A path does not name a WC3 object-data file (`war3map.w3u`, …).
    UnknownObjectKind {
        path: String,
    },
}

impl fmt::Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing { path, root } => {
                write!(f, "resource `{path}` not found under {}", root.display())
            }
            Self::Io { path, source } => write!(f, "read `{path}`: {source}"),
            Self::Format { path, source } => write!(f, "parse `{path}`: {source}"),
            Self::MissingColumn { table, column } => {
                write!(f, "table `{table}` has no column `{column}`")
            }
            Self::BadCell {
                table,
                row,
                column,
                value,
            } => write!(f, "table `{table}` cell [{row}].{column} = {value:?} is not numeric"),
            Self::MissingObjectData { kind } => {
                write!(f, "no object data supplied for {}", kind.extension())
            }
            Self::UnknownObjectKind { path } => {
                write!(f, "`{path}` is not a WC3 object-data file (.w3u/.w3a/.w3t/.w3q/.w3b/.w3d/.w3h)")
            }
        }
    }
}

impl std::error::Error for DataError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Format { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl DataError {
    pub(crate) fn format(path: impl Into<String>, source: GamedataError) -> Self {
        Self::Format {
            path: path.into(),
            source,
        }
    }
}

/// Convenience alias.
pub type Result<T> = std::result::Result<T, DataError>;
