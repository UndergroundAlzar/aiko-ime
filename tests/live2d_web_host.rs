use std::fs;
use std::path::PathBuf;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn webview2_host_is_feature_gated_and_optional() {
    let manifest = fs::read_to_string(repository_root().join("Cargo.toml")).unwrap();
    assert!(
        manifest.contains("webview2-renderer = [\"dep:webview2-com\", \"dep:windows-webview\"]")
    );
    assert!(manifest.contains("webview2-com = { version = \"0.39.1\", optional = true }"));
    assert!(manifest.contains(
        "windows-webview = { package = \"windows\", version = \"0.62\", optional = true"
    ));

    let ui_module = fs::read_to_string(repository_root().join("src/ui/mod.rs")).unwrap();
    assert!(ui_module.contains("#[cfg(feature = \"webview2-renderer\")]"));
    assert!(ui_module.contains("pub mod live2d_web;"));
}

#[test]
fn native_host_surface_covers_required_operations() {
    let source = fs::read_to_string(repository_root().join("src/ui/live2d_web/window.rs")).unwrap();
    for api in [
        "pub fn create(",
        "pub fn create_with_options(",
        "pub fn navigate(",
        "pub fn set_virtual_host_name_to_folder_mapping(",
        "pub fn execute_script(",
        "pub fn set_web_message_received_handler",
        "pub fn resize(",
        "pub fn close(",
    ] {
        assert!(source.contains(api), "missing WebView2 host API: {api}");
    }
    assert!(source.contains("SetDefaultBackgroundColor(COREWEBVIEW2_COLOR"));
    assert!(source.contains("A: 0"));
}

#[test]
fn host_does_not_bundle_cubism_core_or_a_placeholder_model() {
    let source = fs::read_to_string(repository_root().join("src/ui/live2d_web/window.rs")).unwrap();
    assert!(!source.contains("live2dcubismcore.min.js"));
    assert!(!source.contains(".model3.json"));

    let runtime =
        fs::read_to_string(repository_root().join("src/ui/live2d_web/runtime.rs")).unwrap();
    assert!(runtime.contains("intentionally not bundled"));
    assert!(runtime.contains("A real Cubism export"));
}

#[cfg(feature = "webview2-renderer")]
#[test]
fn public_host_types_are_available_with_the_feature() {
    use aiko_ime::ui::live2d_web::{
        HostBounds, HostError, TransparentWebView2Host, VirtualHostAccess, WebView2HostOptions,
    };

    let bounds = HostBounds::new(0, 0, 320, 480);
    let options = WebView2HostOptions::new(0, bounds);
    assert_eq!(options.bounds, bounds);
    assert_eq!(VirtualHostAccess::DenyCors, VirtualHostAccess::DenyCors);

    let error = TransparentWebView2Host::create(0, bounds).unwrap_err();
    #[cfg(windows)]
    assert_eq!(error, HostError::InvalidParentWindow);
    #[cfg(not(windows))]
    assert_eq!(error, HostError::UnsupportedPlatform);
}

#[cfg(all(windows, feature = "webview2-renderer"))]
mod windows_smoke {
    use std::ffi::c_void;
    use std::fs;
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use aiko_ime::ui::live2d_web::{
        HostBounds, HostError, TransparentWebView2Host, VirtualHostAccess, WebView2HostOptions,
    };
    use windows_webview::core::{w, PCWSTR};
    use windows_webview::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_webview::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_webview::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, IsWindow,
        PeekMessageW, RegisterClassW, ShowWindow, TranslateMessage, UnregisterClassW, MSG,
        PM_REMOVE, SW_SHOW, WINDOW_EX_STYLE, WM_QUIT, WNDCLASSW, WS_OVERLAPPEDWINDOW,
    };

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    struct TestWindow {
        class_name: Vec<u16>,
        instance: HINSTANCE,
        hwnd: HWND,
        _thread_affinity: std::marker::PhantomData<Rc<()>>,
    }

    impl TestWindow {
        fn create() -> Self {
            let instance = unsafe { GetModuleHandleW(None) }
                .map(|module| HINSTANCE(module.0))
                .expect("get test module handle");
            let class_name = format!(
                "AikoIme.WebView2Smoke.{}.{}",
                std::process::id(),
                TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            )
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
            let window_class = WNDCLASSW {
                lpfnWndProc: Some(test_window_proc),
                hInstance: instance,
                lpszClassName: PCWSTR(class_name.as_ptr()),
                ..Default::default()
            };
            assert_ne!(
                unsafe { RegisterClassW(&window_class) },
                0,
                "register smoke parent window class"
            );
            let hwnd = unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    PCWSTR(class_name.as_ptr()),
                    w!("Aiko WebView2 smoke"),
                    WS_OVERLAPPEDWINDOW,
                    0,
                    0,
                    640,
                    480,
                    None,
                    None,
                    Some(instance),
                    None,
                )
            }
            .expect("create smoke parent window");
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOW);
            }
            Self {
                class_name,
                instance,
                hwnd,
                _thread_affinity: std::marker::PhantomData,
            }
        }

        fn raw(&self) -> isize {
            self.hwnd.0 as isize
        }
    }

    impl Drop for TestWindow {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyWindow(self.hwnd);
                let _ = UnregisterClassW(PCWSTR(self.class_name.as_ptr()), Some(self.instance));
            }
        }
    }

    struct SmokeDirectory(PathBuf);

    impl SmokeDirectory {
        fn create() -> Self {
            let path = std::env::temp_dir().join(format!(
                "aiko-webview2-smoke-{}-{}",
                std::process::id(),
                TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(path.join("content")).expect("create smoke content directory");
            fs::write(
                path.join("content/index.html"),
                r#"<!doctype html>
<html>
<head><meta charset="utf-8"><title>Aiko Smoke</title></head>
<body>
<script>
window.chrome.webview.postMessage({ kind: "ready", value: 21 });
</script>
</body>
</html>"#,
            )
            .expect("write smoke page");
            Self(path)
        }

        fn content(&self) -> PathBuf {
            self.0.join("content")
        }

        fn profile(&self) -> PathBuf {
            self.0.join("profile")
        }
    }

    impl Drop for SmokeDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn real_webview2_host_smoke_runs_on_parent_ui_thread() {
        let parent = TestWindow::create();
        let directory = SmokeDirectory::create();
        let mut options = WebView2HostOptions::new(parent.raw(), HostBounds::new(4, 6, 320, 240));
        options.user_data_folder = Some(directory.profile());

        let create_started = Instant::now();
        let mut host =
            TransparentWebView2Host::create_with_options(options).expect("create WebView2 host");
        assert!(
            create_started.elapsed() < Duration::from_secs(30),
            "WebView2 host creation exceeded the smoke-test deadline"
        );

        let child = host.child_window();
        assert_ne!(child, 0);
        let (message_sender, message_receiver) = mpsc::channel();
        host.set_web_message_received_handler(move |json| {
            let _ = message_sender.send(json);
        })
        .expect("set WebMessageReceived handler");
        host.set_virtual_host_name_to_folder_mapping(
            "aiko-smoke.test",
            directory.content(),
            VirtualHostAccess::Allow,
        )
        .expect("map local virtual host");
        host.navigate("https://aiko-smoke.test/index.html")
            .expect("navigate to local virtual host");

        let message = receive_with_message_pump(&message_receiver, Duration::from_secs(15));
        let message: serde_json::Value =
            serde_json::from_str(&message).expect("parse WebMessageReceived JSON");
        assert_eq!(message["kind"], "ready");
        assert_eq!(message["value"], 21);

        let title = host
            .execute_script("document.title")
            .expect("execute script on smoke page");
        assert_eq!(title, r#""Aiko Smoke""#);

        host.resize(HostBounds::new(10, 12, 400, 300))
            .expect("resize WebView2 host");
        let mut rect = RECT::default();
        unsafe { GetClientRect(HWND(child as *mut c_void), &mut rect) }
            .expect("read resized child bounds");
        assert_eq!(rect.right - rect.left, 400);
        assert_eq!(rect.bottom - rect.top, 300);

        host.clear_virtual_host_name_to_folder_mapping("aiko-smoke.test")
            .expect("clear local virtual host");
        host.close().expect("close WebView2 host");
        assert!(!unsafe { IsWindow(Some(HWND(child as *mut c_void))).as_bool() });
    }

    #[test]
    fn creating_host_off_parent_thread_is_rejected() {
        let parent = TestWindow::create();
        let parent_window = parent.raw();
        let error = std::thread::spawn(move || {
            TransparentWebView2Host::create(parent_window, HostBounds::new(0, 0, 320, 240))
                .unwrap_err()
        })
        .join()
        .expect("join wrong-thread probe");
        assert!(matches!(error, HostError::WrongThread { .. }));
    }

    fn receive_with_message_pump(receiver: &mpsc::Receiver<String>, timeout: Duration) -> String {
        let deadline = Instant::now() + timeout;
        let mut message = MSG::default();
        loop {
            if let Ok(value) = receiver.try_recv() {
                return value;
            }
            while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() } {
                assert_ne!(message.message, WM_QUIT, "smoke UI thread received WM_QUIT");
                unsafe {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for WebMessageReceived"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    unsafe extern "system" fn test_window_proc(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        unsafe { DefWindowProcW(window, message, wparam, lparam) }
    }
}
