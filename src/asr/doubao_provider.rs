//! Adapter exposing the existing Doubao client through [`AsrProvider`].

use futures_util::future::BoxFuture;
use std::sync::Arc;
use tokio::sync::{mpsc, watch};

use super::provider::{
    AsrEvent, AsrProvider, AudioFrame, ProviderCapabilities, ProviderError, ProviderKind,
    ProviderSession, SessionCommand, SessionEndReason, SessionOptions,
};
use super::{AsrClient, ResponseType};

pub struct DoubaoProvider {
    client: Arc<AsrClient>,
}

impl DoubaoProvider {
    pub fn new(client: Arc<AsrClient>) -> Self {
        Self { client }
    }
}

impl AsrProvider for DoubaoProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Doubao
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            streaming: true,
            endpoint_detection: true,
            accepts_opus: true,
            accepts_pcm: false,
            requires_network: true,
        }
    }

    fn start_session<'a>(
        &'a self,
        options: SessionOptions,
    ) -> BoxFuture<'a, Result<ProviderSession, ProviderError>> {
        Box::pin(async move {
            let capacity = options.result_channel_capacity.max(1);
            let (legacy_audio_tx, legacy_audio_rx) = mpsc::channel::<Vec<u8>>(500);
            let legacy_result_rx =
                self.client
                    .start_realtime(legacy_audio_rx)
                    .await
                    .map_err(|error| {
                        ProviderError::SessionFailed(ProviderKind::Doubao, error.to_string())
                    })?;

            let (audio_tx, audio_rx) = mpsc::channel::<AudioFrame>(500);
            let (event_tx, event_rx) = mpsc::channel::<AsrEvent>(capacity);
            let (command_tx, command_rx) = watch::channel(SessionCommand::Running);

            tokio::spawn(bridge_audio(
                audio_rx,
                legacy_audio_tx,
                command_rx.clone(),
                event_tx.clone(),
            ));
            tokio::spawn(bridge_results(legacy_result_rx, command_rx, event_tx));

            Ok(ProviderSession::new(
                ProviderKind::Doubao,
                audio_tx,
                event_rx,
                command_tx,
            ))
        })
    }
}

async fn bridge_audio(
    mut audio_rx: mpsc::Receiver<AudioFrame>,
    legacy_audio_tx: mpsc::Sender<Vec<u8>>,
    mut command_rx: watch::Receiver<SessionCommand>,
    event_tx: mpsc::Sender<AsrEvent>,
) {
    loop {
        tokio::select! {
            command = command_rx.changed() => {
                if command.is_err() || *command_rx.borrow() != SessionCommand::Running {
                    break;
                }
            }
            frame = audio_rx.recv() => {
                let Some(frame) = frame else {
                    break;
                };
                match frame {
                    AudioFrame::Opus { data, .. } => {
                        if legacy_audio_tx.send(data).await.is_err() {
                            break;
                        }
                    }
                    other => {
                        let _ = event_tx.send(AsrEvent::Error(ProviderError::UnsupportedAudio {
                            provider: ProviderKind::Doubao,
                            encoding: other.encoding_name().to_string(),
                        })).await;
                        break;
                    }
                }
            }
        }
    }
}

async fn bridge_results(
    mut result_rx: mpsc::Receiver<super::AsrResponse>,
    mut command_rx: watch::Receiver<SessionCommand>,
    event_tx: mpsc::Sender<AsrEvent>,
) {
    if event_tx
        .send(AsrEvent::SessionStarted {
            provider: ProviderKind::Doubao,
        })
        .await
        .is_err()
    {
        return;
    }

    loop {
        tokio::select! {
            command = command_rx.changed() => {
                if command.is_err() {
                    break;
                }
                if *command_rx.borrow() == SessionCommand::Cancel {
                    let _ = event_tx.send(AsrEvent::SessionFinished {
                        reason: SessionEndReason::Cancelled,
                        final_text: None,
                    }).await;
                    break;
                }
            }
            response = result_rx.recv() => {
                let Some(response) = response else {
                    let _ = event_tx.send(AsrEvent::SessionFinished {
                        reason: SessionEndReason::InputClosed,
                        final_text: None,
                    }).await;
                    break;
                };

                let event = match response.response_type {
                    ResponseType::VadStart => Some(AsrEvent::SpeechStarted),
                    ResponseType::InterimResult => Some(AsrEvent::PartialResult {
                        text: response.text,
                    }),
                    ResponseType::FinalResult => Some(AsrEvent::FinalResult {
                        text: response.text,
                        endpoint: response.vad_finished,
                    }),
                    ResponseType::SessionFinished => Some(AsrEvent::SessionFinished {
                        reason: SessionEndReason::Completed,
                        final_text: None,
                    }),
                    ResponseType::Error => Some(AsrEvent::Error(ProviderError::SessionFailed(
                        ProviderKind::Doubao,
                        response.error_msg,
                    ))),
                    _ => None,
                };

                let is_terminal = matches!(
                    event,
                    Some(AsrEvent::SessionFinished { .. } | AsrEvent::Error(_))
                );
                if let Some(event) = event {
                    if event_tx.send(event).await.is_err() {
                        break;
                    }
                }
                if is_terminal {
                    break;
                }
            }
        }
    }
}
