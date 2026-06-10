//! System Tray
//!
//! Implements the system tray icon and menu with proper Windows message loop.

use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    TrayIconBuilder,
};

use crate::business::{HotkeyManager, VoiceController};
use crate::data::AppConfig;
use crate::ui::{
    open_settings, ButtonState, DesktopPet, DesktopPetEvent, DesktopPetHandle,
    DesktopPetWindowConfig, FloatingButton, FloatingButtonConfig, FloatingButtonEvent,
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

fn spawn_start_recording(
    runtime_handle: &tokio::runtime::Handle,
    voice_controller: Arc<Mutex<VoiceController>>,
    setter: FloatingButtonStateSetter,
    pet_handle: DesktopPetHandle,
    source: &'static str,
) {
    runtime_handle.spawn(async move {
        let mut controller = voice_controller.lock().await;
        if controller.is_recording() {
            return;
        }

        tracing::info!("Starting voice input from {}", source);
        setter.set_state(ButtonState::Recording);
        pet_handle.set_listening();
        let monitor_vc = voice_controller.clone();
        let monitor_setter = setter.clone();
        if let Err(e) = controller.start().await {
            tracing::error!("Failed to start voice input from {}: {}", source, e);
            setter.set_state(ButtonState::Idle);
            pet_handle.set_error();
        } else {
            drop(controller);
            spawn_recording_state_monitor(monitor_vc, monitor_setter);
        }
    });
}

fn spawn_stop_recording(
    runtime_handle: &tokio::runtime::Handle,
    voice_controller: Arc<Mutex<VoiceController>>,
    setter: FloatingButtonStateSetter,
    pet_handle: DesktopPetHandle,
    source: &'static str,
) {
    runtime_handle.spawn(async move {
        let mut controller = voice_controller.lock().await;
        if !controller.is_recording() {
            pet_handle.set_idle();
            return;
        }

        tracing::info!("Stopping voice input from {}", source);
        setter.set_state(ButtonState::Processing);
        pet_handle.set_processing();
        if let Err(e) = controller.stop().await {
            tracing::error!("Failed to stop voice input from {}: {}", source, e);
            pet_handle.set_error();
        } else {
            pet_handle.set_success();
        }
        setter.set_state(ButtonState::Idle);
    });
}

fn spawn_cancel_recording(
    runtime_handle: &tokio::runtime::Handle,
    voice_controller: Arc<Mutex<VoiceController>>,
    setter: FloatingButtonStateSetter,
    pet_handle: DesktopPetHandle,
    source: &'static str,
) {
    runtime_handle.spawn(async move {
        let mut controller = voice_controller.lock().await;
        if !controller.is_recording() {
            pet_handle.set_idle();
            return;
        }

        tracing::info!("Cancelling voice input from {}", source);
        setter.set_state(ButtonState::Processing);
        pet_handle.set_processing();
        if let Err(e) = controller.cancel().await {
            tracing::error!("Failed to cancel voice input from {}: {}", source, e);
            pet_handle.set_error();
        } else {
            pet_handle.set_idle();
        }
        setter.set_state(ButtonState::Idle);
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

    // Create the optional desktop pet. The thread always exists so the tray menu
    // can show it later even when it starts hidden.
    let mut desktop_pet = DesktopPet::new();
    let desktop_pet_handle = desktop_pet.handle();
    let pet_rx = desktop_pet.take_event_receiver();
    let pet_config = DesktopPetWindowConfig {
        visible: config.desktop_pet.enabled,
        initial_x: config.desktop_pet.position_x,
        initial_y: config.desktop_pet.position_y,
        size: config.desktop_pet.size,
    };
    std::thread::spawn(move || {
        desktop_pet.run(pet_config);
    });

    // Create tray icon on main thread
    let icon = load_icon()?;
    let menu = Menu::new();

    let start_item = MenuItem::new("开始语音输入", true, None);
    let stop_item = MenuItem::new("停止语音输入", true, None);
    let separator1 = PredefinedMenuItem::separator();
    let toggle_pet_item =
        CheckMenuItem::new("显示/隐藏桌宠", true, config.desktop_pet.enabled, None);
    let separator_pet = PredefinedMenuItem::separator();
    let settings_item = MenuItem::new("设置...", true, None);
    let separator2 = PredefinedMenuItem::separator();
    let quit_item = MenuItem::new("退出", true, None);

    let start_id = start_item.id().clone();
    let stop_id = stop_item.id().clone();
    let toggle_pet_id = toggle_pet_item.id().clone();
    let settings_id = settings_item.id().clone();
    let quit_id = quit_item.id().clone();

    menu.append(&start_item)?;
    menu.append(&stop_item)?;
    menu.append(&separator1)?;
    menu.append(&toggle_pet_item)?;
    menu.append(&separator_pet)?;
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
    let pet_for_hotkey = desktop_pet_handle.clone();
    let handle_for_hotkey = runtime_handle.clone();
    hotkey_manager.on_trigger(move || {
        let vc = vc_for_hotkey.clone();
        let setter = state_for_hotkey.clone();
        let pet = pet_for_hotkey.clone();
        let handle = handle_for_hotkey.clone();
        let nested_handle = handle.clone();
        handle.spawn(async move {
            let recording = {
                let controller = vc.lock().await;
                controller.is_recording()
            };
            if recording {
                spawn_stop_recording(&nested_handle, vc, setter, pet, "hotkey");
            } else {
                spawn_start_recording(&nested_handle, vc, setter, pet, "hotkey");
            }
        });
    });

    // Spawn event handler thread for menu and floating button events
    let running_clone = running.clone();
    let vc_clone = voice_controller.clone();
    let state_setter_clone = button_state_setter.clone();
    let pet_handle = desktop_pet_handle.clone();
    let _keep_hotkey_manager = hotkey_manager;
    let mut config = config;
    let mut desktop_pet_visible = config.desktop_pet.enabled;

    std::thread::spawn(move || {
        let _keep = _keep_hotkey_manager;
        while running_clone.load(Ordering::SeqCst) {
            // Check menu events
            if let Ok(event) = menu_rx.recv_timeout(std::time::Duration::from_millis(50)) {
                if event.id == start_id {
                    let vc = vc_clone.clone();
                    let setter = state_setter_clone.clone();
                    spawn_start_recording(
                        &runtime_handle,
                        vc,
                        setter,
                        pet_handle.clone(),
                        "tray menu",
                    );
                } else if event.id == stop_id {
                    let vc = vc_clone.clone();
                    let setter = state_setter_clone.clone();
                    spawn_stop_recording(
                        &runtime_handle,
                        vc,
                        setter,
                        pet_handle.clone(),
                        "tray menu",
                    );
                } else if event.id == toggle_pet_id {
                    desktop_pet_visible = !desktop_pet_visible;
                    if desktop_pet_visible {
                        pet_handle.show();
                        tracing::info!("Desktop pet shown from menu");
                    } else {
                        pet_handle.hide();
                        tracing::info!("Desktop pet hidden from menu");
                    }
                    config.desktop_pet.enabled = desktop_pet_visible;
                    if let Err(e) = config.save() {
                        tracing::error!("Failed to save desktop pet config: {}", e);
                    }
                } else if event.id == settings_id {
                    tracing::info!("Settings from menu");
                    if let Err(e) = open_settings() {
                        tracing::error!("Failed to open settings: {}", e);
                    }
                } else if event.id == quit_id {
                    tracing::info!("Quit from menu");
                    pet_handle.exit();
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
                            spawn_stop_recording(
                                &runtime_handle,
                                vc,
                                setter,
                                pet_handle.clone(),
                                "floating confirm",
                            );
                        }
                        FloatingButtonEvent::CancelRecording => {
                            let vc = vc_clone.clone();
                            let setter = state_setter_clone.clone();
                            spawn_cancel_recording(
                                &runtime_handle,
                                vc,
                                setter,
                                pet_handle.clone(),
                                "floating cancel",
                            );
                        }
                        FloatingButtonEvent::Exit => {
                            tracing::info!("Exit from floating button");
                            pet_handle.exit();
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

            if let Some(ref rx) = pet_rx {
                while let Ok(event) = rx.try_recv() {
                    match event {
                        DesktopPetEvent::StartListeningRequested => {
                            spawn_start_recording(
                                &runtime_handle,
                                vc_clone.clone(),
                                state_setter_clone.clone(),
                                pet_handle.clone(),
                                "desktop pet",
                            );
                        }
                        DesktopPetEvent::StopListeningRequested => {
                            spawn_stop_recording(
                                &runtime_handle,
                                vc_clone.clone(),
                                state_setter_clone.clone(),
                                pet_handle.clone(),
                                "desktop pet",
                            );
                        }
                        DesktopPetEvent::PositionSaveRequested { x, y } => {
                            config.desktop_pet.position_x = x;
                            config.desktop_pet.position_y = y;
                            if let Err(e) = config.save() {
                                tracing::error!("Failed to save desktop pet position: {}", e);
                            }
                        }
                        DesktopPetEvent::SizeSaveRequested { size } => {
                            config.desktop_pet.size = size;
                            if let Err(e) = config.save() {
                                tracing::error!("Failed to save desktop pet size: {}", e);
                            }
                        }
                        DesktopPetEvent::Petted { count } => {
                            tracing::debug!("Desktop pet petted {} times", count);
                        }
                        DesktopPetEvent::ContextMenuRequested { x, y } => {
                            tracing::trace!("Desktop pet context menu at ({}, {})", x, y);
                        }
                        DesktopPetEvent::HoverChanged { hovered } => {
                            tracing::trace!("Desktop pet hover changed: {}", hovered);
                        }
                        DesktopPetEvent::StateChanged(state) => {
                            tracing::trace!("Desktop pet state changed: {:?}", state);
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
    let image =
        image::load_from_memory(include_bytes!("../../assets/aiko_tray_icon.png"))?.into_rgba8();
    let (width, height) = image.dimensions();
    let icon = tray_icon::Icon::from_rgba(image.into_raw(), width, height)?;
    Ok(icon)
}

#[cfg(target_os = "windows")]
#[allow(dead_code)]
struct RecorderState {
    current_keys: String,
    saved: bool,
}

#[cfg(target_os = "windows")]
#[allow(dead_code)]
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
#[allow(dead_code)]
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
#[allow(dead_code)]
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
