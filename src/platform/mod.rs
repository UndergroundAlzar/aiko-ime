//! Platform-specific reliability helpers.

mod single_instance;
mod window_target;

pub fn notify_input_protection(message: &str) {
    #[cfg(target_os = "windows")]
    {
        use windows::core::PCWSTR;
        use windows::Win32::UI::WindowsAndMessaging::{
            MessageBoxW, MB_ICONWARNING, MB_OK, MB_SETFOREGROUND,
        };

        let message = wide(message);
        let caption = wide("Aiko IME 输入保护");
        unsafe {
            let _ = MessageBoxW(
                None,
                PCWSTR(message.as_ptr()),
                PCWSTR(caption.as_ptr()),
                MB_OK | MB_ICONWARNING | MB_SETFOREGROUND,
            );
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        tracing::error!("Aiko IME input protection: {}", message);
    }
}

#[cfg(target_os = "windows")]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

pub use single_instance::{SingleInstance, SingleInstanceStatus};
pub use window_target::{
    capture_foreground_target, validate_input_target, InputTarget, InputTargetError,
    TargetObservation, WindowIdentity,
};
