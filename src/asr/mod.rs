//! ASR (Automatic Speech Recognition) module
//!
//! Provider-neutral ASR interfaces plus the Doubao online implementation.

mod client;
mod constants;
mod device;
mod doubao_provider;
mod protocol;
mod provider;
mod selection;

pub use client::AsrClient;
pub use constants::*;
pub use device::{get_asr_token, register_device, DeviceCredentials};
pub use doubao_provider::DoubaoProvider;
pub use protocol::{AsrResponse, ResponseType};
#[cfg(all(windows, feature = "sherpa-onnx"))]
pub(crate) use provider::SessionCommand;
pub use provider::{
    AsrEvent, AsrProvider, AudioFrame, EndpointOptions, ProviderCapabilities, ProviderError,
    ProviderKind, ProviderSession, SessionEndReason, SessionOptions,
};
pub use selection::{
    build_provider_from_config, load_offline_model, provider_capabilities, select_provider_kind,
};

// Include the generated protobuf code
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/asr.rs"));
}
