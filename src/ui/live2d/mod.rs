//! Feature-gated Live2D Cubism Native runtime foundation.
//!
//! This module loads the official `Live2DCubismCore.dll` C ABI and real
//! `.model3.json`/`.moc3` assets. Rendering and motion evaluation remain
//! Native Framework responsibilities; resolved commands are exposed for that
//! adapter instead of substituting sprite or PNG animation.

mod command;
mod core;
mod discovery;
mod error;
mod manifest;
mod runtime;

#[cfg(test)]
mod test_support;

pub use command::{resolve_command, Live2dCommand, MotionPriority, ResolvedCommand};
pub use core::{CoreInfo, CoreModelInfo, RuntimeOptions};
pub use discovery::{discover_models, DiscoveryFailure, DiscoveryReport};
pub use error::Live2dError;
pub use manifest::{ExpressionAsset, ModelManifest, MotionAsset};
pub use runtime::CubismRuntime;
