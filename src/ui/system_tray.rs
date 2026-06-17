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

/// Reload the configuration from disk, apply one mutation, and save.
///
/// Position/size/visibility updates arrive from several threads; mutating a
/// long-lived stale copy would silently revert changes saved by the settings
/// window in the meantime.
fn update_config<F: FnOnce(&mut AppConfig)>(context: &str, mutator: F) {
    match AppConfig::load_or_default() {
        Ok(mut config) => {
            mutator(&mut config);
            if let Err(e) = config.save() {
                tracing::error!("Failed to save config ({}): {}", context, e);
            }
        }
        Err(e) => {
            tracing::error!("Failed to load config for update ({}): {}", context, e);
        }
    }
}

/// Watch a running session and put the HUD and pet back into a sane state when
/// the session ends on its own (server hangup, target window lost, error).
/// User-initiated stop/cancel paths set their own pet states.
fn spawn_recording_state_monitor(
    voice_controller: Arc<Mutex<VoiceController>>,
    setter: FloatingButtonStateSetter,
    pet_handle: DesktopPetHandle,
) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let (recording, stopped_by_user, had_error) = {
                let controller = voice_controller.lock().await;
                (
                    controller.is_recording(),
                    controller.was_stopped_by_user(),
                    controller.last_input_error().is_some(),
                )
            };

            if !recording {
                setter.set_state(ButtonState::Idle);
                if !stopped_by_user {
                    if had_error {
                        pet_handle.set_error();
                    } else {
                        pet_handle.set_idle();
                    }
                }
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
        let monitor_pet = pet_handle.clone();
        if let Err(e) = controller.start().await {
            tracing::error!("Failed to start voice input from {}: {}", source, e);
            setter.set_state(ButtonState::Idle);
            pet_handle.set_error();
        } else {
            drop(controller);
            spawn_recording_state_monitor(monitor_vc, monitor_setter, monitor_pet);
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

/// Ask the main-thread Win32 message loop to exit.
///
/// `PostQuitMessage` only posts to the *calling* thread's queue, so calling it
/// from the event-handler thread would leave the main `GetMessageW` loop
/// blocked until an unrelated message arrived. Post `WM_QUIT` to the main
/// thread explicitly instead.
#[cfg(target_os = "windows")]
fn request_main_loop_exit(main_thread_id: u32) {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};
    unsafe {
        let _ = PostThreadMessageW(main_thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
    }
}

#[cfg(not(target_os = "windows"))]
fn request_main_loop_exit(_main_thread_id: u32) {}

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

    // The Win32 message loop below runs on this thread; the event-handler
    // thread needs its id to deliver WM_QUIT.
    #[cfg(target_os = "windows")]
    let main_thread_id = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
    #[cfg(not(target_os = "windows"))]
    let main_thread_id = 0u32;

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
                    let visible = desktop_pet_visible;
                    update_config("desktop pet visibility", |config| {
                        config.desktop_pet.enabled = visible;
                    });
                } else if event.id == settings_id {
                    tracing::info!("Settings from menu");
                    if let Err(e) = open_settings() {
                        tracing::error!("Failed to open settings: {}", e);
                    }
                } else if event.id == quit_id {
                    tracing::info!("Quit from menu");
                    pet_handle.exit();
                    running_clone.store(false, Ordering::SeqCst);
                    request_main_loop_exit(main_thread_id);
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
                            request_main_loop_exit(main_thread_id);
                        }
                        FloatingButtonEvent::UpdatePosition { x, y } => {
                            update_config("floating button position", |config| {
                                config.floating_button.position_x = x;
                                config.floating_button.position_y = y;
                            });
                            tracing::debug!("Saved floating button position: ({}, {})", x, y);
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
                            update_config("desktop pet position", |config| {
                                config.desktop_pet.position_x = x;
                                config.desktop_pet.position_y = y;
                            });
                        }
                        DesktopPetEvent::SizeSaveRequested { size } => {
                            update_config("desktop pet size", |config| {
                                config.desktop_pet.size = size;
                            });
                        }
                        DesktopPetEvent::VisibilityChanged { visible } => {
                            // Keep the tray toggle's backing flag and the saved
                            // config in sync with the pet's own context menu.
                            desktop_pet_visible = visible;
                            update_config("desktop pet visibility", |config| {
                                config.desktop_pet.enabled = visible;
                            });
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
