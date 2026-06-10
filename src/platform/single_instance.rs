//! Windows single-instance guard.

use anyhow::Result;

#[derive(Debug)]
pub enum SingleInstanceStatus {
    Primary(SingleInstance),
    AlreadyRunning,
}

#[cfg(target_os = "windows")]
#[derive(Debug)]
pub struct SingleInstance {
    handle: windows::Win32::Foundation::HANDLE,
}

#[cfg(target_os = "windows")]
impl SingleInstance {
    pub fn acquire() -> Result<SingleInstanceStatus> {
        use windows::core::w;

        Self::acquire_named(w!("Local\\AikoIME.SingleInstance"))
    }

    fn acquire_named(name: windows::core::PCWSTR) -> Result<SingleInstanceStatus> {
        use windows::Win32::Foundation::{CloseHandle, SetLastError, WIN32_ERROR};
        use windows::Win32::System::Threading::CreateMutexW;

        unsafe {
            SetLastError(WIN32_ERROR(0));
        }
        let handle = unsafe { CreateMutexW(None, true, name)? };
        let already_running = std::io::Error::last_os_error().raw_os_error() == Some(183);

        if already_running {
            unsafe {
                let _ = CloseHandle(handle);
            }
            Ok(SingleInstanceStatus::AlreadyRunning)
        } else {
            Ok(SingleInstanceStatus::Primary(Self { handle }))
        }
    }

    pub fn notify_already_running() {
        use windows::core::w;
        use windows::Win32::UI::WindowsAndMessaging::{
            MessageBoxW, MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND,
        };

        unsafe {
            let _ = MessageBoxW(
                None,
                w!("Aiko IME 已经在运行，请使用系统托盘中的现有实例。"),
                w!("Aiko IME"),
                MB_OK | MB_ICONINFORMATION | MB_SETFOREGROUND,
            );
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for SingleInstance {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::{SingleInstance, SingleInstanceStatus};
    use windows::core::{HSTRING, PCWSTR};

    #[test]
    fn second_named_mutex_is_reported_as_an_existing_instance() {
        let name = HSTRING::from(format!(
            "Local\\AikoIME.SingleInstance.Test.{}",
            std::process::id()
        ));
        let name = PCWSTR(name.as_ptr());

        let first = SingleInstance::acquire_named(name).expect("first mutex should be created");
        assert!(matches!(first, SingleInstanceStatus::Primary(_)));

        let second = SingleInstance::acquire_named(name).expect("second mutex call should succeed");
        assert!(matches!(second, SingleInstanceStatus::AlreadyRunning));
    }
}

#[cfg(not(target_os = "windows"))]
#[derive(Debug)]
pub struct SingleInstance;

#[cfg(not(target_os = "windows"))]
impl SingleInstance {
    pub fn acquire() -> Result<SingleInstanceStatus> {
        Ok(SingleInstanceStatus::Primary(Self))
    }

    pub fn notify_already_running() {}
}
