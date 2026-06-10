//! Foreground-window capture and validation for a voice-input session.

use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowIdentity {
    pub handle: isize,
    pub process_id: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InputTarget {
    identity: WindowIdentity,
}

impl InputTarget {
    pub fn identity(self) -> WindowIdentity {
        self.identity
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TargetObservation {
    pub foreground: Option<WindowIdentity>,
    pub target_window_alive: bool,
    pub target_process_alive: bool,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum InputTargetError {
    #[error("没有可用的前台窗口，无法开始语音输入")]
    NoForegroundWindow,
    #[error("录音目标进程已退出 (PID {process_id})")]
    TargetProcessExited { process_id: u32 },
    #[error("录音目标窗口已关闭")]
    TargetWindowClosed,
    #[error("输入焦点已离开录音开始时的目标窗口")]
    FocusLost,
}

pub fn evaluate_target(
    target: WindowIdentity,
    observation: TargetObservation,
) -> Result<(), InputTargetError> {
    if !observation.target_process_alive {
        return Err(InputTargetError::TargetProcessExited {
            process_id: target.process_id,
        });
    }
    if !observation.target_window_alive {
        return Err(InputTargetError::TargetWindowClosed);
    }

    match observation.foreground {
        None => Err(InputTargetError::FocusLost),
        Some(current) if current == target => Ok(()),
        Some(_) => Err(InputTargetError::FocusLost),
    }
}

#[cfg(target_os = "windows")]
pub fn capture_foreground_target() -> Result<InputTarget, InputTargetError> {
    foreground_identity()
        .map(|identity| InputTarget { identity })
        .ok_or(InputTargetError::NoForegroundWindow)
}

#[cfg(not(target_os = "windows"))]
pub fn capture_foreground_target() -> Result<InputTarget, InputTargetError> {
    Err(InputTargetError::NoForegroundWindow)
}

#[cfg(target_os = "windows")]
pub fn validate_input_target(target: InputTarget) -> Result<(), InputTargetError> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::IsWindow;

    let identity = target.identity();
    let hwnd = HWND(identity.handle);
    let observation = TargetObservation {
        foreground: foreground_identity(),
        target_window_alive: unsafe { IsWindow(hwnd).as_bool() },
        target_process_alive: process_is_alive(identity.process_id),
    };

    evaluate_target(identity, observation)
}

#[cfg(not(target_os = "windows"))]
pub fn validate_input_target(_target: InputTarget) -> Result<(), InputTargetError> {
    Err(InputTargetError::NoForegroundWindow)
}

#[cfg(target_os = "windows")]
fn foreground_identity() -> Option<WindowIdentity> {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0 == 0 {
            return None;
        }

        let mut process_id = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));
        if process_id == 0 {
            return None;
        }

        Some(WindowIdentity {
            handle: hwnd.0,
            process_id,
        })
    }
}

#[cfg(target_os = "windows")]
fn process_is_alive(process_id: u32) -> bool {
    use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
    };

    let handle = match unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, process_id) } {
        Ok(handle) => handle,
        Err(_) => return true,
    };
    let wait_result = unsafe { WaitForSingleObject(handle, 0) };
    unsafe {
        let _ = CloseHandle(handle);
    }
    wait_result != WAIT_OBJECT_0
}

#[cfg(test)]
mod tests {
    use super::*;

    const TARGET: WindowIdentity = WindowIdentity {
        handle: 100,
        process_id: 42,
    };

    fn observation(foreground: Option<WindowIdentity>) -> TargetObservation {
        TargetObservation {
            foreground,
            target_window_alive: true,
            target_process_alive: true,
        }
    }

    #[test]
    fn accepts_the_original_foreground_window() {
        assert_eq!(evaluate_target(TARGET, observation(Some(TARGET))), Ok(()));
    }

    #[test]
    fn rejects_focus_moving_to_another_window() {
        let other = WindowIdentity {
            handle: 200,
            process_id: 77,
        };
        assert_eq!(
            evaluate_target(TARGET, observation(Some(other))),
            Err(InputTargetError::FocusLost)
        );
    }

    #[test]
    fn rejects_another_window_from_the_same_process() {
        let sibling = WindowIdentity {
            handle: 201,
            process_id: TARGET.process_id,
        };
        assert_eq!(
            evaluate_target(TARGET, observation(Some(sibling))),
            Err(InputTargetError::FocusLost)
        );
    }

    #[test]
    fn reports_process_exit_before_window_loss() {
        let observation = TargetObservation {
            foreground: None,
            target_window_alive: false,
            target_process_alive: false,
        };
        assert_eq!(
            evaluate_target(TARGET, observation),
            Err(InputTargetError::TargetProcessExited {
                process_id: TARGET.process_id
            })
        );
    }

    #[test]
    fn reports_a_closed_window_when_process_is_still_alive() {
        let observation = TargetObservation {
            foreground: None,
            target_window_alive: false,
            target_process_alive: true,
        };
        assert_eq!(
            evaluate_target(TARGET, observation),
            Err(InputTargetError::TargetWindowClosed)
        );
    }
}
