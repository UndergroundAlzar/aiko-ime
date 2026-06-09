//! UI Module
//!
//! Handles system tray and floating button UI.

mod desktop_pet;
mod floating_button;
mod system_tray;

pub use desktop_pet::{DesktopPet, DesktopPetHandle, DesktopPetWindowConfig};
pub use floating_button::{
    ButtonState, FloatingButton, FloatingButtonConfig, FloatingButtonEvent,
    FloatingButtonStateSetter,
};
pub use system_tray::run_app;
