//! UI Module
//!
//! Handles system tray and floating button UI.

mod desktop_pet;
mod floating_button;
#[cfg(feature = "cubism-live2d")]
pub mod live2d;
pub mod live2d_controller;
#[cfg(feature = "webview2-renderer")]
pub mod live2d_web;
mod settings;
mod system_tray;

pub use desktop_pet::{
    DesktopPet, DesktopPetEvent, DesktopPetHandle, DesktopPetWindowConfig, PetState,
};
pub use floating_button::{
    ButtonState, FloatingButton, FloatingButtonConfig, FloatingButtonEvent,
    FloatingButtonStateSetter,
};
pub use live2d_controller::{
    HitArea, HitAreaAction, Live2dCommand, Live2dController, Live2dHitAction, Live2dHitArea,
    Live2dMotionPriority, Live2dSkin, Skin,
};
pub use settings::{open_settings, open_settings_window};
pub use system_tray::run_app;
