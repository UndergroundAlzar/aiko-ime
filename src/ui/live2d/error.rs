use std::io;
use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Live2dError {
    #[error("failed to read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("invalid Cubism JSON at {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("invalid Cubism manifest at {path}: {message}")]
    InvalidManifest { path: PathBuf, message: String },

    #[error("Cubism asset referenced by {manifest} is missing: {asset}")]
    MissingAsset { manifest: PathBuf, asset: PathBuf },

    #[error("Cubism asset path escapes the model directory in {manifest}: {asset}")]
    AssetOutsideModel { manifest: PathBuf, asset: PathBuf },

    #[error("{message}")]
    CoreUnavailable { message: String },

    #[error("failed to load Cubism Core symbol {symbol} from {library}: {message}")]
    CoreSymbol {
        library: PathBuf,
        symbol: &'static str,
        message: String,
    },

    #[error("Cubism Core rejected {moc}: {message}")]
    MocRejected { moc: PathBuf, message: String },

    #[error("Cubism runtime is only available on Windows")]
    UnsupportedPlatform,

    #[error("no Cubism model is loaded")]
    NoModelLoaded,

    #[error("unknown Cubism motion group '{group}'")]
    UnknownMotionGroup { group: String },

    #[error("Cubism motion index {index} is out of range for group '{group}'")]
    MotionIndexOutOfRange { group: String, index: usize },

    #[error("unknown Cubism expression '{id}'")]
    UnknownExpression { id: String },
}

pub(crate) fn io_error(path: impl Into<PathBuf>, source: io::Error) -> Live2dError {
    Live2dError::Io {
        path: path.into(),
        source,
    }
}
