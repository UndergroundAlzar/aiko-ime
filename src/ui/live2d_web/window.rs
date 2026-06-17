use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::adapter::ScriptExecutor;

/// Client-area position and size for the child WebView2 host window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostBounds {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl HostBounds {
    pub const fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    fn validate(self) -> Result<Self, HostError> {
        if self.width <= 0 || self.height <= 0 {
            Err(HostError::InvalidBounds(self))
        } else {
            Ok(self)
        }
    }
}

/// Cross-origin policy for files exposed through a WebView2 virtual host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtualHostAccess {
    Deny,
    Allow,
    DenyCors,
}

/// Handler called with the JSON representation from `WebMessageReceived`.
///
/// The Windows implementation invokes this on the WebView2 UI thread. Keep the
/// handler short and hand work to another thread before calling host methods.
pub type WebMessageHandler = Arc<dyn Fn(String) + Send + Sync + 'static>;

#[derive(Debug, Clone)]
pub struct WebView2HostOptions {
    /// Native HWND value of the parent window.
    pub parent_window: isize,
    pub bounds: HostBounds,
    /// Optional fixed-version runtime directory. `None` uses the installed
    /// Evergreen WebView2 Runtime.
    pub browser_executable_folder: Option<PathBuf>,
    /// Optional WebView2 profile directory.
    pub user_data_folder: Option<PathBuf>,
    pub enable_dev_tools: bool,
}

impl WebView2HostOptions {
    pub fn new(parent_window: isize, bounds: HostBounds) -> Self {
        Self {
            parent_window,
            bounds,
            browser_executable_folder: None,
            user_data_folder: None,
            enable_dev_tools: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostError {
    UnsupportedPlatform,
    InvalidParentWindow,
    InvalidBounds(HostBounds),
    InvalidHostName,
    FolderUnavailable(PathBuf),
    ThreadUnavailable,
    WrongThread { expected: u32, actual: u32 },
    OperationTimedOut(&'static str),
    Windows(String),
    WebView2(String),
}

impl fmt::Display for HostError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPlatform => {
                formatter.write_str("the WebView2 renderer is only available on Windows")
            }
            Self::InvalidParentWindow => formatter.write_str("the parent HWND is not a window"),
            Self::InvalidBounds(bounds) => write!(
                formatter,
                "invalid WebView2 host bounds: {}x{} at {},{}",
                bounds.width, bounds.height, bounds.x, bounds.y
            ),
            Self::InvalidHostName => formatter.write_str("invalid WebView2 virtual host name"),
            Self::FolderUnavailable(path) => write!(
                formatter,
                "WebView2 virtual host folder is unavailable: {}",
                path.display()
            ),
            Self::ThreadUnavailable => {
                formatter.write_str("the WebView2 host thread is no longer available")
            }
            Self::WrongThread { expected, actual } => write!(
                formatter,
                "WebView2 host must be used on thread {expected}, not thread {actual}"
            ),
            Self::OperationTimedOut(operation) => {
                write!(formatter, "WebView2 operation timed out: {operation}")
            }
            Self::Windows(message) => write!(formatter, "Win32 host operation failed: {message}"),
            Self::WebView2(message) => write!(formatter, "WebView2 operation failed: {message}"),
        }
    }
}

impl Error for HostError {}

fn validate_host_name(host_name: &str) -> Result<(), HostError> {
    let valid = !host_name.is_empty()
        && !host_name.starts_with('.')
        && !host_name.ends_with('.')
        && host_name.split('.').all(|label| {
            !label.is_empty()
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        });
    if valid {
        Ok(())
    } else {
        Err(HostError::InvalidHostName)
    }
}

#[cfg(windows)]
mod platform {
    use std::ffi::c_void;
    use std::fmt;
    use std::marker::PhantomData;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{mpsc, Arc, RwLock};
    use std::time::{Duration, Instant};

    use webview2_com::Microsoft::Web::WebView2::Win32::{
        CreateCoreWebView2EnvironmentWithOptions, ICoreWebView2, ICoreWebView2Controller,
        ICoreWebView2Controller2, ICoreWebView2Environment, ICoreWebView2EnvironmentOptions,
        ICoreWebView2_3, COREWEBVIEW2_COLOR, COREWEBVIEW2_HOST_RESOURCE_ACCESS_KIND,
        COREWEBVIEW2_HOST_RESOURCE_ACCESS_KIND_ALLOW, COREWEBVIEW2_HOST_RESOURCE_ACCESS_KIND_DENY,
        COREWEBVIEW2_HOST_RESOURCE_ACCESS_KIND_DENY_CORS,
    };
    use webview2_com::{
        CoTaskMemPWSTR, CoreWebView2EnvironmentOptions,
        CreateCoreWebView2ControllerCompletedHandler,
        CreateCoreWebView2EnvironmentCompletedHandler, ExecuteScriptCompletedHandler,
        WebMessageReceivedEventHandler,
    };
    use windows_webview::core::{w, Interface, PCWSTR, PWSTR};
    use windows_webview::Win32::Foundation::{
        E_POINTER, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM,
    };
    use windows_webview::Win32::System::Com::{
        CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED,
    };
    use windows_webview::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_webview::Win32::System::Threading::GetCurrentThreadId;
    use windows_webview::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetWindowThreadProcessId,
        IsWindow, PeekMessageW, RegisterClassW, SetWindowPos, ShowWindow, TranslateMessage,
        UnregisterClassW, CS_HREDRAW, CS_VREDRAW, MSG, PM_REMOVE, SWP_NOACTIVATE, SWP_NOZORDER,
        SW_SHOW, WINDOW_EX_STYLE, WM_QUIT, WNDCLASSW, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS,
    };

    use super::{
        validate_host_name, HostBounds, HostError, Path, VirtualHostAccess, WebMessageHandler,
        WebView2HostOptions,
    };

    const ASYNC_OPERATION_TIMEOUT: Duration = Duration::from_secs(20);
    static CLASS_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    struct HostContext {
        class_name: Vec<u16>,
        instance: HINSTANCE,
        child: HWND,
        controller: ICoreWebView2Controller,
        webview: ICoreWebView2,
        web_message_token: i64,
    }

    pub struct TransparentWebView2Host {
        owner_thread_id: u32,
        context: Option<HostContext>,
        message_handler: Arc<RwLock<Option<WebMessageHandler>>>,
        _thread_affinity: PhantomData<Rc<()>>,
    }

    impl fmt::Debug for TransparentWebView2Host {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter
                .debug_struct("TransparentWebView2Host")
                .field("owner_thread_id", &self.owner_thread_id)
                .field(
                    "child_window",
                    &self
                        .context
                        .as_ref()
                        .map(|context| raw_from_hwnd(context.child)),
                )
                .field("closed", &self.context.is_none())
                .finish_non_exhaustive()
        }
    }

    impl TransparentWebView2Host {
        pub fn create(parent_window: isize, bounds: HostBounds) -> Result<Self, HostError> {
            Self::create_with_options(WebView2HostOptions::new(parent_window, bounds))
        }

        pub fn create_with_options(mut options: WebView2HostOptions) -> Result<Self, HostError> {
            options.bounds = options.bounds.validate()?;
            let parent = hwnd_from_raw(options.parent_window);
            if options.parent_window == 0 || !unsafe { IsWindow(Some(parent)).as_bool() } {
                return Err(HostError::InvalidParentWindow);
            }

            let owner_thread_id = unsafe { GetCurrentThreadId() };
            let parent_thread_id = unsafe { GetWindowThreadProcessId(parent, None) };
            if parent_thread_id != owner_thread_id {
                return Err(HostError::WrongThread {
                    expected: parent_thread_id,
                    actual: owner_thread_id,
                });
            }

            let apartment = ComApartment::initialize()?;
            let message_handler = Arc::new(RwLock::new(None));
            let context = create_context(&options, Arc::clone(&message_handler))?;
            apartment.commit();

            Ok(Self {
                owner_thread_id,
                context: Some(context),
                message_handler,
                _thread_affinity: PhantomData,
            })
        }

        pub fn child_window(&self) -> isize {
            self.context
                .as_ref()
                .map_or(0, |context| raw_from_hwnd(context.child))
        }

        pub fn navigate(&self, uri: impl Into<String>) -> Result<(), HostError> {
            let context = self.context()?;
            let uri = CoTaskMemPWSTR::from(uri.into().as_str());
            unsafe { context.webview.Navigate(*uri.as_ref().as_pcwstr()) }.map_err(windows_error)
        }

        pub fn set_virtual_host_name_to_folder_mapping(
            &self,
            host_name: impl Into<String>,
            folder: impl AsRef<Path>,
            access: VirtualHostAccess,
        ) -> Result<(), HostError> {
            let host_name = host_name.into();
            validate_host_name(&host_name)?;
            let folder = folder.as_ref();
            if !folder.is_dir() {
                return Err(HostError::FolderUnavailable(folder.to_path_buf()));
            }
            let folder = folder.canonicalize().map_err(|error| {
                HostError::Windows(format!("canonicalize {}: {error}", folder.display()))
            })?;

            set_virtual_host(&self.context()?.webview, &host_name, &folder, access)
        }

        pub fn clear_virtual_host_name_to_folder_mapping(
            &self,
            host_name: impl Into<String>,
        ) -> Result<(), HostError> {
            let host_name = host_name.into();
            validate_host_name(&host_name)?;
            clear_virtual_host(&self.context()?.webview, &host_name)
        }

        pub fn execute_script(&self, script: impl Into<String>) -> Result<String, HostError> {
            execute_script(&self.context()?.webview, script.into())
        }

        pub fn set_web_message_received_handler<F>(&self, handler: F) -> Result<(), HostError>
        where
            F: Fn(String) + Send + Sync + 'static,
        {
            self.set_web_message_handler(Some(Arc::new(handler)))
        }

        pub fn set_web_message_handler(
            &self,
            handler: Option<WebMessageHandler>,
        ) -> Result<(), HostError> {
            self.ensure_current_thread()?;
            if self.context.is_none() {
                return Err(HostError::ThreadUnavailable);
            }
            *self
                .message_handler
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = handler;
            Ok(())
        }

        pub fn resize(&self, bounds: HostBounds) -> Result<(), HostError> {
            let bounds = bounds.validate()?;
            resize_context(self.context()?, bounds)
        }

        pub fn close(&mut self) -> Result<(), HostError> {
            self.ensure_current_thread()?;
            if let Some(context) = self.context.take() {
                close_context(context);
                unsafe { CoUninitialize() };
            }
            Ok(())
        }

        fn context(&self) -> Result<&HostContext, HostError> {
            self.ensure_current_thread()?;
            self.context.as_ref().ok_or(HostError::ThreadUnavailable)
        }

        fn ensure_current_thread(&self) -> Result<(), HostError> {
            let actual = unsafe { GetCurrentThreadId() };
            if actual == self.owner_thread_id {
                Ok(())
            } else {
                Err(HostError::WrongThread {
                    expected: self.owner_thread_id,
                    actual,
                })
            }
        }
    }

    impl Drop for TransparentWebView2Host {
        fn drop(&mut self) {
            let _ = self.close();
        }
    }

    struct ComApartment {
        committed: bool,
    }

    impl ComApartment {
        fn initialize() -> Result<Self, HostError> {
            unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
                .ok()
                .map_err(windows_error)?;
            Ok(Self { committed: false })
        }

        fn commit(mut self) {
            self.committed = true;
        }
    }

    impl Drop for ComApartment {
        fn drop(&mut self) {
            if !self.committed {
                unsafe { CoUninitialize() };
            }
        }
    }

    fn create_context(
        options: &WebView2HostOptions,
        message_handler: Arc<RwLock<Option<WebMessageHandler>>>,
    ) -> Result<HostContext, HostError> {
        let instance = unsafe { GetModuleHandleW(None) }
            .map(|module| HINSTANCE(module.0))
            .map_err(windows_error)?;
        let class_name = format!(
            "AikoIme.WebView2Host.{}.{}",
            std::process::id(),
            CLASS_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
        let window_class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(host_window_proc),
            hInstance: instance,
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        if unsafe { RegisterClassW(&window_class) } == 0 {
            return Err(windows_error(windows_webview::core::Error::from_thread()));
        }

        let parent = hwnd_from_raw(options.parent_window);
        let bounds = options.bounds;
        let child = match unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                PCWSTR(class_name.as_ptr()),
                w!(""),
                WS_CHILD | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
                bounds.x,
                bounds.y,
                bounds.width,
                bounds.height,
                Some(parent),
                None,
                Some(instance),
                None,
            )
        } {
            Ok(child) => child,
            Err(error) => {
                unsafe {
                    let _ = UnregisterClassW(PCWSTR(class_name.as_ptr()), Some(instance));
                }
                return Err(windows_error(error));
            }
        };

        let result = (|| {
            let environment = create_environment(options)?;
            let controller = create_controller(&environment, child)?;
            unsafe {
                controller.SetBounds(RECT {
                    left: 0,
                    top: 0,
                    right: bounds.width,
                    bottom: bounds.height,
                })
            }
            .map_err(windows_error)?;

            let controller2: ICoreWebView2Controller2 = controller.cast().map_err(windows_error)?;
            unsafe {
                controller2.SetDefaultBackgroundColor(COREWEBVIEW2_COLOR {
                    A: 0,
                    R: 0,
                    G: 0,
                    B: 0,
                })
            }
            .map_err(windows_error)?;

            let webview = unsafe { controller.CoreWebView2() }.map_err(windows_error)?;
            let settings = unsafe { webview.Settings() }.map_err(windows_error)?;
            unsafe {
                settings
                    .SetAreDefaultContextMenusEnabled(options.enable_dev_tools)
                    .map_err(windows_error)?;
                settings
                    .SetAreDevToolsEnabled(options.enable_dev_tools)
                    .map_err(windows_error)?;
                controller.SetIsVisible(true).map_err(windows_error)?;
                let _ = ShowWindow(child, SW_SHOW);
            }

            let mut web_message_token = 0;
            let event_handler =
                WebMessageReceivedEventHandler::create(Box::new(move |_sender, args| {
                    if let Some(args) = args {
                        let mut json = PWSTR::null();
                        unsafe { args.WebMessageAsJson(&mut json) }?;
                        let json = CoTaskMemPWSTR::from(json).to_string();
                        let handler = message_handler
                            .read()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .clone();
                        if let Some(handler) = handler {
                            handler(json);
                        }
                    }
                    Ok(())
                }));
            unsafe { webview.add_WebMessageReceived(&event_handler, &mut web_message_token) }
                .map_err(windows_error)?;

            Ok(HostContext {
                class_name: class_name.clone(),
                instance,
                child,
                controller,
                webview,
                web_message_token,
            })
        })();

        if result.is_err() {
            unsafe {
                let _ = DestroyWindow(child);
                let _ = UnregisterClassW(PCWSTR(class_name.as_ptr()), Some(instance));
            }
        }
        result
    }

    fn create_environment(
        options: &WebView2HostOptions,
    ) -> Result<ICoreWebView2Environment, HostError> {
        let (sender, receiver) = mpsc::channel();
        let browser_folder = wide_path(options.browser_executable_folder.as_deref());
        let user_data_folder = wide_path(options.user_data_folder.as_deref());
        let environment_options: ICoreWebView2EnvironmentOptions =
            CoreWebView2EnvironmentOptions::default().into();

        let handler =
            CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(move |error, value| {
                let result = error.and_then(|_| {
                    value.ok_or_else(|| windows_webview::core::Error::from(E_POINTER))
                });
                let _ = sender.send(result);
                Ok(())
            }));
        unsafe {
            CreateCoreWebView2EnvironmentWithOptions(
                optional_pcwstr(&browser_folder),
                optional_pcwstr(&user_data_folder),
                &environment_options,
                &handler,
            )
        }
        .map_err(windows_error)?;

        wait_for_async_result(receiver, "create environment")
    }

    fn create_controller(
        environment: &ICoreWebView2Environment,
        child: HWND,
    ) -> Result<ICoreWebView2Controller, HostError> {
        let (sender, receiver) = mpsc::channel();
        let environment = environment.clone();
        let handler =
            CreateCoreWebView2ControllerCompletedHandler::create(Box::new(move |error, value| {
                let result = error.and_then(|_| {
                    value.ok_or_else(|| windows_webview::core::Error::from(E_POINTER))
                });
                let _ = sender.send(result);
                Ok(())
            }));
        unsafe { environment.CreateCoreWebView2Controller(child, &handler) }
            .map_err(windows_error)?;

        wait_for_async_result(receiver, "create controller")
    }

    fn set_virtual_host(
        webview: &ICoreWebView2,
        host_name: &str,
        folder: &Path,
        access: VirtualHostAccess,
    ) -> Result<(), HostError> {
        let webview3: ICoreWebView2_3 = webview.cast().map_err(windows_error)?;
        let host_name = CoTaskMemPWSTR::from(host_name);
        let folder = webview_folder_path(folder);
        let folder = CoTaskMemPWSTR::from(folder.as_str());
        unsafe {
            webview3.SetVirtualHostNameToFolderMapping(
                *host_name.as_ref().as_pcwstr(),
                *folder.as_ref().as_pcwstr(),
                access_kind(access),
            )
        }
        .map_err(windows_error)
    }

    fn webview_folder_path(folder: &Path) -> String {
        let folder = folder.to_string_lossy();
        if let Some(path) = folder.strip_prefix(r"\\?\UNC\") {
            format!(r"\\{path}")
        } else if let Some(path) = folder.strip_prefix(r"\\?\") {
            path.to_string()
        } else {
            folder.into_owned()
        }
    }

    fn clear_virtual_host(webview: &ICoreWebView2, host_name: &str) -> Result<(), HostError> {
        let webview3: ICoreWebView2_3 = webview.cast().map_err(windows_error)?;
        let host_name = CoTaskMemPWSTR::from(host_name);
        unsafe { webview3.ClearVirtualHostNameToFolderMapping(*host_name.as_ref().as_pcwstr()) }
            .map_err(windows_error)
    }

    fn execute_script(webview: &ICoreWebView2, script: String) -> Result<String, HostError> {
        let (sender, receiver) = mpsc::channel();
        let webview = webview.clone();
        let handler = ExecuteScriptCompletedHandler::create(Box::new(move |error, json| {
            let result = error.map(|_| json);
            let _ = sender.send(result);
            Ok(())
        }));
        let script = CoTaskMemPWSTR::from(script.as_str());
        unsafe { webview.ExecuteScript(*script.as_ref().as_pcwstr(), &handler) }
            .map_err(windows_error)?;
        wait_for_async_result(receiver, "execute script")
    }

    fn resize_context(context: &HostContext, bounds: HostBounds) -> Result<(), HostError> {
        unsafe {
            SetWindowPos(
                context.child,
                None,
                bounds.x,
                bounds.y,
                bounds.width,
                bounds.height,
                SWP_NOACTIVATE | SWP_NOZORDER,
            )
            .map_err(windows_error)?;
            context
                .controller
                .SetBounds(RECT {
                    left: 0,
                    top: 0,
                    right: bounds.width,
                    bottom: bounds.height,
                })
                .map_err(windows_error)?;
            context
                .controller
                .NotifyParentWindowPositionChanged()
                .map_err(windows_error)?;
        }
        Ok(())
    }

    fn close_context(context: HostContext) {
        unsafe {
            let _ = context
                .webview
                .remove_WebMessageReceived(context.web_message_token);
            let _ = context.controller.Close();
            let _ = DestroyWindow(context.child);
            let _ = UnregisterClassW(PCWSTR(context.class_name.as_ptr()), Some(context.instance));
        }
    }

    fn wait_for_async_result<T>(
        receiver: mpsc::Receiver<windows_webview::core::Result<T>>,
        operation: &'static str,
    ) -> Result<T, HostError> {
        let deadline = Instant::now() + ASYNC_OPERATION_TIMEOUT;
        let mut message = MSG::default();
        loop {
            match receiver.try_recv() {
                Ok(result) => return result.map_err(windows_error),
                Err(mpsc::TryRecvError::Disconnected) => return Err(HostError::ThreadUnavailable),
                Err(mpsc::TryRecvError::Empty) => {}
            }

            while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() } {
                if message.message == WM_QUIT {
                    return Err(HostError::ThreadUnavailable);
                }
                unsafe {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }

            if Instant::now() >= deadline {
                return Err(HostError::OperationTimedOut(operation));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn wide_path(path: Option<&Path>) -> Option<Vec<u16>> {
        path.map(|path| {
            path.as_os_str()
                .to_string_lossy()
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect()
        })
    }

    fn optional_pcwstr(value: &Option<Vec<u16>>) -> PCWSTR {
        value
            .as_ref()
            .map_or_else(PCWSTR::null, |value| PCWSTR(value.as_ptr()))
    }

    const fn access_kind(access: VirtualHostAccess) -> COREWEBVIEW2_HOST_RESOURCE_ACCESS_KIND {
        match access {
            VirtualHostAccess::Deny => COREWEBVIEW2_HOST_RESOURCE_ACCESS_KIND_DENY,
            VirtualHostAccess::Allow => COREWEBVIEW2_HOST_RESOURCE_ACCESS_KIND_ALLOW,
            VirtualHostAccess::DenyCors => COREWEBVIEW2_HOST_RESOURCE_ACCESS_KIND_DENY_CORS,
        }
    }

    fn windows_error(error: windows_webview::core::Error) -> HostError {
        HostError::Windows(error.to_string())
    }

    const fn hwnd_from_raw(value: isize) -> HWND {
        HWND(value as *mut c_void)
    }

    fn raw_from_hwnd(value: HWND) -> isize {
        value.0 as isize
    }

    unsafe extern "system" fn host_window_proc(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        unsafe { DefWindowProcW(window, message, wparam, lparam) }
    }
}

#[cfg(windows)]
pub use platform::TransparentWebView2Host;

#[cfg(not(windows))]
#[derive(Debug)]
pub struct TransparentWebView2Host;

#[cfg(not(windows))]
impl TransparentWebView2Host {
    pub fn create(_: isize, bounds: HostBounds) -> Result<Self, HostError> {
        bounds.validate()?;
        Err(HostError::UnsupportedPlatform)
    }

    pub fn create_with_options(options: WebView2HostOptions) -> Result<Self, HostError> {
        options.bounds.validate()?;
        Err(HostError::UnsupportedPlatform)
    }

    pub const fn child_window(&self) -> isize {
        0
    }

    pub fn navigate(&self, _: impl Into<String>) -> Result<(), HostError> {
        Err(HostError::UnsupportedPlatform)
    }

    pub fn set_virtual_host_name_to_folder_mapping(
        &self,
        host_name: impl Into<String>,
        _: impl AsRef<Path>,
        _: VirtualHostAccess,
    ) -> Result<(), HostError> {
        validate_host_name(&host_name.into())?;
        Err(HostError::UnsupportedPlatform)
    }

    pub fn clear_virtual_host_name_to_folder_mapping(
        &self,
        host_name: impl Into<String>,
    ) -> Result<(), HostError> {
        validate_host_name(&host_name.into())?;
        Err(HostError::UnsupportedPlatform)
    }

    pub fn execute_script(&self, _: impl Into<String>) -> Result<String, HostError> {
        Err(HostError::UnsupportedPlatform)
    }

    pub fn set_web_message_received_handler<F>(&self, _: F) -> Result<(), HostError>
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        Err(HostError::UnsupportedPlatform)
    }

    pub fn set_web_message_handler(&self, _: Option<WebMessageHandler>) -> Result<(), HostError> {
        Err(HostError::UnsupportedPlatform)
    }

    pub fn resize(&self, bounds: HostBounds) -> Result<(), HostError> {
        bounds.validate()?;
        Err(HostError::UnsupportedPlatform)
    }

    pub fn close(&mut self) -> Result<(), HostError> {
        Ok(())
    }
}

impl ScriptExecutor for TransparentWebView2Host {
    type Error = HostError;

    fn execute_script(&mut self, script: &str) -> Result<(), Self::Error> {
        TransparentWebView2Host::execute_script(self, script).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::{validate_host_name, HostBounds, HostError};

    #[test]
    fn bounds_must_have_positive_area() {
        let error = HostBounds::new(0, 0, 0, 100).validate().unwrap_err();
        assert!(matches!(error, HostError::InvalidBounds(_)));
    }

    #[test]
    fn virtual_hosts_use_dns_style_names() {
        assert!(validate_host_name("aiko.local").is_ok());
        assert!(validate_host_name("-aiko.local").is_err());
        assert!(validate_host_name("aiko..local").is_err());
        assert!(validate_host_name("https://aiko.local").is_err());
    }
}
