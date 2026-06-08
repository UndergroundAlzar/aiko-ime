//! Voice Controller
//!
//! Coordinates voice input between audio capture, ASR, and text insertion.

use anyhow::Result;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;

use crate::asr::{AsrClient, ResponseType};
use crate::audio::AudioCapture;
use crate::business::TextInserter;
use crate::data::AppConfig;

/// Voice input controller
pub struct VoiceController {
    asr_client: Arc<AsrClient>,
    audio_capture: Arc<AudioCapture>,
    text_inserter: Arc<TextInserter>,
    is_recording: Arc<AtomicBool>,
    stop_signal: Arc<AtomicBool>,
    /// Net characters this session has typed into the focused window and not yet
    /// committed away. Used by `cancel()` to undo the whole dictation.
    session_chars: Arc<AtomicI64>,
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
            session_chars: Arc::new(AtomicI64::new(0)),
            config,
        }
    }

    /// Check if currently recording
    pub fn is_recording(&self) -> bool {
        self.is_recording.load(Ordering::SeqCst)
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

        // Reload configuration from file to get latest settings
        if let Ok(latest_config) = crate::data::AppConfig::load_or_default() {
            self.config = latest_config;
        }

        tracing::info!("Starting voice input...");
        self.is_recording.store(true, Ordering::SeqCst);
        self.stop_signal.store(false, Ordering::SeqCst);
        self.session_chars.store(0, Ordering::SeqCst);

        // Start audio capture
        tracing::debug!("Starting audio capture...");
        let audio_rx = match self.audio_capture.start() {
            Ok(rx) => rx,
            Err(e) => {
                self.is_recording.store(false, Ordering::SeqCst);
                self.stop_signal.store(false, Ordering::SeqCst);
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
                self.is_recording.store(false, Ordering::SeqCst);
                self.stop_signal.store(false, Ordering::SeqCst);
                return Err(e);
            }
        };
        tracing::info!("ASR connection established");

        // Clone for the task
        let text_inserter = self.text_inserter.clone();
        let is_recording = self.is_recording.clone();
        let stop_signal = self.stop_signal.clone();
        let audio_capture = self.audio_capture.clone();
        let session_chars = self.session_chars.clone();
        let config = self.config.clone();

        // Spawn result processing task
        tokio::spawn(async move {
            let mut last_text = String::new();
            let mut response_count = 0u32;

            tracing::info!("ASR result processing task started");

            loop {
                // Check stop signal
                if stop_signal.load(Ordering::SeqCst) {
                    tracing::info!(
                        "Voice input stopped by user (processed {} responses)",
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
                                println!("📝 [识别中] {}", response.text);
                                if !response.text.is_empty() {
                                    // Apply custom vocabulary to interim results so they look correct in real-time
                                    let mut processed_text = response.text.clone();
                                    if !config.custom_vocabulary.is_empty() {
                                        processed_text = apply_custom_vocab(
                                            &processed_text,
                                            &config.custom_vocabulary,
                                        );
                                    }

                                    // Intercept and format for app-aware profiling
                                    processed_text = format_text_for_app(&processed_text);

                                    match update_text(&text_inserter, &last_text, &processed_text) {
                                        Ok(delta) => {
                                            session_chars.fetch_add(delta, Ordering::SeqCst);
                                        }
                                        Err(e) => tracing::error!("Failed to update text: {}", e),
                                    }
                                    last_text = processed_text;
                                }
                            }
                            ResponseType::FinalResult => {
                                tracing::info!("[FINAL #{}] {}", response_count, response.text);
                                println!("✅ [确认] {}", response.text);
                                if !response.text.is_empty() {
                                    // 1. Clear interim text from screen
                                    match update_text(&text_inserter, &last_text, "") {
                                        Ok(delta) => {
                                            session_chars.fetch_add(delta, Ordering::SeqCst);
                                        }
                                        Err(e) => {
                                            tracing::error!("Failed to clear interim text: {}", e)
                                        }
                                    }
                                    last_text = String::new();

                                    // 2. Apply custom vocabulary to finalized text
                                    let mut finalized_text = response.text.clone();
                                    if !config.custom_vocabulary.is_empty() {
                                        finalized_text = apply_custom_vocab(
                                            &finalized_text,
                                            &config.custom_vocabulary,
                                        );
                                    }

                                    // 3. AI Post-Processing & Bilingual Translation (async APIs)
                                    if config.ai.enabled && !finalized_text.is_empty() {
                                        let mut temp_text = finalized_text.clone();

                                        // AI Post-processing
                                        if config.ai.post_process_enabled {
                                            match AsrClient::call_ai_api(
                                                &config.ai.api_endpoint,
                                                &config.ai.api_key,
                                                &config.ai.model,
                                                &config.ai.post_process_prompt,
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
                                                    tracing::error!(
                                                        "AI post-process failed: {}",
                                                        e
                                                    );
                                                }
                                            }
                                        }

                                        // Bilingual Translation
                                        if config.ai.translation_enabled {
                                            match AsrClient::call_ai_api(
                                                &config.ai.api_endpoint,
                                                &config.ai.api_key,
                                                &config.ai.model,
                                                &config.ai.translation_prompt,
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

                                    // Intercept and format for app-aware profiling
                                    finalized_text = format_text_for_app(&finalized_text);

                                    // 4. Voice Command Parsing
                                    match process_voice_commands(&finalized_text, &text_inserter) {
                                        Ok((remaining_text, key_delta)) => {
                                            session_chars.fetch_add(key_delta, Ordering::SeqCst);
                                            if !remaining_text.is_empty() {
                                                if let Err(e) =
                                                    text_inserter.insert(&remaining_text)
                                                {
                                                    tracing::error!(
                                                        "Failed to insert finalized text: {}",
                                                        e
                                                    );
                                                } else {
                                                    session_chars.fetch_add(
                                                        remaining_text.chars().count() as i64,
                                                        Ordering::SeqCst,
                                                    );
                                                    log_dictation_history(&remaining_text, &config);
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            tracing::error!(
                                                "Failed to process voice command: {}",
                                                e
                                            );
                                            // Fallback: insert raw text
                                            if let Err(err) = text_inserter.insert(&finalized_text)
                                            {
                                                tracing::error!(
                                                    "Failed to insert finalized text fallback: {}",
                                                    err
                                                );
                                            } else {
                                                session_chars.fetch_add(
                                                    finalized_text.chars().count() as i64,
                                                    Ordering::SeqCst,
                                                );
                                                log_dictation_history(&finalized_text, &config);
                                            }
                                        }
                                    }
                                }
                            }
                            ResponseType::SessionFinished => {
                                tracing::info!(
                                    "ASR session finished (total {} responses)",
                                    response_count
                                );
                                println!("🏁 [会话结束]");
                                break;
                            }
                            ResponseType::Error => {
                                tracing::error!("ASR error: {}", response.error_msg);
                                println!("❌ [错误] {}", response.error_msg);
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
        });

        Ok(())
    }

    /// Stop voice input and **keep** the text already typed (the ✓ / confirm action).
    pub async fn stop(&mut self) -> Result<()> {
        if !self.is_recording() {
            return Ok(());
        }

        tracing::info!("Stopping voice input...");

        // Signal stop
        self.stop_signal.store(true, Ordering::SeqCst);
        self.audio_capture.stop();

        // Wait a bit for the task to finish
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        self.is_recording.store(false, Ordering::SeqCst);
        // Text is committed; nothing left for cancel() to undo.
        self.session_chars.store(0, Ordering::SeqCst);

        Ok(())
    }

    /// Stop voice input and **discard** everything typed this session (the ✗ / cancel
    /// action). Sends backspaces to remove the dictated text from the focused window.
    pub async fn cancel(&mut self) -> Result<()> {
        if !self.is_recording() {
            return Ok(());
        }

        tracing::info!("Cancelling voice input...");

        // Stop producing new text first, then let the result task drain.
        self.stop_signal.store(true, Ordering::SeqCst);
        self.audio_capture.stop();
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        self.is_recording.store(false, Ordering::SeqCst);

        // Undo whatever this session typed.
        let n = self.session_chars.swap(0, Ordering::SeqCst);
        if n > 0 {
            tracing::info!("Cancel: deleting {} dictated characters", n);
            if let Err(e) = self.text_inserter.delete_chars(n as usize) {
                tracing::error!("Failed to delete text on cancel: {}", e);
            }
        }

        Ok(())
    }
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
/// Returns the net change in on-screen character count (`appended - deleted`),
/// which the caller accumulates so `cancel()` can undo the whole session.
fn update_text(text_inserter: &TextInserter, old_text: &str, new_text: &str) -> Result<i64> {
    // 找到公共前缀长度（无需删除和重新输入的部分）
    let common_prefix_len = old_text
        .chars()
        .zip(new_text.chars())
        .take_while(|(a, b)| a == b)
        .count();

    // 计算需要删除的字符数 = 旧文本超出公共前缀的部分
    let chars_to_delete = old_text.chars().count() - common_prefix_len;

    // 需要追加的文本 = 新文本超出公共前缀的部分
    let text_to_append: String = new_text.chars().skip(common_prefix_len).collect();
    let chars_to_append = text_to_append.chars().count();

    // 执行增量更新
    if chars_to_delete > 0 {
        text_inserter.delete_chars(chars_to_delete)?;
    }
    if !text_to_append.is_empty() {
        text_inserter.insert(&text_to_append)?;
    }

    tracing::debug!(
        "Updated text incrementally: '{}' -> '{}' (kept {} chars, deleted {}, appended '{}')",
        old_text,
        new_text,
        common_prefix_len,
        chars_to_delete,
        text_to_append
    );
    Ok(chars_to_append as i64 - chars_to_delete as i64)
}

fn process_voice_commands(text: &str, text_inserter: &TextInserter) -> Result<(String, i64)> {
    let trimmed = text.trim_matches(|c: char| {
        c.is_whitespace() || c == '。' || c == '，' || c == '.' || c == ','
    });

    // Exact command match
    if trimmed == "退格" || trimmed == "删除" {
        text_inserter.delete_chars(1)?;
        return Ok((String::new(), -1));
    }
    if trimmed == "换行" || trimmed == "回车" {
        text_inserter.press_enter()?;
        return Ok((String::new(), 1));
    }

    // Ends with command match
    if trimmed.ends_with("退格") {
        let clean_prefix = trimmed[..trimmed.len() - "退格".len()].trim_end_matches(|c: char| {
            c.is_whitespace() || c == '。' || c == '，' || c == '.' || c == ','
        });
        text_inserter.insert(clean_prefix)?;
        text_inserter.delete_chars(1)?;
        return Ok((String::new(), clean_prefix.chars().count() as i64 - 1));
    }
    if trimmed.ends_with("删除") {
        let clean_prefix = trimmed[..trimmed.len() - "删除".len()].trim_end_matches(|c: char| {
            c.is_whitespace() || c == '。' || c == '，' || c == '.' || c == ','
        });
        text_inserter.insert(clean_prefix)?;
        text_inserter.delete_chars(1)?;
        return Ok((String::new(), clean_prefix.chars().count() as i64 - 1));
    }
    if trimmed.ends_with("换行") {
        let clean_prefix = trimmed[..trimmed.len() - "换行".len()].trim_end_matches(|c: char| {
            c.is_whitespace() || c == '。' || c == '，' || c == '.' || c == ','
        });
        text_inserter.insert(clean_prefix)?;
        text_inserter.press_enter()?;
        return Ok((String::new(), clean_prefix.chars().count() as i64 + 1));
    }
    if trimmed.ends_with("回车") {
        let clean_prefix = trimmed[..trimmed.len() - "回车".len()].trim_end_matches(|c: char| {
            c.is_whitespace() || c == '。' || c == '，' || c == '.' || c == ','
        });
        text_inserter.insert(clean_prefix)?;
        text_inserter.press_enter()?;
        return Ok((String::new(), clean_prefix.chars().count() as i64 + 1));
    }

    // No command word triggered
    Ok((text.to_string(), 0))
}

fn apply_custom_vocab(text: &str, vocab: &std::collections::HashMap<String, String>) -> String {
    let mut result = text.to_string();
    for (key, val) in vocab {
        result = result.replace(key, val);
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
    (u >= 0x1F300 && u <= 0x1Faff)
        || (u >= 0x1F600 && u <= 0x1F64F)
        || (u >= 0x1F680 && u <= 0x1F6FF)
        || (u >= 0x1F900 && u <= 0x1F9FF)
        || (u >= 0x2600 && u <= 0x26FF)
        || (u >= 0x2700 && u <= 0x27BF)
}

fn strip_emojis(s: &str) -> String {
    s.chars().filter(|&c| !is_emoji(c)).collect()
}

fn convert_emojis(s: &str) -> String {
    let mut result = s.to_string();
    let mappings = [
        ("笑脸", "😊"),
        ("大笑", "😄"),
        ("哭", "😭"),
        ("捂脸", "🤦"),
        ("点赞", "👍"),
        ("开心", "🥰"),
        ("流泪", "😭"),
        ("狗头", "🐶"),
        ("胜利", "✌️"),
        ("拜托", "🙏"),
        ("谢谢", "🙏"),
        ("握手", "🤝"),
        ("红心", "❤️"),
        ("玫瑰", "🌹"),
    ];
    for (k, v) in mappings {
        result = result.replace(k, v);
    }
    result
}

fn format_text_for_app(text: &str) -> String {
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
    } else if is_messenger {
        convert_emojis(text)
    } else {
        strip_emojis(text)
    }
}
