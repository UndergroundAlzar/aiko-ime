//! Voice Controller
//!
//! Coordinates voice input between audio capture, ASR, and text insertion.
//! The online Doubao backend streams Opus frames over WebSocket; the offline
//! sherpa-onnx backend (when configured and available) consumes raw PCM via
//! the provider contract. When the offline backend cannot start, the session
//! falls back to the online backend after notifying the user.

use anyhow::Result;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;

use crate::asr::{
    build_provider_from_config, select_provider_kind, AsrClient, AsrEvent, AudioFrame,
    ProviderKind, ResponseType, SessionOptions,
};
use crate::audio::AudioCapture;
use crate::business::{TextInputError, TextInserter};
use crate::data::AppConfig;
use crate::platform::{notify_input_protection, InputTarget};

/// Voice input controller
pub struct VoiceController {
    asr_client: Arc<AsrClient>,
    audio_capture: Arc<AudioCapture>,
    text_inserter: Arc<TextInserter>,
    is_recording: Arc<AtomicBool>,
    /// Abort signal used by cancel paths. Confirm/stop should drain final ASR results.
    stop_signal: Arc<AtomicBool>,
    /// UI-facing marker that this session ended because the user pressed confirm/cancel/hotkey.
    user_stop_signal: Arc<AtomicBool>,
    /// Net characters this session has typed into the focused window and not yet
    /// committed away. Used by `cancel()` to undo the whole dictation.
    session_chars: Arc<AtomicI64>,
    session_target: Option<InputTarget>,
    last_input_error: Arc<std::sync::Mutex<Option<TextInputError>>>,
    result_task: Option<tokio::task::JoinHandle<()>>,
    config: AppConfig,
}

impl VoiceController {
    /// Create a new voice controller
    pub fn new(
        asr_client: Arc<AsrClient>,
        audio_capture: Arc<AudioCapture>,
        text_inserter: Arc<TextInserter>,
        config: AppConfig,
    ) -> Self {
        Self {
            asr_client,
            audio_capture,
            text_inserter,
            is_recording: Arc::new(AtomicBool::new(false)),
            stop_signal: Arc::new(AtomicBool::new(false)),
            user_stop_signal: Arc::new(AtomicBool::new(false)),
            session_chars: Arc::new(AtomicI64::new(0)),
            session_target: None,
            last_input_error: Arc::new(std::sync::Mutex::new(None)),
            result_task: None,
            config,
        }
    }

    /// Check if currently recording
    pub fn is_recording(&self) -> bool {
        self.is_recording.load(Ordering::SeqCst)
    }

    /// Whether the current/most recent session was ended by an explicit user
    /// stop or cancel (as opposed to ending on its own, e.g. server hangup).
    pub fn was_stopped_by_user(&self) -> bool {
        self.user_stop_signal.load(Ordering::SeqCst)
    }

    /// Return the most recent target-validation or text-injection failure.
    pub fn last_input_error(&self) -> Option<TextInputError> {
        self.last_input_error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Toggle voice input on/off
    pub async fn toggle(&mut self) -> Result<()> {
        if self.is_recording() {
            self.stop().await
        } else {
            self.start().await
        }
    }

    /// Start voice input
    pub async fn start(&mut self) -> Result<()> {
        if self.is_recording() {
            return Ok(());
        }

        if let Some(task) = self.result_task.take() {
            Self::await_result_task(task, "previous session cleanup").await;
        }

        // Reload configuration from file to get latest settings
        if let Ok(latest_config) = crate::data::AppConfig::load_or_default() {
            self.config = latest_config;
        }

        let input_target = match self.text_inserter.capture_target() {
            Ok(target) => target,
            Err(error) => {
                notify_input_failure(
                    "无法开始语音输入",
                    &error,
                    "请先点击要输入文字的窗口，然后再开始录音。",
                );
                return Err(error.into());
            }
        };
        self.session_target = Some(input_target);
        *self
            .last_input_error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;

        tracing::info!("Starting voice input...");
        self.is_recording.store(true, Ordering::SeqCst);
        self.stop_signal.store(false, Ordering::SeqCst);
        self.user_stop_signal.store(false, Ordering::SeqCst);
        self.session_chars.store(0, Ordering::SeqCst);

        // The offline backend is attempted first when configured; any failure
        // (missing feature build, missing model, missing DLL) falls back to the
        // online backend so dictation keeps working.
        if matches!(
            select_provider_kind(&self.config.asr),
            Ok(ProviderKind::SherpaOnnx)
        ) {
            match self.start_offline_session(input_target).await {
                Ok(()) => return Ok(()),
                Err(error) => {
                    tracing::warn!("Offline ASR unavailable, falling back to online: {error}");
                    let message =
                        format!("离线识别暂不可用，已回退到在线识别。\n\n原因 / Reason:\n{error}");
                    std::thread::spawn(move || notify_input_protection(&message));
                }
            }
        }

        self.start_online_session(input_target).await
    }

    /// Start a session against the online Doubao WebSocket backend.
    async fn start_online_session(&mut self, input_target: InputTarget) -> Result<()> {
        // Start audio capture
        tracing::debug!("Starting audio capture...");
        let audio_rx = match self.audio_capture.start(&self.config.audio.input_device) {
            Ok(rx) => rx,
            Err(e) => {
                self.reset_session_flags();
                return Err(e);
            }
        };
        tracing::info!("Audio capture started, frames will be sent to ASR");

        // Start ASR
        tracing::debug!("Connecting to ASR server...");
        let mut result_rx = match self.asr_client.start_realtime(audio_rx).await {
            Ok(rx) => rx,
            Err(e) => {
                self.audio_capture.stop();
                self.reset_session_flags();
                return Err(e);
            }
        };
        tracing::info!("ASR connection established");

        let io = SessionIo {
            text_inserter: self.text_inserter.clone(),
            target: input_target,
            session_chars: self.session_chars.clone(),
            last_input_error: self.last_input_error.clone(),
            config: self.config.clone(),
        };
        let is_recording = self.is_recording.clone();
        let stop_signal = self.stop_signal.clone();
        let audio_capture = self.audio_capture.clone();

        // Spawn result processing task
        self.result_task = Some(tokio::spawn(async move {
            let mut last_text = String::new();
            let mut response_count = 0u32;

            tracing::info!("ASR result processing task started");

            'results: loop {
                // Check stop signal
                if stop_signal.load(Ordering::SeqCst) {
                    tracing::info!(
                        "Voice input cancelled by user (processed {} responses)",
                        response_count
                    );
                    break;
                }

                // Use timeout to periodically check stop signal
                match tokio::time::timeout(std::time::Duration::from_millis(100), result_rx.recv())
                    .await
                {
                    Ok(Some(response)) => {
                        response_count += 1;
                        match response.response_type {
                            ResponseType::InterimResult => {
                                tracing::debug!("[INTERIM #{}] {}", response_count, response.text);
                                if !io.apply_interim(&mut last_text, &response.text) {
                                    break 'results;
                                }
                            }
                            ResponseType::FinalResult => {
                                tracing::info!("[FINAL #{}] {}", response_count, response.text);
                                if !io.apply_final(&mut last_text, &response.text).await {
                                    break 'results;
                                }
                            }
                            ResponseType::SessionFinished => {
                                tracing::info!(
                                    "ASR session finished (total {} responses)",
                                    response_count
                                );
                                break;
                            }
                            ResponseType::Error => {
                                tracing::error!("ASR error: {}", response.error_msg);
                                break;
                            }
                            _ => {
                                tracing::trace!(
                                    "Other response type: {:?}",
                                    response.response_type
                                );
                            }
                        }
                    }
                    Ok(None) => {
                        // Channel closed
                        tracing::warn!("ASR result channel closed unexpectedly");
                        break;
                    }
                    Err(_) => {
                        // Timeout, continue loop to check stop signal
                        continue;
                    }
                }
            }

            // Cleanup
            audio_capture.stop();
            is_recording.store(false, Ordering::SeqCst);
        }));

        Ok(())
    }

    /// Start a session against the configured offline provider (sherpa-onnx).
    async fn start_offline_session(&mut self, input_target: InputTarget) -> Result<()> {
        let provider = build_provider_from_config(&self.config.asr, self.asr_client.clone())
            .map_err(|error| anyhow::anyhow!("{error}"))?;

        let mut pcm_rx = self
            .audio_capture
            .start_pcm(&self.config.audio.input_device)?;

        let mut session = match provider.start_session(SessionOptions::default()).await {
            Ok(session) => session,
            Err(error) => {
                self.audio_capture.stop();
                // Drain the channel so the capture thread can exit promptly.
                pcm_rx.close();
                return Err(anyhow::anyhow!("{error}"));
            }
        };
        tracing::info!("Offline ASR session started ({})", session.provider());

        let Some(audio_tx) = session.audio_sender() else {
            self.audio_capture.stop();
            return Err(anyhow::anyhow!("offline session has no audio input"));
        };

        // Feeder: capture thread -> provider session. Dropping `audio_tx` when
        // the capture channel closes signals end-of-input to the provider.
        tokio::spawn(async move {
            while let Some(samples) = pcm_rx.recv().await {
                let frame = AudioFrame::pcm_i16(samples, 16_000, 1);
                if audio_tx.send(frame).await.is_err() {
                    break;
                }
            }
        });

        let io = SessionIo {
            text_inserter: self.text_inserter.clone(),
            target: input_target,
            session_chars: self.session_chars.clone(),
            last_input_error: self.last_input_error.clone(),
            config: self.config.clone(),
        };
        let is_recording = self.is_recording.clone();
        let stop_signal = self.stop_signal.clone();
        let audio_capture = self.audio_capture.clone();

        self.result_task = Some(tokio::spawn(async move {
            let mut last_text = String::new();
            let mut finish_requested = false;
            let mut inserted_any_final = false;

            tracing::info!("Offline ASR result processing task started");

            'results: loop {
                if stop_signal.load(Ordering::SeqCst) && !finish_requested {
                    finish_requested = true;
                    // Cancel paths should not wait for or insert a final result.
                    let _ = session.cancel();
                }

                match tokio::time::timeout(std::time::Duration::from_millis(100), session.recv())
                    .await
                {
                    Ok(Some(event)) => match event {
                        AsrEvent::SessionStarted { provider } => {
                            tracing::debug!("Offline session started: {provider}");
                        }
                        AsrEvent::SpeechStarted => {
                            tracing::trace!("Speech started");
                        }
                        AsrEvent::PartialResult { text } => {
                            tracing::debug!("[OFFLINE PARTIAL] {}", text);
                            if !io.apply_interim(&mut last_text, &text) {
                                break 'results;
                            }
                        }
                        AsrEvent::FinalResult { text, .. } => {
                            tracing::info!("[OFFLINE FINAL] {}", text);
                            if !text.is_empty() {
                                inserted_any_final = true;
                            }
                            if !io.apply_final(&mut last_text, &text).await {
                                break 'results;
                            }
                        }
                        AsrEvent::SessionFinished { final_text, .. } => {
                            // A short utterance may only surface here.
                            if let Some(text) = final_text {
                                if !text.is_empty() && !inserted_any_final {
                                    let _ = io.apply_final(&mut last_text, &text).await;
                                }
                            }
                            tracing::info!("Offline ASR session finished");
                            break;
                        }
                        AsrEvent::Error(error) => {
                            tracing::error!("Offline ASR error: {error}");
                            report_input_error(
                                &io.last_input_error,
                                "离线识别已停止",
                                TextInputError::Injection(error.to_string()),
                                "离线识别运行中返回错误，Aiko IME 已停止本次输入，避免写入不完整文本。",
                            );
                            break;
                        }
                    },
                    Ok(None) => {
                        tracing::warn!("Offline ASR event channel closed");
                        break;
                    }
                    Err(_) => continue,
                }
            }

            audio_capture.stop();
            is_recording.store(false, Ordering::SeqCst);
        }));

        Ok(())
    }

    fn reset_session_flags(&mut self) {
        self.is_recording.store(false, Ordering::SeqCst);
        self.stop_signal.store(false, Ordering::SeqCst);
        self.user_stop_signal.store(false, Ordering::SeqCst);
        self.session_target = None;
    }

    async fn await_result_task(task: tokio::task::JoinHandle<()>, context: &str) {
        let mut task = task;
        match tokio::time::timeout(std::time::Duration::from_secs(8), &mut task).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                tracing::warn!("{} result task ended with join error: {}", context, error);
            }
            Err(_) => {
                tracing::warn!("{} timed out while waiting for ASR finalization", context);
                task.abort();
                let _ = task.await;
            }
        }
    }

    /// Stop voice input and **keep** the text already typed (the ✓ / confirm action).
    pub async fn stop(&mut self) -> Result<()> {
        if !self.is_recording() {
            return Ok(());
        }

        tracing::info!("Stopping voice input...");

        // Confirm keeps typed text: stop capture and let the backend flush final results.
        self.user_stop_signal.store(true, Ordering::SeqCst);
        self.audio_capture.stop();

        if let Some(task) = self.result_task.take() {
            Self::await_result_task(task, "confirm stop").await;
        }

        self.is_recording.store(false, Ordering::SeqCst);
        self.stop_signal.store(false, Ordering::SeqCst);
        // Text is committed; nothing left for cancel() to undo.
        self.session_chars.store(0, Ordering::SeqCst);
        self.session_target = None;

        Ok(())
    }

    /// Stop voice input and **discard** everything typed this session (the ✗ / cancel
    /// action). Sends backspaces to remove the dictated text from the focused window.
    pub async fn cancel(&mut self) -> Result<()> {
        if !self.is_recording() && self.session_chars.load(Ordering::SeqCst) == 0 {
            return Ok(());
        }

        tracing::info!("Cancelling voice input...");

        // Stop producing new text first, then let the result task drain.
        self.user_stop_signal.store(true, Ordering::SeqCst);
        self.stop_signal.store(true, Ordering::SeqCst);
        self.audio_capture.stop();
        if let Some(task) = self.result_task.take() {
            Self::await_result_task(task, "cancel").await;
        }
        self.is_recording.store(false, Ordering::SeqCst);
        self.stop_signal.store(false, Ordering::SeqCst);

        // Undo whatever this session typed.
        let n = self.session_chars.load(Ordering::SeqCst);
        if n > 0 {
            tracing::info!("Cancel: deleting {} dictated characters", n);
            let target = match self.session_target {
                Some(target) => target,
                None => {
                    let message =
                        "取消操作已停止：录音目标窗口状态已丢失\n\n为避免误删，Aiko IME 没有发送退格。";
                    notify_input_protection(message);
                    return Err(anyhow::anyhow!("录音目标窗口状态已丢失，已阻止取消退格"));
                }
            };
            if let Err(error) = self.text_inserter.delete_chars_from(target, n as usize) {
                report_input_error(
                    &self.last_input_error,
                    "取消操作已停止",
                    error.clone(),
                    "当前窗口不是录音开始时的目标窗口。为避免误删，Aiko IME 没有发送退格。",
                );
                return Err(error.into());
            }
            self.session_chars.store(0, Ordering::SeqCst);
        }
        self.session_target = None;

        Ok(())
    }
}

/// Everything a recognition loop needs to turn raw ASR text into typed text.
///
/// Shared between the online (Doubao) and offline (sherpa-onnx) result loops
/// so both backends get identical vocabulary, AI post-processing, voice
/// command, and history behavior.
struct SessionIo {
    text_inserter: Arc<TextInserter>,
    target: InputTarget,
    session_chars: Arc<AtomicI64>,
    last_input_error: Arc<std::sync::Mutex<Option<TextInputError>>>,
    config: AppConfig,
}

impl SessionIo {
    /// Apply an interim (still changing) recognition result.
    /// Returns false when the session must abort (target window lost).
    fn apply_interim(&self, last_text: &mut String, raw_text: &str) -> bool {
        if raw_text.is_empty() {
            return true;
        }

        let mut processed_text = raw_text.to_string();
        if !self.config.custom_vocabulary.is_empty() {
            processed_text = apply_custom_vocab(&processed_text, &self.config.custom_vocabulary);
        }
        processed_text = format_text_for_app(
            &processed_text,
            self.config.general.emoji_conversion_enabled,
        );

        match update_text(
            &self.text_inserter,
            self.target,
            last_text,
            &processed_text,
            &self.session_chars,
        ) {
            Ok(()) => {
                *last_text = processed_text;
                true
            }
            Err(error) => {
                tracing::error!("Voice input target became unavailable: {}", error);
                report_input_error(
                    &self.last_input_error,
                    "本次语音输入已停止",
                    error,
                    "录音开始时的目标窗口不再匹配。为避免误输入，Aiko IME 没有继续发送文字。",
                );
                false
            }
        }
    }

    /// Apply a finalized recognition result: clear the interim text, then run
    /// vocabulary, AI post-processing, app-aware formatting, voice commands,
    /// and history logging. Returns false when the session must abort.
    async fn apply_final(&self, last_text: &mut String, raw_text: &str) -> bool {
        if raw_text.is_empty() {
            return true;
        }

        // 1. Clear interim text from screen
        match update_text(
            &self.text_inserter,
            self.target,
            last_text,
            "",
            &self.session_chars,
        ) {
            Ok(()) => {}
            Err(error) => {
                tracing::error!("Failed to clear interim text safely: {}", error);
                report_input_error(
                    &self.last_input_error,
                    "本次语音输入已停止",
                    error,
                    "清理临时识别文字前发现目标窗口不再匹配，已阻止退格。",
                );
                return false;
            }
        }
        last_text.clear();

        // 2. Apply custom vocabulary to finalized text
        let mut finalized_text = raw_text.to_string();
        if !self.config.custom_vocabulary.is_empty() {
            finalized_text = apply_custom_vocab(&finalized_text, &self.config.custom_vocabulary);
        }

        // 3. AI post-processing and translation. `call_ai_api` carries its own
        // request timeout so a stalled endpoint cannot wedge stop().
        if self.config.ai.enabled && !finalized_text.is_empty() {
            let mut temp_text = finalized_text.clone();

            if self.config.ai.post_process_enabled {
                match AsrClient::call_ai_api(
                    &self.config.ai.api_endpoint,
                    &self.config.ai.api_key,
                    &self.config.ai.model,
                    &self.config.ai.post_process_prompt,
                    &temp_text,
                )
                .await
                {
                    Ok(processed) => {
                        tracing::info!(
                            "AI post-process successful: '{}' -> '{}'",
                            temp_text,
                            processed
                        );
                        temp_text = processed;
                    }
                    Err(e) => {
                        tracing::error!("AI post-process failed: {}", e);
                    }
                }
            }

            if self.config.ai.translation_enabled {
                match AsrClient::call_ai_api(
                    &self.config.ai.api_endpoint,
                    &self.config.ai.api_key,
                    &self.config.ai.model,
                    &self.config.ai.translation_prompt,
                    &temp_text,
                )
                .await
                {
                    Ok(translated) => {
                        tracing::info!(
                            "AI translation successful: '{}' -> '{}'",
                            temp_text,
                            translated
                        );
                        temp_text = translated;
                    }
                    Err(e) => {
                        tracing::error!("AI translation failed: {}", e);
                    }
                }
            }
            finalized_text = temp_text;
        }

        // 4. App-aware formatting
        finalized_text = format_text_for_app(
            &finalized_text,
            self.config.general.emoji_conversion_enabled,
        );

        // 5. Voice command parsing
        match process_voice_commands(
            &finalized_text,
            &self.text_inserter,
            self.target,
            &self.session_chars,
        ) {
            Ok(remaining_text) => {
                if !remaining_text.is_empty() {
                    if let Err(error) = self.text_inserter.insert_into(self.target, &remaining_text)
                    {
                        tracing::error!("Failed to insert finalized text: {}", error);
                        report_input_error(
                            &self.last_input_error,
                            "本次语音输入已停止",
                            error,
                            "插入最终识别文字前发现目标窗口不再匹配，已阻止输入。",
                        );
                        return false;
                    }
                    self.session_chars
                        .fetch_add(remaining_text.chars().count() as i64, Ordering::SeqCst);
                    log_dictation_history(&remaining_text, &self.config);
                }
                true
            }
            Err(error) => {
                tracing::error!("Failed to process voice command: {}", error);
                report_input_error(
                    &self.last_input_error,
                    "本次语音输入已停止",
                    error,
                    "执行语音命令前发现目标窗口不再匹配，已阻止输入或退格。",
                );
                false
            }
        }
    }
}

fn store_input_error(
    destination: &std::sync::Mutex<Option<TextInputError>>,
    error: TextInputError,
) {
    *destination
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(error);
}

fn report_input_error(
    destination: &std::sync::Mutex<Option<TextInputError>>,
    context: &str,
    error: TextInputError,
    guidance: &str,
) {
    store_input_error(destination, error.clone());
    notify_input_failure(context, &error, guidance);
}

fn notify_input_failure(context: &str, error: &TextInputError, guidance: &str) {
    notify_input_protection(&format!("{context}：{error}\n\n{guidance}"));
}

/// Update text in the focused window using incremental updates
///
/// Uses prefix matching to minimize deletions and insertions:
/// 1. Find the common prefix between old and new text
/// 2. Only delete characters beyond the common prefix
/// 3. Only append the new suffix
///
/// This significantly reduces visual flickering compared to full replacement.
///
/// Updates the session character count after each successful operation so
/// `cancel()` only undoes text that was actually sent.
#[derive(Debug, Eq, PartialEq)]
struct TextEditPlan {
    chars_to_delete: usize,
    text_to_append: String,
}

#[cfg(test)]
impl TextEditPlan {
    fn delta(&self) -> i64 {
        self.text_to_append.chars().count() as i64 - self.chars_to_delete as i64
    }
}

fn plan_text_update(old_text: &str, new_text: &str) -> TextEditPlan {
    let common_prefix_len = old_text
        .chars()
        .zip(new_text.chars())
        .take_while(|(a, b)| a == b)
        .count();

    TextEditPlan {
        chars_to_delete: old_text.chars().count() - common_prefix_len,
        text_to_append: new_text.chars().skip(common_prefix_len).collect(),
    }
}

fn update_text(
    text_inserter: &TextInserter,
    target: InputTarget,
    old_text: &str,
    new_text: &str,
    session_chars: &AtomicI64,
) -> Result<(), TextInputError> {
    let plan = plan_text_update(old_text, new_text);

    // 执行增量更新
    if plan.chars_to_delete > 0 {
        text_inserter.delete_chars_from(target, plan.chars_to_delete)?;
        session_chars.fetch_sub(plan.chars_to_delete as i64, Ordering::SeqCst);
    }
    if !plan.text_to_append.is_empty() {
        text_inserter.insert_into(target, &plan.text_to_append)?;
        session_chars.fetch_add(plan.text_to_append.chars().count() as i64, Ordering::SeqCst);
    }

    tracing::debug!(
        "Updated text incrementally: '{}' -> '{}' (kept {} chars, deleted {}, appended '{}')",
        old_text,
        new_text,
        old_text.chars().count() - plan.chars_to_delete,
        plan.chars_to_delete,
        plan.text_to_append
    );
    Ok(())
}

fn process_voice_commands(
    text: &str,
    text_inserter: &TextInserter,
    target: InputTarget,
    session_chars: &AtomicI64,
) -> Result<String, TextInputError> {
    let trimmed = text.trim_matches(|c: char| {
        c.is_whitespace() || c == '。' || c == '，' || c == '.' || c == ','
    });

    // Exact command match
    if trimmed == "退格" || trimmed == "删除" {
        text_inserter.delete_chars_from(target, 1)?;
        session_chars.fetch_sub(1, Ordering::SeqCst);
        return Ok(String::new());
    }
    if trimmed == "换行" || trimmed == "回车" {
        text_inserter.press_enter_in(target)?;
        session_chars.fetch_add(1, Ordering::SeqCst);
        return Ok(String::new());
    }

    // Ends with command match
    for (command, is_delete) in [
        ("退格", true),
        ("删除", true),
        ("换行", false),
        ("回车", false),
    ] {
        if let Some(prefix) = trimmed.strip_suffix(command) {
            let clean_prefix = prefix.trim_end_matches(|c: char| {
                c.is_whitespace() || c == '。' || c == '，' || c == '.' || c == ','
            });
            text_inserter.insert_into(target, clean_prefix)?;
            session_chars.fetch_add(clean_prefix.chars().count() as i64, Ordering::SeqCst);
            if is_delete {
                text_inserter.delete_chars_from(target, 1)?;
                session_chars.fetch_sub(1, Ordering::SeqCst);
            } else {
                text_inserter.press_enter_in(target)?;
                session_chars.fetch_add(1, Ordering::SeqCst);
            }
            return Ok(String::new());
        }
    }

    // No command word triggered
    Ok(text.to_string())
}

/// Replace custom vocabulary terms, longest source first so overlapping terms
/// behave deterministically regardless of HashMap iteration order.
fn apply_custom_vocab(text: &str, vocab: &std::collections::HashMap<String, String>) -> String {
    let mut entries: Vec<(&String, &String)> = vocab.iter().collect();
    entries.sort_by(|a, b| {
        b.0.chars()
            .count()
            .cmp(&a.0.chars().count())
            .then_with(|| a.0.cmp(b.0))
    });

    let mut result = text.to_string();
    for (key, val) in entries {
        result = result.replace(key.as_str(), val.as_str());
    }
    result
}

fn log_dictation_history(text: &str, config: &AppConfig) {
    if !config.general.history_log_enabled {
        return;
    }

    let mut log_path = std::path::PathBuf::from(&config.general.history_log_path);
    if !log_path.is_absolute() {
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(parent) = exe_path.parent() {
                log_path = parent.join(&config.general.history_log_path);
            }
        }
    }

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let log_entry = serde_json::json!({
        "timestamp": timestamp,
        "text": text
    });

    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        let mut line = log_entry.to_string();
        line.push('\n');
        let _ = std::io::Write::write_all(&mut file, line.as_bytes());
    }
}

#[cfg(target_os = "windows")]
fn get_foreground_process_name() -> Option<String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0 == 0 {
            return None;
        }
        let mut process_id = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));
        if process_id == 0 {
            return None;
        }

        let process_handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id);
        if let Ok(handle) = process_handle {
            if handle.is_invalid() {
                return None;
            }
            let mut buffer = [0u16; 512];
            let mut size = buffer.len() as u32;
            let res = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_FORMAT(0),
                windows::core::PWSTR(buffer.as_mut_ptr()),
                &mut size,
            );
            let _ = CloseHandle(handle);
            if res.is_ok() {
                let path = String::from_utf16_lossy(&buffer[..size as usize]);
                if let Some(filename) = std::path::Path::new(&path).file_name() {
                    return Some(filename.to_string_lossy().to_string());
                }
            }
        }
    }
    None
}

#[cfg(not(target_os = "windows"))]
fn get_foreground_process_name() -> Option<String> {
    None
}

fn strip_ending_punctuation(s: &str) -> String {
    s.trim_end_matches(|c: char| {
        c == '。'
            || c == '，'
            || c == '、'
            || c == '！'
            || c == '？'
            || c == '.'
            || c == ','
            || c == '!'
            || c == '?'
            || c == ';'
            || c == '；'
    })
    .to_string()
}

fn is_emoji(c: char) -> bool {
    let u = c as u32;
    (0x1F300..=0x1FAFF).contains(&u)
        || (0x2600..=0x26FF).contains(&u)
        || (0x2700..=0x27BF).contains(&u)
}

fn strip_emojis(s: &str) -> String {
    s.chars().filter(|&c| !is_emoji(c)).collect()
}

fn convert_emojis(s: &str) -> String {
    let mut result = s.to_string();
    let mappings = [
        ("笑脸", "😊"),
        ("大笑", "😄"),
        ("捂脸", "🤦"),
        ("点赞", "👍"),
        ("狗头", "🐶"),
        ("红心", "❤️"),
        ("玫瑰", "🌹"),
    ];
    for (k, v) in mappings {
        result = result.replace(k, v);
    }
    result
}

/// Adjust dictated text for the foreground application:
/// - IDEs and editors: no emoji, no trailing punctuation (code-friendly).
/// - Messengers: optionally convert spoken emoji keywords into emoji.
/// - Everything else: pass the recognized text through untouched.
fn format_text_for_app(text: &str, emoji_conversion_enabled: bool) -> String {
    let proc_name = get_foreground_process_name()
        .unwrap_or_default()
        .to_lowercase();
    tracing::debug!("Foreground process detected: {}", proc_name);

    let is_ide = proc_name.contains("code")
        || proc_name == "devenv.exe"
        || proc_name.contains("eclipse")
        || proc_name.contains("idea")
        || proc_name.contains("clion")
        || proc_name.contains("pycharm")
        || proc_name.contains("webstorm")
        || proc_name.contains("rider")
        || proc_name.contains("rustrover")
        || proc_name.contains("sublime")
        || proc_name.contains("notepad")
        || proc_name.contains("cursor");

    let is_messenger = proc_name.contains("wechat")
        || proc_name.contains("slack")
        || proc_name.contains("discord")
        || proc_name.contains("telegram")
        || proc_name.contains("dingtalk")
        || proc_name.contains("feishu")
        || proc_name.contains("qq")
        || proc_name.contains("whatsapp")
        || proc_name.contains("teams");

    if is_ide {
        let clean = strip_emojis(text);
        strip_ending_punctuation(&clean)
    } else if is_messenger && emoji_conversion_enabled {
        convert_emojis(text)
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_custom_vocab, plan_text_update, TextEditPlan};

    #[test]
    fn update_plan_appends_only_the_new_suffix() {
        assert_eq!(
            plan_text_update("Aiko", "Aiko IME"),
            TextEditPlan {
                chars_to_delete: 0,
                text_to_append: " IME".to_string(),
            }
        );
    }

    #[test]
    fn update_plan_counts_unicode_characters_not_bytes() {
        assert_eq!(
            plan_text_update("你好世界", "你好，Aiko"),
            TextEditPlan {
                chars_to_delete: 2,
                text_to_append: "，Aiko".to_string(),
            }
        );
    }

    #[test]
    fn update_plan_can_clear_all_interim_text() {
        let plan = plan_text_update("语音输入", "");
        assert_eq!(plan.chars_to_delete, 4);
        assert!(plan.text_to_append.is_empty());
        assert_eq!(plan.delta(), -4);
    }

    #[test]
    fn custom_vocab_replaces_longest_terms_first() {
        let mut vocab = std::collections::HashMap::new();
        vocab.insert("AI".to_string(), "人工智能".to_string());
        vocab.insert("AI 助手".to_string(), "Aiko".to_string());

        // The longer term must win even though HashMap order is random.
        assert_eq!(apply_custom_vocab("AI 助手上线", &vocab), "Aiko上线");
    }
}
