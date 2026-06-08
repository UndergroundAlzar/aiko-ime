//! System Tray
//!
//! Implements the system tray icon and menu with proper Windows message loop.

use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    TrayIconBuilder,
};

use crate::business::{HotkeyManager, VoiceController};
use crate::data::AppConfig;
use crate::ui::{
    ButtonState, FloatingButton, FloatingButtonConfig, FloatingButtonEvent,
    FloatingButtonStateSetter,
};

fn spawn_recording_state_monitor(
    voice_controller: Arc<Mutex<VoiceController>>,
    setter: FloatingButtonStateSetter,
) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let recording = {
                let controller = voice_controller.lock().await;
                controller.is_recording()
            };

            if !recording {
                setter.set_state(ButtonState::Idle);
                break;
            }
        }
    });
}

/// Run the application with system tray and floating button
pub async fn run_app(
    config: AppConfig,
    voice_controller: Arc<Mutex<VoiceController>>,
    hotkey_manager: HotkeyManager,
) -> Result<()> {
    // Create floating button
    let mut floating_button = FloatingButton::new();
    let button_state_setter = floating_button.state_setter();
    let floating_rx = floating_button.take_event_receiver();

    // Configure floating button position from config
    let fb_config = FloatingButtonConfig {
        initial_x: config.floating_button.position_x,
        initial_y: config.floating_button.position_y,
        size: 56,
        stiffness: config.floating_button.stiffness,
        damping: config.floating_button.damping,
    };

    // Spawn floating button thread if enabled
    if config.floating_button.enabled {
        std::thread::spawn(move || {
            floating_button.run(fb_config);
        });
    }

    // Create tray icon on main thread
    let icon = load_icon()?;
    let menu = Menu::new();

    let start_item = MenuItem::new("开始语音输入", true, None);
    let stop_item = MenuItem::new("停止语音输入", true, None);
    let separator1 = PredefinedMenuItem::separator();
    let settings_item = MenuItem::new("设置...", true, None);
    let separator2 = PredefinedMenuItem::separator();
    let quit_item = MenuItem::new("退出", true, None);

    let start_id = start_item.id().clone();
    let stop_id = stop_item.id().clone();
    let settings_id = settings_item.id().clone();
    let quit_id = quit_item.id().clone();

    menu.append(&start_item)?;
    menu.append(&stop_item)?;
    menu.append(&separator1)?;
    menu.append(&settings_item)?;
    menu.append(&separator2)?;
    menu.append(&quit_item)?;

    let _tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("AikoIME - 双击Ctrl开始/停止")
        .with_icon(icon)
        .build()?;

    tracing::info!("System tray initialized");

    // Running flag
    let running = Arc::new(AtomicBool::new(true));

    // Get menu and floating button receivers
    let menu_rx = MenuEvent::receiver();

    // Get tokio runtime handle for async operations
    let runtime_handle = tokio::runtime::Handle::current();

    // Set up hotkey callback with state sync
    let vc_for_hotkey = voice_controller.clone();
    let state_for_hotkey = button_state_setter.clone();
    let handle_for_hotkey = runtime_handle.clone();
    hotkey_manager.on_trigger(move || {
        let vc = vc_for_hotkey.clone();
        let setter = state_for_hotkey.clone();
        let handle = handle_for_hotkey.clone();
        handle.spawn(async move {
            let mut controller = vc.lock().await;
            if controller.is_recording() {
                tracing::info!("Hotkey: stopping voice input");
                setter.set_state(ButtonState::Processing);
                if let Err(e) = controller.stop().await {
                    tracing::error!("Failed to stop voice input: {}", e);
                }
                setter.set_state(ButtonState::Idle);
            } else {
                tracing::info!("Hotkey: starting voice input");
                setter.set_state(ButtonState::Recording);
                let monitor_vc = vc.clone();
                let monitor_setter = setter.clone();
                if let Err(e) = controller.start().await {
                    tracing::error!("Failed to start voice input: {}", e);
                    setter.set_state(ButtonState::Idle);
                } else {
                    drop(controller);
                    spawn_recording_state_monitor(monitor_vc, monitor_setter);
                }
            }
        });
    });

    // Spawn event handler thread for menu and floating button events
    let running_clone = running.clone();
    let vc_clone = voice_controller.clone();
    let state_setter_clone = button_state_setter.clone();
    let _keep_hotkey_manager = hotkey_manager;
    let mut config = config;

    std::thread::spawn(move || {
        let _keep = _keep_hotkey_manager;
        while running_clone.load(Ordering::SeqCst) {
            // Check menu events
            if let Ok(event) = menu_rx.recv_timeout(std::time::Duration::from_millis(50)) {
                if event.id == start_id {
                    let vc = vc_clone.clone();
                    let setter = state_setter_clone.clone();
                    runtime_handle.spawn(async move {
                        let mut controller = vc.lock().await;
                        if !controller.is_recording() {
                            tracing::info!("Starting from menu");
                            setter.set_state(ButtonState::Recording);
                            let monitor_vc = vc.clone();
                            let monitor_setter = setter.clone();
                            if let Err(e) = controller.start().await {
                                tracing::error!("Failed to start: {}", e);
                                setter.set_state(ButtonState::Idle);
                            } else {
                                drop(controller);
                                spawn_recording_state_monitor(monitor_vc, monitor_setter);
                            }
                        }
                    });
                } else if event.id == stop_id {
                    let vc = vc_clone.clone();
                    let setter = state_setter_clone.clone();
                    runtime_handle.spawn(async move {
                        let mut controller = vc.lock().await;
                        if controller.is_recording() {
                            tracing::info!("Stopping from menu");
                            setter.set_state(ButtonState::Processing);
                            if let Err(e) = controller.stop().await {
                                tracing::error!("Failed to stop: {}", e);
                            }
                            setter.set_state(ButtonState::Idle);
                        }
                    });
                } else if event.id == settings_id {
                    tracing::info!("Settings from menu");
                    #[cfg(target_os = "windows")]
                    {
                        run_hotkey_recorder();
                    }
                } else if event.id == quit_id {
                    tracing::info!("Quit from menu");
                    running_clone.store(false, Ordering::SeqCst);
                    #[cfg(target_os = "windows")]
                    unsafe {
                        windows::Win32::UI::WindowsAndMessaging::PostQuitMessage(0);
                    }
                }
            }

            // Check floating button events
            if let Some(ref rx) = floating_rx {
                if let Ok(event) = rx.try_recv() {
                    match event {
                        FloatingButtonEvent::ConfirmRecording => {
                            let vc = vc_clone.clone();
                            let setter = state_setter_clone.clone();
                            runtime_handle.spawn(async move {
                                let mut controller = vc.lock().await;
                                if controller.is_recording() {
                                    tracing::info!("Floating: confirm (stop, keep text)");
                                    setter.set_state(ButtonState::Processing);
                                    if let Err(e) = controller.stop().await {
                                        tracing::error!("Failed to stop: {}", e);
                                    }
                                    setter.set_state(ButtonState::Idle);
                                }
                            });
                        }
                        FloatingButtonEvent::CancelRecording => {
                            let vc = vc_clone.clone();
                            let setter = state_setter_clone.clone();
                            runtime_handle.spawn(async move {
                                let mut controller = vc.lock().await;
                                if controller.is_recording() {
                                    tracing::info!("Floating: cancel (stop, discard text)");
                                    setter.set_state(ButtonState::Processing);
                                    if let Err(e) = controller.cancel().await {
                                        tracing::error!("Failed to cancel: {}", e);
                                    }
                                    setter.set_state(ButtonState::Idle);
                                }
                            });
                        }
                        FloatingButtonEvent::Exit => {
                            tracing::info!("Exit from floating button");
                            running_clone.store(false, Ordering::SeqCst);
                            #[cfg(target_os = "windows")]
                            unsafe {
                                windows::Win32::UI::WindowsAndMessaging::PostQuitMessage(0);
                            }
                        }
                        FloatingButtonEvent::UpdatePosition { x, y } => {
                            config.floating_button.position_x = x;
                            config.floating_button.position_y = y;
                            if let Err(e) = config.save() {
                                tracing::error!("Failed to save config: {}", e);
                            } else {
                                tracing::debug!("Saved floating button position: ({}, {})", x, y);
                            }
                        }
                    }
                }
            }
        }
    });

    // Run Win32 message loop on main thread (REQUIRED for tray icon to work)
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, GetMessageW, TranslateMessage, MSG,
        };

        tracing::info!("Running Win32 message loop on main thread");
        let mut msg = MSG::default();
        unsafe {
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);

                if !running.load(Ordering::SeqCst) {
                    break;
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        while running.load(Ordering::SeqCst) {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    tracing::info!("Application exiting");
    Ok(())
}

/// Load the tray icon with modern appearance
fn load_icon() -> Result<tray_icon::Icon> {
    let width = 32u32;
    let height = 32u32;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);

    let center_x = width as f32 / 2.0;
    let center_y = height as f32 / 2.0;
    let radius = (width.min(height) as f32 / 2.0) - 1.0;

    // Modern gradient colors (purple to blue)
    let color_start = (139u8, 92u8, 246u8); // Purple
    let color_end = (59u8, 130u8, 246u8); // Blue

    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 - center_x;
            let dy = y as f32 - center_y;
            let dist = (dx * dx + dy * dy).sqrt();

            if dist <= radius {
                // Gradient based on position (top-left to bottom-right)
                let gradient_t = ((x as f32 / width as f32) + (y as f32 / height as f32)) / 2.0;
                let r = (color_start.0 as f32 * (1.0 - gradient_t)
                    + color_end.0 as f32 * gradient_t) as u8;
                let g = (color_start.1 as f32 * (1.0 - gradient_t)
                    + color_end.1 as f32 * gradient_t) as u8;
                let b = (color_start.2 as f32 * (1.0 - gradient_t)
                    + color_end.2 as f32 * gradient_t) as u8;

                // Soft edge anti-aliasing
                let alpha = if dist > radius - 1.5 {
                    ((radius - dist + 1.5) / 1.5 * 255.0) as u8
                } else {
                    255
                };

                rgba.push(r);
                rgba.push(g);
                rgba.push(b);
                rgba.push(alpha);
            } else {
                rgba.push(0);
                rgba.push(0);
                rgba.push(0);
                rgba.push(0);
            }
        }
    }

    // Draw modern microphone icon (white, clean design)
    let mic_color = (255u8, 255u8, 255u8, 255u8);
    let cx = center_x as i32;
    let cy = center_y as i32;

    // Mic head (rounded rectangle)
    for dy in -5..=3 {
        for dx in -3..=3 {
            let in_corner = (dy == -5 || dy == 3) && (dx == -3 || dx == 3);
            if !in_corner {
                let idx = ((cy + dy) as u32 * width + (cx + dx) as u32) as usize * 4;
                if idx + 3 < rgba.len() {
                    rgba[idx] = mic_color.0;
                    rgba[idx + 1] = mic_color.1;
                    rgba[idx + 2] = mic_color.2;
                    rgba[idx + 3] = mic_color.3;
                }
            }
        }
    }

    // Mic holder arc (U shape)
    for dx in -5..=5 {
        let idx = ((cy + 6) as u32 * width + (cx + dx) as u32) as usize * 4;
        if idx + 3 < rgba.len() {
            rgba[idx] = mic_color.0;
            rgba[idx + 1] = mic_color.1;
            rgba[idx + 2] = mic_color.2;
            rgba[idx + 3] = mic_color.3;
        }
    }
    for dy in 3..=6 {
        for dx in [-5, 5] {
            let idx = ((cy + dy) as u32 * width + (cx + dx) as u32) as usize * 4;
            if idx + 3 < rgba.len() {
                rgba[idx] = mic_color.0;
                rgba[idx + 1] = mic_color.1;
                rgba[idx + 2] = mic_color.2;
                rgba[idx + 3] = mic_color.3;
            }
        }
    }

    // Mic stand
    for dy in 7..=10 {
        let idx = ((cy + dy) as u32 * width + cx as u32) as usize * 4;
        if idx + 3 < rgba.len() {
            rgba[idx] = mic_color.0;
            rgba[idx + 1] = mic_color.1;
            rgba[idx + 2] = mic_color.2;
            rgba[idx + 3] = mic_color.3;
        }
    }

    // Mic base
    for dx in -3..=3 {
        let idx = ((cy + 10) as u32 * width + (cx + dx) as u32) as usize * 4;
        if idx + 3 < rgba.len() {
            rgba[idx] = mic_color.0;
            rgba[idx + 1] = mic_color.1;
            rgba[idx + 2] = mic_color.2;
            rgba[idx + 3] = mic_color.3;
        }
    }

    let icon = tray_icon::Icon::from_rgba(rgba, width, height)?;
    Ok(icon)
}

#[cfg(target_os = "windows")]
struct RecorderState {
    current_keys: String,
    saved: bool,
}

#[cfg(target_os = "windows")]
fn vk_to_string(vk: u32) -> Option<&'static str> {
    match vk {
        0x41..=0x5A => {
            // A..Z
            match vk {
                0x41 => Some("A"),
                0x42 => Some("B"),
                0x43 => Some("C"),
                0x44 => Some("D"),
                0x45 => Some("E"),
                0x46 => Some("F"),
                0x47 => Some("G"),
                0x48 => Some("H"),
                0x49 => Some("I"),
                0x4A => Some("J"),
                0x4B => Some("K"),
                0x4C => Some("L"),
                0x4D => Some("M"),
                0x4E => Some("N"),
                0x4F => Some("O"),
                0x50 => Some("P"),
                0x51 => Some("Q"),
                0x52 => Some("R"),
                0x53 => Some("S"),
                0x54 => Some("T"),
                0x55 => Some("U"),
                0x56 => Some("V"),
                0x57 => Some("W"),
                0x58 => Some("X"),
                0x59 => Some("Y"),
                0x5A => Some("Z"),
                _ => None,
            }
        }
        0x30..=0x39 => {
            // 0..9
            match vk {
                0x30 => Some("0"),
                0x31 => Some("1"),
                0x32 => Some("2"),
                0x33 => Some("3"),
                0x34 => Some("4"),
                0x35 => Some("5"),
                0x36 => Some("6"),
                0x37 => Some("7"),
                0x38 => Some("8"),
                0x39 => Some("9"),
                _ => None,
            }
        }
        0x70..=0x7B => {
            // F1..F12
            match vk {
                0x70 => Some("F1"),
                0x71 => Some("F2"),
                0x72 => Some("F3"),
                0x73 => Some("F4"),
                0x74 => Some("F5"),
                0x75 => Some("F6"),
                0x76 => Some("F7"),
                0x77 => Some("F8"),
                0x78 => Some("F9"),
                0x79 => Some("F10"),
                0x7A => Some("F11"),
                0x7B => Some("F12"),
                _ => None,
            }
        }
        0x20 => Some("Space"),
        _ => None,
    }
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn recorder_wnd_proc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::core::w;
    use windows::Win32::Foundation::{COLORREF, LRESULT, RECT};
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    use windows::Win32::UI::WindowsAndMessaging::*;

    match msg {
        WM_CREATE => {
            let create_struct = lparam.0 as *const CREATESTRUCTW;
            if !create_struct.is_null() {
                let state_ptr = (*create_struct).lpCreateParams as *mut RecorderState;
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            if hdc.0 != 0 {
                let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut RecorderState;
                if !state_ptr.is_null() {
                    let state = &*state_ptr;

                    let mut rect = RECT::default();
                    let _ = GetClientRect(hwnd, &mut rect);

                    let white_brush = HBRUSH(GetStockObject(WHITE_BRUSH).0);
                    FillRect(hdc, &rect, white_brush);

                    let _ = SetBkMode(hdc, TRANSPARENT);

                    let _ = SetTextColor(hdc, COLORREF(0x000000));
                    let mut title = w!("录制快捷键组合 (Hotkey Recorder)").as_wide().to_vec();
                    let mut title_rect = RECT {
                        left: 20,
                        top: 20,
                        right: rect.right - 20,
                        bottom: 50,
                    };
                    let _ = DrawTextW(
                        hdc,
                        &mut title,
                        &mut title_rect,
                        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
                    );

                    let mut instr = w!("请按住 Ctrl / Shift / Alt 并按下字母/数字键进行录制：")
                        .as_wide()
                        .to_vec();
                    let mut instr_rect = RECT {
                        left: 20,
                        top: 50,
                        right: rect.right - 20,
                        bottom: 90,
                    };
                    let _ = DrawTextW(hdc, &mut instr, &mut instr_rect, DT_LEFT | DT_WORDBREAK);

                    let key_text = format!(
                        "当前组合: {}",
                        if state.current_keys.is_empty() {
                            "无"
                        } else {
                            &state.current_keys
                        }
                    );
                    let mut wide_key: Vec<u16> = key_text.encode_utf16().collect();
                    wide_key.push(0);

                    let mut key_rect = RECT {
                        left: 20,
                        top: 90,
                        right: rect.right - 20,
                        bottom: 130,
                    };
                    let _ = SetTextColor(hdc, COLORREF(0xCC3300));
                    let _ = DrawTextW(
                        hdc,
                        &mut wide_key,
                        &mut key_rect,
                        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
                    );

                    let mut footer = w!("按 Enter 保存并写入 config.toml，按 Esc 取消。")
                        .as_wide()
                        .to_vec();
                    let mut footer_rect = RECT {
                        left: 20,
                        top: 130,
                        right: rect.right - 20,
                        bottom: 170,
                    };
                    let _ = SetTextColor(hdc, COLORREF(0x555555));
                    let _ = DrawTextW(
                        hdc,
                        &mut footer,
                        &mut footer_rect,
                        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
                    );
                }
            }
            EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut RecorderState;
            if !state_ptr.is_null() {
                let vk = wparam.0 as u32;
                if vk == 0x1B {
                    // Esc
                    let _ = DestroyWindow(hwnd);
                    return LRESULT(0);
                }
                if vk == 0x0D {
                    // Enter
                    let state = &mut *state_ptr;
                    if !state.current_keys.is_empty() {
                        state.saved = true;
                        let _ = DestroyWindow(hwnd);
                    }
                    return LRESULT(0);
                }

                let ctrl = GetAsyncKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000 != 0;
                let shift = GetAsyncKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000 != 0;
                let alt = GetAsyncKeyState(VK_MENU.0 as i32) as u16 & 0x8000 != 0;

                if vk != VK_CONTROL.0 as u32
                    && vk != VK_SHIFT.0 as u32
                    && vk != VK_MENU.0 as u32
                    && vk != 0x5B
                    && vk != 0x5C
                    && vk != 0x14
                {
                    if let Some(key_str) = vk_to_string(vk) {
                        let mut combo = String::new();
                        if ctrl {
                            combo.push_str("Ctrl+");
                        }
                        if shift {
                            combo.push_str("Shift+");
                        }
                        if alt {
                            combo.push_str("Alt+");
                        }
                        combo.push_str(key_str);

                        let state = &mut *state_ptr;
                        state.current_keys = combo;
                        let _ = InvalidateRect(hwnd, None, true);
                    }
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

#[cfg(target_os = "windows")]
pub fn run_hotkey_recorder() {
    std::thread::spawn(|| {
        unsafe {
            use windows::core::w;
            use windows::Win32::Graphics::Gdi::HBRUSH;
            use windows::Win32::System::LibraryLoader::GetModuleHandleW;
            use windows::Win32::UI::WindowsAndMessaging::*;

            let h_instance = GetModuleHandleW(None).unwrap_or_default();
            let class_name = w!("HotkeyRecorderClass");

            let mut state = Box::new(RecorderState {
                current_keys: String::new(),
                saved: false,
            });

            let wnd_class = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(recorder_wnd_proc),
                hInstance: h_instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hbrBackground: HBRUSH(6), // COLOR_WINDOW + 1
                lpszClassName: class_name,
                ..Default::default()
            };

            let _ = RegisterClassW(&wnd_class);

            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class_name,
                w!("录制快捷键"),
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                450,
                220,
                None,
                None,
                h_instance,
                Some(state.as_mut() as *mut RecorderState as *const std::ffi::c_void),
            );

            if hwnd.0 != 0 {
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = windows::Win32::Graphics::Gdi::UpdateWindow(hwnd);

                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }

                if state.saved && !state.current_keys.is_empty() {
                    if let Ok(mut config) = AppConfig::load_or_default() {
                        config.hotkey.combo_key = state.current_keys.clone();
                        if let Err(e) = config.save() {
                            tracing::error!("Failed to save hotkey config: {}", e);
                        } else {
                            tracing::info!("Saved new hotkey combo: {}", state.current_keys);
                        }
                    }
                }
            }
        }
    });
}

#[cfg(not(target_os = "windows"))]
pub fn run_hotkey_recorder() {}
