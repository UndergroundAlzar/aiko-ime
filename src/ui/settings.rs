//! Native Windows settings center for Aiko IME.

use anyhow::Result;

/// Open the settings center on its own UI thread.
///
/// The function is intentionally non-blocking so it can be called directly
/// from the tray event loop. Repeated calls focus the existing window.
#[cfg(target_os = "windows")]
pub fn open_settings_window() -> Result<()> {
    windows_settings::open()
}

#[cfg(not(target_os = "windows"))]
pub fn open_settings_window() -> Result<()> {
    anyhow::bail!("Aiko IME settings are only available on Windows")
}

/// Short alias suitable for tray menu callbacks.
pub fn open_settings() -> Result<()> {
    open_settings_window()
}

#[cfg(target_os = "windows")]
mod windows_settings {
    use crate::asr::{AsrClient, DeviceCredentials};
    use crate::data::AppConfig;
    use crate::offline::{ModelManager, SherpaOnnxConfig};
    use anyhow::{bail, Context, Result};
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::SampleFormat;
    use std::collections::HashMap;
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        CreateFontW, GetStockObject, UpdateWindow, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS,
        COLOR_WINDOW, DEFAULT_CHARSET, DEFAULT_GUI_FONT, DEFAULT_PITCH, FW_NORMAL,
        OUT_DEFAULT_PRECIS,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
    use windows::Win32::UI::WindowsAndMessaging::*;

    const CLASS_NAME: &str = "AikoImeSettingsWindow";
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    const PAGE_GENERAL: usize = 0;
    const PAGE_RECOGNITION: usize = 1;
    const PAGE_VOCABULARY: usize = 2;
    const PAGE_AI: usize = 3;

    const ID_PAGE_GENERAL: i32 = 100;
    const ID_PAGE_RECOGNITION: i32 = 101;
    const ID_PAGE_VOCABULARY: i32 = 102;
    const ID_PAGE_AI: i32 = 103;
    const ID_SAVE: i32 = 110;
    const ID_CANCEL: i32 = 111;
    const ID_TEST_MIC: i32 = 112;
    const ID_CLEAR_HISTORY: i32 = 113;
    const ID_HOTKEY_MODE: i32 = 114;
    const ID_ASR_BACKEND: i32 = 115;
    const ID_TEST_RECOGNITION: i32 = 116;

    static SETTINGS_OPEN: AtomicBool = AtomicBool::new(false);
    static SETTINGS_HWND: AtomicIsize = AtomicIsize::new(0);
    static SETTINGS_FONT: AtomicIsize = AtomicIsize::new(0);

    #[derive(Default)]
    struct Controls {
        page_title: HWND,
        status: HWND,
        microphone: HWND,
        hotkey_mode: HWND,
        combo_key: HWND,
        double_tap_key: HWND,
        double_tap_interval: HWND,
        auto_start: HWND,
        floating_button: HWND,
        desktop_pet: HWND,
        desktop_pet_size: HWND,
        language: HWND,
        asr_backend: HWND,
        online_provider: HWND,
        offline_provider: HWND,
        offline_model_dir: HWND,
        vad_enabled: HWND,
        history_enabled: HWND,
        history_path: HWND,
        vocabulary: HWND,
        ai_enabled: HWND,
        ai_endpoint: HWND,
        ai_key: HWND,
        ai_model: HWND,
        post_process_enabled: HWND,
        post_process_prompt: HWND,
        translation_enabled: HWND,
        translation_prompt: HWND,
    }

    struct WindowState {
        config: AppConfig,
        controls: Controls,
        pages: [Vec<HWND>; 4],
        microphone_names: Vec<String>,
        current_page: usize,
    }

    impl WindowState {
        fn new(config: AppConfig) -> Self {
            Self {
                config,
                controls: Controls::default(),
                pages: std::array::from_fn(|_| Vec::new()),
                microphone_names: enumerate_microphones(),
                current_page: PAGE_GENERAL,
            }
        }
    }

    pub(super) fn open() -> Result<()> {
        if SETTINGS_OPEN.swap(true, Ordering::SeqCst) {
            let hwnd = HWND(SETTINGS_HWND.load(Ordering::SeqCst));
            if hwnd.0 != 0 {
                unsafe {
                    let _ = ShowWindow(hwnd, SW_RESTORE);
                    let _ = SetForegroundWindow(hwnd);
                }
            }
            return Ok(());
        }

        let config = match AppConfig::load_or_default() {
            Ok(config) => config,
            Err(error) => {
                SETTINGS_OPEN.store(false, Ordering::SeqCst);
                return Err(error);
            }
        };

        std::thread::Builder::new()
            .name("aiko-settings".to_string())
            .spawn(move || {
                if let Err(error) = run_window(config) {
                    tracing::error!("Settings window failed: {error:#}");
                    show_message(
                        HWND(0),
                        &format!("设置窗口无法打开：\n{error:#}"),
                        "Aiko IME",
                        MB_OK | MB_ICONERROR,
                    );
                }
                SETTINGS_HWND.store(0, Ordering::SeqCst);
                SETTINGS_OPEN.store(false, Ordering::SeqCst);
            })
            .context("failed to start settings UI thread")?;

        Ok(())
    }

    fn run_window(config: AppConfig) -> Result<()> {
        unsafe {
            let instance = GetModuleHandleW(None)?;
            let class_name = wide(CLASS_NAME);
            let cursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();
            let window_class = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(window_proc),
                hInstance: instance.into(),
                hCursor: cursor,
                hbrBackground: windows::Win32::Graphics::Gdi::HBRUSH((COLOR_WINDOW.0 + 1) as isize),
                lpszClassName: PCWSTR(class_name.as_ptr()),
                ..Default::default()
            };
            let _ = RegisterClassW(&window_class);

            let mut state = Box::new(WindowState::new(config));
            let title = wide("Aiko IME 设置中心 / Settings");
            let hwnd = CreateWindowExW(
                WS_EX_APPWINDOW,
                PCWSTR(class_name.as_ptr()),
                PCWSTR(title.as_ptr()),
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                960,
                740,
                None,
                None,
                instance,
                Some(state.as_mut() as *mut WindowState as *const std::ffi::c_void),
            );
            if hwnd.0 == 0 {
                bail!("CreateWindowExW returned null");
            }

            SETTINGS_HWND.store(hwnd.0, Ordering::SeqCst);
            center_window(hwnd);
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = UpdateWindow(hwnd);

            let mut message = MSG::default();
            while GetMessageW(&mut message, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        Ok(())
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_CREATE => {
                let create = lparam.0 as *const CREATESTRUCTW;
                if create.is_null() {
                    return LRESULT(-1);
                }
                let state = (*create).lpCreateParams as *mut WindowState;
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize);
                if let Err(error) = create_controls(hwnd, &mut *state) {
                    tracing::error!("Failed to create settings controls: {error:#}");
                    return LRESULT(-1);
                }
                LRESULT(0)
            }
            WM_COMMAND => {
                let id = (wparam.0 & 0xffff) as i32;
                let notification = ((wparam.0 >> 16) & 0xffff) as u32;
                let state = state_mut(hwnd);
                if state.is_null() {
                    return DefWindowProcW(hwnd, message, wparam, lparam);
                }
                match id {
                    ID_PAGE_GENERAL => show_page(&mut *state, PAGE_GENERAL),
                    ID_PAGE_RECOGNITION => show_page(&mut *state, PAGE_RECOGNITION),
                    ID_PAGE_VOCABULARY => show_page(&mut *state, PAGE_VOCABULARY),
                    ID_PAGE_AI => show_page(&mut *state, PAGE_AI),
                    ID_CANCEL => {
                        let _ = DestroyWindow(hwnd);
                    }
                    ID_SAVE => save_from_form(hwnd, &mut *state),
                    ID_TEST_MIC => test_selected_microphone(hwnd, &*state),
                    ID_TEST_RECOGNITION => test_recognition_backend(hwnd, &*state),
                    ID_CLEAR_HISTORY => clear_history(hwnd, &*state),
                    ID_HOTKEY_MODE if notification == CBN_SELCHANGE => {
                        update_hotkey_controls(&*state)
                    }
                    ID_ASR_BACKEND if notification == CBN_SELCHANGE => update_asr_controls(&*state),
                    _ => {}
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                SETTINGS_HWND.store(0, Ordering::SeqCst);
                SETTINGS_OPEN.store(false, Ordering::SeqCst);
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }

    unsafe fn state_mut(hwnd: HWND) -> *mut WindowState {
        GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState
    }

    unsafe fn create_controls(hwnd: HWND, state: &mut WindowState) -> Result<()> {
        let instance: HINSTANCE = GetModuleHandleW(None)?.into();

        create_control(
            instance,
            hwnd,
            "STATIC",
            "Aiko IME",
            24,
            18,
            130,
            28,
            WS_CHILD | WS_VISIBLE,
            0,
            0,
        );
        create_control(
            instance,
            hwnd,
            "STATIC",
            "Aiko 语音助手",
            24,
            46,
            145,
            30,
            WS_CHILD | WS_VISIBLE,
            0,
            0,
        );

        for (id, label, y) in [
            (ID_PAGE_GENERAL, "常规 / General", 92),
            (ID_PAGE_RECOGNITION, "识别与隐私 / ASR", 134),
            (ID_PAGE_VOCABULARY, "词典 / Vocabulary", 176),
            (ID_PAGE_AI, "AI 设置 / AI", 218),
        ] {
            create_control(
                instance,
                hwnd,
                "BUTTON",
                label,
                20,
                y,
                140,
                34,
                WS_CHILD | WS_VISIBLE | WS_TABSTOP,
                0,
                id,
            );
        }

        state.controls.page_title = create_control(
            instance,
            hwnd,
            "STATIC",
            "常规 / General",
            185,
            18,
            720,
            28,
            WS_CHILD | WS_VISIBLE,
            0,
            0,
        );
        state.controls.status = create_control(
            instance,
            hwnd,
            "STATIC",
            "更改保存后，部分运行中组件会在下次启动或录音时生效。",
            185,
            648,
            520,
            30,
            WS_CHILD | WS_VISIBLE,
            0,
            0,
        );
        create_control(
            instance,
            hwnd,
            "BUTTON",
            "保存 / Save",
            710,
            642,
            105,
            34,
            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
            0,
            ID_SAVE,
        );
        create_control(
            instance,
            hwnd,
            "BUTTON",
            "取消 / Cancel",
            820,
            642,
            105,
            34,
            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
            0,
            ID_CANCEL,
        );

        create_general_page(instance, hwnd, state);
        create_recognition_page(instance, hwnd, state);
        create_vocabulary_page(instance, hwnd, state);
        create_ai_page(instance, hwnd, state);
        populate_form(state);
        show_page(state, PAGE_GENERAL);
        Ok(())
    }

    unsafe fn create_general_page(instance: HINSTANCE, hwnd: HWND, state: &mut WindowState) {
        add_group(
            instance,
            hwnd,
            state,
            PAGE_GENERAL,
            "音频输入 / Audio Input",
            180,
            52,
            740,
            108,
        );
        add_label(
            instance,
            hwnd,
            state,
            PAGE_GENERAL,
            "麦克风 / Mic",
            202,
            84,
            150,
            22,
        );
        state.controls.microphone =
            add_combo(instance, hwnd, state, PAGE_GENERAL, 352, 80, 405, 180, 0);
        add_page_control(
            instance,
            hwnd,
            state,
            PAGE_GENERAL,
            "BUTTON",
            "测试 / Test",
            770,
            79,
            125,
            29,
            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
            0,
            ID_TEST_MIC,
        );
        add_label(
            instance,
            hwnd,
            state,
            PAGE_GENERAL,
            "“系统默认”会跟随 Windows 当前默认输入设备。",
            352,
            116,
            500,
            22,
        );

        add_group(
            instance,
            hwnd,
            state,
            PAGE_GENERAL,
            "快捷键 / Hotkey",
            180,
            174,
            740,
            190,
        );
        add_label(
            instance,
            hwnd,
            state,
            PAGE_GENERAL,
            "模式 / Mode",
            202,
            208,
            145,
            22,
        );
        state.controls.hotkey_mode = add_combo(
            instance,
            hwnd,
            state,
            PAGE_GENERAL,
            352,
            204,
            190,
            120,
            ID_HOTKEY_MODE,
        );
        add_label(
            instance,
            hwnd,
            state,
            PAGE_GENERAL,
            "组合键 / Combo",
            202,
            248,
            145,
            22,
        );
        state.controls.combo_key =
            add_edit(instance, hwnd, state, PAGE_GENERAL, 352, 244, 190, 26, 0);
        add_label(
            instance,
            hwnd,
            state,
            PAGE_GENERAL,
            "双击键 / Double tap",
            574,
            208,
            155,
            22,
        );
        state.controls.double_tap_key =
            add_combo(instance, hwnd, state, PAGE_GENERAL, 730, 204, 165, 130, 0);
        add_label(
            instance,
            hwnd,
            state,
            PAGE_GENERAL,
            "间隔 / Interval (ms)",
            574,
            248,
            155,
            22,
        );
        state.controls.double_tap_interval =
            add_edit(instance, hwnd, state, PAGE_GENERAL, 730, 244, 165, 26, 0);
        add_label(
            instance,
            hwnd,
            state,
            PAGE_GENERAL,
            "组合键示例：Ctrl+Shift+V；更改全局热键后请重启 Aiko IME。",
            202,
            292,
            650,
            24,
        );

        add_group(
            instance,
            hwnd,
            state,
            PAGE_GENERAL,
            "界面 / Appearance",
            180,
            378,
            740,
            142,
        );
        state.controls.floating_button = add_checkbox(
            instance,
            hwnd,
            state,
            PAGE_GENERAL,
            "显示悬浮条 / Show floating control",
            202,
            412,
            300,
            24,
        );
        state.controls.desktop_pet = add_checkbox(
            instance,
            hwnd,
            state,
            PAGE_GENERAL,
            "显示桌宠 / Show desktop pet",
            202,
            452,
            300,
            24,
        );
        add_label(
            instance,
            hwnd,
            state,
            PAGE_GENERAL,
            "桌宠尺寸 / Pet size",
            574,
            414,
            155,
            22,
        );
        state.controls.desktop_pet_size =
            add_edit(instance, hwnd, state, PAGE_GENERAL, 730, 410, 100, 26, 0);
        add_label(
            instance,
            hwnd,
            state,
            PAGE_GENERAL,
            "96 - 320 px",
            838,
            414,
            70,
            22,
        );

        add_group(
            instance,
            hwnd,
            state,
            PAGE_GENERAL,
            "系统 / System",
            180,
            534,
            740,
            82,
        );
        state.controls.auto_start = add_checkbox(
            instance,
            hwnd,
            state,
            PAGE_GENERAL,
            "登录 Windows 时自动启动 / Launch at sign-in",
            202,
            566,
            360,
            24,
        );
        add_label(
            instance,
            hwnd,
            state,
            PAGE_GENERAL,
            "语言 / Language",
            574,
            566,
            125,
            22,
        );
        state.controls.language =
            add_edit(instance, hwnd, state, PAGE_GENERAL, 700, 562, 195, 26, 0);
    }

    unsafe fn create_recognition_page(instance: HINSTANCE, hwnd: HWND, state: &mut WindowState) {
        add_group(
            instance,
            hwnd,
            state,
            PAGE_RECOGNITION,
            "语音识别 / Speech Recognition",
            180,
            52,
            740,
            280,
        );
        add_label(
            instance,
            hwnd,
            state,
            PAGE_RECOGNITION,
            "后端 / Backend",
            202,
            88,
            160,
            22,
        );
        state.controls.asr_backend = add_combo(
            instance,
            hwnd,
            state,
            PAGE_RECOGNITION,
            365,
            84,
            250,
            120,
            ID_ASR_BACKEND,
        );
        add_page_control(
            instance,
            hwnd,
            state,
            PAGE_RECOGNITION,
            "BUTTON",
            "测试后端 / Test",
            740,
            83,
            150,
            30,
            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
            0,
            ID_TEST_RECOGNITION,
        );
        add_label(
            instance,
            hwnd,
            state,
            PAGE_RECOGNITION,
            "在线服务 / Online provider",
            202,
            132,
            160,
            22,
        );
        state.controls.online_provider = add_edit(
            instance,
            hwnd,
            state,
            PAGE_RECOGNITION,
            365,
            128,
            250,
            26,
            0,
        );
        add_label(
            instance,
            hwnd,
            state,
            PAGE_RECOGNITION,
            "当前支持：doubao",
            640,
            132,
            250,
            22,
        );
        add_label(
            instance,
            hwnd,
            state,
            PAGE_RECOGNITION,
            "离线引擎 / Offline provider",
            202,
            176,
            160,
            22,
        );
        state.controls.offline_provider = add_edit(
            instance,
            hwnd,
            state,
            PAGE_RECOGNITION,
            365,
            172,
            250,
            26,
            0,
        );
        add_label(
            instance,
            hwnd,
            state,
            PAGE_RECOGNITION,
            "当前支持：sherpa_onnx",
            640,
            176,
            250,
            22,
        );
        add_label(
            instance,
            hwnd,
            state,
            PAGE_RECOGNITION,
            "模型目录 / Model folder",
            202,
            220,
            160,
            22,
        );
        state.controls.offline_model_dir = add_edit(
            instance,
            hwnd,
            state,
            PAGE_RECOGNITION,
            365,
            216,
            525,
            26,
            0,
        );
        state.controls.vad_enabled = add_checkbox(
            instance,
            hwnd,
            state,
            PAGE_RECOGNITION,
            "启用语音活动检测 VAD / Voice activity detection",
            202,
            260,
            430,
            24,
        );
        add_label(
            instance,
            hwnd,
            state,
            PAGE_RECOGNITION,
            "离线模式不上传音频，需要先准备兼容的本地模型。",
            202,
            296,
            600,
            22,
        );

        add_group(
            instance,
            hwnd,
            state,
            PAGE_RECOGNITION,
            "隐私与历史 / Privacy & History",
            180,
            350,
            740,
            220,
        );
        state.controls.history_enabled = add_checkbox(
            instance,
            hwnd,
            state,
            PAGE_RECOGNITION,
            "保存本地听写历史（默认关闭）/ Save local history (off by default)",
            202,
            386,
            560,
            24,
        );
        add_label(
            instance,
            hwnd,
            state,
            PAGE_RECOGNITION,
            "历史文件 / History file",
            202,
            430,
            160,
            22,
        );
        state.controls.history_path = add_edit(
            instance,
            hwnd,
            state,
            PAGE_RECOGNITION,
            365,
            426,
            525,
            26,
            0,
        );
        add_page_control(
            instance,
            hwnd,
            state,
            PAGE_RECOGNITION,
            "BUTTON",
            "清除历史 / Clear",
            365,
            474,
            150,
            30,
            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
            0,
            ID_CLEAR_HISTORY,
        );
        add_label(
            instance,
            hwnd,
            state,
            PAGE_RECOGNITION,
            "Aiko 只会在该开关开启时写入识别文本；清除操作不可撤销。",
            530,
            478,
            360,
            40,
        );
    }

    unsafe fn create_vocabulary_page(instance: HINSTANCE, hwnd: HWND, state: &mut WindowState) {
        add_label(
            instance,
            hwnd,
            state,
            PAGE_VOCABULARY,
            "每行一条“识别文本=替换文本”。空行和以 # 开头的行会被忽略。",
            185,
            58,
            720,
            24,
        );
        add_label(
            instance,
            hwnd,
            state,
            PAGE_VOCABULARY,
            "One entry per line: recognized text=replacement text.",
            185,
            84,
            720,
            24,
        );
        state.controls.vocabulary = add_page_control(
            instance,
            hwnd,
            state,
            PAGE_VOCABULARY,
            "EDIT",
            "",
            185,
            120,
            725,
            490,
            WS_CHILD
                | WS_VISIBLE
                | WS_TABSTOP
                | WS_VSCROLL
                | WINDOW_STYLE((ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN) as u32),
            WS_EX_CLIENTEDGE.0,
            0,
        );
    }

    unsafe fn create_ai_page(instance: HINSTANCE, hwnd: HWND, state: &mut WindowState) {
        state.controls.ai_enabled = add_checkbox(
            instance,
            hwnd,
            state,
            PAGE_AI,
            "启用 AI 后处理 / Enable AI post-processing",
            185,
            58,
            420,
            24,
        );
        add_label(
            instance,
            hwnd,
            state,
            PAGE_AI,
            "API Endpoint",
            185,
            98,
            140,
            22,
        );
        state.controls.ai_endpoint = add_edit(instance, hwnd, state, PAGE_AI, 330, 94, 580, 26, 0);
        add_label(instance, hwnd, state, PAGE_AI, "API Key", 185, 138, 140, 22);
        state.controls.ai_key = add_edit(
            instance,
            hwnd,
            state,
            PAGE_AI,
            330,
            134,
            580,
            26,
            ES_PASSWORD,
        );
        add_label(
            instance,
            hwnd,
            state,
            PAGE_AI,
            "模型 / Model",
            185,
            178,
            140,
            22,
        );
        state.controls.ai_model = add_edit(instance, hwnd, state, PAGE_AI, 330, 174, 580, 26, 0);
        state.controls.post_process_enabled = add_checkbox(
            instance,
            hwnd,
            state,
            PAGE_AI,
            "语句润色 / Correct and format",
            185,
            218,
            330,
            24,
        );
        state.controls.post_process_prompt = add_page_control(
            instance,
            hwnd,
            state,
            PAGE_AI,
            "EDIT",
            "",
            185,
            250,
            725,
            120,
            WS_CHILD
                | WS_VISIBLE
                | WS_TABSTOP
                | WS_VSCROLL
                | WINDOW_STYLE((ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN) as u32),
            WS_EX_CLIENTEDGE.0,
            0,
        );
        state.controls.translation_enabled = add_checkbox(
            instance,
            hwnd,
            state,
            PAGE_AI,
            "自动翻译 / Automatic translation",
            185,
            390,
            330,
            24,
        );
        state.controls.translation_prompt = add_page_control(
            instance,
            hwnd,
            state,
            PAGE_AI,
            "EDIT",
            "",
            185,
            422,
            725,
            120,
            WS_CHILD
                | WS_VISIBLE
                | WS_TABSTOP
                | WS_VSCROLL
                | WINDOW_STYLE((ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN) as u32),
            WS_EX_CLIENTEDGE.0,
            0,
        );
        add_label(
            instance,
            hwnd,
            state,
            PAGE_AI,
            "API Key 会保存在本机 config.toml。使用共享电脑时请谨慎开启。",
            185,
            558,
            650,
            30,
        );
    }

    unsafe fn populate_form(state: &mut WindowState) {
        combo_add(state.controls.microphone, "系统默认 / Windows default");
        let mut selected_microphone = 0;
        for (index, name) in state.microphone_names.iter().enumerate() {
            combo_add(state.controls.microphone, name);
            if *name == state.config.audio.input_device {
                selected_microphone = index + 1;
            }
        }
        combo_select(state.controls.microphone, selected_microphone);

        for value in ["双击 / Double tap", "组合键 / Combo"] {
            combo_add(state.controls.hotkey_mode, value);
        }
        combo_select(
            state.controls.hotkey_mode,
            usize::from(state.config.hotkey.mode == "combo"),
        );
        set_text(state.controls.combo_key, &state.config.hotkey.combo_key);

        for value in ["Ctrl", "Shift", "Alt", "CapsLock"] {
            combo_add(state.controls.double_tap_key, value);
        }
        let double_tap_selection = ["Ctrl", "Shift", "Alt", "CapsLock"]
            .iter()
            .position(|value| value.eq_ignore_ascii_case(&state.config.hotkey.double_tap_key))
            .unwrap_or(0);
        combo_select(state.controls.double_tap_key, double_tap_selection);
        set_text(
            state.controls.double_tap_interval,
            &state.config.hotkey.double_tap_interval.to_string(),
        );

        set_checked(state.controls.auto_start, state.config.general.auto_start);
        set_checked(
            state.controls.floating_button,
            state.config.floating_button.enabled,
        );
        set_checked(state.controls.desktop_pet, state.config.desktop_pet.enabled);
        set_text(
            state.controls.desktop_pet_size,
            &state.config.desktop_pet.size.to_string(),
        );
        set_text(state.controls.language, &state.config.general.language);

        for value in ["在线 / Online", "离线 / Offline"] {
            combo_add(state.controls.asr_backend, value);
        }
        combo_select(
            state.controls.asr_backend,
            usize::from(state.config.asr.backend == "offline"),
        );
        set_text(
            state.controls.online_provider,
            &state.config.asr.online_provider,
        );
        set_text(
            state.controls.offline_provider,
            &state.config.asr.offline_provider,
        );
        set_text(
            state.controls.offline_model_dir,
            &state.config.asr.offline_model_dir,
        );
        set_checked(state.controls.vad_enabled, state.config.asr.vad_enabled);
        set_checked(
            state.controls.history_enabled,
            state.config.general.history_log_enabled,
        );
        set_text(
            state.controls.history_path,
            &state.config.general.history_log_path,
        );

        let mut vocabulary: Vec<_> = state.config.custom_vocabulary.iter().collect();
        vocabulary.sort_by(|left, right| left.0.cmp(right.0));
        let vocabulary = vocabulary
            .into_iter()
            .map(|(source, replacement)| format!("{source}={replacement}"))
            .collect::<Vec<_>>()
            .join("\r\n");
        set_text(state.controls.vocabulary, &vocabulary);

        set_checked(state.controls.ai_enabled, state.config.ai.enabled);
        set_text(state.controls.ai_endpoint, &state.config.ai.api_endpoint);
        set_text(state.controls.ai_key, &state.config.ai.api_key);
        set_text(state.controls.ai_model, &state.config.ai.model);
        set_checked(
            state.controls.post_process_enabled,
            state.config.ai.post_process_enabled,
        );
        set_text(
            state.controls.post_process_prompt,
            &state.config.ai.post_process_prompt,
        );
        set_checked(
            state.controls.translation_enabled,
            state.config.ai.translation_enabled,
        );
        set_text(
            state.controls.translation_prompt,
            &state.config.ai.translation_prompt,
        );

        update_hotkey_controls(state);
        update_asr_controls(state);
    }

    unsafe fn save_from_form(hwnd: HWND, state: &mut WindowState) {
        match read_config(state).and_then(|config| {
            set_auto_start(config.general.auto_start)?;
            config.save()?;
            Ok(config)
        }) {
            Ok(config) => {
                state.config = config;
                set_text(
                    state.controls.status,
                    "已保存。热键和部分界面设置将在重启后完全生效。",
                );
                show_message(
                    hwnd,
                    "设置已保存。\nSettings saved successfully.",
                    "Aiko IME",
                    MB_OK | MB_ICONINFORMATION,
                );
            }
            Err(error) => {
                set_text(state.controls.status, "保存失败，请检查高亮页面中的输入。");
                show_message(
                    hwnd,
                    &format!("无法保存设置：\n{error:#}"),
                    "Aiko IME",
                    MB_OK | MB_ICONERROR,
                );
            }
        }
    }

    unsafe fn read_config(state: &WindowState) -> Result<AppConfig> {
        let mut config = state.config.clone();

        let microphone_index = combo_selection(state.controls.microphone);
        config.audio.input_device = if microphone_index == 0 {
            String::new()
        } else {
            state
                .microphone_names
                .get(microphone_index.saturating_sub(1))
                .cloned()
                .unwrap_or_default()
        };

        config.hotkey.mode = if combo_selection(state.controls.hotkey_mode) == 1 {
            "combo"
        } else {
            "double_tap"
        }
        .to_string();
        config.hotkey.combo_key = get_text(state.controls.combo_key).trim().to_string();
        config.hotkey.double_tap_key = ["Ctrl", "Shift", "Alt", "CapsLock"]
            .get(combo_selection(state.controls.double_tap_key))
            .unwrap_or(&"Ctrl")
            .to_string();
        config.hotkey.double_tap_interval = parse_field::<u64>(
            state.controls.double_tap_interval,
            "双击间隔 / Double-tap interval",
        )?;

        config.general.auto_start = is_checked(state.controls.auto_start);
        config.general.language = get_text(state.controls.language).trim().to_string();
        config.floating_button.enabled = is_checked(state.controls.floating_button);
        config.desktop_pet.enabled = is_checked(state.controls.desktop_pet);
        config.desktop_pet.size =
            parse_field::<i32>(state.controls.desktop_pet_size, "桌宠尺寸 / Pet size")?;

        config.asr.backend = if combo_selection(state.controls.asr_backend) == 1 {
            "offline"
        } else {
            "online"
        }
        .to_string();
        config.asr.online_provider = get_text(state.controls.online_provider)
            .trim()
            .to_ascii_lowercase();
        config.asr.offline_provider = get_text(state.controls.offline_provider)
            .trim()
            .to_ascii_lowercase();
        config.asr.offline_model_dir = get_text(state.controls.offline_model_dir)
            .trim()
            .to_string();
        config.asr.vad_enabled = is_checked(state.controls.vad_enabled);
        config.general.history_log_enabled = is_checked(state.controls.history_enabled);
        config.general.history_log_path = get_text(state.controls.history_path).trim().to_string();

        config.custom_vocabulary = parse_vocabulary(&get_text(state.controls.vocabulary))?;

        config.ai.enabled = is_checked(state.controls.ai_enabled);
        config.ai.api_endpoint = get_text(state.controls.ai_endpoint).trim().to_string();
        config.ai.api_key = get_text(state.controls.ai_key).trim().to_string();
        config.ai.model = get_text(state.controls.ai_model).trim().to_string();
        config.ai.post_process_enabled = is_checked(state.controls.post_process_enabled);
        config.ai.post_process_prompt = get_text(state.controls.post_process_prompt);
        config.ai.translation_enabled = is_checked(state.controls.translation_enabled);
        config.ai.translation_prompt = get_text(state.controls.translation_prompt);

        config.validate()?;
        Ok(config)
    }

    fn parse_vocabulary(text: &str) -> Result<HashMap<String, String>> {
        let mut vocabulary = HashMap::new();
        for (index, raw_line) in text.lines().enumerate() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((source, replacement)) = line.split_once('=') else {
                bail!("词典第 {} 行缺少 '='", index + 1);
            };
            let source = source.trim();
            let replacement = replacement.trim();
            if source.is_empty() || replacement.is_empty() {
                bail!("词典第 {} 行包含空白词条", index + 1);
            }
            if vocabulary
                .insert(source.to_string(), replacement.to_string())
                .is_some()
            {
                bail!("词典第 {} 行重复定义了“{}”", index + 1, source);
            }
        }
        Ok(vocabulary)
    }

    unsafe fn clear_history(hwnd: HWND, state: &WindowState) {
        let answer = show_message(
            hwnd,
            "确定清除全部本地听写历史吗？\nClear all local dictation history?",
            "Aiko IME",
            MB_YESNO | MB_ICONWARNING,
        );
        if answer != IDYES {
            return;
        }

        let mut config = state.config.clone();
        let path = get_text(state.controls.history_path).trim().to_string();
        if !path.is_empty() {
            config.general.history_log_path = path;
        }
        match config.clear_history() {
            Ok(true) => {
                set_text(state.controls.status, "本地听写历史已清除。");
                show_message(
                    hwnd,
                    "本地听写历史已清除。\nLocal history cleared.",
                    "Aiko IME",
                    MB_OK | MB_ICONINFORMATION,
                );
            }
            Ok(false) => {
                set_text(state.controls.status, "没有找到听写历史文件。");
                show_message(
                    hwnd,
                    "没有可清除的历史记录。\nNo history file was found.",
                    "Aiko IME",
                    MB_OK | MB_ICONINFORMATION,
                );
            }
            Err(error) => {
                show_message(
                    hwnd,
                    &format!("清除历史失败：\n{error:#}"),
                    "Aiko IME",
                    MB_OK | MB_ICONERROR,
                );
            }
        };
    }

    unsafe fn test_recognition_backend(hwnd: HWND, state: &WindowState) {
        let config = match read_config(state) {
            Ok(config) => config,
            Err(error) => {
                show_message(
                    hwnd,
                    &format!("无法读取当前识别设置：\n{error:#}"),
                    "Aiko IME 识别测试",
                    MB_OK | MB_ICONERROR,
                );
                return;
            }
        };

        set_text(state.controls.status, "正在测试语音识别后端…");
        let hwnd_value = hwnd.0;
        std::thread::spawn(move || {
            let result = probe_recognition_backend(&config);
            let hwnd = HWND(hwnd_value);
            match result {
                Ok(message) => {
                    show_message(
                        hwnd,
                        &format!("{message}\nRecognition backend test succeeded."),
                        "Aiko IME 识别测试",
                        MB_OK | MB_ICONINFORMATION,
                    );
                }
                Err(error) => {
                    show_message(
                        hwnd,
                        &format!("识别测试失败：\n{error:#}"),
                        "Aiko IME 识别测试",
                        MB_OK | MB_ICONERROR,
                    );
                }
            }
        });
    }

    fn probe_recognition_backend(config: &AppConfig) -> Result<String> {
        config.validate()?;
        match config.asr.backend.as_str() {
            "online" => probe_online_backend(config),
            "offline" => probe_offline_backend(config),
            other => bail!("未知识别后端 / unknown ASR backend: {other}"),
        }
    }

    fn probe_online_backend(config: &AppConfig) -> Result<String> {
        if config.asr.online_provider != "doubao" {
            bail!(
                "当前构建只支持 doubao 在线识别，当前为 {}",
                config.asr.online_provider
            );
        }

        let credentials_path = AppConfig::credentials_path();
        let credentials = DeviceCredentials::load(&credentials_path).with_context(|| {
            format!(
                "无法读取在线识别凭据：{}。请先启动一次主程序完成设备注册。",
                credentials_path.display()
            )
        })?;
        if !credentials.is_complete() {
            bail!("在线识别凭据不完整，请检查网络后重启 Aiko IME 以重新注册设备");
        }
        if !credentials.is_fresh() {
            bail!("在线识别凭据已过期，请重启 Aiko IME 以刷新 Doubao 设备令牌");
        }

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("无法启动识别测试运行时")?;
        runtime
            .block_on(AsrClient::new(credentials).test_connection())
            .context("Doubao WebSocket 连通性检查失败")?;
        Ok("在线识别后端可连接：doubao。".to_string())
    }

    fn probe_offline_backend(config: &AppConfig) -> Result<String> {
        if config.asr.offline_provider != "sherpa_onnx"
            && config.asr.offline_provider != "sherpa-onnx"
        {
            bail!(
                "当前构建只支持 sherpa_onnx 离线识别，当前为 {}",
                config.asr.offline_provider
            );
        }

        let model_dir = resolve_app_path(&config.asr.offline_model_dir);
        let model = ModelManager::load_dir(&model_dir).with_context(|| {
            format!(
                "离线模型目录不可用：{}。请放入包含 aiko-sherpa-model.json 的模型包。",
                model_dir.display()
            )
        })?;
        let probe = SherpaOnnxConfig::new(model)
            .context("离线模型配置无效")?
            .probe_backend()
            .context("sherpa-onnx 后端探测失败")?;
        Ok(format!("离线识别后端可用：sherpa_onnx，运行库 {probe}。"))
    }

    fn resolve_app_path(path: &str) -> std::path::PathBuf {
        let path = std::path::PathBuf::from(path.trim());
        if path.is_absolute() {
            return path;
        }
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|parent| parent.join(&path)))
            .unwrap_or(path)
    }

    unsafe fn test_selected_microphone(hwnd: HWND, state: &WindowState) {
        let index = combo_selection(state.controls.microphone);
        let device_name = if index == 0 {
            String::new()
        } else {
            state
                .microphone_names
                .get(index.saturating_sub(1))
                .cloned()
                .unwrap_or_default()
        };
        set_text(state.controls.status, "正在测试麦克风，请说话…");
        let hwnd_value = hwnd.0;

        std::thread::spawn(move || {
            let result = measure_microphone(&device_name);
            let hwnd = HWND(hwnd_value);
            match result {
                Ok((name, level)) => {
                    let message = if level == 0 {
                        format!(
                            "设备：{name}\n未检测到明显声音，请检查权限和输入音量。\nNo clear input was detected."
                        )
                    } else {
                        format!(
                            "设备：{name}\n检测成功，峰值约 {level}% 。\nMicrophone test succeeded."
                        )
                    };
                    show_message(
                        hwnd,
                        &message,
                        "Aiko IME 麦克风测试",
                        MB_OK | MB_ICONINFORMATION,
                    );
                }
                Err(error) => {
                    show_message(
                        hwnd,
                        &format!("麦克风测试失败：\n{error:#}"),
                        "Aiko IME 麦克风测试",
                        MB_OK | MB_ICONERROR,
                    );
                }
            };
        });
    }

    fn measure_microphone(requested_name: &str) -> Result<(String, u32)> {
        let host = cpal::default_host();
        let device = if requested_name.is_empty() {
            host.default_input_device()
                .context("Windows 没有默认输入设备")?
        } else {
            host.input_devices()?
                .find(|device| device.name().ok().as_deref() == Some(requested_name))
                .with_context(|| format!("找不到输入设备：{requested_name}"))?
        };
        let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        let supported = device.default_input_config()?;
        let stream_config = supported.config();
        let peak = Arc::new(AtomicU32::new(0));
        let error = |error| tracing::warn!("Microphone test stream error: {error}");

        let stream = match supported.sample_format() {
            SampleFormat::F32 => {
                let peak = peak.clone();
                device.build_input_stream(
                    &stream_config,
                    move |data: &[f32], _| {
                        let value = data
                            .iter()
                            .fold(0.0_f32, |current, sample| current.max(sample.abs()));
                        update_peak(&peak, (value.min(1.0) * 1_000.0) as u32);
                    },
                    error,
                    None,
                )?
            }
            SampleFormat::I16 => {
                let peak = peak.clone();
                device.build_input_stream(
                    &stream_config,
                    move |data: &[i16], _| {
                        let value = data
                            .iter()
                            .map(|sample| sample.unsigned_abs() as u32)
                            .max()
                            .unwrap_or(0);
                        update_peak(&peak, value.saturating_mul(1_000) / i16::MAX as u32);
                    },
                    error,
                    None,
                )?
            }
            SampleFormat::U16 => {
                let peak = peak.clone();
                device.build_input_stream(
                    &stream_config,
                    move |data: &[u16], _| {
                        let value = data
                            .iter()
                            .map(|sample| (*sample as i32 - 32_768).unsigned_abs())
                            .max()
                            .unwrap_or(0);
                        update_peak(&peak, value.saturating_mul(1_000) / 32_767);
                    },
                    error,
                    None,
                )?
            }
            format => bail!("不支持的麦克风采样格式：{format:?}"),
        };

        stream.play()?;
        std::thread::sleep(Duration::from_millis(1_500));
        drop(stream);
        Ok((name, peak.load(Ordering::Relaxed).min(1_000) / 10))
    }

    fn update_peak(peak: &AtomicU32, value: u32) {
        let mut current = peak.load(Ordering::Relaxed);
        while value > current {
            match peak.compare_exchange_weak(current, value, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(observed) => current = observed,
            }
        }
    }

    fn enumerate_microphones() -> Vec<String> {
        let host = cpal::default_host();
        let mut names: Vec<String> = host
            .input_devices()
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|device| device.name().ok())
            .collect();
        names.sort_by_key(|name| name.to_ascii_lowercase());
        names.dedup();
        names
    }

    fn set_auto_start(enabled: bool) -> Result<()> {
        let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
        let mut command = Command::new("reg.exe");
        command.creation_flags(CREATE_NO_WINDOW);
        if enabled {
            let executable = std::env::current_exe()?;
            command.args([
                "add",
                key,
                "/v",
                "Aiko IME",
                "/t",
                "REG_SZ",
                "/d",
                &format!("\"{}\"", executable.display()),
                "/f",
            ]);
            let output = command.output()?;
            if !output.status.success() {
                bail!(
                    "failed to enable auto-start: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                );
            }
        } else {
            command.args(["delete", key, "/v", "Aiko IME", "/f"]);
            // Deleting a value that does not exist is already the desired state.
            let _ = command.output()?;
        }
        Ok(())
    }

    unsafe fn update_hotkey_controls(state: &WindowState) {
        let combo_mode = combo_selection(state.controls.hotkey_mode) == 1;
        let _ = EnableWindow(state.controls.combo_key, combo_mode);
        let _ = EnableWindow(state.controls.double_tap_key, !combo_mode);
        let _ = EnableWindow(state.controls.double_tap_interval, !combo_mode);
    }

    unsafe fn update_asr_controls(state: &WindowState) {
        let offline = combo_selection(state.controls.asr_backend) == 1;
        let _ = EnableWindow(state.controls.offline_model_dir, offline);
    }

    unsafe fn show_page(state: &mut WindowState, page: usize) {
        const TITLES: [&str; 4] = [
            "常规 / General",
            "识别与隐私 / Recognition & Privacy",
            "自定义词典 / Custom Vocabulary",
            "AI 设置 / AI Settings",
        ];
        for (index, controls) in state.pages.iter().enumerate() {
            for control in controls {
                let _ = ShowWindow(*control, if index == page { SW_SHOW } else { SW_HIDE });
            }
        }
        state.current_page = page;
        set_text(state.controls.page_title, TITLES[page]);
    }

    unsafe fn add_group(
        instance: HINSTANCE,
        parent: HWND,
        state: &mut WindowState,
        page: usize,
        text: &str,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> HWND {
        add_page_control(
            instance,
            parent,
            state,
            page,
            "BUTTON",
            text,
            x,
            y,
            width,
            height,
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(BS_GROUPBOX as u32),
            0,
            0,
        )
    }

    unsafe fn add_label(
        instance: HINSTANCE,
        parent: HWND,
        state: &mut WindowState,
        page: usize,
        text: &str,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> HWND {
        add_page_control(
            instance,
            parent,
            state,
            page,
            "STATIC",
            text,
            x,
            y,
            width,
            height,
            WS_CHILD | WS_VISIBLE,
            0,
            0,
        )
    }

    unsafe fn add_edit(
        instance: HINSTANCE,
        parent: HWND,
        state: &mut WindowState,
        page: usize,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        extra_edit_style: i32,
    ) -> HWND {
        add_page_control(
            instance,
            parent,
            state,
            page,
            "EDIT",
            "",
            x,
            y,
            width,
            height,
            WS_CHILD
                | WS_VISIBLE
                | WS_TABSTOP
                | WINDOW_STYLE((ES_AUTOHSCROLL | extra_edit_style) as u32),
            WS_EX_CLIENTEDGE.0,
            0,
        )
    }

    unsafe fn add_combo(
        instance: HINSTANCE,
        parent: HWND,
        state: &mut WindowState,
        page: usize,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        id: i32,
    ) -> HWND {
        add_page_control(
            instance,
            parent,
            state,
            page,
            "COMBOBOX",
            "",
            x,
            y,
            width,
            height,
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_VSCROLL | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
            0,
            id,
        )
    }

    unsafe fn add_checkbox(
        instance: HINSTANCE,
        parent: HWND,
        state: &mut WindowState,
        page: usize,
        text: &str,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> HWND {
        add_page_control(
            instance,
            parent,
            state,
            page,
            "BUTTON",
            text,
            x,
            y,
            width,
            height,
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            0,
            0,
        )
    }

    #[allow(clippy::too_many_arguments)]
    unsafe fn add_page_control(
        instance: HINSTANCE,
        parent: HWND,
        state: &mut WindowState,
        page: usize,
        class_name: &str,
        text: &str,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        style: WINDOW_STYLE,
        extended_style: u32,
        id: i32,
    ) -> HWND {
        let control = create_control(
            instance,
            parent,
            class_name,
            text,
            x,
            y,
            width,
            height,
            style,
            extended_style,
            id,
        );
        state.pages[page].push(control);
        control
    }

    #[allow(clippy::too_many_arguments)]
    unsafe fn create_control(
        instance: HINSTANCE,
        parent: HWND,
        class_name: &str,
        text: &str,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        style: WINDOW_STYLE,
        extended_style: u32,
        id: i32,
    ) -> HWND {
        let class_name = wide(class_name);
        let text = wide(text);
        let control = CreateWindowExW(
            WINDOW_EX_STYLE(extended_style),
            PCWSTR(class_name.as_ptr()),
            PCWSTR(text.as_ptr()),
            style,
            x,
            y,
            width,
            height,
            parent,
            HMENU(id as isize),
            instance,
            None,
        );
        let font = settings_font();
        let _ = SendMessageW(control, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
        control
    }

    unsafe fn settings_font() -> windows::Win32::Graphics::Gdi::HGDIOBJ {
        let existing = SETTINGS_FONT.load(Ordering::SeqCst);
        if existing != 0 {
            return windows::Win32::Graphics::Gdi::HGDIOBJ(existing);
        }

        let face = wide("Microsoft YaHei UI");
        let created = CreateFontW(
            -16,
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET.0 as u32,
            OUT_DEFAULT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLEARTYPE_QUALITY.0 as u32,
            DEFAULT_PITCH.0 as u32,
            PCWSTR(face.as_ptr()),
        );
        let font = if created.0 == 0 {
            GetStockObject(DEFAULT_GUI_FONT)
        } else {
            windows::Win32::Graphics::Gdi::HGDIOBJ(created.0)
        };
        SETTINGS_FONT.store(font.0, Ordering::SeqCst);
        font
    }

    unsafe fn set_checked(control: HWND, checked: bool) {
        let _ = SendMessageW(
            control,
            BM_SETCHECK,
            WPARAM(usize::from(checked)),
            LPARAM(0),
        );
    }

    unsafe fn is_checked(control: HWND) -> bool {
        SendMessageW(control, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 == 1
    }

    unsafe fn combo_add(control: HWND, value: &str) {
        let value = wide(value);
        let _ = SendMessageW(
            control,
            CB_ADDSTRING,
            WPARAM(0),
            LPARAM(value.as_ptr() as isize),
        );
    }

    unsafe fn combo_select(control: HWND, index: usize) {
        let _ = SendMessageW(control, CB_SETCURSEL, WPARAM(index), LPARAM(0));
    }

    unsafe fn combo_selection(control: HWND) -> usize {
        let result = SendMessageW(control, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0;
        if result < 0 {
            0
        } else {
            result as usize
        }
    }

    unsafe fn set_text(control: HWND, text: &str) {
        let text = wide(text);
        let _ = SetWindowTextW(control, PCWSTR(text.as_ptr()));
    }

    unsafe fn get_text(control: HWND) -> String {
        let length = GetWindowTextLengthW(control).max(0) as usize;
        let mut buffer = vec![0_u16; length + 1];
        let copied = GetWindowTextW(control, &mut buffer).max(0) as usize;
        String::from_utf16_lossy(&buffer[..copied])
    }

    unsafe fn parse_field<T>(control: HWND, label: &str) -> Result<T>
    where
        T: std::str::FromStr,
        T::Err: std::fmt::Display,
    {
        let value = get_text(control);
        value
            .trim()
            .parse::<T>()
            .map_err(|error| anyhow::anyhow!("{label} 不是有效数字：{error}"))
    }

    unsafe fn center_window(hwnd: HWND) {
        let mut rectangle = windows::Win32::Foundation::RECT::default();
        if GetWindowRect(hwnd, &mut rectangle).is_ok() {
            let width = rectangle.right - rectangle.left;
            let height = rectangle.bottom - rectangle.top;
            let screen_width = GetSystemMetrics(SM_CXSCREEN);
            let screen_height = GetSystemMetrics(SM_CYSCREEN);
            let _ = SetWindowPos(
                hwnd,
                None,
                ((screen_width - width) / 2).max(0),
                ((screen_height - height) / 2).max(0),
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER,
            );
        }
    }

    fn show_message(
        parent: HWND,
        message: &str,
        caption: &str,
        style: MESSAGEBOX_STYLE,
    ) -> MESSAGEBOX_RESULT {
        let message = wide(message);
        let caption = wide(caption);
        unsafe {
            MessageBoxW(
                parent,
                PCWSTR(message.as_ptr()),
                PCWSTR(caption.as_ptr()),
                style,
            )
        }
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    #[cfg(test)]
    mod tests {
        use super::parse_vocabulary;

        #[test]
        fn vocabulary_parser_accepts_comments_and_unicode() {
            let parsed = parse_vocabulary("# comment\n爱可=Aiko\ncoding agent=编程助手").unwrap();
            assert_eq!(parsed.get("爱可").unwrap(), "Aiko");
            assert_eq!(parsed.get("coding agent").unwrap(), "编程助手");
        }

        #[test]
        fn vocabulary_parser_rejects_bad_and_duplicate_lines() {
            assert!(parse_vocabulary("missing separator").is_err());
            assert!(parse_vocabulary("A=B\nA=C").is_err());
        }
    }
}
