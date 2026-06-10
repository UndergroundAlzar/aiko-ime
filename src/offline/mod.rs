//! Offline speech recognition support.
//!
//! Model discovery is always available. The native sherpa-onnx backend is
//! compiled only with the `sherpa-onnx` Cargo feature and loads its DLL at
//! runtime so portable builds can ship the backend beside the application.

mod model;
mod sherpa;

pub use model::{
    ModelDiscovery, ModelFamily, ModelManager, ModelManifest, ResolvedModel, ResolvedModelFiles,
    MODEL_MANIFEST_FILE,
};
pub use sherpa::{SherpaOnnxConfig, SherpaOnnxProvider};
