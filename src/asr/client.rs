//! ASR WebSocket Client
//!
//! Handles the WebSocket connection to the ASR server.

use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::net::{lookup_host, TcpStream};
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_tungstenite::{
    client_async_tls_with_config,
    tungstenite::{handshake::client::Response, Message},
    MaybeTlsStream, WebSocketStream,
};
use uuid::Uuid;

use super::constants::*;
use super::device::DeviceCredentials;
use super::proto::FrameState;
use super::protocol::{
    build_finish_session, build_start_session, build_start_task, build_task_request,
    parse_response, AsrResponse, ResponseType, SessionConfig,
};

const WS_CONNECT_TIMEOUT: Duration = Duration::from_secs(12);
const WS_TCP_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(4);
const WS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const WS_HOST: &str = "frontier-audio-ime-ws.doubao.com";
const WS_PORT: u16 = 443;

/// ASR Client for real-time speech recognition
pub struct AsrClient {
    credentials: DeviceCredentials,
}

impl AsrClient {
    /// Create a new ASR client with credentials
    pub fn new(credentials: DeviceCredentials) -> Self {
        Self { credentials }
    }

    /// Get WebSocket URL with parameters
    fn ws_url(&self) -> String {
        format!(
            "{}?aid={}&device_id={}",
            WEBSOCKET_URL, AID, self.credentials.device_id
        )
    }

    /// Start real-time ASR session
    ///
    /// Returns a receiver for ASR responses
    pub async fn start_realtime(
        &self,
        mut audio_rx: mpsc::Receiver<Vec<u8>>,
    ) -> Result<mpsc::Receiver<AsrResponse>> {
        let url = self.ws_url();
        let request_id = Uuid::new_v4().to_string();
        let token = self.credentials.token.clone();
        let device_id = self.credentials.device_id.clone();

        // Build request with headers
        let request = tokio_tungstenite::tungstenite::http::Request::builder()
            .uri(&url)
            .header("User-Agent", USER_AGENT)
            .header("proto-version", "v2")
            .header("x-custom-keepalive", "true")
            .header("Host", "frontier-audio-ime-ws.doubao.com")
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header(
                "Sec-WebSocket-Key",
                tokio_tungstenite::tungstenite::handshake::client::generate_key(),
            )
            .body(())?;

        tracing::info!("Connecting to ASR WebSocket: {}", url);
        let (ws_stream, _) = timeout(WS_CONNECT_TIMEOUT, connect_websocket_ipv4_first(request))
            .await
            .map_err(|_| {
                anyhow!(
                    "ASR WebSocket connect timed out after {}s",
                    WS_CONNECT_TIMEOUT.as_secs()
                )
            })??;
        tracing::info!("WebSocket connected successfully");
        let (mut write, mut read) = ws_stream.split();

        // Create response channel
        let (result_tx, result_rx) = mpsc::channel::<AsrResponse>(100);

        // Clone values for tasks
        let request_id_clone = request_id.clone();
        let token_clone = token.clone();

        // Send StartTask
        tracing::debug!("Sending StartTask (request_id: {})", &request_id[..8]);
        let start_task_msg = build_start_task(&request_id, &token);
        write.send(Message::Binary(start_task_msg)).await?;

        // Wait for TaskStarted response
        let task_started = timeout(WS_HANDSHAKE_TIMEOUT, read.next())
            .await
            .map_err(|_| {
                anyhow!(
                    "ASR StartTask timed out after {}s",
                    WS_HANDSHAKE_TIMEOUT.as_secs()
                )
            })?;
        match task_started {
            Some(Ok(Message::Binary(data))) => {
                let response = parse_response(&data);
                if response.response_type == ResponseType::Error {
                    return Err(anyhow!("StartTask failed: {}", response.error_msg));
                }
                tracing::debug!("TaskStarted received");
            }
            Some(Ok(msg)) => return Err(anyhow!("Unexpected StartTask response: {:?}", msg)),
            Some(Err(e)) => return Err(anyhow!("StartTask response failed: {}", e)),
            None => return Err(anyhow!("ASR WebSocket closed before TaskStarted")),
        }

        // Send StartSession
        tracing::debug!("Sending StartSession");
        let session_config = SessionConfig::new(&device_id);
        let start_session_msg = build_start_session(&request_id, &token, &session_config);
        write.send(Message::Binary(start_session_msg)).await?;

        // Wait for SessionStarted response
        let session_started = timeout(WS_HANDSHAKE_TIMEOUT, read.next())
            .await
            .map_err(|_| {
                anyhow!(
                    "ASR StartSession timed out after {}s",
                    WS_HANDSHAKE_TIMEOUT.as_secs()
                )
            })?;
        match session_started {
            Some(Ok(Message::Binary(data))) => {
                let response = parse_response(&data);
                if response.response_type == ResponseType::Error {
                    return Err(anyhow!("StartSession failed: {}", response.error_msg));
                }
                tracing::debug!("SessionStarted received");
            }
            Some(Ok(msg)) => return Err(anyhow!("Unexpected StartSession response: {:?}", msg)),
            Some(Err(e)) => return Err(anyhow!("StartSession response failed: {}", e)),
            None => return Err(anyhow!("ASR WebSocket closed before SessionStarted")),
        }

        // Spawn audio sending task
        tracing::info!("Starting audio frame sender task");
        tokio::spawn(async move {
            let mut frame_index = 0u64;
            let start_time = current_time_ms();

            // Process audio frames until channel is closed
            while let Some(opus_frame) = audio_rx.recv().await {
                let frame_state = if frame_index == 0 {
                    FrameState::First
                } else {
                    FrameState::Middle
                };

                let timestamp_ms = start_time + frame_index * FRAME_DURATION_MS as u64;
                let msg =
                    build_task_request(&request_id_clone, opus_frame, frame_state, timestamp_ms);

                if write.send(Message::Binary(msg)).await.is_err() {
                    tracing::warn!("Failed to send audio frame {}", frame_index);
                    break;
                }

                frame_index += 1;

                // Log every 50 frames (about 1 second)
                if frame_index % 50 == 0 {
                    tracing::info!(
                        "Sent {} audio frames ({:.1}s)",
                        frame_index,
                        frame_index as f64 * 0.02
                    );
                }
            }

            tracing::info!("Audio channel closed, sent {} total frames", frame_index);

            // Send last frame to signal end
            if frame_index > 0 {
                let timestamp_ms = start_time + frame_index * FRAME_DURATION_MS as u64;
                let silent_frame = vec![0u8; 100];
                let msg = build_task_request(
                    &request_id_clone,
                    silent_frame,
                    FrameState::Last,
                    timestamp_ms,
                );
                let _ = write.send(Message::Binary(msg)).await;

                // Send FinishSession
                let finish_msg = build_finish_session(&request_id_clone, &token_clone);
                let _ = write.send(Message::Binary(finish_msg)).await;
                tracing::info!("Sent FinishSession");
            }
        });

        // Spawn response receiving task
        let result_tx_clone = result_tx.clone();
        tokio::spawn(async move {
            while let Some(Ok(msg)) = read.next().await {
                if let Message::Binary(data) = msg {
                    let response = parse_response(&data);

                    match response.response_type {
                        ResponseType::Error | ResponseType::SessionFinished => {
                            let _ = result_tx_clone.send(response).await;
                            break;
                        }
                        ResponseType::Heartbeat => {
                            // Ignore heartbeats
                            continue;
                        }
                        _ => {
                            if result_tx_clone.send(response).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            }
        });

        Ok(result_rx)
    }

    /// Verify that the ASR WebSocket endpoint is reachable without consuming an ASR session.
    pub async fn test_connection(&self) -> Result<()> {
        let url = self.ws_url();
        let request = tokio_tungstenite::tungstenite::http::Request::builder()
            .uri(&url)
            .header("User-Agent", USER_AGENT)
            .header("proto-version", "v2")
            .header("x-custom-keepalive", "true")
            .header("Host", WS_HOST)
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header(
                "Sec-WebSocket-Key",
                tokio_tungstenite::tungstenite::handshake::client::generate_key(),
            )
            .body(())?;

        let (ws_stream, _) = timeout(WS_CONNECT_TIMEOUT, connect_websocket_ipv4_first(request))
            .await
            .map_err(|_| {
                anyhow!(
                    "ASR WebSocket connect timed out after {}s",
                    WS_CONNECT_TIMEOUT.as_secs()
                )
            })??;
        drop(ws_stream);
        Ok(())
    }

    /// Call OpenAI/DeepSeek compatible API for post-processing or translation
    pub async fn call_ai_api(
        endpoint: &str,
        api_key: &str,
        model: &str,
        system_prompt: &str,
        user_content: &str,
    ) -> Result<String> {
        let client = reqwest::Client::new();
        let url = if endpoint.ends_with('/') {
            format!("{}chat/completions", endpoint)
        } else {
            format!("{}/chat/completions", endpoint)
        };

        let body = serde_json::json!({
            "model": model,
            "messages": [
                {
                    "role": "system",
                    "content": system_prompt
                },
                {
                    "role": "user",
                    "content": user_content
                }
            ],
            "temperature": 0.3
        });

        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let err_text = response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "API request failed with status {}: {}",
                status,
                err_text
            ));
        }

        let res_json: serde_json::Value = response.json().await?;
        let choice = res_json
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|a| a.first())
            .ok_or_else(|| anyhow!("Invalid response: choices not found"))?;

        let content = choice
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .ok_or_else(|| anyhow!("Invalid response: content not found"))?;

        Ok(content.trim().to_string())
    }
}

async fn connect_websocket_ipv4_first(
    request: tokio_tungstenite::tungstenite::http::Request<()>,
) -> Result<(WebSocketStream<MaybeTlsStream<TcpStream>>, Response)> {
    let mut addrs: Vec<SocketAddr> = lookup_host((WS_HOST, WS_PORT)).await?.collect();
    if addrs.is_empty() {
        return Err(anyhow!("ASR host resolved to no addresses"));
    }

    // Some networks expose an unreachable IPv6 route for this Doubao endpoint.
    // Prefer IPv4 so the connection does not stall before trying a reachable address.
    addrs.sort_by_key(|addr| if addr.is_ipv4() { 0 } else { 1 });

    let mut last_error = None;
    for addr in addrs {
        tracing::debug!("Trying ASR WebSocket address: {}", addr);
        match timeout(WS_TCP_ATTEMPT_TIMEOUT, TcpStream::connect(addr)).await {
            Ok(Ok(socket)) => {
                if let Err(e) = socket.set_nodelay(true) {
                    tracing::warn!("Failed to set TCP_NODELAY for ASR socket: {}", e);
                }

                return client_async_tls_with_config(request.clone(), socket, None, None)
                    .await
                    .map_err(|e| anyhow!("ASR WebSocket handshake failed: {}", e));
            }
            Ok(Err(e)) => {
                tracing::warn!("ASR TCP connect failed for {}: {}", addr, e);
                last_error = Some(e.to_string());
            }
            Err(_) => {
                tracing::warn!(
                    "ASR TCP connect timed out for {} after {}s",
                    addr,
                    WS_TCP_ATTEMPT_TIMEOUT.as_secs()
                );
                last_error = Some(format!("connect timed out for {}", addr));
            }
        }
    }

    Err(anyhow!(
        "ASR TCP connect failed for all resolved addresses{}",
        last_error
            .map(|e| format!("; last error: {}", e))
            .unwrap_or_default()
    ))
}

/// Get current timestamp in milliseconds
fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
