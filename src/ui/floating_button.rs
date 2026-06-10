//! Floating Button
//!
//! A Typeless-style floating pill that appears while recording: a cancel (✗)
//! button on the left, a live audio-reactive waveform in the middle, and a
//! confirm (✓) button on the right. Rendered as a per-pixel-alpha layered
//! Win32 window with fade-in/out and press/hover animations.

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, AtomicU8, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

#[cfg(target_os = "windows")]
#[derive(Clone, Copy)]
struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    life: f32,
    color: [u8; 4],
    size: f32,
}

#[cfg(target_os = "windows")]
thread_local! {
    static PARTICLES: std::cell::RefCell<Vec<Particle>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(target_os = "windows")]
fn spawn_particle_burst(region: i32) {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let cx = if region == 1 {
        BTN_LX as f32
    } else {
        BTN_RX as f32
    };
    let cy = BTN_CY as f32;

    PARTICLES.with(|p| {
        let mut particles = p.borrow_mut();
        for _ in 0..24 {
            let angle = rng.gen_range(0.0..std::f32::consts::TAU);
            let speed = rng.gen_range(60.0..220.0);
            let size = rng.gen_range(1.5..4.0);
            let color = if region == 1 {
                let c = rng.gen_range(180..240) as u8;
                [c, c, c + 10, 255]
            } else {
                match rng.gen_range(0..3) {
                    0 => [147, 51, 234, 255],
                    1 => [236, 72, 153, 255],
                    _ => [255, 255, 255, 255],
                }
            };
            particles.push(Particle {
                x: cx,
                y: cy,
                vx: angle.cos() * speed,
                vy: angle.sin() * speed,
                life: 1.0,
                color,
                size,
            });
        }
    });
}

/// Floating button state
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum ButtonState {
    /// Idle - not recording (window hidden)
    Idle = 0,
    /// Recording in progress
    Recording = 1,
    /// Processing (waiting for ASR result)
    Processing = 2,
}

impl From<u8> for ButtonState {
    fn from(v: u8) -> Self {
        match v {
            1 => ButtonState::Recording,
            2 => ButtonState::Processing,
            _ => ButtonState::Idle,
        }
    }
}

/// Events from the floating button
#[derive(Debug, Clone)]
pub enum FloatingButtonEvent {
    /// User clicked ✓ — stop recording and keep the dictated text
    ConfirmRecording,
    /// User clicked ✗ — stop recording and discard the dictated text
    CancelRecording,
    /// User requested to exit
    Exit,
    /// User dragged the floating window to a new position
    UpdatePosition { x: i32, y: i32 },
}

/// Floating button configuration
#[derive(Clone)]
pub struct FloatingButtonConfig {
    pub initial_x: i32,
    pub initial_y: i32,
    pub size: i32,
    pub stiffness: f32,
    pub damping: f32,
}

impl Default for FloatingButtonConfig {
    fn default() -> Self {
        Self {
            initial_x: 100,
            initial_y: 100,
            size: 56,
            stiffness: 180.0,
            damping: 12.0,
        }
    }
}

/// State setter for the floating button (thread-safe)
#[derive(Clone)]
pub struct FloatingButtonStateSetter {
    state: Arc<AtomicU8>,
    hwnd: Arc<AtomicIsize>,
}

impl FloatingButtonStateSetter {
    /// Set the button state. The window thread's animation timer reads this and
    /// drives the fade/show/hide; here we only store it and nudge a repaint.
    pub fn set_state(&self, state: ButtonState) {
        self.state.store(state as u8, Ordering::SeqCst);
        #[cfg(target_os = "windows")]
        {
            let hwnd_val = self.hwnd.load(Ordering::SeqCst);
            if hwnd_val != 0 {
                unsafe {
                    use windows::Win32::Foundation::{HWND, TRUE};
                    use windows::Win32::Graphics::Gdi::InvalidateRect;
                    let _ = InvalidateRect(HWND(hwnd_val), None, TRUE);
                }
            }
        }
        tracing::debug!("Floating button state: {:?}", state);
    }

    /// Get the current state
    pub fn get_state(&self) -> ButtonState {
        self.state.load(Ordering::SeqCst).into()
    }
}

/// Floating button manager
pub struct FloatingButton {
    state: Arc<AtomicU8>,
    hwnd: Arc<AtomicIsize>,
    event_tx: Sender<FloatingButtonEvent>,
    event_rx: Option<Receiver<FloatingButtonEvent>>,
}

impl FloatingButton {
    /// Create a new floating button
    pub fn new() -> Self {
        let (event_tx, event_rx) = channel();
        Self {
            state: Arc::new(AtomicU8::new(ButtonState::Idle as u8)),
            hwnd: Arc::new(AtomicIsize::new(0)),
            event_tx,
            event_rx: Some(event_rx),
        }
    }

    /// Get a state setter that can be used from other threads
    pub fn state_setter(&self) -> FloatingButtonStateSetter {
        FloatingButtonStateSetter {
            state: self.state.clone(),
            hwnd: self.hwnd.clone(),
        }
    }

    /// Take the event receiver (can only be called once)
    pub fn take_event_receiver(&mut self) -> Option<Receiver<FloatingButtonEvent>> {
        self.event_rx.take()
    }
}

#[cfg(target_os = "windows")]
unsafe fn get_monitor_rect(
    hwnd: windows::Win32::Foundation::HWND,
) -> windows::Win32::Foundation::RECT {
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
    let mut info = MONITORINFO::default();
    info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(monitor, &mut info).as_bool() {
        info.rcMonitor
    } else {
        use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
        windows::Win32::Foundation::RECT {
            left: 0,
            top: 0,
            right: GetSystemMetrics(SM_CXSCREEN),
            bottom: GetSystemMetrics(SM_CYSCREEN),
        }
    }
}

#[cfg(target_os = "windows")]
unsafe fn get_caret_or_cursor_pos() -> Option<(i32, i32)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetCaretPos, GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId, GUITHREADINFO,
    };

    let hwnd = GetForegroundWindow();
    if hwnd.0 != 0 {
        let thread_id = GetWindowThreadProcessId(hwnd, None);
        let mut info = GUITHREADINFO::default();
        info.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
        if GetGUIThreadInfo(thread_id, &mut info).is_ok() && info.hwndCaret.0 != 0 {
            let is_empty = info.rcCaret.left == 0
                && info.rcCaret.top == 0
                && info.rcCaret.right == 0
                && info.rcCaret.bottom == 0;
            if !is_empty {
                let mut pt = POINT {
                    x: info.rcCaret.left,
                    y: info.rcCaret.bottom,
                };
                if ClientToScreen(info.hwndCaret, &mut pt).as_bool() {
                    return Some((pt.x, pt.y));
                }
            }
        }
    }

    let mut pt = POINT::default();
    if GetCaretPos(&mut pt).is_ok() {
        let hwnd_focused = GetForegroundWindow();
        if hwnd_focused.0 != 0 {
            if ClientToScreen(hwnd_focused, &mut pt).as_bool() {
                return Some((pt.x, pt.y));
            }
        }
    }

    if windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pt).is_ok() {
        return Some((pt.x, pt.y));
    }

    None
}

impl FloatingButton {
    /// Run the floating button (blocking, call from a dedicated thread)
    #[cfg(target_os = "windows")]
    pub fn run(self, config: FloatingButtonConfig) {
        use windows::core::w;
        use windows::Win32::Foundation::*;
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows::Win32::UI::HiDpi::{
            GetDpiForSystem, SetProcessDpiAwarenessContext,
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        };
        use windows::Win32::UI::WindowsAndMessaging::*;

        // Drag / press tracking (window thread only)
        static MOUSE_DOWN: AtomicBool = AtomicBool::new(false);
        static START_CURSOR_X: AtomicI32 = AtomicI32::new(0);
        static START_CURSOR_Y: AtomicI32 = AtomicI32::new(0);
        static START_WIN_X: AtomicI32 = AtomicI32::new(0);
        static START_WIN_Y: AtomicI32 = AtomicI32::new(0);
        // Animation / interaction state
        static HOVER: AtomicI32 = AtomicI32::new(0); // 0 none, 1 cancel, 2 confirm
        static PRESSED: AtomicI32 = AtomicI32::new(0); // 0 none, 1 cancel, 2 confirm
        static PRESS_MODE: AtomicI32 = AtomicI32::new(0); // 0 none, 1 drag, 2 button
                                                          // Click ripple: which button (0/1/2) and age in ticks (-1 = inactive).
        static RIPPLE_REGION: AtomicI32 = AtomicI32::new(0);
        static RIPPLE_AGE: AtomicI32 = AtomicI32::new(-1);
        // UI scale in milli (1000 = 100%), set once from the display DPI.
        static UI_SCALE: AtomicI32 = AtomicI32::new(1000);
        // Per-bar waveform peak (milli-pixels) for peak-hold + slow-decay caps.
        static PEAKS: [AtomicI32; 16] = [const { AtomicI32::new(0) }; 16];

        // Edge Docking state
        static DOCK_STATE: AtomicU8 = AtomicU8::new(0); // 0=None, 1=Left, 2=Right, 3=Top, 4=Bottom
        static IS_SLID_OUT: AtomicBool = AtomicBool::new(false);
        static DOCK_X: AtomicI32 = AtomicI32::new(0);
        static DOCK_Y: AtomicI32 = AtomicI32::new(0);

        thread_local! {
            static SHARED_STATE: std::cell::RefCell<Option<Arc<AtomicU8>>> = const { std::cell::RefCell::new(None) };
            static EVENT_SENDER: std::cell::RefCell<Option<Sender<FloatingButtonEvent>>> = const { std::cell::RefCell::new(None) };
        }

        let state = self.state.clone();
        let hwnd_store = self.hwnd.clone();
        let event_tx = self.event_tx.clone();

        SHARED_STATE.with(|s| *s.borrow_mut() = Some(state.clone()));
        EVENT_SENDER.with(|s| *s.borrow_mut() = Some(event_tx));

        /// Render the HUD into a layered window at the given opacity.
        unsafe fn update_layered_hud(
            hwnd: HWND,
            state_val: u8,
            tick: i32,
            fade: f32,   // 0.0..=1.0
            scale: f32,  // 0.0..=1.0
            pill_w: f32, // Morphing width of the pill
            pressed: i32,
            l_scale: f32,
            r_scale: f32,
            l_hover: f32,
            r_hover: f32,
            bar_heights: &[f32; 16],
        ) {
            use windows::Win32::Foundation::*;
            use windows::Win32::Graphics::Gdi::*;
            use windows::Win32::UI::WindowsAndMessaging::*;

            // Render natively at physical resolution for zero softening:
            // s = display scale * spring-in. All base coords go through px/py/sz.
            let dpi_scale = (UI_SCALE.load(Ordering::SeqCst) as f32 / 1000.0).max(0.5);
            let s = (dpi_scale * scale).max(0.01);
            let pw = (WIN_W as f32 * dpi_scale).round().max(1.0) as i32;
            let ph = (WIN_H as f32 * dpi_scale).round().max(1.0) as i32;
            // Centre the (spring-shrunk) content within the fixed physical window.
            let off_x = (pw as f32 - WIN_W as f32 * s) * 0.5;
            let off_y = (ph as f32 - WIN_H as f32 * s) * 0.5;
            let mut rgba = vec![0u8; (pw * ph * 4) as usize];
            // base-space -> physical-pixel transforms
            let px = |v: f32| (off_x + v * s).round() as i32;
            let py = |v: f32| (off_y + v * s).round() as i32;
            let sz = |v: f32| ((v * s).round() as i32).max(1);

            fn blend_pixel(
                buf: &mut [u8],
                width: i32,
                height: i32,
                x: i32,
                y: i32,
                color: [u8; 4],
            ) {
                if x < 0 || y < 0 || x >= width || y >= height || color[3] == 0 {
                    return;
                }
                let idx = ((y * width + x) * 4) as usize;
                let src_a = color[3] as u32;
                let dst_a = buf[idx + 3] as u32;
                let out_a = src_a + (dst_a * (255 - src_a) / 255);
                for channel in 0..3 {
                    let src = color[channel] as u32;
                    let dst = buf[idx + channel] as u32;
                    let out = if out_a == 0 {
                        0
                    } else {
                        (src * src_a + dst * dst_a * (255 - src_a) / 255) / out_a
                    };
                    buf[idx + channel] = out.min(255) as u8;
                }
                buf[idx + 3] = out_a.min(255) as u8;
            }

            // Anti-aliased filled circle (1px feathered edge).
            fn draw_circle(
                buf: &mut [u8],
                width: i32,
                height: i32,
                cx: i32,
                cy: i32,
                radius: i32,
                color: [u8; 4],
            ) {
                if radius <= 0 {
                    return;
                }
                let rf = radius as f32;
                for y in (cy - radius - 1).max(0)..=(cy + radius + 1).min(height - 1) {
                    for x in (cx - radius - 1).max(0)..=(cx + radius + 1).min(width - 1) {
                        let dx = (x - cx) as f32;
                        let dy = (y - cy) as f32;
                        let d = (dx * dx + dy * dy).sqrt();
                        let cov = (rf + 0.5 - d).clamp(0.0, 1.0);
                        if cov > 0.0 {
                            let mut c = color;
                            c[3] = (color[3] as f32 * cov) as u8;
                            blend_pixel(buf, width, height, x, y, c);
                        }
                    }
                }
            }

            // Rounded rect with anti-aliased corners; straight edges stay crisp.
            fn draw_rounded_rect(
                buf: &mut [u8],
                width: i32,
                height: i32,
                x: i32,
                y: i32,
                rect_w: i32,
                rect_h: i32,
                radius: i32,
                color: [u8; 4],
            ) {
                if rect_w <= 0 || rect_h <= 0 {
                    return;
                }
                let left = x.max(0);
                let top = y.max(0);
                let right = (x + rect_w).min(width);
                let bottom = (y + rect_h).min(height);
                let r = radius
                    .max(0)
                    .min((rect_w - 1).max(0) / 2)
                    .min((rect_h - 1).max(0) / 2);
                let rf = r as f32;
                let clamp_left = x + r;
                let clamp_right = x + rect_w - r - 1;
                let clamp_top = y + r;
                let clamp_bottom = y + rect_h - r - 1;

                for py in top..bottom {
                    for px in left..right {
                        let cov = if r == 0 || clamp_left > clamp_right || clamp_top > clamp_bottom
                        {
                            1.0
                        } else {
                            let cx = px.clamp(clamp_left, clamp_right);
                            let cy = py.clamp(clamp_top, clamp_bottom);
                            let dx = (px - cx) as f32;
                            let dy = (py - cy) as f32;
                            if dx == 0.0 && dy == 0.0 {
                                1.0
                            } else {
                                let d = (dx * dx + dy * dy).sqrt();
                                (rf + 0.5 - d).clamp(0.0, 1.0)
                            }
                        };
                        if cov > 0.0 {
                            let mut c = color;
                            c[3] = (color[3] as f32 * cov) as u8;
                            blend_pixel(buf, width, height, px, py, c);
                        }
                    }
                }
            }

            fn draw_line(
                buf: &mut [u8],
                width: i32,
                height: i32,
                x1: i32,
                y1: i32,
                x2: i32,
                y2: i32,
                thickness: i32,
                color: [u8; 4],
            ) {
                let dx = x2 - x1;
                let dy = y2 - y1;
                let steps = dx.abs().max(dy.abs()).max(1);
                for step in 0..=steps {
                    let x = x1 + dx * step / steps;
                    let y = y1 + dy * step / steps;
                    draw_circle(buf, width, height, x, y, thickness, color);
                }
            }

            // Anti-aliased hollow ring (used by the click ripple).
            fn draw_ring(
                buf: &mut [u8],
                width: i32,
                height: i32,
                cx: i32,
                cy: i32,
                radius: i32,
                thickness: i32,
                color: [u8; 4],
            ) {
                if radius <= 0 {
                    return;
                }
                let rf = radius as f32;
                let tf = thickness as f32;
                for y in (cy - radius - 2).max(0)..=(cy + radius + 2).min(height - 1) {
                    for x in (cx - radius - 2).max(0)..=(cx + radius + 2).min(width - 1) {
                        let dx = (x - cx) as f32;
                        let dy = (y - cy) as f32;
                        let d = (dx * dx + dy * dy).sqrt();
                        let cov = (tf * 0.5 + 0.5 - (d - rf).abs()).clamp(0.0, 1.0);
                        if cov > 0.0 {
                            let mut c = color;
                            c[3] = (color[3] as f32 * cov) as u8;
                            blend_pixel(buf, width, height, x, y, c);
                        }
                    }
                }
            }

            fn draw_rounded_rect_border_gradient(
                buf: &mut [u8],
                width: i32,
                height: i32,
                x: i32,
                y: i32,
                rect_w: i32,
                rect_h: i32,
                radius: i32,
                thickness: f32,
                tick: i32,
                state: ButtonState,
                fade: f32,
            ) {
                if rect_w <= 0 || rect_h <= 0 {
                    return;
                }
                let left = x.max(0);
                let top = y.max(0);
                let right = (x + rect_w).min(width);
                let bottom = (y + rect_h).min(height);
                let r = radius
                    .max(0)
                    .min((rect_w - 1).max(0) / 2)
                    .min((rect_h - 1).max(0) / 2);
                let rf = r as f32;
                let clamp_left = x + r;
                let clamp_right = x + rect_w - r - 1;
                let clamp_top = y + r;
                let clamp_bottom = y + rect_h - r - 1;
                if clamp_left > clamp_right || clamp_top > clamp_bottom {
                    return;
                }

                let cx_center = (x + rect_w / 2) as f32;
                let cy_center = (y + rect_h / 2) as f32;

                let angle_offset = match state {
                    ButtonState::Processing => tick as f32 * 0.08,
                    _ => tick as f32 * 0.02,
                };

                let breath = if state == ButtonState::Idle {
                    0.5 + 0.5 * (tick as f32 * 0.05).sin()
                } else {
                    1.0
                };

                let half_t = thickness * 0.5;

                for py in top..bottom {
                    for px in left..right {
                        let cx = px.clamp(clamp_left, clamp_right);
                        let cy = py.clamp(clamp_top, clamp_bottom);
                        let dx = (px - cx) as f32;
                        let dy = (py - cy) as f32;

                        let dist = if dx == 0.0 && dy == 0.0 {
                            let dist_to_left = (px - clamp_left).abs();
                            let dist_to_right = (px - clamp_right).abs();
                            let dist_to_top = (py - clamp_top).abs();
                            let dist_to_bottom = (py - clamp_bottom).abs();
                            -(dist_to_left
                                .min(dist_to_right)
                                .min(dist_to_top)
                                .min(dist_to_bottom)) as f32
                        } else {
                            (dx * dx + dy * dy).sqrt() - rf
                        };

                        let dist_from_border = dist.abs();
                        let cov = (half_t + 0.5 - dist_from_border).clamp(0.0, 1.0);

                        if cov > 0.0 {
                            let angle =
                                (py as f32 - cy_center).atan2(px as f32 - cx_center) + angle_offset;
                            let t = (angle.sin() + 1.0) * 0.5;

                            let (c1, c2) = if state == ButtonState::Processing {
                                ([59, 130, 246], [147, 51, 234])
                            } else {
                                ([147, 51, 234], [236, 72, 153])
                            };

                            let r_col = (c1[0] as f32 * (1.0 - t) + c2[0] as f32 * t) as u8;
                            let g_col = (c1[1] as f32 * (1.0 - t) + c2[1] as f32 * t) as u8;
                            let b_col = (c1[2] as f32 * (1.0 - t) + c2[2] as f32 * t) as u8;

                            let alpha = (200.0 * cov * breath * fade) as u8;

                            blend_pixel(buf, width, height, px, py, [r_col, g_col, b_col, alpha]);
                        }
                    }
                }
            }

            let state = ButtonState::from(state_val);
            let processing = state == ButtonState::Processing;

            // Subtle upward drift while appearing (base px).
            let oy = (1.0 - fade).max(0.0) * 6.0;
            let py0 = PILL_Y as f32 + oy;
            let cyf = BTN_CY as f32 + oy;

            // Dynamically center the morphed pill
            let pill_x = (WIN_W as f32 - pill_w) / 2.0;

            // Soft drop shadow (stacked low-alpha passes).
            for (off, al) in [(7.0f32, 16u8), (5.0, 16), (3.0, 14)] {
                draw_rounded_rect(
                    &mut rgba,
                    pw,
                    ph,
                    px(pill_x),
                    py(py0 + off),
                    sz(pill_w),
                    sz(PILL_H as f32),
                    sz(PILL_R as f32),
                    [0, 0, 0, al],
                );
            }
            // Pill body + subtle top gloss.
            draw_rounded_rect(
                &mut rgba,
                pw,
                ph,
                px(pill_x),
                py(py0),
                sz(pill_w),
                sz(PILL_H as f32),
                sz(PILL_R as f32),
                [20, 20, 22, 245],
            );
            draw_rounded_rect(
                &mut rgba,
                pw,
                ph,
                px(pill_x + 2.0),
                py(py0 + 2.0),
                sz(pill_w - 4.0),
                sz(PILL_H as f32 / 2.0),
                sz(PILL_R as f32 - 2.0),
                [255, 255, 255, 9],
            );

            // Rotating color gradient streamer border
            draw_rounded_rect_border_gradient(
                &mut rgba,
                pw,
                ph,
                px(pill_x),
                py(py0),
                sz(pill_w),
                sz(PILL_H as f32),
                sz(PILL_R as f32),
                sz(1.5) as f32,
                tick,
                state,
                fade,
            );

            if processing {
                // Rotating comet spinner while waiting for the final ASR result.
                let scx = WIN_W as f32 / 2.0;
                let sr = 11.0f32;
                let dots = 12;
                for i in 0..dots {
                    let ang = (i as f32 / dots as f32) * std::f32::consts::TAU - tick as f32 * 0.18;
                    let dx = scx + sr * ang.cos();
                    let dy = cyf + sr * ang.sin();
                    let a = (30 + i * 210 / dots) as u8;
                    // Neon aura glow circle
                    draw_circle(
                        &mut rgba,
                        pw,
                        ph,
                        px(dx),
                        py(dy),
                        sz(4.0),
                        [150, 190, 250, a / 5],
                    );
                    // Core dot
                    draw_circle(
                        &mut rgba,
                        pw,
                        ph,
                        px(dx),
                        py(dy),
                        sz(2.0),
                        [150, 190, 250, a],
                    );
                }
            } else {
                // Expansion ratio of the pill: buttons and wave fade out as it shrinks
                let expansion = ((pill_w - 48.0) / 152.0).clamp(0.0, 1.0);

                // Live waveform between the two buttons.
                let n_bars = 16;
                let bar_w = 3.0f32;
                let pitch = (WAVE_R - WAVE_L) as f32 / n_bars as f32;
                let x0 = WAVE_L as f32
                    + ((WAVE_R - WAVE_L) as f32 - (n_bars as f32 - 1.0) * pitch - bar_w) / 2.0;
                for i in 0..n_bars {
                    let xb = x0 + i as f32 * pitch;
                    let h = bar_heights[i as usize];

                    // Peak-hold + slow decay — classic analyser cap.
                    let hm = (h * 1000.0) as i32;
                    let pcur = PEAKS[i as usize].load(Ordering::SeqCst);
                    let pnew = if hm > pcur {
                        hm
                    } else {
                        (pcur - PEAK_DECAY).max(hm)
                    };
                    PEAKS[i as usize].store(pnew, Ordering::SeqCst);
                    let peak_h = pnew as f32 / 1000.0;

                    // Neon Gradient math: from purple [147, 51, 234] to pink [236, 72, 153]
                    let t = i as f32 / (n_bars - 1) as f32;
                    let r = (147.0 * (1.0 - t) + 236.0 * t) as u8;
                    let g = (51.0 * (1.0 - t) + 72.0 * t) as u8;
                    let b = (234.0 * (1.0 - t) + 153.0 * t) as u8;

                    // Wave color (dynamic alpha based on volume level and expansion)
                    let alpha = ((170.0 + h * 2.0).min(255.0) * expansion) as u8;
                    let color = [r, g, b, alpha];

                    // Draw drop shadow for bars (1px offset)
                    draw_rounded_rect(
                        &mut rgba,
                        pw,
                        ph,
                        px(xb + 1.0),
                        py(cyf - h / 2.0 + 1.0),
                        sz(bar_w),
                        sz(h),
                        sz(bar_w / 2.0),
                        [0, 0, 0, (80.0 * expansion) as u8],
                    );

                    // Draw neon glow (wider, very low alpha)
                    draw_rounded_rect(
                        &mut rgba,
                        pw,
                        ph,
                        px(xb - 2.0),
                        py(cyf - h / 2.0 - 2.0),
                        sz(bar_w + 4.0),
                        sz(h + 4.0),
                        sz((bar_w + 4.0) / 2.0),
                        [r, g, b, alpha / 6],
                    );

                    // Draw neon colored bars
                    draw_rounded_rect(
                        &mut rgba,
                        pw,
                        ph,
                        px(xb),
                        py(cyf - h / 2.0),
                        sz(bar_w),
                        sz(h),
                        sz(bar_w / 2.0),
                        color,
                    );

                    // Falling peak cap, only when it floats clearly above the bar.
                    if peak_h > h + 1.5 {
                        let peak_alpha = (((peak_h - h) * 40.0).min(210.0) * expansion) as u8;
                        draw_rounded_rect(
                            &mut rgba,
                            pw,
                            ph,
                            px(xb),
                            py(cyf - peak_h / 2.0 - 1.0),
                            sz(bar_w),
                            sz(2.0),
                            sz(1.0),
                            [255, 255, 255, peak_alpha],
                        );
                    }
                }

                // Left: cancel (✗) — gray circle, light X. Spring scale + eased hover tint.
                let l_pressed = pressed == 1;
                let lr = BTN_R as f32 * l_scale;
                let lerp = |a: i32, b: i32| (a as f32 + (b - a) as f32 * l_hover) as u8;
                let lc = if l_pressed {
                    [46, 46, 50]
                } else {
                    [lerp(60, 86), lerp(60, 86), lerp(64, 92)]
                };
                let btn_alpha = (255.0 * expansion) as u8;
                let icon_alpha = (236.0 * expansion) as u8;
                draw_circle(
                    &mut rgba,
                    pw,
                    ph,
                    px(BTN_LX as f32),
                    py(cyf),
                    sz(lr),
                    [lc[0], lc[1], lc[2], btn_alpha],
                );
                draw_line(
                    &mut rgba,
                    pw,
                    ph,
                    px(BTN_LX as f32 - 5.0),
                    py(cyf - 5.0),
                    px(BTN_LX as f32 + 5.0),
                    py(cyf + 5.0),
                    sz(1.0),
                    [230, 232, 237, icon_alpha],
                );
                draw_line(
                    &mut rgba,
                    pw,
                    ph,
                    px(BTN_LX as f32 + 5.0),
                    py(cyf - 5.0),
                    px(BTN_LX as f32 - 5.0),
                    py(cyf + 5.0),
                    sz(1.0),
                    [230, 232, 237, icon_alpha],
                );

                // Right: confirm (✓) — white circle, dark check, eased hover ring.
                let r_pressed = pressed == 2;
                let rr = BTN_R as f32 * r_scale;
                if r_hover > 0.01 {
                    let ring_alpha = (60.0 * r_hover * expansion) as u8;
                    draw_circle(
                        &mut rgba,
                        pw,
                        ph,
                        px(BTN_RX as f32),
                        py(cyf),
                        sz(rr + 2.0),
                        [255, 255, 255, ring_alpha],
                    );
                }
                let rc = if r_pressed {
                    [226, 226, 224]
                } else {
                    [248, 248, 246]
                };
                let r_btn_alpha = (255.0 * expansion) as u8;
                let r_icon_alpha = (245.0 * expansion) as u8;
                draw_circle(
                    &mut rgba,
                    pw,
                    ph,
                    px(BTN_RX as f32),
                    py(cyf),
                    sz(rr),
                    [rc[0], rc[1], rc[2], r_btn_alpha],
                );
                draw_line(
                    &mut rgba,
                    pw,
                    ph,
                    px(BTN_RX as f32 - 6.0),
                    py(cyf),
                    px(BTN_RX as f32 - 2.0),
                    py(cyf + 5.0),
                    sz(1.0),
                    [24, 24, 26, r_icon_alpha],
                );
                draw_line(
                    &mut rgba,
                    pw,
                    ph,
                    px(BTN_RX as f32 - 2.0),
                    py(cyf + 5.0),
                    px(BTN_RX as f32 + 7.0),
                    py(cyf - 6.0),
                    sz(1.0),
                    [24, 24, 26, r_icon_alpha],
                );
            }

            // Click ripple, drawn over whatever state is active.
            let ripple_age = RIPPLE_AGE.load(Ordering::SeqCst);
            let ripple_region = RIPPLE_REGION.load(Ordering::SeqCst);
            if ripple_age >= 0 && ripple_region != 0 {
                let rcx = if ripple_region == 1 {
                    BTN_LX as f32
                } else {
                    BTN_RX as f32
                };
                let prog = ripple_age as f32 / RIPPLE_MAX as f32;
                let rad = BTN_R as f32 + prog * 16.0;
                let a = ((1.0 - prog) * 110.0) as u8;
                let col = if ripple_region == 1 {
                    [200, 202, 208, a]
                } else {
                    [255, 255, 255, a]
                };
                draw_ring(&mut rgba, pw, ph, px(rcx), py(cyf), sz(rad), sz(2.0), col);
            }

            // Draw active particles
            PARTICLES.with(|p| {
                let particles = p.borrow();
                for part in particles.iter() {
                    if part.life <= 0.0 {
                        continue;
                    }
                    let mut col = part.color;
                    col[3] = (part.color[3] as f32 * part.life * fade).clamp(0.0, 255.0) as u8;
                    draw_circle(
                        &mut rgba,
                        pw,
                        ph,
                        px(part.x),
                        py(part.y),
                        sz(part.size),
                        col,
                    );
                }
            });

            // Push to the layered window, scaling all alpha by `fade`.
            let hdc_screen = GetDC(HWND::default());
            let hdc_mem = CreateCompatibleDC(hdc_screen);
            // 1:1 copy of the natively-rendered buffer (premultiplied, faded).
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: pw,
                    biHeight: -ph,
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
                    let fade_u32 = (fade * 255.0).clamp(0.0, 255.0) as u32;

                    for pixel in 0..(pw * ph) as usize {
                        let sidx = pixel * 4;
                        let a = (rgba[sidx + 3] as u32 * fade_u32 / 255).min(255);
                        // premultiplied BGRA
                        *pixel_data.add(sidx) = ((rgba[sidx + 2] as u32 * a) / 255) as u8;
                        *pixel_data.add(sidx + 1) = ((rgba[sidx + 1] as u32 * a) / 255) as u8;
                        *pixel_data.add(sidx + 2) = ((rgba[sidx] as u32 * a) / 255) as u8;
                        *pixel_data.add(sidx + 3) = a as u8;
                    }

                    let blend = BLENDFUNCTION {
                        BlendOp: 0,
                        BlendFlags: 0,
                        SourceConstantAlpha: 255,
                        AlphaFormat: 1,
                    };
                    let size = SIZE { cx: pw, cy: ph };
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

        unsafe extern "system" fn wnd_proc(
            hwnd: HWND,
            msg: u32,
            wparam: WPARAM,
            lparam: LPARAM,
        ) -> LRESULT {
            use windows::Win32::Foundation::*;
            use windows::Win32::Graphics::Gdi::*;
            use windows::Win32::UI::Input::KeyboardAndMouse::{
                GetAsyncKeyState, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
            };
            use windows::Win32::UI::WindowsAndMessaging::*;

            const WM_CREATE: u32 = 0x0001;
            const WM_DESTROY: u32 = 0x0002;
            const WM_ERASEBKGND: u32 = 0x0014;
            const WM_PAINT: u32 = 0x000F;
            const WM_TIMER: u32 = 0x0113;
            const WM_MOUSEMOVE: u32 = 0x0200;
            const WM_LBUTTONDOWN: u32 = 0x0201;
            const WM_RBUTTONUP: u32 = 0x0205;
            const WM_MOUSELEAVE: u32 = 0x02A3;
            const WM_DPICHANGED: u32 = 0x02E0;
            const DRAG_TIMER_ID: usize = 1;
            const ANIMATION_TIMER_ID: usize = 2;

            // Extract signed client coords from an LPARAM (lo=x, hi=y).
            fn lparam_xy(lparam: LPARAM) -> (i32, i32) {
                let x = (lparam.0 & 0xFFFF) as u16 as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
                (x, y)
            }

            fn read_state() -> u8 {
                SHARED_STATE.with(|s| {
                    s.borrow()
                        .as_ref()
                        .map(|st| st.load(Ordering::SeqCst))
                        .unwrap_or(0)
                })
            }

            // Physical->base coordinate divisor (matches the render upscale).
            fn ui_scale() -> f32 {
                (UI_SCALE.load(Ordering::SeqCst) as f32 / 1000.0).max(0.5)
            }

            match msg {
                WM_CREATE => {
                    update_layered_hud(
                        hwnd, 0, 0, 0.0, 0.0, 48.0, 0, 1.0, 1.0, 0.0, 0.0, &[3.0; 16],
                    );
                    let _ = ShowWindow(hwnd, SW_HIDE);
                    LRESULT(0)
                }
                WM_ERASEBKGND => LRESULT(1),
                WM_PAINT => {
                    let mut ps = PAINTSTRUCT::default();
                    let _ = BeginPaint(hwnd, &mut ps);
                    let _ = EndPaint(hwnd, &ps);
                    LRESULT(0)
                }
                WM_TIMER if wparam.0 == DRAG_TIMER_ID && MOUSE_DOWN.load(Ordering::SeqCst) => {
                    let still_down = (GetAsyncKeyState(0x01) & (0x8000u16 as i16)) != 0;
                    let mut pt = POINT::default();
                    let _ = GetCursorPos(&mut pt);

                    if !still_down {
                        // Release: only one handler finalizes.
                        if MOUSE_DOWN.swap(false, Ordering::SeqCst) {
                            let _ = KillTimer(hwnd, DRAG_TIMER_ID);
                            let mode = PRESS_MODE.swap(0, Ordering::SeqCst);
                            let pressed_region = PRESSED.swap(0, Ordering::SeqCst);
                            if mode == 1 {
                                let mut rect = RECT::default();
                                if GetWindowRect(hwnd, &mut rect).is_ok() {
                                    // Check docking
                                    let monitor_rect = get_monitor_rect(hwnd);
                                    let win_w = rect.right - rect.left;
                                    let win_h = rect.bottom - rect.top;

                                    let dist_left = (rect.left - monitor_rect.left).abs();
                                    let dist_right = (rect.right - monitor_rect.right).abs();
                                    let dist_top = (rect.top - monitor_rect.top).abs();
                                    let dist_bottom = (rect.bottom - monitor_rect.bottom).abs();

                                    let threshold = 40; // Snap distance threshold
                                    let mut dock = 0; // 0=None, 1=Left, 2=Right, 3=Top, 4=Bottom

                                    let mut base_x = rect.left;
                                    let mut base_y = rect.top;

                                    if dist_left < threshold {
                                        dock = 1;
                                        base_x = monitor_rect.left;
                                    } else if dist_right < threshold {
                                        dock = 2;
                                        base_x = monitor_rect.right - win_w;
                                    } else if dist_top < threshold {
                                        dock = 3;
                                        base_y = monitor_rect.top;
                                    } else if dist_bottom < threshold {
                                        dock = 4;
                                        base_y = monitor_rect.bottom - win_h;
                                    }

                                    DOCK_STATE.store(dock, Ordering::SeqCst);
                                    DOCK_X.store(base_x, Ordering::SeqCst);
                                    DOCK_Y.store(base_y, Ordering::SeqCst);
                                    IS_SLID_OUT.store(true, Ordering::SeqCst); // Keep it visible when just dropped

                                    EVENT_SENDER.with(|s| {
                                        if let Some(ref tx) = *s.borrow() {
                                            let _ = tx.send(FloatingButtonEvent::UpdatePosition {
                                                x: base_x,
                                                y: base_y,
                                            });
                                        }
                                    });
                                }
                            }
                            if mode == 2 {
                                let mut rect = RECT::default();
                                let _ = GetWindowRect(hwnd, &mut rect);
                                let s = ui_scale();
                                let region = hit_region(
                                    ((pt.x - rect.left) as f32 / s) as i32,
                                    ((pt.y - rect.top) as f32 / s) as i32,
                                );
                                if region != 0
                                    && region == pressed_region
                                    && ButtonState::from(read_state()) == ButtonState::Recording
                                {
                                    EVENT_SENDER.with(|s| {
                                        if let Some(ref tx) = *s.borrow() {
                                            let ev = if region == 1 {
                                                FloatingButtonEvent::CancelRecording
                                            } else {
                                                FloatingButtonEvent::ConfirmRecording
                                            };
                                            let _ = tx.send(ev);
                                        }
                                    });
                                    // Kick off the click ripple from this button.
                                    RIPPLE_REGION.store(region, Ordering::SeqCst);
                                    RIPPLE_AGE.store(0, Ordering::SeqCst);
                                    spawn_particle_burst(region);
                                }
                            }
                            let _ = InvalidateRect(hwnd, None, FALSE);
                        }
                    } else {
                        let dx = pt.x - START_CURSOR_X.load(Ordering::SeqCst);
                        let dy = pt.y - START_CURSOR_Y.load(Ordering::SeqCst);
                        // A press that drags far enough becomes a window move.
                        if PRESS_MODE.load(Ordering::SeqCst) == 2 && (dx.abs() > 8 || dy.abs() > 8)
                        {
                            PRESS_MODE.store(1, Ordering::SeqCst);
                            PRESSED.store(0, Ordering::SeqCst);
                            let _ = InvalidateRect(hwnd, None, FALSE);
                        }
                        if PRESS_MODE.load(Ordering::SeqCst) == 1 {
                            let new_x = START_WIN_X.load(Ordering::SeqCst) + dx;
                            let new_y = START_WIN_Y.load(Ordering::SeqCst) + dy;
                            let _ = SetWindowPos(
                                hwnd,
                                HWND_TOPMOST,
                                new_x,
                                new_y,
                                0,
                                0,
                                SWP_NOSIZE | SWP_NOZORDER,
                            );
                        }
                    }
                    LRESULT(0)
                }
                WM_TIMER => LRESULT(0),
                WM_MOUSEMOVE => {
                    let dock_val = DOCK_STATE.load(Ordering::SeqCst);
                    if dock_val != 0 {
                        IS_SLID_OUT.store(true, Ordering::SeqCst);
                    }

                    let mut tme = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    let _ = unsafe { TrackMouseEvent(&mut tme) };

                    if !MOUSE_DOWN.load(Ordering::SeqCst) {
                        let (x, y) = lparam_xy(lparam);
                        let s = ui_scale();
                        let region = if ButtonState::from(read_state()) == ButtonState::Recording {
                            hit_region((x as f32 / s) as i32, (y as f32 / s) as i32)
                        } else {
                            0
                        };
                        if HOVER.swap(region, Ordering::SeqCst) != region {
                            let _ = InvalidateRect(hwnd, None, FALSE);
                        }
                    }
                    LRESULT(0)
                }
                WM_MOUSELEAVE => {
                    let dock_val = DOCK_STATE.load(Ordering::SeqCst);
                    if dock_val != 0 {
                        IS_SLID_OUT.store(false, Ordering::SeqCst);
                    }
                    if HOVER.swap(0, Ordering::SeqCst) != 0 {
                        let _ = InvalidateRect(hwnd, None, FALSE);
                    }
                    LRESULT(0)
                }
                WM_LBUTTONDOWN => {
                    MOUSE_DOWN.store(true, Ordering::SeqCst);

                    let mut pt = POINT::default();
                    let _ = GetCursorPos(&mut pt);
                    START_CURSOR_X.store(pt.x, Ordering::SeqCst);
                    START_CURSOR_Y.store(pt.y, Ordering::SeqCst);

                    let mut rect = RECT::default();
                    let _ = GetWindowRect(hwnd, &mut rect);
                    START_WIN_X.store(rect.left, Ordering::SeqCst);
                    START_WIN_Y.store(rect.top, Ordering::SeqCst);

                    let (x, y) = lparam_xy(lparam);
                    let s = ui_scale();
                    let region = hit_region((x as f32 / s) as i32, (y as f32 / s) as i32);
                    if region != 0 && ButtonState::from(read_state()) == ButtonState::Recording {
                        PRESS_MODE.store(2, Ordering::SeqCst);
                        PRESSED.store(region, Ordering::SeqCst);
                    } else {
                        PRESS_MODE.store(1, Ordering::SeqCst);
                        PRESSED.store(0, Ordering::SeqCst);
                        // Clear dock state when starting to drag
                        DOCK_STATE.store(0, Ordering::SeqCst);
                    }

                    let _ = SetTimer(hwnd, DRAG_TIMER_ID, 16, None);
                    let _ = InvalidateRect(hwnd, None, FALSE);
                    LRESULT(0)
                }
                WM_RBUTTONUP => {
                    use windows::core::w;
                    use windows::Win32::UI::WindowsAndMessaging::{
                        MessageBoxW, IDYES, MB_ICONQUESTION, MB_YESNO,
                    };
                    let result = MessageBoxW(
                        hwnd,
                        w!("确定要退出 AikoIME 吗？"),
                        w!("退出确认"),
                        MB_YESNO | MB_ICONQUESTION,
                    );
                    if result == IDYES {
                        EVENT_SENDER.with(|s| {
                            if let Some(ref tx) = *s.borrow() {
                                let _ = tx.send(FloatingButtonEvent::Exit);
                            }
                        });
                        let _ = DestroyWindow(hwnd);
                    }
                    LRESULT(0)
                }
                WM_DPICHANGED => {
                    // Moved to a monitor with a different scale: resize to match and
                    // jump to the suggested rectangle Windows provides.
                    let new_dpi = (wparam.0 & 0xFFFF) as i32;
                    if new_dpi > 0 {
                        UI_SCALE.store((new_dpi * 1000 / 96).max(500), Ordering::SeqCst);
                        let s = new_dpi as f32 / 96.0;
                        let nw = (WIN_W as f32 * s).round() as i32;
                        let nh = (WIN_H as f32 * s).round() as i32;
                        let prc = lparam.0 as *const RECT;
                        if !prc.is_null() {
                            let r = *prc;
                            let _ = SetWindowPos(
                                hwnd,
                                HWND_TOPMOST,
                                r.left,
                                r.top,
                                nw,
                                nh,
                                SWP_NOZORDER | SWP_NOACTIVATE,
                            );
                        } else {
                            let _ = SetWindowPos(
                                hwnd,
                                HWND_TOPMOST,
                                0,
                                0,
                                nw,
                                nh,
                                SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOMOVE,
                            );
                        }
                        let _ = InvalidateRect(hwnd, None, FALSE);
                    }
                    LRESULT(0)
                }
                WM_DESTROY => {
                    let _ = KillTimer(hwnd, DRAG_TIMER_ID);
                    let _ = KillTimer(hwnd, ANIMATION_TIMER_ID);
                    PostQuitMessage(0);
                    LRESULT(0)
                }
                _ => DefWindowProcW(hwnd, msg, wparam, lparam),
            }
        }

        unsafe {
            let inst = match GetModuleHandleW(None) {
                Ok(h) => h,
                Err(e) => {
                    tracing::error!("GetModuleHandleW failed: {:?}", e);
                    return;
                }
            };

            let cls = w!("AikoFloatingButton");
            let cursor = LoadCursorW(None, IDC_HAND)
                .unwrap_or_else(|_| LoadCursorW(None, IDC_ARROW).unwrap_or_default());

            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(wnd_proc),
                hInstance: inst.into(),
                hCursor: cursor,
                lpszClassName: cls,
                ..Default::default()
            };
            RegisterClassExW(&wc);

            // Per-monitor DPI awareness so the HUD renders crisp on the real pixel
            // grid. If this fails (already set elsewhere), GetDpiForSystem returns 96
            // and we simply behave as a 100% window.
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let dpi = GetDpiForSystem() as i32;
            UI_SCALE.store((dpi * 1000 / 96).max(500), Ordering::SeqCst);
            let scale = dpi as f32 / 96.0;
            let win_w = (WIN_W as f32 * scale).round() as i32;
            let win_h = (WIN_H as f32 * scale).round() as i32;

            let default_position = config.initial_x == 100 && config.initial_y == 100;
            let screen_w = GetSystemMetrics(SM_CXSCREEN);
            let screen_h = GetSystemMetrics(SM_CYSCREEN);
            let initial_x = if default_position {
                (screen_w - win_w) / 2
            } else {
                config.initial_x
            };
            let initial_y = if default_position {
                screen_h - win_h - (96.0 * scale) as i32
            } else {
                config.initial_y
            };

            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                cls,
                w!("AikoIME"),
                WS_POPUP,
                initial_x,
                initial_y,
                win_w,
                win_h,
                HWND::default(),
                HMENU::default(),
                inst,
                None,
            );

            if hwnd.0 == 0 {
                tracing::error!("CreateWindowExW failed");
                return;
            }

            let _ = ShowWindow(hwnd, SW_HIDE);

            hwnd_store.store(hwnd.0, Ordering::SeqCst);
            tracing::info!("Floating button window created");

            // Define Spring helper
            struct Spring {
                value: f32,
                velocity: f32,
                target: f32,
                stiffness: f32,
                damping: f32,
            }
            impl Spring {
                fn new(val: f32, stiffness: f32, damping: f32) -> Self {
                    Self {
                        value: val,
                        velocity: 0.0,
                        target: val,
                        stiffness,
                        damping,
                    }
                }
                fn update(&mut self, dt: f32) {
                    let dt = dt.min(0.1); // Avoid instability on huge frame drops
                    let force =
                        -self.stiffness * (self.value - self.target) - self.damping * self.velocity;
                    self.velocity += force * dt;
                    self.value += self.velocity * dt;
                }
            }

            let mut scale_spring = Spring::new(0.0, config.stiffness, config.damping); // window pop scale
            let mut fade_spring = Spring::new(0.0, config.stiffness, config.damping + 3.0); // opacity (slightly more damped)
            let mut width_spring = Spring::new(200.0, config.stiffness, config.damping); // pill width morphing
            let mut l_scale_spring =
                Spring::new(1.0, config.stiffness + 40.0, config.damping + 2.0); // left button scale
            let mut r_scale_spring =
                Spring::new(1.0, config.stiffness + 40.0, config.damping + 2.0); // right button scale
            let mut l_hover_spring = Spring::new(0.0, config.stiffness, config.damping + 3.0); // left hover opacity
            let mut r_hover_spring = Spring::new(0.0, config.stiffness, config.damping + 3.0); // right hover opacity
            let mut bar_springs: Vec<Spring> = (0..16)
                .map(|_| Spring::new(3.0, config.stiffness - 40.0, config.damping - 2.0))
                .collect();
            let mut dock_slide_spring = Spring::new(1.0, config.stiffness, config.damping); // 1.0 = fully slid out

            let mut msg = MSG::default();
            let mut last_time = std::time::Instant::now();
            let mut tick = 0i32;
            let mut agc_peak: f32 = 0.1; // dynamic vocal peak for audio AGC
            let mut last_state = ButtonState::Idle;

            while msg.message != WM_QUIT {
                // 1. Process Win32 messages
                use windows::Win32::Graphics::Dwm::DwmFlush;
                use windows::Win32::UI::WindowsAndMessaging::{
                    DispatchMessageW, PeekMessageW, TranslateMessage, PM_REMOVE,
                };

                while PeekMessageW(&mut msg, HWND::default(), 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }

                let state_val = state.load(Ordering::SeqCst);
                let state_enum = ButtonState::from(state_val);

                // Caret position tracking
                if last_state == ButtonState::Idle && state_enum == ButtonState::Recording {
                    if let Some((cx, cy)) = get_caret_or_cursor_pos() {
                        let dpi = GetDpiForSystem() as i32;
                        let scale = dpi as f32 / 96.0;
                        let win_w = (WIN_W as f32 * scale).round() as i32;
                        let win_h = (WIN_H as f32 * scale).round() as i32;
                        let screen_w = GetSystemMetrics(SM_CXSCREEN);
                        let screen_h = GetSystemMetrics(SM_CYSCREEN);
                        let target_x = (cx - win_w / 2).clamp(0, screen_w - win_w);
                        let target_y = (cy + 10).clamp(0, screen_h - win_h);
                        let _ = SetWindowPos(
                            hwnd,
                            HWND_TOPMOST,
                            target_x,
                            target_y,
                            0,
                            0,
                            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                        );
                    }
                }
                last_state = state_enum;

                // 2. Wasting prevention: when completely idle, sleep
                let is_idle = state_enum == ButtonState::Idle && fade_spring.value < 0.01;
                if is_idle {
                    std::thread::sleep(std::time::Duration::from_millis(30));
                    last_time = std::time::Instant::now(); // Reset time to avoid dt spike

                    // Reset springs to 0 when idle so they don't drift
                    scale_spring.value = 0.0;
                    scale_spring.velocity = 0.0;
                    scale_spring.target = 0.0;

                    fade_spring.value = 0.0;
                    fade_spring.velocity = 0.0;
                    fade_spring.target = 0.0;

                    width_spring.value = 48.0;
                    width_spring.velocity = 0.0;
                    width_spring.target = 48.0;
                    continue;
                }

                // 3. Calculate dt
                let now = std::time::Instant::now();
                let dt = now.duration_since(last_time).as_secs_f32();
                last_time = now;

                tick = tick.wrapping_add(1);

                // 4. Update spring targets
                let is_active = state_enum != ButtonState::Idle;

                // Window pop scale target
                scale_spring.target = if is_active { 1.0 } else { 0.7 };
                fade_spring.target = if is_active { 1.0 } else { 0.0 };

                // Dock slide target
                let dock_val = DOCK_STATE.load(Ordering::SeqCst);
                if dock_val != 0 {
                    let slid_out = IS_SLID_OUT.load(Ordering::SeqCst);
                    dock_slide_spring.target = if slid_out { 1.0 } else { 0.0 };
                } else {
                    dock_slide_spring.target = 1.0;
                }

                // Pill width morphing target
                width_spring.target = if is_active {
                    if state_enum == ButtonState::Processing {
                        48.0
                    } else {
                        200.0
                    }
                } else {
                    48.0
                };

                // Get hover/press statics from wnd_proc
                let hov = HOVER.load(Ordering::SeqCst);
                let prs = PRESSED.load(Ordering::SeqCst);
                let recording = state_enum == ButtonState::Recording;

                // Left button scale target
                l_scale_spring.target = if !recording {
                    1.0
                } else if prs == 1 {
                    0.88
                } else if hov == 1 {
                    1.09
                } else {
                    1.0
                };

                // Right button scale target
                r_scale_spring.target = if !recording {
                    1.0
                } else if prs == 2 {
                    0.88
                } else if hov == 2 {
                    1.09
                } else {
                    1.0
                };

                // Hover color targets
                l_hover_spring.target = if recording && hov == 1 { 1.0 } else { 0.0 };
                r_hover_spring.target = if recording && hov == 2 { 1.0 } else { 0.0 };

                // Audio input normalization (AGC)
                let raw_volume = crate::audio::INPUT_LEVEL.load(Ordering::Relaxed) as f32 / 1000.0;
                if raw_volume > agc_peak {
                    agc_peak = raw_volume;
                } else {
                    agc_peak = (agc_peak - 0.05 * dt).max(0.1);
                }
                let volume = if agc_peak > 0.01 {
                    (raw_volume / agc_peak).clamp(0.0, 1.0)
                } else {
                    0.0
                };

                // Soundwave bars targets (Pseudo-spectral analysis)
                for i in 0..16 {
                    let envelope = if i < 8 {
                        0.4 + 0.6 * (i as f32 / 7.0)
                    } else {
                        1.0 - 0.6 * ((i - 8) as f32 / 7.0)
                    };
                    let noise1 = (i as f32 * 0.65 + tick as f32 * 0.18).sin().abs();
                    let noise2 = (i as f32 * 1.45 - tick as f32 * 0.35).cos().abs();
                    let noise = 0.4 * noise1 + 0.6 * noise2;
                    let target_h = (3.0 + volume * 38.0 * envelope * noise).clamp(3.0, 45.0);
                    bar_springs[i].target = target_h;
                }

                // 5. Solve physics
                scale_spring.update(dt);
                fade_spring.update(dt);
                width_spring.update(dt);
                l_scale_spring.update(dt);
                r_scale_spring.update(dt);
                l_hover_spring.update(dt);
                r_hover_spring.update(dt);
                dock_slide_spring.update(dt);

                let mut bar_heights = [0.0f32; 16];
                for i in 0..16 {
                    bar_springs[i].update(dt);
                    bar_heights[i] = bar_springs[i].value;
                }

                // Apply Edge Docking movement
                if dock_val != 0 && !MOUSE_DOWN.load(Ordering::SeqCst) {
                    let base_x = DOCK_X.load(Ordering::SeqCst);
                    let base_y = DOCK_Y.load(Ordering::SeqCst);
                    let dpi = GetDpiForSystem() as i32;
                    let scale = dpi as f32 / 96.0;
                    let win_w = (WIN_W as f32 * scale).round() as i32;
                    let win_h = (WIN_H as f32 * scale).round() as i32;
                    let monitor_rect = get_monitor_rect(hwnd);

                    let sliver = (8.0 * scale).round() as i32;
                    let slide_val = dock_slide_spring.value;

                    let (x, y) = match dock_val {
                        1 => {
                            // Left
                            let target_x = monitor_rect.left
                                - ((1.0 - slide_val) * (win_w - sliver) as f32).round() as i32;
                            (target_x, base_y)
                        }
                        2 => {
                            // Right
                            let target_x = monitor_rect.right
                                - sliver
                                - (slide_val * (win_w - sliver) as f32).round() as i32;
                            (target_x, base_y)
                        }
                        3 => {
                            // Top
                            let target_y = monitor_rect.top
                                - ((1.0 - slide_val) * (win_h - sliver) as f32).round() as i32;
                            (base_x, target_y)
                        }
                        4 => {
                            // Bottom
                            let target_y = monitor_rect.bottom
                                - sliver
                                - (slide_val * (win_h - sliver) as f32).round() as i32;
                            (base_x, target_y)
                        }
                        _ => (base_x, base_y),
                    };

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

                // Age the click ripple (matching previous logic)
                let ra = RIPPLE_AGE.load(Ordering::SeqCst);
                if ra >= 0 {
                    if ra + 1 > RIPPLE_MAX {
                        RIPPLE_AGE.store(-1, Ordering::SeqCst);
                        RIPPLE_REGION.store(0, Ordering::SeqCst);
                    } else {
                        RIPPLE_AGE.store(ra + 1, Ordering::SeqCst);
                    }
                }

                // Solve particle physics
                PARTICLES.with(|p| {
                    let mut particles = p.borrow_mut();
                    for part in particles.iter_mut() {
                        part.x += part.vx * dt;
                        part.y += part.vy * dt;
                        part.vx *= 0.92f32.powf(dt * 60.0);
                        part.vy *= 0.92f32.powf(dt * 60.0);
                        part.life -= dt * 2.0;
                    }
                    particles.retain(|part| part.life > 0.0);
                });

                // 6. Render before the first show so DWM never presents an
                // uninitialized layered surface as a white rectangle.
                let visible = IsWindowVisible(hwnd).as_bool();
                if is_active && !visible {
                    let mut bar_heights_clone = [3.0f32; 16];
                    bar_heights_clone.copy_from_slice(&bar_heights);
                    update_layered_hud(
                        hwnd,
                        state_val,
                        tick,
                        fade_spring.value,
                        scale_spring.value,
                        width_spring.value,
                        prs,
                        l_scale_spring.value,
                        r_scale_spring.value,
                        l_hover_spring.value,
                        r_hover_spring.value,
                        &bar_heights_clone,
                    );
                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                } else if !is_active && visible && fade_spring.value < 0.01 {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                } else if visible {
                    let mut bar_heights_clone = [3.0f32; 16];
                    bar_heights_clone.copy_from_slice(&bar_heights);

                    update_layered_hud(
                        hwnd,
                        state_val,
                        tick,
                        fade_spring.value,
                        scale_spring.value,
                        width_spring.value,
                        prs,
                        l_scale_spring.value,
                        r_scale_spring.value,
                        l_hover_spring.value,
                        r_hover_spring.value,
                        &bar_heights_clone,
                    );
                }

                // 7. Wait for V-Sync
                if DwmFlush().is_err() {
                    std::thread::sleep(std::time::Duration::from_millis(8));
                }
            }

            tracing::info!("Floating button window closed");
        }
    }

    #[cfg(not(target_os = "windows"))]
    pub fn run(self, _config: FloatingButtonConfig) {
        tracing::warn!("Floating button not supported on this platform");
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
}

// ---- Layout (shared by drawing and hit-testing) ----
#[cfg(target_os = "windows")]
const WIN_W: i32 = 220;
#[cfg(target_os = "windows")]
const WIN_H: i32 = 72;
#[cfg(target_os = "windows")]
const PILL_X: i32 = 10;
#[cfg(target_os = "windows")]
const PILL_Y: i32 = 8;
#[cfg(target_os = "windows")]
const PILL_W: i32 = 200;
#[cfg(target_os = "windows")]
const PILL_H: i32 = 48;
#[cfg(target_os = "windows")]
const PILL_R: i32 = 24;
#[cfg(target_os = "windows")]
const BTN_CY: i32 = PILL_Y + PILL_H / 2; // 32
#[cfg(target_os = "windows")]
const BTN_LX: i32 = PILL_X + 26; // 36
#[cfg(target_os = "windows")]
const BTN_RX: i32 = PILL_X + PILL_W - 26; // 184
#[cfg(target_os = "windows")]
const BTN_R: i32 = 16; // button visual radius
#[cfg(target_os = "windows")]
const HIT_R: i32 = 22; // button click radius
#[cfg(target_os = "windows")]
const WAVE_L: i32 = BTN_LX + 26; // 62
#[cfg(target_os = "windows")]
const WAVE_R: i32 = BTN_RX - 26; // 158
/// Lifetime (animation ticks) of the click ripple.
#[cfg(target_os = "windows")]
const RIPPLE_MAX: i32 = 16;
/// Milli-pixels the waveform peak cap falls per frame.
#[cfg(target_os = "windows")]
const PEAK_DECAY: i32 = 600;

/// Map client coords to a button: 1 = cancel (left), 2 = confirm (right), 0 = none.
#[cfg(target_os = "windows")]
fn hit_region(x: i32, y: i32) -> i32 {
    let dl = (x - BTN_LX).pow(2) + (y - BTN_CY).pow(2);
    if dl <= HIT_R * HIT_R {
        return 1;
    }
    let dr = (x - BTN_RX).pow(2) + (y - BTN_CY).pow(2);
    if dr <= HIT_R * HIT_R {
        return 2;
    }
    0
}
