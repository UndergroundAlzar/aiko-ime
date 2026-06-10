//! Interactive layered desktop-pet window for Aiko.
//!
//! The platform-independent state machine and frame animation API live in
//! `ui/pet`. This module bridges them to a transparent Win32 layered window.

#[path = "pet/mod.rs"]
mod pet;

#[allow(unused_imports)]
pub use pet::{
    PetAction, PetAnimationAdvance, PetAnimationClip, PetAnimationFrame, PetAnimationPlayer,
    PetFrameImage, PetState, PetStateMachine,
};

use anyhow::{anyhow, Result};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

const PET_COMMAND_MESSAGE: u32 = 0x8000 + 42;
#[cfg(target_os = "windows")]
const PET_WM_MOUSELEAVE: u32 = 0x02A3;

/// Runtime configuration for the desktop pet window.
#[derive(Debug, Clone)]
pub struct DesktopPetWindowConfig {
    pub visible: bool,
    pub initial_x: i32,
    pub initial_y: i32,
    pub size: i32,
}

/// Commands sent from application code to the pet window thread.
#[derive(Debug, Clone)]
pub enum DesktopPetCommand {
    Show,
    Hide,
    SetState(PetState),
    SetInteractionsEnabled(bool),
    SetPosition { x: i32, y: i32 },
    SetSize(i32),
    SetFrameImages(Vec<PetFrameImage>),
    PlayAnimation(PetAnimationClip),
    Pet,
    Exit,
}

/// User intents and persistence requests emitted by the pet window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopPetEvent {
    StartListeningRequested,
    StopListeningRequested,
    ContextMenuRequested { x: i32, y: i32 },
    PositionSaveRequested { x: i32, y: i32 },
    SizeSaveRequested { size: i32 },
    HoverChanged { hovered: bool },
    Petted { count: u32 },
    StateChanged(PetState),
}

/// Thread-safe handle for controlling the desktop pet.
#[derive(Clone)]
pub struct DesktopPetHandle {
    tx: Sender<DesktopPetCommand>,
    hwnd: Arc<AtomicIsize>,
}

impl DesktopPetHandle {
    pub fn show(&self) {
        self.send(DesktopPetCommand::Show);
    }

    pub fn hide(&self) {
        self.send(DesktopPetCommand::Hide);
    }

    pub fn set_state(&self, state: PetState) {
        self.send(DesktopPetCommand::SetState(state));
    }

    pub fn set_idle(&self) {
        self.set_state(PetState::Idle);
    }

    pub fn set_listening(&self) {
        self.set_state(PetState::Listening);
    }

    pub fn set_processing(&self) {
        self.set_state(PetState::Processing);
    }

    pub fn set_success(&self) {
        self.set_state(PetState::Success);
    }

    pub fn set_error(&self) {
        self.set_state(PetState::Error);
    }

    pub fn set_sleepy(&self) {
        self.set_state(PetState::Sleepy);
    }

    pub fn set_interactions_enabled(&self, enabled: bool) {
        self.send(DesktopPetCommand::SetInteractionsEnabled(enabled));
    }

    pub fn set_position(&self, x: i32, y: i32) {
        self.send(DesktopPetCommand::SetPosition { x, y });
    }

    pub fn set_size(&self, size: i32) {
        self.send(DesktopPetCommand::SetSize(size));
    }

    pub fn play_animation(&self, clip: PetAnimationClip) {
        self.send(DesktopPetCommand::PlayAnimation(clip));
    }

    /// Replace the image bank used by animation frame `sprite_index` values.
    pub fn set_frame_images(&self, images: Vec<PetFrameImage>) {
        self.send(DesktopPetCommand::SetFrameImages(images));
    }

    pub fn pet(&self) {
        self.send(DesktopPetCommand::Pet);
    }

    pub fn exit(&self) {
        self.send(DesktopPetCommand::Exit);
    }

    fn send(&self, command: DesktopPetCommand) {
        if self.tx.send(command).is_err() {
            return;
        }

        #[cfg(target_os = "windows")]
        {
            let hwnd = self.hwnd.load(Ordering::Acquire);
            if hwnd != 0 {
                unsafe {
                    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
                    use windows::Win32::UI::WindowsAndMessaging::PostMessageW;
                    let _ = PostMessageW(HWND(hwnd), PET_COMMAND_MESSAGE, WPARAM(0), LPARAM(0));
                }
            }
        }
    }
}

/// Owns the command and event channels for the desktop pet.
pub struct DesktopPet {
    tx: Sender<DesktopPetCommand>,
    rx: Option<Receiver<DesktopPetCommand>>,
    event_tx: Sender<DesktopPetEvent>,
    event_rx: Option<Receiver<DesktopPetEvent>>,
    hwnd: Arc<AtomicIsize>,
}

impl DesktopPet {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        Self {
            tx,
            rx: Some(rx),
            event_tx,
            event_rx: Some(event_rx),
            hwnd: Arc::new(AtomicIsize::new(0)),
        }
    }

    pub fn handle(&self) -> DesktopPetHandle {
        DesktopPetHandle {
            tx: self.tx.clone(),
            hwnd: self.hwnd.clone(),
        }
    }

    /// Take the event receiver. This can only be called once.
    pub fn take_event_receiver(&mut self) -> Option<Receiver<DesktopPetEvent>> {
        self.event_rx.take()
    }

    #[cfg(target_os = "windows")]
    pub fn run(mut self, config: DesktopPetWindowConfig) {
        if let Some(rx) = self.rx.take() {
            if let Err(error) =
                run_windows_pet(rx, self.event_tx.clone(), self.hwnd.clone(), config)
            {
                tracing::error!("Desktop pet failed: {}", error);
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
const ANIMATION_TIMER_ID: usize = 1;
#[cfg(target_os = "windows")]
const PET_CONTEXT_HIDE: usize = 10_001;
#[cfg(target_os = "windows")]
const PET_CONTEXT_SIZE_128: usize = 10_128;
#[cfg(target_os = "windows")]
const PET_CONTEXT_SIZE_192: usize = 10_192;
#[cfg(target_os = "windows")]
const PET_CONTEXT_SIZE_256: usize = 10_256;
#[cfg(target_os = "windows")]
const PET_CONTEXT_SIZE_320: usize = 10_320;

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, Copy)]
struct HeartParticle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    life_ms: f32,
    total_life_ms: f32,
    size: i32,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Default)]
struct DragState {
    pressed: bool,
    dragging: bool,
    start_cursor_x: i32,
    start_cursor_y: i32,
    start_window_x: i32,
    start_window_y: i32,
}

#[cfg(target_os = "windows")]
struct PetWindowState {
    rx: Receiver<DesktopPetCommand>,
    event_tx: Sender<DesktopPetEvent>,
    hwnd_shared: Arc<AtomicIsize>,
    frame_images: Vec<PetFrameImage>,
    width: i32,
    height: i32,
    visible: bool,
    machine: PetStateMachine,
    animation: PetAnimationPlayer,
    animation_override: bool,
    particles: Vec<HeartParticle>,
    drag: DragState,
    tracking_mouse: bool,
    last_tick: std::time::Instant,
    visual_clock_ms: u32,
}

#[cfg(target_os = "windows")]
fn run_windows_pet(
    rx: Receiver<DesktopPetCommand>,
    event_tx: Sender<DesktopPetEvent>,
    hwnd_shared: Arc<AtomicIsize>,
    config: DesktopPetWindowConfig,
) -> Result<()> {
    use windows::core::w;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::*;

    let size = config.size.clamp(96, 320);
    let frame_images = load_default_pet_frame_images(size)?;

    let mut state = Box::new(PetWindowState {
        rx,
        event_tx,
        hwnd_shared: hwnd_shared.clone(),
        frame_images,
        width: size,
        height: size,
        visible: config.visible,
        machine: PetStateMachine::new(),
        animation: PetAnimationPlayer::new(PetAnimationClip::idle()),
        animation_override: false,
        particles: Vec::new(),
        drag: DragState::default(),
        tracking_mouse: false,
        last_tick: std::time::Instant::now(),
        visual_clock_ms: 0,
    });

    unsafe {
        let h_instance = GetModuleHandleW(None).unwrap_or_default();
        let class_name = w!("AikoDesktopPetWindow");

        let wnd_class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(pet_wnd_proc),
            hInstance: h_instance.into(),
            hCursor: LoadCursorW(None, IDC_HAND).unwrap_or_default(),
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
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
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

        hwnd_shared.store(hwnd.0, Ordering::Release);
        std::mem::forget(state);

        let _ = PostMessageW(
            hwnd,
            PET_COMMAND_MESSAGE,
            windows::Win32::Foundation::WPARAM(0),
            windows::Win32::Foundation::LPARAM(0),
        );
        render_pet(hwnd);
        schedule_next_tick(hwnd);
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
fn load_default_pet_frame_images(size: i32) -> Result<Vec<PetFrameImage>> {
    let size = size.clamp(96, 320) as u32;
    [
        include_bytes!("../../assets/aiko_desktop_pet.png").as_slice(),
        include_bytes!("../../assets/mascot/states/aiko_listening.png").as_slice(),
        include_bytes!("../../assets/mascot/states/aiko_processing.png").as_slice(),
        include_bytes!("../../assets/mascot/states/aiko_success.png").as_slice(),
        include_bytes!("../../assets/mascot/states/aiko_error.png").as_slice(),
        include_bytes!("../../assets/mascot/states/aiko_petted.png").as_slice(),
        include_bytes!("../../assets/mascot/states/aiko_sleepy.png").as_slice(),
    ]
    .into_iter()
    .map(|bytes| decode_pet_frame_image(bytes, size))
    .collect()
}

#[cfg(target_os = "windows")]
fn decode_pet_frame_image(bytes: &[u8], size: u32) -> Result<PetFrameImage> {
    use image::imageops::{overlay, resize, FilterType};
    use image::RgbaImage;

    let source = image::load_from_memory(bytes)?.into_rgba8();
    let (source_w, source_h) = source.dimensions();
    let scale = (size as f32 / source_w as f32).min(size as f32 / source_h as f32);
    let target_w = ((source_w as f32 * scale).round() as u32).max(1);
    let target_h = ((source_h as f32 * scale).round() as u32).max(1);
    let resized = resize(&source, target_w, target_h, FilterType::Lanczos3);
    let mut canvas = RgbaImage::new(size, size);
    let x = (size.saturating_sub(target_w) / 2) as i64;
    let y = (size.saturating_sub(target_h) / 2) as i64;
    overlay(&mut canvas, &resized, x, y);

    PetFrameImage::new(size, size, canvas.into_raw())
        .ok_or_else(|| anyhow!("decoded pet image has invalid RGBA dimensions"))
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn pet_wnd_proc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    _wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::Foundation::*;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
    };
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
        PET_COMMAND_MESSAGE => {
            let state = window_state_mut(hwnd);
            if state.is_none() {
                return LRESULT(0);
            }
            let state = state.unwrap();
            let mut exit_requested = false;
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
                    DesktopPetCommand::SetState(pet_state) => {
                        let actions = state.machine.set_state(pet_state);
                        apply_actions(hwnd, state, actions);
                    }
                    DesktopPetCommand::SetInteractionsEnabled(enabled) => {
                        let actions = state.machine.set_interactions_enabled(enabled);
                        apply_actions(hwnd, state, actions);
                        apply_interaction_style(hwnd, enabled);
                    }
                    DesktopPetCommand::SetPosition { x, y } => {
                        let _ = SetWindowPos(
                            hwnd,
                            HWND_TOPMOST,
                            x,
                            y,
                            0,
                            0,
                            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                        );
                    }
                    DesktopPetCommand::SetSize(size) => {
                        resize_pet_window(hwnd, state, size);
                    }
                    DesktopPetCommand::SetFrameImages(images) => {
                        let images: Vec<_> =
                            images.into_iter().filter(PetFrameImage::is_valid).collect();
                        if !images.is_empty() {
                            state.frame_images = images;
                        }
                    }
                    DesktopPetCommand::PlayAnimation(clip) => {
                        state.animation.set_clip(clip);
                        state.animation_override = true;
                    }
                    DesktopPetCommand::Pet => {
                        let actions = state.machine.pet();
                        apply_actions(hwnd, state, actions);
                    }
                    DesktopPetCommand::Exit => {
                        exit_requested = true;
                        break;
                    }
                }
            }
            if exit_requested {
                let _ = DestroyWindow(hwnd);
                return LRESULT(0);
            }
            render_pet(hwnd);
            schedule_next_tick(hwnd);
            LRESULT(0)
        }
        WM_TIMER => {
            let Some(state) = window_state_mut(hwnd) else {
                return LRESULT(0);
            };
            let now = std::time::Instant::now();
            let elapsed_ms = now
                .duration_since(state.last_tick)
                .as_millis()
                .clamp(1, u32::MAX as u128) as u32;
            state.last_tick = now;
            let old_overlay_key = visual_overlay_key(state);
            state.visual_clock_ms = state.visual_clock_ms.wrapping_add(elapsed_ms);

            let advance = state.animation.advance(elapsed_ms);
            let mut needs_render = advance.frame_changed;
            debug_assert_eq!(advance.completed, state.animation.is_completed());
            if state.animation.is_completed() && state.animation_override {
                state.animation.set_clip(state.machine.state().animation());
                state.animation_override = false;
                needs_render = true;
            }

            let actions = state.machine.tick(elapsed_ms);
            if !actions.is_empty() {
                apply_actions(hwnd, state, actions);
                needs_render = true;
            }
            if update_particles(state, elapsed_ms) {
                needs_render = true;
            }
            if visual_overlay_key(state) != old_overlay_key {
                needs_render = true;
            }

            if needs_render && state.visible {
                render_pet(hwnd);
            }
            schedule_next_tick(hwnd);
            LRESULT(0)
        }
        WM_NCHITTEST => {
            use windows::Win32::Graphics::Gdi::ScreenToClient;

            let Some(state) = window_state_mut(hwnd) else {
                return LRESULT(HTTRANSPARENT as isize);
            };
            if !state.machine.interactions_enabled() {
                return LRESULT(HTTRANSPARENT as isize);
            }
            let mut point = POINT {
                x: low_word_signed(lparam.0),
                y: high_word_signed(lparam.0),
            };
            let _ = ScreenToClient(hwnd, &mut point);
            if pet_is_opaque_at(state, point.x, point.y) {
                LRESULT(HTCLIENT as isize)
            } else {
                LRESULT(HTTRANSPARENT as isize)
            }
        }
        WM_MOUSEMOVE => {
            let Some(state) = window_state_mut(hwnd) else {
                return LRESULT(0);
            };
            if !state.machine.interactions_enabled() {
                return LRESULT(0);
            }

            if !state.tracking_mouse {
                let mut track = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                let _ = TrackMouseEvent(&mut track);
                state.tracking_mouse = true;
                let actions = state.machine.pointer_entered();
                apply_actions(hwnd, state, actions);
            }

            if state.drag.pressed {
                let mut cursor = POINT::default();
                let _ = GetCursorPos(&mut cursor);
                let dx = cursor.x - state.drag.start_cursor_x;
                let dy = cursor.y - state.drag.start_cursor_y;
                if !state.drag.dragging && (dx.abs() > 5 || dy.abs() > 5) {
                    state.drag.dragging = true;
                }
                if state.drag.dragging {
                    let _ = SetWindowPos(
                        hwnd,
                        HWND_TOPMOST,
                        state.drag.start_window_x + dx,
                        state.drag.start_window_y + dy,
                        0,
                        0,
                        SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
            } else {
                let x = low_word_signed(lparam.0);
                let y = high_word_signed(lparam.0);
                let actions = state.machine.pointer_moved(x, y);
                apply_actions(hwnd, state, actions);
            }
            render_pet(hwnd);
            schedule_next_tick(hwnd);
            LRESULT(0)
        }
        PET_WM_MOUSELEAVE => {
            let Some(state) = window_state_mut(hwnd) else {
                return LRESULT(0);
            };
            state.tracking_mouse = false;
            let actions = state.machine.pointer_left();
            apply_actions(hwnd, state, actions);
            render_pet(hwnd);
            schedule_next_tick(hwnd);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let Some(state) = window_state_mut(hwnd) else {
                return LRESULT(0);
            };
            if !state.machine.interactions_enabled() {
                return LRESULT(0);
            }

            let mut cursor = POINT::default();
            let mut rect = RECT::default();
            let _ = GetCursorPos(&mut cursor);
            let _ = GetWindowRect(hwnd, &mut rect);
            state.drag = DragState {
                pressed: true,
                dragging: false,
                start_cursor_x: cursor.x,
                start_cursor_y: cursor.y,
                start_window_x: rect.left,
                start_window_y: rect.top,
            };
            let _ = SetCapture(hwnd);
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let Some(state) = window_state_mut(hwnd) else {
                return LRESULT(0);
            };
            if !state.drag.pressed {
                return LRESULT(0);
            }

            let was_dragging = state.drag.dragging;
            state.drag = DragState::default();
            let _ = ReleaseCapture();

            if was_dragging {
                let mut rect = RECT::default();
                if GetWindowRect(hwnd, &mut rect).is_ok() {
                    let _ = state.event_tx.send(DesktopPetEvent::PositionSaveRequested {
                        x: rect.left,
                        y: rect.top,
                    });
                }
            } else {
                let actions = state.machine.primary_clicked();
                apply_actions(hwnd, state, actions);
            }
            render_pet(hwnd);
            schedule_next_tick(hwnd);
            LRESULT(0)
        }
        WM_CAPTURECHANGED => {
            if let Some(state) = window_state_mut(hwnd) {
                state.drag = DragState::default();
            }
            LRESULT(0)
        }
        WM_RBUTTONUP | WM_CONTEXTMENU => {
            let Some(state) = window_state_mut(hwnd) else {
                return LRESULT(0);
            };
            if !state.machine.interactions_enabled() {
                return LRESULT(0);
            }

            let mut point = POINT::default();
            let _ = GetCursorPos(&mut point);
            let _ = state.event_tx.send(DesktopPetEvent::ContextMenuRequested {
                x: point.x,
                y: point.y,
            });
            show_pet_context_menu(hwnd, state, point.x, point.y);
            LRESULT(0)
        }
        WM_DESTROY => {
            let _ = KillTimer(hwnd, ANIMATION_TIMER_ID);
            let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut PetWindowState;
            if !state_ptr.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                (*state_ptr).hwnd_shared.store(0, Ordering::Release);
                drop(Box::from_raw(state_ptr));
            }
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, _wparam, lparam),
    }
}

#[cfg(target_os = "windows")]
unsafe fn window_state_mut(
    hwnd: windows::Win32::Foundation::HWND,
) -> Option<&'static mut PetWindowState> {
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, GWLP_USERDATA};
    let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut PetWindowState;
    state_ptr.as_mut()
}

#[cfg(target_os = "windows")]
unsafe fn apply_actions(
    _hwnd: windows::Win32::Foundation::HWND,
    state: &mut PetWindowState,
    actions: Vec<PetAction>,
) {
    for action in actions {
        match action {
            PetAction::StateChanged(pet_state) => {
                state.animation.set_clip(pet_state.animation());
                state.animation_override = false;
                let _ = state
                    .event_tx
                    .send(DesktopPetEvent::StateChanged(pet_state));
                if pet_state == PetState::Success {
                    spawn_hearts(state, 10);
                }
            }
            PetAction::StartListeningRequested => {
                let _ = state
                    .event_tx
                    .send(DesktopPetEvent::StartListeningRequested);
            }
            PetAction::StopListeningRequested => {
                let _ = state.event_tx.send(DesktopPetEvent::StopListeningRequested);
            }
            PetAction::HoverChanged(hovered) => {
                let _ = state
                    .event_tx
                    .send(DesktopPetEvent::HoverChanged { hovered });
            }
            PetAction::HappyFeedback { pet_count } => {
                trigger_happy_feedback(state);
                let _ = state
                    .event_tx
                    .send(DesktopPetEvent::Petted { count: pet_count });
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn trigger_happy_feedback(state: &mut PetWindowState) {
    state.animation.set_clip(PetAnimationClip::happy());
    state.animation_override = true;
    spawn_hearts(state, 7);
}

#[cfg(target_os = "windows")]
fn spawn_hearts(state: &mut PetWindowState, count: usize) {
    use rand::Rng;

    let mut rng = rand::thread_rng();
    let capacity = 56usize.saturating_sub(state.particles.len());
    for _ in 0..count.min(capacity) {
        let life = rng.gen_range(720.0..1_150.0);
        state.particles.push(HeartParticle {
            x: state.width as f32 * rng.gen_range(0.35..0.68),
            y: state.height as f32 * rng.gen_range(0.22..0.48),
            vx: rng.gen_range(-22.0..22.0),
            vy: rng.gen_range(-58.0..-34.0),
            life_ms: life,
            total_life_ms: life,
            size: rng.gen_range(4..8),
        });
    }
}

#[cfg(target_os = "windows")]
fn update_particles(state: &mut PetWindowState, elapsed_ms: u32) -> bool {
    if state.particles.is_empty() {
        return false;
    }

    let seconds = elapsed_ms as f32 / 1_000.0;
    for particle in &mut state.particles {
        particle.x += particle.vx * seconds;
        particle.y += particle.vy * seconds;
        particle.vy -= 4.0 * seconds;
        particle.life_ms -= elapsed_ms as f32;
    }
    state.particles.retain(|particle| particle.life_ms > 0.0);
    true
}

#[cfg(target_os = "windows")]
unsafe fn apply_interaction_style(hwnd: windows::Win32::Foundation::HWND, enabled: bool) {
    use windows::Win32::UI::WindowsAndMessaging::*;

    let mut style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
    let transparent = WS_EX_TRANSPARENT.0 as isize;
    if enabled {
        style &= !transparent;
    } else {
        style |= transparent;
    }
    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style);
    let _ = SetWindowPos(
        hwnd,
        HWND_TOPMOST,
        0,
        0,
        0,
        0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
    );
}

#[cfg(target_os = "windows")]
unsafe fn show_pet_context_menu(
    hwnd: windows::Win32::Foundation::HWND,
    state: &mut PetWindowState,
    x: i32,
    y: i32,
) {
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::*;

    let Ok(menu) = CreatePopupMenu() else {
        return;
    };

    let _ = AppendMenuW(menu, MF_STRING, PET_CONTEXT_HIDE, w!("显示/隐藏桌宠"));
    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
    append_pet_size_menu_item(menu, PET_CONTEXT_SIZE_128, w!("小 / 128px"), state.width);
    append_pet_size_menu_item(menu, PET_CONTEXT_SIZE_192, w!("中 / 192px"), state.width);
    append_pet_size_menu_item(menu, PET_CONTEXT_SIZE_256, w!("大 / 256px"), state.width);
    append_pet_size_menu_item(menu, PET_CONTEXT_SIZE_320, w!("超大 / 320px"), state.width);

    let _ = SetForegroundWindow(hwnd);
    let command = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY,
        x,
        y,
        0,
        hwnd,
        None,
    )
    .0 as usize;
    let _ = DestroyMenu(menu);
    let _ = PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));

    match command {
        PET_CONTEXT_HIDE => {
            state.visible = !state.visible;
            if state.visible {
                render_pet(hwnd);
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            } else {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
        }
        PET_CONTEXT_SIZE_128 => resize_pet_window(hwnd, state, 128),
        PET_CONTEXT_SIZE_192 => resize_pet_window(hwnd, state, 192),
        PET_CONTEXT_SIZE_256 => resize_pet_window(hwnd, state, 256),
        PET_CONTEXT_SIZE_320 => resize_pet_window(hwnd, state, 320),
        _ => {}
    }
}

#[cfg(target_os = "windows")]
unsafe fn append_pet_size_menu_item(
    menu: windows::Win32::UI::WindowsAndMessaging::HMENU,
    id: usize,
    label: windows::core::PCWSTR,
    current_size: i32,
) {
    use windows::Win32::UI::WindowsAndMessaging::*;

    let checked = match id {
        PET_CONTEXT_SIZE_128 => current_size <= 160,
        PET_CONTEXT_SIZE_192 => (161..=224).contains(&current_size),
        PET_CONTEXT_SIZE_256 => (225..=288).contains(&current_size),
        PET_CONTEXT_SIZE_320 => current_size >= 289,
        _ => false,
    };
    let flags = if checked {
        MENU_ITEM_FLAGS(MF_STRING.0 | MF_CHECKED.0)
    } else {
        MF_STRING
    };
    let _ = AppendMenuW(menu, flags, id, label);
}

#[cfg(target_os = "windows")]
unsafe fn resize_pet_window(
    hwnd: windows::Win32::Foundation::HWND,
    state: &mut PetWindowState,
    requested_size: i32,
) {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::*;

    let size = requested_size.clamp(96, 320);
    if state.width == size && state.height == size {
        return;
    }

    let frame_images = match load_default_pet_frame_images(size) {
        Ok(images) => images,
        Err(error) => {
            tracing::warn!(
                "failed to reload desktop pet sprites at {}px: {}",
                size,
                error
            );
            return;
        }
    };

    let mut rect = RECT::default();
    let (x, y) = if GetWindowRect(hwnd, &mut rect).is_ok() {
        let center_x = rect.left + (rect.right - rect.left) / 2;
        let center_y = rect.top + (rect.bottom - rect.top) / 2;
        (center_x - size / 2, center_y - size / 2)
    } else {
        (0, 0)
    };

    state.frame_images = frame_images;
    state.width = size;
    state.height = size;
    let _ = SetWindowPos(
        hwnd,
        HWND_TOPMOST,
        x,
        y,
        size,
        size,
        SWP_NOZORDER | SWP_NOACTIVATE,
    );
    let _ = state
        .event_tx
        .send(DesktopPetEvent::SizeSaveRequested { size });
    let _ = state
        .event_tx
        .send(DesktopPetEvent::PositionSaveRequested { x, y });
    if state.visible {
        render_pet(hwnd);
    }
}

#[cfg(target_os = "windows")]
unsafe fn schedule_next_tick(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};

    let Some(state) = window_state_mut(hwnd) else {
        return;
    };
    let mut interval = state.animation.remaining_in_frame_ms().clamp(16, 1_000);
    if !state.particles.is_empty() {
        interval = interval.min(33);
    } else if state.animation_override {
        interval = interval.min(55);
    } else if state.machine.is_hovered() {
        interval = interval.min(100);
    }
    if state.visible && matches!(state.machine.state(), PetState::Idle | PetState::Sleepy) {
        interval = interval.min(100);
    }

    let _ = KillTimer(hwnd, ANIMATION_TIMER_ID);
    SetTimer(hwnd, ANIMATION_TIMER_ID, interval, None);
}

#[cfg(target_os = "windows")]
fn low_word_signed(value: isize) -> i32 {
    value as i16 as i32
}

#[cfg(target_os = "windows")]
fn high_word_signed(value: isize) -> i32 {
    (value >> 16) as i16 as i32
}

#[cfg(target_os = "windows")]
fn pet_draw_rect(state: &PetWindowState) -> (i32, i32, i32, i32) {
    let frame = state.animation.current_frame();
    let draw_w = (state.width * frame.scale_permille as i32 / 1_000).max(1);
    let draw_h = (state.height * frame.scale_permille as i32 / 1_000).max(1);
    let draw_x = (state.width - draw_w) / 2 + frame.offset_x as i32;
    let draw_y = (state.height - draw_h) / 2 + frame.offset_y as i32;
    (draw_x, draw_y, draw_w, draw_h)
}

#[cfg(target_os = "windows")]
fn pet_is_opaque_at(state: &PetWindowState, x: i32, y: i32) -> bool {
    let (draw_x, draw_y, draw_w, draw_h) = pet_draw_rect(state);
    if x < draw_x || y < draw_y || x >= draw_x + draw_w || y >= draw_y + draw_h {
        return false;
    }
    let image = active_frame_image(state);
    let source_x = ((x - draw_x) * image.width as i32 / draw_w).clamp(0, image.width as i32 - 1);
    let source_y = ((y - draw_y) * image.height as i32 / draw_h).clamp(0, image.height as i32 - 1);
    let index = ((source_y * image.width as i32 + source_x) * 4 + 3) as usize;
    image.rgba.get(index).copied().unwrap_or(0) > 18
}

#[cfg(target_os = "windows")]
fn active_frame_image(state: &PetWindowState) -> &PetFrameImage {
    let sprite_index = state.animation.current_frame().sprite_index as usize;
    state
        .frame_images
        .get(sprite_index)
        .unwrap_or(&state.frame_images[0])
}

#[cfg(target_os = "windows")]
unsafe fn render_pet(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::Foundation::*;
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::UI::WindowsAndMessaging::*;

    let Some(state) = window_state_mut(hwnd) else {
        return;
    };
    let canvas = compose_pet_frame(state);

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
            std::ptr::copy_nonoverlapping(canvas.as_ptr(), bits as *mut u8, canvas.len());

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
            let source = POINT { x: 0, y: 0 };
            let _ = UpdateLayeredWindow(
                hwnd,
                hdc_screen,
                None,
                Some(&size),
                hdc_mem,
                Some(&source),
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

#[cfg(target_os = "windows")]
fn compose_pet_frame(state: &PetWindowState) -> Vec<u8> {
    let mut canvas = vec![0u8; (state.width * state.height * 4) as usize];
    let frame = state.animation.current_frame();
    let image = active_frame_image(state);
    let (draw_x, draw_y, draw_w, draw_h) = pet_draw_rect(state);
    let hovered = state.machine.is_hovered();

    for destination_y in draw_y.max(0)..(draw_y + draw_h).min(state.height) {
        for destination_x in draw_x.max(0)..(draw_x + draw_w).min(state.width) {
            let source_x = ((destination_x - draw_x) * image.width as i32 / draw_w)
                .clamp(0, image.width as i32 - 1);
            let source_y = ((destination_y - draw_y) * image.height as i32 / draw_h)
                .clamp(0, image.height as i32 - 1);
            let source_index = ((source_y * image.width as i32 + source_x) * 4) as usize;
            let destination_index = ((destination_y * state.width + destination_x) * 4) as usize;

            let source_alpha = image.rgba[source_index + 3] as u32;
            let alpha = source_alpha * frame.opacity as u32 / 255;
            if alpha == 0 {
                continue;
            }
            let brighten = if hovered { 112u32 } else { 100u32 };
            let red = (image.rgba[source_index] as u32 * brighten / 100).min(255);
            let green = (image.rgba[source_index + 1] as u32 * brighten / 100).min(255);
            let blue = (image.rgba[source_index + 2] as u32 * brighten / 100).min(255);

            canvas[destination_index] = (blue * alpha / 255) as u8;
            canvas[destination_index + 1] = (green * alpha / 255) as u8;
            canvas[destination_index + 2] = (red * alpha / 255) as u8;
            canvas[destination_index + 3] = alpha as u8;
        }
    }

    draw_status_indicator(&mut canvas, state);
    draw_expression_overlay(&mut canvas, state);
    if hovered {
        draw_heart(
            &mut canvas,
            state.width,
            state.height,
            state.width * 28 / 100,
            state.height * 23 / 100,
            5,
            [255, 88, 139, 215],
        );
    }
    for particle in &state.particles {
        let alpha = (particle.life_ms / particle.total_life_ms * 235.0).clamp(0.0, 235.0) as u8;
        draw_heart(
            &mut canvas,
            state.width,
            state.height,
            particle.x.round() as i32,
            particle.y.round() as i32,
            particle.size,
            [255, 69, 126, alpha],
        );
    }

    canvas
}

#[cfg(target_os = "windows")]
fn draw_status_indicator(canvas: &mut [u8], state: &PetWindowState) {
    let (color, radius) = match state.machine.state() {
        PetState::Idle => ([255, 255, 255, 155], 3),
        PetState::Listening => ([255, 52, 92, 235], 5),
        PetState::Processing => ([255, 181, 51, 225], 4),
        PetState::Success => ([74, 222, 128, 235], 5),
        PetState::Error => ([248, 70, 70, 235], 5),
        PetState::Petted => ([255, 88, 139, 225], 4),
        PetState::Sleepy => ([148, 163, 184, 185], 3),
    };
    let x = state.width * 76 / 100;
    let y = state.height * 20 / 100;
    draw_circle(
        canvas,
        state.width,
        state.height,
        x,
        y,
        radius + 2,
        [255, 255, 255, 95],
    );
    draw_circle(canvas, state.width, state.height, x, y, radius, color);
}

#[cfg(target_os = "windows")]
fn visual_overlay_key(state: &PetWindowState) -> u8 {
    match state.machine.state() {
        PetState::Sleepy => 10 + ((state.visual_clock_ms / 360) % 3) as u8,
        _ if should_blink(state) => 1,
        _ => 0,
    }
}

#[cfg(target_os = "windows")]
fn should_blink(state: &PetWindowState) -> bool {
    if state.machine.state() == PetState::Sleepy {
        return false;
    }
    let phase = state.visual_clock_ms % 4_800;
    (4_260..=4_380).contains(&phase)
}

#[cfg(target_os = "windows")]
fn draw_expression_overlay(canvas: &mut [u8], state: &PetWindowState) {
    if should_blink(state) {
        draw_blink(canvas, state.width, state.height);
    }
    if state.machine.state() == PetState::Sleepy {
        draw_sleepy_marks(canvas, state);
    }
}

#[cfg(target_os = "windows")]
fn draw_blink(canvas: &mut [u8], width: i32, height: i32) {
    let y = height * 43 / 100;
    let left_x = width * 40 / 100;
    let right_x = width * 58 / 100;
    let eye_w = (width / 18).max(5);
    let color = [78, 58, 92, 185];
    draw_line(
        canvas,
        width,
        height,
        left_x - eye_w,
        y,
        left_x + eye_w,
        y + 1,
        2,
        color,
    );
    draw_line(
        canvas,
        width,
        height,
        right_x - eye_w,
        y + 1,
        right_x + eye_w,
        y,
        2,
        color,
    );
}

#[cfg(target_os = "windows")]
fn draw_sleepy_marks(canvas: &mut [u8], state: &PetWindowState) {
    let phase = ((state.visual_clock_ms / 360) % 3) as i32;
    let base_x = state.width * 68 / 100 + phase * 4;
    let base_y = state.height * 21 / 100 - phase * 5;
    let size = (state.width / 26).max(5) + phase;
    let alpha = (205 - phase * 36).clamp(90, 205) as u8;
    let color = [112, 145, 255, alpha];

    draw_line(
        canvas,
        state.width,
        state.height,
        base_x,
        base_y,
        base_x + size,
        base_y,
        1,
        color,
    );
    draw_line(
        canvas,
        state.width,
        state.height,
        base_x + size,
        base_y,
        base_x,
        base_y + size,
        1,
        color,
    );
    draw_line(
        canvas,
        state.width,
        state.height,
        base_x,
        base_y + size,
        base_x + size,
        base_y + size,
        1,
        color,
    );
}

#[cfg(target_os = "windows")]
fn draw_circle(
    canvas: &mut [u8],
    width: i32,
    height: i32,
    center_x: i32,
    center_y: i32,
    radius: i32,
    color: [u8; 4],
) {
    for y in -radius..=radius {
        for x in -radius..=radius {
            if x * x + y * y <= radius * radius {
                blend_pixel(canvas, width, height, center_x + x, center_y + y, color);
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn draw_line(
    canvas: &mut [u8],
    width: i32,
    height: i32,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    radius: i32,
    color: [u8; 4],
) {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let steps = dx.abs().max(dy.abs()).max(1);
    for step in 0..=steps {
        let x = x1 + dx * step / steps;
        let y = y1 + dy * step / steps;
        draw_circle(canvas, width, height, x, y, radius, color);
    }
}

#[cfg(target_os = "windows")]
fn draw_heart(
    canvas: &mut [u8],
    width: i32,
    height: i32,
    center_x: i32,
    center_y: i32,
    radius: i32,
    color: [u8; 4],
) {
    let extent = radius * 2;
    for y in -extent..=extent {
        for x in -extent..=extent {
            let nx = x as f32 / radius.max(1) as f32;
            let ny = -(y as f32) / radius.max(1) as f32;
            let base = nx * nx + ny * ny - 1.0;
            if base * base * base - nx * nx * ny * ny * ny <= 0.0 {
                blend_pixel(canvas, width, height, center_x + x, center_y + y, color);
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn blend_pixel(canvas: &mut [u8], width: i32, height: i32, x: i32, y: i32, rgba: [u8; 4]) {
    if x < 0 || y < 0 || x >= width || y >= height {
        return;
    }
    let index = ((y * width + x) * 4) as usize;
    let source_alpha = rgba[3] as u32;
    let inverse_alpha = 255 - source_alpha;
    let source_b = rgba[2] as u32 * source_alpha / 255;
    let source_g = rgba[1] as u32 * source_alpha / 255;
    let source_r = rgba[0] as u32 * source_alpha / 255;

    canvas[index] = (source_b + canvas[index] as u32 * inverse_alpha / 255).min(255) as u8;
    canvas[index + 1] = (source_g + canvas[index + 1] as u32 * inverse_alpha / 255).min(255) as u8;
    canvas[index + 2] = (source_r + canvas[index + 2] as u32 * inverse_alpha / 255).min(255) as u8;
    canvas[index + 3] =
        (source_alpha + canvas[index + 3] as u32 * inverse_alpha / 255).min(255) as u8;
}
