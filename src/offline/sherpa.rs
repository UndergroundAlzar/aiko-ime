//! Feature-gated sherpa-onnx streaming provider.

use futures_util::future::BoxFuture;
use std::env;
use std::path::{Path, PathBuf};

use crate::asr::{
    AsrProvider, ProviderCapabilities, ProviderError, ProviderKind, ProviderSession, SessionOptions,
};

use super::ResolvedModel;

pub const SHERPA_DLL_NAME: &str = "sherpa-onnx-c-api.dll";
#[cfg(all(windows, feature = "sherpa-onnx"))]
pub const MINIMUM_SHERPA_VERSION: (u32, u32, u32) = (1, 13, 2);

#[derive(Debug, Clone)]
pub struct SherpaOnnxConfig {
    pub model: ResolvedModel,
    /// Explicit DLL file or a directory containing `sherpa-onnx-c-api.dll`.
    pub library_path: Option<PathBuf>,
    pub num_threads: i32,
    pub execution_provider: String,
    pub debug: bool,
    pub decoding_method: String,
    pub max_active_paths: i32,
}

impl SherpaOnnxConfig {
    pub fn new(model: ResolvedModel) -> Result<Self, ProviderError> {
        model.validate()?;
        Ok(Self {
            model,
            library_path: None,
            num_threads: 2,
            execution_provider: "cpu".to_string(),
            debug: false,
            decoding_method: "greedy_search".to_string(),
            max_active_paths: 4,
        })
    }

    pub fn resolve_library_path(&self) -> Result<PathBuf, ProviderError> {
        let candidates = self.library_candidates();
        candidates
            .iter()
            .find(|path| path.is_file())
            .cloned()
            .ok_or_else(|| {
                ProviderError::BackendUnavailable(
                    ProviderKind::SherpaOnnx,
                    format!(
                        "未找到 {} / DLL not found; searched: {}",
                        SHERPA_DLL_NAME,
                        candidates
                            .iter()
                            .map(|path| path.display().to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                )
            })
    }

    pub fn library_candidates(&self) -> Vec<PathBuf> {
        let mut candidates = Vec::new();
        if let Some(path) = &self.library_path {
            push_library_candidate(&mut candidates, path);
        }
        if let Some(path) = env::var_os("AIKO_IME_SHERPA_DLL") {
            push_library_candidate(&mut candidates, Path::new(&path));
        }

        push_library_candidate(&mut candidates, &self.model.root);
        push_library_candidate(&mut candidates, &self.model.root.join("lib"));
        push_library_candidate(&mut candidates, &self.model.root.join("runtime"));

        if let Ok(exe) = env::current_exe() {
            if let Some(dir) = exe.parent() {
                push_library_candidate(&mut candidates, dir);
                push_library_candidate(&mut candidates, &dir.join("lib"));
                push_library_candidate(
                    &mut candidates,
                    &dir.join("runtime").join("sherpa-onnx").join("lib"),
                );
            }
        }
        if let Some(local_app_data) = env::var_os("LOCALAPPDATA") {
            push_library_candidate(
                &mut candidates,
                &PathBuf::from(local_app_data)
                    .join("AikoIME")
                    .join("runtime")
                    .join("sherpa-onnx")
                    .join("lib"),
            );
        }

        candidates
    }

    /// Load the native library and verify the C API symbols without creating a
    /// recognizer. This is suitable for a settings-page backend health check.
    pub fn probe_backend(&self) -> Result<String, ProviderError> {
        #[cfg(all(windows, feature = "sherpa-onnx"))]
        {
            native::probe_library(&self.resolve_library_path()?)
        }

        #[cfg(not(all(windows, feature = "sherpa-onnx")))]
        {
            Err(ProviderError::BackendUnavailable(
                ProviderKind::SherpaOnnx,
                "当前构建未启用 sherpa-onnx / this build does not include the `sherpa-onnx` feature"
                    .to_string(),
            ))
        }
    }
}

fn push_library_candidate(candidates: &mut Vec<PathBuf>, path: &Path) {
    let candidate = if path
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(SHERPA_DLL_NAME))
    {
        path.to_path_buf()
    } else {
        path.join(SHERPA_DLL_NAME)
    };
    if !candidates.contains(&candidate) {
        candidates.push(candidate);
    }
}

#[derive(Debug, Clone)]
pub struct SherpaOnnxProvider {
    config: SherpaOnnxConfig,
}

impl SherpaOnnxProvider {
    pub fn new(config: SherpaOnnxConfig) -> Result<Self, ProviderError> {
        config.model.validate()?;
        Ok(Self { config })
    }

    pub fn config(&self) -> &SherpaOnnxConfig {
        &self.config
    }
}

impl AsrProvider for SherpaOnnxProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::SherpaOnnx
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            streaming: true,
            endpoint_detection: true,
            accepts_opus: false,
            accepts_pcm: true,
            requires_network: false,
        }
    }

    fn start_session<'a>(
        &'a self,
        options: SessionOptions,
    ) -> BoxFuture<'a, Result<ProviderSession, ProviderError>> {
        #[cfg(all(windows, feature = "sherpa-onnx"))]
        {
            let config = self.config.clone();
            Box::pin(async move { native::start_session(config, options).await })
        }

        #[cfg(not(all(windows, feature = "sherpa-onnx")))]
        {
            let _ = options;
            Box::pin(async {
                Err(ProviderError::BackendUnavailable(
                    ProviderKind::SherpaOnnx,
                    "当前构建未启用 sherpa-onnx；请使用 Cargo feature `sherpa-onnx` 构建 Windows 版本 / this build does not include the `sherpa-onnx` feature"
                        .to_string(),
                ))
            })
        }
    }
}

#[cfg(all(windows, feature = "sherpa-onnx"))]
mod native {
    use super::*;
    use crate::asr::{AsrEvent, AudioFrame, SessionCommand, SessionEndReason};
    use crate::offline::ResolvedModelFiles;
    use std::ffi::{c_char, c_void, CStr, CString};
    use std::mem;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;
    use std::sync::{mpsc as std_mpsc, Arc};
    use tokio::sync::{mpsc, watch};
    use windows::core::{PCSTR, PCWSTR};
    use windows::Win32::Foundation::{FreeLibrary, HANDLE, HMODULE};
    use windows::Win32::System::LibraryLoader::{
        GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
        LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
    };

    pub fn probe_library(path: &Path) -> Result<String, ProviderError> {
        let api = unsafe { SherpaApi::load(path)? };
        Ok(api.version())
    }

    pub async fn start_session(
        config: SherpaOnnxConfig,
        options: SessionOptions,
    ) -> Result<ProviderSession, ProviderError> {
        let library_path = config.resolve_library_path()?;
        let capacity = options.result_channel_capacity.max(1);
        let engine = tokio::task::spawn_blocking(move || {
            SherpaEngine::create(config, &library_path, options)
        })
        .await
        .map_err(|error| {
            ProviderError::SessionFailed(
                ProviderKind::SherpaOnnx,
                format!("初始化线程失败 / initialization task failed: {error}"),
            )
        })??;

        let (audio_tx, audio_rx) = mpsc::channel::<AudioFrame>(500);
        let (event_tx, event_rx) = mpsc::channel::<AsrEvent>(capacity);
        let (command_tx, command_rx) = watch::channel(SessionCommand::Running);
        let (worker_tx, worker_rx) = std_mpsc::channel::<WorkerMessage>();

        tokio::spawn(forward_session_input(audio_rx, command_rx, worker_tx));
        tokio::task::spawn_blocking(move || run_worker(engine, worker_rx, event_tx));

        Ok(ProviderSession::new(
            ProviderKind::SherpaOnnx,
            audio_tx,
            event_rx,
            command_tx,
        ))
    }

    async fn forward_session_input(
        mut audio_rx: mpsc::Receiver<AudioFrame>,
        mut command_rx: watch::Receiver<SessionCommand>,
        worker_tx: std_mpsc::Sender<WorkerMessage>,
    ) {
        loop {
            tokio::select! {
                command = command_rx.changed() => {
                    if command.is_err() {
                        let _ = worker_tx.send(WorkerMessage::Cancel);
                        break;
                    }
                    let command = *command_rx.borrow_and_update();
                    match command {
                        SessionCommand::Running => {}
                        SessionCommand::Cancel => {
                            let _ = worker_tx.send(WorkerMessage::Cancel);
                            break;
                        }
                        SessionCommand::Finish => {
                            while let Some(frame) = audio_rx.recv().await {
                                if worker_tx.send(WorkerMessage::Audio(frame)).is_err() {
                                    return;
                                }
                            }
                            let _ = worker_tx.send(WorkerMessage::Finish);
                            break;
                        }
                    }
                }
                frame = audio_rx.recv() => {
                    match frame {
                        Some(frame) => {
                            if worker_tx.send(WorkerMessage::Audio(frame)).is_err() {
                                break;
                            }
                        }
                        None => {
                            let command = *command_rx.borrow();
                            let message = if command == SessionCommand::Cancel {
                                WorkerMessage::Cancel
                            } else {
                                WorkerMessage::Finish
                            };
                            let _ = worker_tx.send(message);
                            break;
                        }
                    }
                }
            }
        }
    }

    enum WorkerMessage {
        Audio(AudioFrame),
        Finish,
        Cancel,
    }

    fn run_worker(
        mut engine: SherpaEngine,
        worker_rx: std_mpsc::Receiver<WorkerMessage>,
        event_tx: mpsc::Sender<AsrEvent>,
    ) {
        if event_tx
            .blocking_send(AsrEvent::SessionStarted {
                provider: ProviderKind::SherpaOnnx,
            })
            .is_err()
        {
            return;
        }

        let mut last_text = String::new();
        let mut speech_started = false;
        let mut segments = Vec::new();
        while let Ok(message) = worker_rx.recv() {
            match message {
                WorkerMessage::Audio(frame) => {
                    let (samples, sample_rate) = match frame.into_mono_f32() {
                        Ok(value) => value,
                        Err(error) => {
                            let _ = event_tx.blocking_send(AsrEvent::Error(error));
                            return;
                        }
                    };
                    if samples.is_empty() {
                        continue;
                    }

                    engine.accept_waveform(sample_rate, &samples);
                    engine.decode_ready();
                    if !publish_incremental(&engine, &event_tx, &mut last_text, &mut speech_started)
                    {
                        return;
                    }

                    if engine.is_endpoint() {
                        let text = engine.result_text();
                        if !text.is_empty() {
                            segments.push(text.clone());
                            if event_tx
                                .blocking_send(AsrEvent::FinalResult {
                                    text,
                                    endpoint: true,
                                })
                                .is_err()
                            {
                                return;
                            }
                        }
                        engine.reset();
                        last_text.clear();
                        speech_started = false;
                    }
                }
                WorkerMessage::Finish => {
                    engine.input_finished();
                    engine.decode_ready();
                    let text = engine.result_text();
                    if !text.is_empty() && segments.last() != Some(&text) {
                        segments.push(text.clone());
                        if event_tx
                            .blocking_send(AsrEvent::FinalResult {
                                text,
                                endpoint: false,
                            })
                            .is_err()
                        {
                            return;
                        }
                    }
                    let final_text = (!segments.is_empty()).then(|| segments.join(""));
                    let _ = event_tx.blocking_send(AsrEvent::SessionFinished {
                        reason: SessionEndReason::Completed,
                        final_text,
                    });
                    return;
                }
                WorkerMessage::Cancel => {
                    let _ = event_tx.blocking_send(AsrEvent::SessionFinished {
                        reason: SessionEndReason::Cancelled,
                        final_text: None,
                    });
                    return;
                }
            }
        }

        let _ = event_tx.blocking_send(AsrEvent::SessionFinished {
            reason: SessionEndReason::InputClosed,
            final_text: None,
        });
    }

    fn publish_incremental(
        engine: &SherpaEngine,
        event_tx: &mpsc::Sender<AsrEvent>,
        last_text: &mut String,
        speech_started: &mut bool,
    ) -> bool {
        let text = engine.result_text();
        if text.is_empty() || text == *last_text {
            return true;
        }
        if !*speech_started {
            if event_tx.blocking_send(AsrEvent::SpeechStarted).is_err() {
                return false;
            }
            *speech_started = true;
        }
        *last_text = text.clone();
        event_tx
            .blocking_send(AsrEvent::PartialResult { text })
            .is_ok()
    }

    struct SherpaEngine {
        api: Arc<SherpaApi>,
        recognizer: *const SherpaOnnxOnlineRecognizer,
        stream: *const SherpaOnnxOnlineStream,
        _strings: NativeStrings,
    }

    unsafe impl Send for SherpaEngine {}

    impl SherpaEngine {
        fn create(
            config: SherpaOnnxConfig,
            library_path: &Path,
            options: SessionOptions,
        ) -> Result<Self, ProviderError> {
            let api = Arc::new(unsafe { SherpaApi::load(library_path)? });
            let strings = NativeStrings::new(&config)?;
            let native_config = strings.recognizer_config(&config, options);
            let recognizer = unsafe { (api.create_recognizer)(&native_config) };
            if recognizer.is_null() {
                return Err(ProviderError::SessionFailed(
                    ProviderKind::SherpaOnnx,
                    format!(
                        "无法创建识别器 / failed to create recognizer (DLL version {})",
                        api.version()
                    ),
                ));
            }
            let stream = unsafe { (api.create_stream)(recognizer) };
            if stream.is_null() {
                unsafe { (api.destroy_recognizer)(recognizer) };
                return Err(ProviderError::SessionFailed(
                    ProviderKind::SherpaOnnx,
                    "无法创建流 / failed to create online stream".to_string(),
                ));
            }

            Ok(Self {
                api,
                recognizer,
                stream,
                _strings: strings,
            })
        }

        fn accept_waveform(&mut self, sample_rate: u32, samples: &[f32]) {
            unsafe {
                (self.api.accept_waveform)(
                    self.stream,
                    sample_rate.min(i32::MAX as u32) as i32,
                    samples.as_ptr(),
                    samples.len().min(i32::MAX as usize) as i32,
                );
            }
        }

        fn decode_ready(&mut self) {
            let mut steps = 0usize;
            unsafe {
                while (self.api.is_ready)(self.recognizer, self.stream) != 0 {
                    (self.api.decode)(self.recognizer, self.stream);
                    steps += 1;
                    if steps >= 10_000 {
                        tracing::error!("sherpa-onnx decode loop exceeded safety limit");
                        break;
                    }
                }
            }
        }

        fn result_text(&self) -> String {
            unsafe {
                let result = (self.api.get_result)(self.recognizer, self.stream);
                if result.is_null() {
                    return String::new();
                }
                let text = if (*result).text.is_null() {
                    String::new()
                } else {
                    CStr::from_ptr((*result).text)
                        .to_string_lossy()
                        .into_owned()
                };
                (self.api.destroy_result)(result);
                text
            }
        }

        fn is_endpoint(&self) -> bool {
            unsafe { (self.api.is_endpoint)(self.recognizer, self.stream) != 0 }
        }

        fn reset(&mut self) {
            unsafe { (self.api.reset)(self.recognizer, self.stream) }
        }

        fn input_finished(&mut self) {
            unsafe { (self.api.input_finished)(self.stream) }
        }
    }

    impl Drop for SherpaEngine {
        fn drop(&mut self) {
            unsafe {
                (self.api.destroy_stream)(self.stream);
                (self.api.destroy_recognizer)(self.recognizer);
            }
        }
    }

    struct NativeStrings {
        encoder: Option<CString>,
        decoder: Option<CString>,
        joiner: Option<CString>,
        model: Option<CString>,
        tokens: CString,
        provider: CString,
        decoding_method: CString,
    }

    impl NativeStrings {
        fn new(config: &SherpaOnnxConfig) -> Result<Self, ProviderError> {
            let path = |value: &Path| {
                CString::new(value.to_string_lossy().as_bytes()).map_err(|_| {
                    ProviderError::ModelUnavailable(format!(
                        "模型路径包含 NUL 字符 / model path contains NUL: {}",
                        value.display()
                    ))
                })
            };
            let string = |value: &str, label: &str| {
                CString::new(value).map_err(|_| {
                    ProviderError::SessionFailed(
                        ProviderKind::SherpaOnnx,
                        format!("{label} contains a NUL byte"),
                    )
                })
            };

            let (encoder, decoder, joiner, model, tokens) = match &config.model.files {
                ResolvedModelFiles::Transducer {
                    encoder,
                    decoder,
                    joiner,
                    tokens,
                } => (
                    Some(path(encoder)?),
                    Some(path(decoder)?),
                    Some(path(joiner)?),
                    None,
                    path(tokens)?,
                ),
                ResolvedModelFiles::Paraformer {
                    encoder,
                    decoder,
                    tokens,
                } => (
                    Some(path(encoder)?),
                    Some(path(decoder)?),
                    None,
                    None,
                    path(tokens)?,
                ),
                ResolvedModelFiles::Zipformer2Ctc { model, tokens } => {
                    (None, None, None, Some(path(model)?), path(tokens)?)
                }
            };

            Ok(Self {
                encoder,
                decoder,
                joiner,
                model,
                tokens,
                provider: string(&config.execution_provider, "execution_provider")?,
                decoding_method: string(&config.decoding_method, "decoding_method")?,
            })
        }

        fn recognizer_config(
            &self,
            config: &SherpaOnnxConfig,
            options: SessionOptions,
        ) -> SherpaOnnxOnlineRecognizerConfig {
            let mut native: SherpaOnnxOnlineRecognizerConfig = unsafe { mem::zeroed() };
            native.feat_config.sample_rate = config.model.sample_rate as i32;
            native.feat_config.feature_dim = config.model.feature_dim as i32;
            native.model_config.tokens = self.tokens.as_ptr();
            native.model_config.num_threads = config.num_threads.max(1);
            native.model_config.provider = self.provider.as_ptr();
            native.model_config.debug = i32::from(config.debug);
            native.decoding_method = self.decoding_method.as_ptr();
            native.max_active_paths = config.max_active_paths.max(1);
            native.enable_endpoint = i32::from(options.endpoint.enabled);
            native.rule1_min_trailing_silence =
                options.endpoint.rule1_min_trailing_silence.max(0.0);
            native.rule2_min_trailing_silence =
                options.endpoint.rule2_min_trailing_silence.max(0.0);
            native.rule3_min_utterance_length =
                options.endpoint.rule3_min_utterance_length.max(0.0);

            match config.model.family {
                super::super::ModelFamily::OnlineTransducer => {
                    native.model_config.transducer.encoder = ptr(self.encoder.as_ref());
                    native.model_config.transducer.decoder = ptr(self.decoder.as_ref());
                    native.model_config.transducer.joiner = ptr(self.joiner.as_ref());
                }
                super::super::ModelFamily::OnlineParaformer => {
                    native.model_config.paraformer.encoder = ptr(self.encoder.as_ref());
                    native.model_config.paraformer.decoder = ptr(self.decoder.as_ref());
                }
                super::super::ModelFamily::OnlineZipformer2Ctc => {
                    native.model_config.zipformer2_ctc.model = ptr(self.model.as_ref());
                }
            }
            native
        }
    }

    fn ptr(value: Option<&CString>) -> *const c_char {
        value.map_or(ptr::null(), |value| value.as_ptr())
    }

    struct SherpaApi {
        module: HMODULE,
        get_version: unsafe extern "C" fn() -> *const c_char,
        create_recognizer: unsafe extern "C" fn(
            *const SherpaOnnxOnlineRecognizerConfig,
        ) -> *const SherpaOnnxOnlineRecognizer,
        destroy_recognizer: unsafe extern "C" fn(*const SherpaOnnxOnlineRecognizer),
        create_stream: unsafe extern "C" fn(
            *const SherpaOnnxOnlineRecognizer,
        ) -> *const SherpaOnnxOnlineStream,
        destroy_stream: unsafe extern "C" fn(*const SherpaOnnxOnlineStream),
        accept_waveform: unsafe extern "C" fn(*const SherpaOnnxOnlineStream, i32, *const f32, i32),
        is_ready: unsafe extern "C" fn(
            *const SherpaOnnxOnlineRecognizer,
            *const SherpaOnnxOnlineStream,
        ) -> i32,
        decode:
            unsafe extern "C" fn(*const SherpaOnnxOnlineRecognizer, *const SherpaOnnxOnlineStream),
        get_result: unsafe extern "C" fn(
            *const SherpaOnnxOnlineRecognizer,
            *const SherpaOnnxOnlineStream,
        ) -> *const SherpaOnnxOnlineRecognizerResult,
        destroy_result: unsafe extern "C" fn(*const SherpaOnnxOnlineRecognizerResult),
        reset:
            unsafe extern "C" fn(*const SherpaOnnxOnlineRecognizer, *const SherpaOnnxOnlineStream),
        input_finished: unsafe extern "C" fn(*const SherpaOnnxOnlineStream),
        is_endpoint: unsafe extern "C" fn(
            *const SherpaOnnxOnlineRecognizer,
            *const SherpaOnnxOnlineStream,
        ) -> i32,
    }

    unsafe impl Send for SherpaApi {}
    unsafe impl Sync for SherpaApi {}

    impl SherpaApi {
        unsafe fn load(path: &Path) -> Result<Self, ProviderError> {
            let wide: Vec<u16> = path
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let module = LoadLibraryExW(
                PCWSTR(wide.as_ptr()),
                HANDLE(0),
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
            )
            .map_err(|error| {
                ProviderError::BackendUnavailable(
                    ProviderKind::SherpaOnnx,
                    format!(
                        "无法加载 DLL / failed to load {}: {}. 请确认 onnxruntime.dll 与其位于同一目录 / keep onnxruntime.dll beside it",
                        path.display(),
                        error
                    ),
                )
            })?;

            macro_rules! symbol {
                ($name:literal, $ty:ty) => {{
                    let raw = GetProcAddress(module, PCSTR(concat!($name, "\0").as_ptr()));
                    let Some(raw) = raw else {
                        let _ = FreeLibrary(module);
                        return Err(ProviderError::BackendUnavailable(
                            ProviderKind::SherpaOnnx,
                            format!("DLL 缺少符号 / missing C API symbol: {}", $name),
                        ));
                    };
                    mem::transmute::<unsafe extern "system" fn() -> isize, $ty>(raw)
                }};
            }

            let api = Self {
                module,
                get_version: symbol!(
                    "SherpaOnnxGetVersionStr",
                    unsafe extern "C" fn() -> *const c_char
                ),
                create_recognizer: symbol!(
                    "SherpaOnnxCreateOnlineRecognizer",
                    unsafe extern "C" fn(
                        *const SherpaOnnxOnlineRecognizerConfig,
                    ) -> *const SherpaOnnxOnlineRecognizer
                ),
                destroy_recognizer: symbol!(
                    "SherpaOnnxDestroyOnlineRecognizer",
                    unsafe extern "C" fn(*const SherpaOnnxOnlineRecognizer)
                ),
                create_stream: symbol!(
                    "SherpaOnnxCreateOnlineStream",
                    unsafe extern "C" fn(
                        *const SherpaOnnxOnlineRecognizer,
                    ) -> *const SherpaOnnxOnlineStream
                ),
                destroy_stream: symbol!(
                    "SherpaOnnxDestroyOnlineStream",
                    unsafe extern "C" fn(*const SherpaOnnxOnlineStream)
                ),
                accept_waveform: symbol!(
                    "SherpaOnnxOnlineStreamAcceptWaveform",
                    unsafe extern "C" fn(*const SherpaOnnxOnlineStream, i32, *const f32, i32)
                ),
                is_ready: symbol!(
                    "SherpaOnnxIsOnlineStreamReady",
                    unsafe extern "C" fn(
                        *const SherpaOnnxOnlineRecognizer,
                        *const SherpaOnnxOnlineStream,
                    ) -> i32
                ),
                decode: symbol!(
                    "SherpaOnnxDecodeOnlineStream",
                    unsafe extern "C" fn(
                        *const SherpaOnnxOnlineRecognizer,
                        *const SherpaOnnxOnlineStream,
                    )
                ),
                get_result: symbol!(
                    "SherpaOnnxGetOnlineStreamResult",
                    unsafe extern "C" fn(
                        *const SherpaOnnxOnlineRecognizer,
                        *const SherpaOnnxOnlineStream,
                    )
                        -> *const SherpaOnnxOnlineRecognizerResult
                ),
                destroy_result: symbol!(
                    "SherpaOnnxDestroyOnlineRecognizerResult",
                    unsafe extern "C" fn(*const SherpaOnnxOnlineRecognizerResult)
                ),
                reset: symbol!(
                    "SherpaOnnxOnlineStreamReset",
                    unsafe extern "C" fn(
                        *const SherpaOnnxOnlineRecognizer,
                        *const SherpaOnnxOnlineStream,
                    )
                ),
                input_finished: symbol!(
                    "SherpaOnnxOnlineStreamInputFinished",
                    unsafe extern "C" fn(*const SherpaOnnxOnlineStream)
                ),
                is_endpoint: symbol!(
                    "SherpaOnnxOnlineStreamIsEndpoint",
                    unsafe extern "C" fn(
                        *const SherpaOnnxOnlineRecognizer,
                        *const SherpaOnnxOnlineStream,
                    ) -> i32
                ),
            };
            let version = api.version();
            if !is_supported_version(&version) {
                return Err(ProviderError::BackendUnavailable(
                    ProviderKind::SherpaOnnx,
                    format!(
                        "sherpa-onnx DLL 版本过旧或无法识别 / unsupported DLL version {version}; minimum is {}.{}.{}",
                        MINIMUM_SHERPA_VERSION.0,
                        MINIMUM_SHERPA_VERSION.1,
                        MINIMUM_SHERPA_VERSION.2
                    ),
                ));
            }
            Ok(api)
        }

        fn version(&self) -> String {
            unsafe {
                let value = (self.get_version)();
                if value.is_null() {
                    "unknown".to_string()
                } else {
                    CStr::from_ptr(value).to_string_lossy().into_owned()
                }
            }
        }
    }

    impl Drop for SherpaApi {
        fn drop(&mut self) {
            unsafe {
                let _ = FreeLibrary(self.module);
            }
        }
    }

    pub(super) fn is_supported_version(version: &str) -> bool {
        let mut parts = version
            .trim_start_matches('v')
            .split('.')
            .take(3)
            .map(|part| {
                part.chars()
                    .take_while(char::is_ascii_digit)
                    .collect::<String>()
                    .parse::<u32>()
            });
        let parsed = match (parts.next(), parts.next(), parts.next()) {
            (Some(Ok(major)), Some(Ok(minor)), Some(Ok(patch))) => (major, minor, patch),
            _ => return false,
        };
        parsed >= MINIMUM_SHERPA_VERSION
    }

    #[repr(C)]
    struct SherpaOnnxOnlineRecognizer {
        _private: [u8; 0],
    }

    #[repr(C)]
    struct SherpaOnnxOnlineStream {
        _private: [u8; 0],
    }

    #[repr(C)]
    struct SherpaOnnxOnlineTransducerModelConfig {
        encoder: *const c_char,
        decoder: *const c_char,
        joiner: *const c_char,
    }

    #[repr(C)]
    struct SherpaOnnxOnlineParaformerModelConfig {
        encoder: *const c_char,
        decoder: *const c_char,
    }

    #[repr(C)]
    struct SherpaOnnxOnlineZipformer2CtcModelConfig {
        model: *const c_char,
    }

    #[repr(C)]
    struct SherpaOnnxOnlineNemoCtcModelConfig {
        model: *const c_char,
    }

    #[repr(C)]
    struct SherpaOnnxOnlineToneCtcModelConfig {
        model: *const c_char,
    }

    #[repr(C)]
    struct SherpaOnnxOnlineModelConfig {
        transducer: SherpaOnnxOnlineTransducerModelConfig,
        paraformer: SherpaOnnxOnlineParaformerModelConfig,
        zipformer2_ctc: SherpaOnnxOnlineZipformer2CtcModelConfig,
        tokens: *const c_char,
        num_threads: i32,
        provider: *const c_char,
        debug: i32,
        model_type: *const c_char,
        modeling_unit: *const c_char,
        bpe_vocab: *const c_char,
        tokens_buf: *const c_char,
        tokens_buf_size: i32,
        nemo_ctc: SherpaOnnxOnlineNemoCtcModelConfig,
        t_one_ctc: SherpaOnnxOnlineToneCtcModelConfig,
    }

    #[repr(C)]
    struct SherpaOnnxFeatureConfig {
        sample_rate: i32,
        feature_dim: i32,
    }

    #[repr(C)]
    struct SherpaOnnxOnlineCtcFstDecoderConfig {
        graph: *const c_char,
        max_active: i32,
    }

    #[repr(C)]
    struct SherpaOnnxHomophoneReplacerConfig {
        dict_dir: *const c_char,
        lexicon: *const c_char,
        rule_fsts: *const c_char,
    }

    #[repr(C)]
    struct SherpaOnnxOnlineRecognizerConfig {
        feat_config: SherpaOnnxFeatureConfig,
        model_config: SherpaOnnxOnlineModelConfig,
        decoding_method: *const c_char,
        max_active_paths: i32,
        enable_endpoint: i32,
        rule1_min_trailing_silence: f32,
        rule2_min_trailing_silence: f32,
        rule3_min_utterance_length: f32,
        hotwords_file: *const c_char,
        hotwords_score: f32,
        ctc_fst_decoder_config: SherpaOnnxOnlineCtcFstDecoderConfig,
        rule_fsts: *const c_char,
        rule_fars: *const c_char,
        blank_penalty: f32,
        hotwords_buf: *const c_char,
        hotwords_buf_size: i32,
        hr: SherpaOnnxHomophoneReplacerConfig,
    }

    #[repr(C)]
    struct SherpaOnnxOnlineRecognizerResult {
        text: *const c_char,
        tokens: *const c_char,
        tokens_arr: *const *const c_char,
        timestamps: *mut f32,
        count: i32,
        json: *const c_char,
    }

    #[allow(dead_code)]
    fn _assert_ffi_uses_c_void(_: *const c_void) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asr::AsrProvider;
    #[cfg(all(windows, feature = "sherpa-onnx"))]
    use crate::asr::{AsrEvent, AudioFrame};
    use crate::offline::{ModelFamily, ResolvedModelFiles};
    #[cfg(all(windows, feature = "sherpa-onnx"))]
    use std::fs;
    #[cfg(all(windows, feature = "sherpa-onnx"))]
    use std::time::Duration;

    fn fake_model(root: PathBuf) -> ResolvedModel {
        ResolvedModel {
            name: "test".to_string(),
            root: root.clone(),
            family: ModelFamily::OnlineTransducer,
            sample_rate: 16_000,
            feature_dim: 80,
            files: ResolvedModelFiles::Transducer {
                encoder: root.join("encoder.onnx"),
                decoder: root.join("decoder.onnx"),
                joiner: root.join("joiner.onnx"),
                tokens: root.join("tokens.txt"),
            },
        }
    }

    #[test]
    fn explicit_library_directory_is_resolved_to_dll_name() {
        let root = PathBuf::from("C:/Aiko/models/test");
        let config = SherpaOnnxConfig {
            model: fake_model(root),
            library_path: Some(PathBuf::from("C:/Aiko/runtime/lib")),
            num_threads: 2,
            execution_provider: "cpu".to_string(),
            debug: false,
            decoding_method: "greedy_search".to_string(),
            max_active_paths: 4,
        };

        assert_eq!(
            config.library_candidates()[0],
            PathBuf::from("C:/Aiko/runtime/lib").join(SHERPA_DLL_NAME)
        );
    }

    #[cfg(not(all(windows, feature = "sherpa-onnx")))]
    #[tokio::test]
    async fn disabled_feature_reports_backend_unavailable() {
        let root = PathBuf::from("C:/Aiko/models/test");
        let provider = SherpaOnnxProvider {
            config: SherpaOnnxConfig {
                model: fake_model(root),
                library_path: None,
                num_threads: 2,
                execution_provider: "cpu".to_string(),
                debug: false,
                decoding_method: "greedy_search".to_string(),
                max_active_paths: 4,
            },
        };

        let error = provider
            .start_session(SessionOptions::default())
            .await
            .err()
            .expect("disabled backend should fail");
        assert!(matches!(
            error,
            ProviderError::BackendUnavailable(ProviderKind::SherpaOnnx, _)
        ));
    }

    #[cfg(all(windows, feature = "sherpa-onnx"))]
    #[test]
    fn native_version_gate_accepts_current_and_rejects_old_builds() {
        assert!(super::native::is_supported_version("1.13.2"));
        assert!(super::native::is_supported_version("v1.14.0"));
        assert!(!super::native::is_supported_version("1.13.1"));
        assert!(!super::native::is_supported_version("unknown"));
    }

    #[cfg(all(windows, feature = "sherpa-onnx"))]
    #[test]
    fn probes_official_dll_when_test_path_is_configured() {
        let Some(path) = env::var_os("AIKO_IME_TEST_SHERPA_DLL") else {
            return;
        };
        let root = PathBuf::from("C:/Aiko/models/test");
        let mut config = SherpaOnnxConfig {
            model: fake_model(root),
            library_path: Some(PathBuf::from(path)),
            num_threads: 2,
            execution_provider: "cpu".to_string(),
            debug: false,
            decoding_method: "greedy_search".to_string(),
            max_active_paths: 4,
        };
        config.library_path = Some(config.resolve_library_path().unwrap());

        let version = config.probe_backend().unwrap();
        assert!(!version.is_empty());
        assert_ne!(version, "unknown");
    }

    #[cfg(all(windows, feature = "sherpa-onnx"))]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn recognizes_test_wave_when_runtime_assets_are_configured() {
        let (Some(dll), Some(model_dir), Some(wave_path)) = (
            env::var_os("AIKO_IME_TEST_SHERPA_DLL"),
            env::var_os("AIKO_IME_TEST_SHERPA_MODEL"),
            env::var_os("AIKO_IME_TEST_SHERPA_WAV"),
        ) else {
            return;
        };

        let model = crate::offline::ModelManager::load_dir(PathBuf::from(model_dir)).unwrap();
        let mut config = SherpaOnnxConfig::new(model).unwrap();
        config.library_path = Some(PathBuf::from(dll));
        config.num_threads = 2;
        let provider = SherpaOnnxProvider::new(config).unwrap();
        let mut session = provider
            .start_session(SessionOptions::default())
            .await
            .unwrap();
        let (samples, sample_rate, channels) = read_pcm16_wave(Path::new(&wave_path));

        for chunk in samples.chunks(320 * channels as usize) {
            session
                .send_audio(AudioFrame::pcm_i16(chunk.to_vec(), sample_rate, channels))
                .await
                .unwrap();
        }
        for _ in 0..125 {
            session
                .send_audio(AudioFrame::pcm_i16(
                    vec![0; 320 * channels as usize],
                    sample_rate,
                    channels,
                ))
                .await
                .unwrap();
        }
        session.finish().unwrap();

        let mut final_text = String::new();
        let mut endpoint_seen = false;
        loop {
            let event = tokio::time::timeout(Duration::from_secs(30), session.recv())
                .await
                .expect("offline recognition timed out")
                .expect("offline event channel closed");
            match event {
                AsrEvent::FinalResult { text, endpoint } => {
                    final_text.push_str(&text);
                    endpoint_seen |= endpoint;
                }
                AsrEvent::SessionFinished { .. } => break,
                AsrEvent::Error(error) => panic!("offline recognition failed: {error}"),
                _ => {}
            }
        }

        assert!(
            !final_text.trim().is_empty(),
            "official test wave should produce a recognition result"
        );
        assert!(endpoint_seen, "trailing silence should trigger endpointing");

        let mut cancelled = provider
            .start_session(SessionOptions::default())
            .await
            .unwrap();
        cancelled
            .send_audio(AudioFrame::pcm_i16(vec![0; 320], 16_000, 1))
            .await
            .unwrap();
        cancelled.cancel().unwrap();
        loop {
            let event = tokio::time::timeout(Duration::from_secs(10), cancelled.recv())
                .await
                .expect("cancel event timed out")
                .expect("cancel event channel closed");
            if let AsrEvent::SessionFinished { reason, .. } = event {
                assert_eq!(reason, crate::asr::SessionEndReason::Cancelled);
                break;
            }
        }
    }

    #[cfg(all(windows, feature = "sherpa-onnx"))]
    fn read_pcm16_wave(path: &Path) -> (Vec<i16>, u32, u16) {
        let bytes = fs::read(path).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");

        let mut offset = 12usize;
        let mut format = None;
        let mut data = None;
        while offset + 8 <= bytes.len() {
            let chunk_id = &bytes[offset..offset + 4];
            let chunk_len =
                u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
            let start = offset + 8;
            let end = start + chunk_len;
            assert!(end <= bytes.len());
            if chunk_id == b"fmt " {
                let audio_format = u16::from_le_bytes(bytes[start..start + 2].try_into().unwrap());
                let channels = u16::from_le_bytes(bytes[start + 2..start + 4].try_into().unwrap());
                let sample_rate =
                    u32::from_le_bytes(bytes[start + 4..start + 8].try_into().unwrap());
                let bits = u16::from_le_bytes(bytes[start + 14..start + 16].try_into().unwrap());
                format = Some((audio_format, channels, sample_rate, bits));
            } else if chunk_id == b"data" {
                data = Some(&bytes[start..end]);
            }
            offset = end + (chunk_len % 2);
        }

        let (audio_format, channels, sample_rate, bits) = format.expect("missing fmt chunk");
        assert_eq!(audio_format, 1, "test wave must use integer PCM");
        assert_eq!(bits, 16, "test wave must use PCM16");
        let samples = data
            .expect("missing data chunk")
            .chunks_exact(2)
            .map(|sample| i16::from_le_bytes([sample[0], sample[1]]))
            .collect();
        (samples, sample_rate, channels)
    }
}
