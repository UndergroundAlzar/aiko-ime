//! Desktop pet window for Aiko.
//!
//! The pet is intentionally small and controlled from the tray menu. It uses a
//! layered Win32 window so the PNG alpha channel stays crisp on the desktop.

use anyhow::{anyhow, Result};
use std::sync::mpsc::{self, Receiver, Sender};

/// Runtime configuration for the desktop pet window.
#[derive(Debug, Clone)]
pub struct DesktopPetWindowConfig {
    pub visible: bool,
    pub initial_x: i32,
    pub initial_y: i32,
    pub size: i32,
}

/// Commands sent from the tray/menu thread to the pet window thread.
#[derive(Debug, Clone, Copy)]
pub enum DesktopPetCommand {
    Show,
    Hide,
    Exit,
}

/// Thread-safe handle for controlling the desktop pet.
#[derive(Clone)]
pub struct DesktopPetHandle {
    tx: Sender<DesktopPetCommand>,
}

impl DesktopPetHandle {
    pub fn show(&self) {
        let _ = self.tx.send(DesktopPetCommand::Show);
    }

    pub fn hide(&self) {
        let _ = self.tx.send(DesktopPetCommand::Hide);
    }

    pub fn exit(&self) {
        let _ = self.tx.send(DesktopPetCommand::Exit);
    }
}

/// Owns the receiving side of the desktop pet command channel.
pub struct DesktopPet {
    tx: Sender<DesktopPetCommand>,
    rx: Option<Receiver<DesktopPetCommand>>,
}

impl DesktopPet {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self { tx, rx: Some(rx) }
    }

    pub fn handle(&self) -> DesktopPetHandle {
        DesktopPetHandle {
            tx: self.tx.clone(),
        }
    }

    #[cfg(target_os = "windows")]
    pub fn run(mut self, config: DesktopPetWindowConfig) {
        if let Some(rx) = self.rx.take() {
            if let Err(e) = run_windows_pet(rx, config) {
                tracing::error!("Desktop pet failed: {}", e);
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    pub fn run(mut self, _config: DesktopPetWindowConfig) {
        if let Some(rx) = self.rx.take() {
            while let Ok(command) = rx.recv() {
                if matches!(command, DesktopPetCommand::Exit) {
                    break;
                }
            }
        }
    }
}

impl Default for DesktopPet {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "windows")]
struct PetWindowState {
    rx: Receiver<DesktopPetCommand>,
    rgba: Vec<u8>,
    width: i32,
    height: i32,
    visible: bool,
}

#[cfg(target_os = "windows")]
fn run_windows_pet(rx: Receiver<DesktopPetCommand>, config: DesktopPetWindowConfig) -> Result<()> {
    use image::imageops::FilterType;
    use windows::core::w;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::*;

    let size = config.size.clamp(96, 320);
    let image = image::load_from_memory(include_bytes!("../../assets/aiko_desktop_pet.png"))?
        .resize(size as u32, size as u32, FilterType::Lanczos3)
        .into_rgba8();

    let mut state = Box::new(PetWindowState {
        rx,
        rgba: image.into_raw(),
        width: size,
        height: size,
        visible: config.visible,
    });

    unsafe {
        let h_instance = GetModuleHandleW(None).unwrap_or_default();
        let class_name = w!("AikoDesktopPetWindow");

        let wnd_class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(pet_wnd_proc),
            hInstance: h_instance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: class_name,
            ..Default::default()
        };
        let _ = RegisterClassW(&wnd_class);

        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        let x = if config.initial_x < 0 {
            screen_w.saturating_sub(size + 36)
        } else {
            config.initial_x
        };
        let y = if config.initial_y < 0 {
            screen_h.saturating_sub(size + 72)
        } else {
            config.initial_y
        };

        let state_ptr = state.as_mut() as *mut PetWindowState;
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class_name,
            w!("Aiko Desktop Pet"),
            WS_POPUP,
            x,
            y,
            size,
            size,
            None,
            None,
            h_instance,
            Some(state_ptr as *const std::ffi::c_void),
        );

        if hwnd.0 == 0 {
            return Err(anyhow!("failed to create desktop pet window"));
        }

        std::mem::forget(state);

        render_pet(hwnd);
        SetTimer(hwnd, 1, 50, None);
        let _ = ShowWindow(
            hwnd,
            if config.visible {
                SW_SHOWNOACTIVATE
            } else {
                SW_HIDE
            },
        );

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    Ok(())
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn pet_wnd_proc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::Foundation::*;
    use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
    use windows::Win32::UI::WindowsAndMessaging::*;

    match msg {
        WM_CREATE => {
            let create_struct = lparam.0 as *const CREATESTRUCTW;
            if !create_struct.is_null() {
                let state_ptr = (*create_struct).lpCreateParams as *mut PetWindowState;
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
            }
            LRESULT(0)
        }
        WM_TIMER => {
            let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut PetWindowState;
            if !state_ptr.is_null() {
                let state = &mut *state_ptr;
                while let Ok(command) = state.rx.try_recv() {
                    match command {
                        DesktopPetCommand::Show => {
                            state.visible = true;
                            render_pet(hwnd);
                            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                        }
                        DesktopPetCommand::Hide => {
                            state.visible = false;
                            let _ = ShowWindow(hwnd, SW_HIDE);
                        }
                        DesktopPetCommand::Exit => {
                            let _ = DestroyWindow(hwnd);
                            break;
                        }
                    }
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            // Let users drag Aiko around without adding extra UI chrome.
            let _ = ReleaseCapture();
            let _ = SendMessageW(
                hwnd,
                WM_NCLBUTTONDOWN,
                WPARAM(HTCAPTION as usize),
                LPARAM(0),
            );
            LRESULT(0)
        }
        WM_DESTROY => {
            let _ = KillTimer(hwnd, 1);
            let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut PetWindowState;
            if !state_ptr.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                drop(Box::from_raw(state_ptr));
            }
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

#[cfg(target_os = "windows")]
unsafe fn render_pet(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::Foundation::*;
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::UI::WindowsAndMessaging::*;

    let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut PetWindowState;
    if state_ptr.is_null() {
        return;
    }
    let state = &*state_ptr;

    let hdc_screen = GetDC(HWND::default());
    let hdc_mem = CreateCompatibleDC(hdc_screen);
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: state.width,
            biHeight: -state.height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: 0,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        bmiColors: [RGBQUAD::default()],
    };

    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    if let Ok(hbmp) = CreateDIBSection(hdc_mem, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
        if !bits.is_null() {
            let old_bmp = SelectObject(hdc_mem, hbmp);
            let pixel_data = bits as *mut u8;

            for pixel in 0..(state.width * state.height) as usize {
                let i = pixel * 4;
                let r = state.rgba[i] as u32;
                let g = state.rgba[i + 1] as u32;
                let b = state.rgba[i + 2] as u32;
                let a = state.rgba[i + 3] as u32;
                *pixel_data.add(i) = ((b * a) / 255) as u8;
                *pixel_data.add(i + 1) = ((g * a) / 255) as u8;
                *pixel_data.add(i + 2) = ((r * a) / 255) as u8;
                *pixel_data.add(i + 3) = a as u8;
            }

            let blend = BLENDFUNCTION {
                BlendOp: 0,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: 1,
            };
            let size = SIZE {
                cx: state.width,
                cy: state.height,
            };
            let pt_src = POINT { x: 0, y: 0 };
            let _ = UpdateLayeredWindow(
                hwnd,
                hdc_screen,
                None,
                Some(&size),
                hdc_mem,
                Some(&pt_src),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );

            SelectObject(hdc_mem, old_bmp);
            let _ = DeleteObject(hbmp);
        }
    }

    let _ = DeleteDC(hdc_mem);
    let _ = ReleaseDC(HWND::default(), hdc_screen);
}
