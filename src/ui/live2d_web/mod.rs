//! Feature-gated WebView2/Cubism Web SDK host and protocol adapter.
//!
//! The native host creates a transparent child WebView2 controller but does not
//! bundle Live2D Cubism Core, the Cubism Framework, or a substitute model.
//! [`RuntimeLayout::validate`] reports every missing private runtime component
//! before rendering is attempted.

mod adapter;
mod bridge;
mod protocol;
mod runtime;
mod window;

pub use adapter::{AdapterError, ScriptExecutor, WebView2HostAdapter};
pub use bridge::{BridgeError, CommandReceipt, HostBridge};
pub use protocol::{
    BuiltinModel, HostCommand, HostCommandEnvelope, MotionPriority, RuntimeEvent,
    RuntimeEventEnvelope, PROTOCOL_VERSION,
};
pub use runtime::{RuntimeError, RuntimeLayout};
pub use window::{
    HostBounds, HostError, TransparentWebView2Host, VirtualHostAccess, WebMessageHandler,
    WebView2HostOptions,
};
