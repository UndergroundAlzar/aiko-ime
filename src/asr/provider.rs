//! Provider-neutral ASR session types.

use futures_util::future::BoxFuture;
use std::fmt;
use tokio::sync::{mpsc, watch};

/// Recognition backend selected for a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    Doubao,
    SherpaOnnx,
}

impl fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Doubao => f.write_str("doubao"),
            Self::SherpaOnnx => f.write_str("sherpa-onnx"),
        }
    }
}

/// Audio accepted by a provider session.
///
/// Doubao consumes Opus frames. sherpa-onnx consumes PCM and performs sample
/// rate conversion internally when the frame rate differs from the model rate.
#[derive(Debug, Clone, PartialEq)]
pub enum AudioFrame {
    Opus {
        data: Vec<u8>,
        sample_rate: u32,
        channels: u16,
        duration_ms: u32,
    },
    PcmI16 {
        samples: Vec<i16>,
        sample_rate: u32,
        channels: u16,
    },
    PcmF32 {
        samples: Vec<f32>,
        sample_rate: u32,
        channels: u16,
    },
}

impl AudioFrame {
    pub fn opus(data: Vec<u8>, sample_rate: u32, channels: u16, duration_ms: u32) -> Self {
        Self::Opus {
            data,
            sample_rate,
            channels,
            duration_ms,
        }
    }

    pub fn pcm_i16(samples: Vec<i16>, sample_rate: u32, channels: u16) -> Self {
        Self::PcmI16 {
            samples,
            sample_rate,
            channels,
        }
    }

    pub fn pcm_f32(samples: Vec<f32>, sample_rate: u32, channels: u16) -> Self {
        Self::PcmF32 {
            samples,
            sample_rate,
            channels,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        match self {
            Self::Opus { sample_rate, .. }
            | Self::PcmI16 { sample_rate, .. }
            | Self::PcmF32 { sample_rate, .. } => *sample_rate,
        }
    }

    pub fn channels(&self) -> u16 {
        match self {
            Self::Opus { channels, .. }
            | Self::PcmI16 { channels, .. }
            | Self::PcmF32 { channels, .. } => *channels,
        }
    }

    pub fn encoding_name(&self) -> &'static str {
        match self {
            Self::Opus { .. } => "opus",
            Self::PcmI16 { .. } => "pcm-i16",
            Self::PcmF32 { .. } => "pcm-f32",
        }
    }

    /// Convert PCM input to normalized mono samples for sherpa-onnx.
    pub fn into_mono_f32(self) -> Result<(Vec<f32>, u32), ProviderError> {
        let sample_rate = self.sample_rate();
        let channels = self.channels();
        if sample_rate == 0 {
            return Err(ProviderError::InvalidAudio(
                "sample rate must be greater than zero".to_string(),
            ));
        }
        if channels == 0 {
            return Err(ProviderError::InvalidAudio(
                "channel count must be greater than zero".to_string(),
            ));
        }

        let channels = channels as usize;
        let samples = match self {
            Self::PcmI16 { samples, .. } => {
                downmix(samples, channels, |sample| sample as f32 / 32768.0)?
            }
            Self::PcmF32 { samples, .. } => {
                downmix(samples, channels, |sample| sample.clamp(-1.0, 1.0))?
            }
            Self::Opus { .. } => {
                return Err(ProviderError::UnsupportedAudio {
                    provider: ProviderKind::SherpaOnnx,
                    encoding: "opus".to_string(),
                });
            }
        };

        Ok((samples, sample_rate))
    }
}

fn downmix<T>(
    samples: Vec<T>,
    channels: usize,
    normalize: impl Fn(T) -> f32,
) -> Result<Vec<f32>, ProviderError>
where
    T: Copy,
{
    if !samples.len().is_multiple_of(channels) {
        return Err(ProviderError::InvalidAudio(format!(
            "{} samples are not divisible by {} channels",
            samples.len(),
            channels
        )));
    }

    if channels == 1 {
        return Ok(samples.into_iter().map(normalize).collect());
    }

    Ok(samples
        .chunks_exact(channels)
        .map(|frame| frame.iter().copied().map(&normalize).sum::<f32>() / channels as f32)
        .collect())
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EndpointOptions {
    pub enabled: bool,
    pub rule1_min_trailing_silence: f32,
    pub rule2_min_trailing_silence: f32,
    pub rule3_min_utterance_length: f32,
}

impl Default for EndpointOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            rule1_min_trailing_silence: 2.4,
            rule2_min_trailing_silence: 1.2,
            rule3_min_utterance_length: 20.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionOptions {
    pub endpoint: EndpointOptions,
    pub result_channel_capacity: usize,
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            endpoint: EndpointOptions::default(),
            result_channel_capacity: 100,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderCapabilities {
    pub streaming: bool,
    pub endpoint_detection: bool,
    pub accepts_opus: bool,
    pub accepts_pcm: bool,
    pub requires_network: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEndReason {
    Completed,
    Cancelled,
    InputClosed,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AsrEvent {
    SessionStarted {
        provider: ProviderKind,
    },
    SpeechStarted,
    PartialResult {
        text: String,
    },
    FinalResult {
        text: String,
        endpoint: bool,
    },
    SessionFinished {
        reason: SessionEndReason,
        final_text: Option<String>,
    },
    Error(ProviderError),
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum ProviderError {
    #[error("invalid ASR provider configuration: {0}")]
    InvalidConfiguration(String),
    #[error("{0} backend is not available: {1}")]
    BackendUnavailable(ProviderKind, String),
    #[error("offline model is missing or incomplete: {0}")]
    ModelUnavailable(String),
    #[error("{provider} does not accept {encoding} audio")]
    UnsupportedAudio {
        provider: ProviderKind,
        encoding: String,
    },
    #[error("invalid audio frame: {0}")]
    InvalidAudio(String),
    #[error("{0} session failed: {1}")]
    SessionFailed(ProviderKind, String),
    #[error("ASR session has already ended")]
    SessionClosed,
}

/// Common provider contract used by online and offline recognition backends.
pub trait AsrProvider: Send + Sync {
    fn kind(&self) -> ProviderKind;
    fn capabilities(&self) -> ProviderCapabilities;

    fn start_session<'a>(
        &'a self,
        options: SessionOptions,
    ) -> BoxFuture<'a, Result<ProviderSession, ProviderError>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionCommand {
    Running,
    Finish,
    Cancel,
}

/// A live recognition session with explicit finish and cancel semantics.
pub struct ProviderSession {
    provider: ProviderKind,
    audio_tx: Option<mpsc::Sender<AudioFrame>>,
    event_rx: mpsc::Receiver<AsrEvent>,
    command_tx: watch::Sender<SessionCommand>,
}

impl ProviderSession {
    pub(crate) fn new(
        provider: ProviderKind,
        audio_tx: mpsc::Sender<AudioFrame>,
        event_rx: mpsc::Receiver<AsrEvent>,
        command_tx: watch::Sender<SessionCommand>,
    ) -> Self {
        Self {
            provider,
            audio_tx: Some(audio_tx),
            event_rx,
            command_tx,
        }
    }

    pub fn provider(&self) -> ProviderKind {
        self.provider
    }

    pub async fn send_audio(&self, frame: AudioFrame) -> Result<(), ProviderError> {
        let tx = self.audio_tx.as_ref().ok_or(ProviderError::SessionClosed)?;
        tx.send(frame)
            .await
            .map_err(|_| ProviderError::SessionClosed)
    }

    pub fn finish(&mut self) -> Result<(), ProviderError> {
        if self.audio_tx.is_none() {
            return Err(ProviderError::SessionClosed);
        }
        self.command_tx.send_replace(SessionCommand::Finish);
        self.audio_tx.take();
        Ok(())
    }

    pub fn cancel(&mut self) -> Result<(), ProviderError> {
        if self.audio_tx.is_none() {
            return Err(ProviderError::SessionClosed);
        }
        self.command_tx.send_replace(SessionCommand::Cancel);
        self.audio_tx.take();
        Ok(())
    }

    pub async fn recv(&mut self) -> Option<AsrEvent> {
        self.event_rx.recv().await
    }
}

impl Drop for ProviderSession {
    fn drop(&mut self) {
        if self.audio_tx.is_some() {
            self.command_tx.send_replace(SessionCommand::Cancel);
            self.audio_tx.take();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcm_i16_stereo_is_normalized_and_downmixed() {
        let frame = AudioFrame::pcm_i16(vec![32767, -32768, 16384, 16384], 48_000, 2);
        let (samples, sample_rate) = frame.into_mono_f32().unwrap();

        assert_eq!(sample_rate, 48_000);
        assert_eq!(samples.len(), 2);
        assert!(samples[0].abs() < 0.0001);
        assert!((samples[1] - 0.5).abs() < 0.0001);
    }

    #[test]
    fn malformed_interleaved_pcm_is_rejected() {
        let frame = AudioFrame::pcm_f32(vec![0.1, 0.2, 0.3], 16_000, 2);
        assert!(matches!(
            frame.into_mono_f32(),
            Err(ProviderError::InvalidAudio(_))
        ));
    }

    #[test]
    fn opus_cannot_be_converted_for_sherpa() {
        let frame = AudioFrame::opus(vec![1, 2, 3], 16_000, 1, 20);
        assert!(matches!(
            frame.into_mono_f32(),
            Err(ProviderError::UnsupportedAudio {
                provider: ProviderKind::SherpaOnnx,
                ..
            })
        ));
    }
}
