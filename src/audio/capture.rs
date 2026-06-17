//! Audio Capture using cpal

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::Arc;
use std::thread;
use tokio::sync::mpsc as tokio_mpsc;

use super::encoder::OpusEncoder;

// Opus encoder always uses 16kHz mono
const OPUS_SAMPLE_RATE: u32 = 16000;
const OPUS_CHANNELS: u16 = 1;
const FRAME_DURATION_MS: u32 = 20;

/// Current microphone input level, scaled 0..=1000 (0 = silence, 1000 = loud).
///
/// Updated once per captured frame from the resampled mono PCM and read by the
/// floating-button UI to drive a live, audio-reactive waveform. A process-wide
/// atomic is fine because only one capture stream runs at a time.
pub static INPUT_LEVEL: AtomicU32 = AtomicU32::new(0);

/// Where captured 16kHz mono frames are delivered.
enum CaptureSink {
    /// Opus-encoded 20ms frames (online Doubao backend).
    Opus(tokio_mpsc::Sender<Vec<u8>>),
    /// Raw 16-bit PCM 20ms frames (offline sherpa-onnx backend).
    Pcm(tokio_mpsc::Sender<Vec<i16>>),
}

pub struct AudioCapture {
    is_recording: Arc<AtomicBool>,
    generation: Arc<AtomicU64>,
}

impl AudioCapture {
    pub fn new() -> Result<Self> {
        let host = cpal::default_host();
        match host.default_input_device() {
            Some(device) => {
                tracing::info!(
                    "[AudioCapture] Default device: {}",
                    device.name().unwrap_or_default()
                );
            }
            None => {
                tracing::warn!("[AudioCapture] No default input device found");
            }
        }

        Ok(Self {
            is_recording: Arc::new(AtomicBool::new(false)),
            generation: Arc::new(AtomicU64::new(0)),
        })
    }

    pub fn is_recording(&self) -> bool {
        self.is_recording.load(Ordering::SeqCst)
    }

    /// Start capturing and receive Opus-encoded 20ms frames.
    ///
    /// `preferred_device` is the configured microphone name; empty string means
    /// the Windows default input device.
    pub fn start(&self, preferred_device: &str) -> Result<tokio_mpsc::Receiver<Vec<u8>>> {
        let (tokio_tx, tokio_rx) = tokio_mpsc::channel::<Vec<u8>>(500);
        self.start_with_sink(preferred_device, CaptureSink::Opus(tokio_tx))?;
        Ok(tokio_rx)
    }

    /// Start capturing and receive raw 16kHz mono PCM 20ms frames.
    pub fn start_pcm(&self, preferred_device: &str) -> Result<tokio_mpsc::Receiver<Vec<i16>>> {
        let (tokio_tx, tokio_rx) = tokio_mpsc::channel::<Vec<i16>>(500);
        self.start_with_sink(preferred_device, CaptureSink::Pcm(tokio_tx))?;
        Ok(tokio_rx)
    }

    fn start_with_sink(&self, preferred_device: &str, sink: CaptureSink) -> Result<()> {
        if self.is_recording.swap(true, Ordering::SeqCst) {
            return Err(anyhow!("Already recording"));
        }

        let is_recording = self.is_recording.clone();
        let generation = self.generation.clone();
        let generation_id = generation.fetch_add(1, Ordering::SeqCst) + 1;
        let preferred_device = preferred_device.trim().to_string();

        thread::spawn(move || {
            #[cfg(target_os = "windows")]
            {
                use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
                unsafe {
                    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                }
            }

            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_audio_capture(
                    sink,
                    &preferred_device,
                    is_recording.clone(),
                    generation.clone(),
                    generation_id,
                )
            }));

            match result {
                Ok(Ok(_)) => {
                    tracing::info!("[AudioCapture] Completed normally");
                }
                Ok(Err(e)) => {
                    tracing::error!("[AudioCapture] ERROR: {}", e);
                }
                Err(panic_info) => {
                    tracing::error!("[AudioCapture] PANIC: {:?}", panic_info);
                }
            }

            if generation.load(Ordering::SeqCst) == generation_id {
                is_recording.store(false, Ordering::SeqCst);
            }
        });

        tracing::info!("Audio capture started");
        Ok(())
    }

    pub fn stop(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.is_recording.store(false, Ordering::SeqCst);
        INPUT_LEVEL.store(0, Ordering::Relaxed);
        tracing::info!("Audio capture stopped");
    }
}

fn capture_is_active(
    is_recording: &AtomicBool,
    generation: &AtomicU64,
    generation_id: u64,
) -> bool {
    is_recording.load(Ordering::SeqCst) && generation.load(Ordering::SeqCst) == generation_id
}

/// Resolve the capture device: the configured one when present, otherwise the
/// Windows default. Returns the device together with the name it was matched by.
fn resolve_input_device(host: &cpal::Host, preferred: &str) -> Option<(cpal::Device, String)> {
    if !preferred.is_empty() {
        if let Ok(devices) = host.input_devices() {
            for device in devices {
                if let Ok(name) = device.name() {
                    if name == preferred {
                        return Some((device, name));
                    }
                }
            }
        }
        tracing::warn!(
            "[AudioCapture] Configured microphone '{}' not found, falling back to system default",
            preferred
        );
    }

    let device = host.default_input_device()?;
    let name = device.name().unwrap_or_default();
    Some((device, name))
}

/// Resample one mono frame to exactly `output_len` samples.
///
/// Integer-ratio downsampling (48k/32k -> 16k) averages each sample group,
/// which doubles as a crude low-pass filter. Fractional ratios fall back to
/// linear interpolation. Both avoid the aliasing of nearest-neighbor picking.
fn resample_mono_i16(input: &[i16], output_len: usize) -> Vec<i16> {
    if output_len == 0 {
        return Vec::new();
    }
    if input.is_empty() {
        return vec![0; output_len];
    }
    if input.len() == output_len {
        return input.to_vec();
    }

    if input.len() > output_len && input.len() % output_len == 0 {
        let group = input.len() / output_len;
        return input
            .chunks_exact(group)
            .map(|chunk| {
                let sum: i32 = chunk.iter().map(|&s| s as i32).sum();
                (sum / group as i32) as i16
            })
            .collect();
    }

    let ratio = if output_len > 1 {
        (input.len() - 1) as f32 / (output_len - 1) as f32
    } else {
        0.0
    };
    (0..output_len)
        .map(|i| {
            let pos = i as f32 * ratio;
            let index = pos as usize;
            let frac = pos - index as f32;
            let a = input[index.min(input.len() - 1)] as f32;
            let b = input[(index + 1).min(input.len() - 1)] as f32;
            (a + (b - a) * frac).round() as i16
        })
        .collect()
}

fn update_input_level(samples: &[i16]) {
    if samples.is_empty() {
        return;
    }
    let sum_sq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    let rms = (sum_sq / samples.len() as f64).sqrt();
    // Below this the meter shows silence; the audio itself is still sent so the
    // ASR backend can use its own (much better) VAD on quiet speech onsets.
    if rms < 150.0 {
        INPUT_LEVEL.store(0, Ordering::Relaxed);
    } else {
        let norm = (rms / 6000.0).min(1.0);
        INPUT_LEVEL.store((norm * 1000.0) as u32, Ordering::Relaxed);
    }
}

fn run_audio_capture(
    sink: CaptureSink,
    preferred_device: &str,
    is_recording: Arc<AtomicBool>,
    generation: Arc<AtomicU64>,
    generation_id: u64,
) -> Result<()> {
    let host = cpal::default_host();
    let frame_counter = Arc::new(AtomicU64::new(0));
    // Outer loop to handle device setup / restart
    while capture_is_active(&is_recording, &generation, generation_id) {
        let Some((device, device_name)) = resolve_input_device(&host, preferred_device) else {
            tracing::warn!("[AudioCapture] No input device available, retrying in 1s...");
            thread::sleep(std::time::Duration::from_secs(1));
            continue;
        };
        let current_device_name = device_name.clone();
        tracing::info!("[AudioCapture] Target device: {}", device_name);

        // Get the device's default config
        let supported_config = match device.default_input_config() {
            Ok(cfg) => cfg,
            Err(e) => {
                tracing::warn!(
                    "[AudioCapture] Failed to get default input config: {}, retrying in 1s...",
                    e
                );
                thread::sleep(std::time::Duration::from_secs(1));
                continue;
            }
        };

        let native_sample_rate = supported_config.sample_rate().0;
        let native_channels = supported_config.channels();
        let sample_format = supported_config.sample_format();
        let config = supported_config.config();

        // Create Opus encoder (16kHz mono) for the online sink
        let mut encoder = match &sink {
            CaptureSink::Opus(_) => match OpusEncoder::new(OPUS_SAMPLE_RATE, OPUS_CHANNELS) {
                Ok(enc) => Some(enc),
                Err(e) => {
                    tracing::error!("[AudioCapture] Opus encoder FAILED: {}", e);
                    return Err(e);
                }
            },
            CaptureSink::Pcm(_) => None,
        };

        // Calculate frame sizes
        let samples_per_frame_native =
            (native_sample_rate * FRAME_DURATION_MS / 1000) as usize * native_channels as usize;
        let samples_per_frame_opus = (OPUS_SAMPLE_RATE * FRAME_DURATION_MS / 1000) as usize; // mono

        let (std_tx, std_rx) = std_mpsc::channel::<Vec<i16>>();
        let is_recording_clone = is_recording.clone();
        let generation_clone = generation.clone();
        let err_fn = |err| {
            tracing::warn!("[AudioCapture] Stream error: {}", err);
        };

        let stream = match sample_format {
            SampleFormat::I16 => {
                let mut buffer = Vec::<i16>::with_capacity(samples_per_frame_native * 2);
                match device.build_input_stream(
                    &config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        if !capture_is_active(&is_recording_clone, &generation_clone, generation_id)
                        {
                            return;
                        }
                        buffer.extend_from_slice(data);
                        while buffer.len() >= samples_per_frame_native {
                            let frame: Vec<i16> =
                                buffer.drain(..samples_per_frame_native).collect();
                            let _ = std_tx.send(frame);
                        }
                    },
                    err_fn,
                    None,
                ) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!(
                            "[AudioCapture] Failed to build I16 stream: {}, retrying in 1s...",
                            e
                        );
                        thread::sleep(std::time::Duration::from_secs(1));
                        continue;
                    }
                }
            }
            SampleFormat::F32 => {
                let mut buffer = Vec::<i16>::with_capacity(samples_per_frame_native * 2);
                let is_recording_clone = is_recording.clone();
                let generation_clone = generation.clone();
                match device.build_input_stream(
                    &config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        if !capture_is_active(&is_recording_clone, &generation_clone, generation_id)
                        {
                            return;
                        }
                        let samples: Vec<i16> = data
                            .iter()
                            .map(|s| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
                            .collect();
                        buffer.extend_from_slice(&samples);
                        while buffer.len() >= samples_per_frame_native {
                            let frame: Vec<i16> =
                                buffer.drain(..samples_per_frame_native).collect();
                            let _ = std_tx.send(frame);
                        }
                    },
                    err_fn,
                    None,
                ) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!(
                            "[AudioCapture] Failed to build F32 stream: {}, retrying in 1s...",
                            e
                        );
                        thread::sleep(std::time::Duration::from_secs(1));
                        continue;
                    }
                }
            }
            format => {
                return Err(anyhow!("Unsupported format: {:?}", format));
            }
        };

        if let Err(e) = stream.play() {
            tracing::warn!(
                "[AudioCapture] Failed to play stream: {}, retrying in 1s...",
                e
            );
            thread::sleep(std::time::Duration::from_secs(1));
            continue;
        }

        tracing::info!("[AudioCapture] Stream playing: {}", device_name);
        let mut last_device_check = std::time::Instant::now();
        let mut device_changed = false;

        // Inner processing loop for the current stream/device
        'capture: while capture_is_active(&is_recording, &generation, generation_id)
            && !device_changed
        {
            match std_rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(frame) => {
                    let mono_frame: Vec<i16> = if native_channels > 1 {
                        frame
                            .chunks(native_channels as usize)
                            .map(|chunk| {
                                let sum: i32 = chunk.iter().map(|&s| s as i32).sum();
                                (sum / native_channels as i32) as i16
                            })
                            .collect()
                    } else {
                        frame
                    };

                    let resampled = resample_mono_i16(&mono_frame, samples_per_frame_opus);
                    update_input_level(&resampled);

                    let count = frame_counter.fetch_add(1, Ordering::SeqCst);
                    if count == 0 {
                        tracing::info!("[AudioCapture] First frame captured");
                    } else if count % 250 == 0 {
                        tracing::debug!(
                            "[AudioCapture] Frames: {} ({:.1}s)",
                            count,
                            count as f32 * 0.02
                        );
                    }

                    match (&sink, &mut encoder) {
                        (CaptureSink::Opus(tx), Some(encoder)) => {
                            let pcm_bytes: Vec<u8> =
                                resampled.iter().flat_map(|s| s.to_le_bytes()).collect();
                            match encoder.encode(&pcm_bytes) {
                                Ok(opus_frame) => match tx.try_send(opus_frame) {
                                    Ok(()) => {}
                                    Err(tokio_mpsc::error::TrySendError::Full(_)) => {
                                        tracing::warn!(
                                            "[AudioCapture] Channel full, dropping frame"
                                        );
                                    }
                                    Err(tokio_mpsc::error::TrySendError::Closed(_)) => {
                                        tracing::warn!(
                                            "[AudioCapture] Receiver closed, stopping capture"
                                        );
                                        break 'capture;
                                    }
                                },
                                Err(e) => {
                                    if count == 0 {
                                        tracing::error!("[AudioCapture] First encode error: {}", e);
                                    }
                                }
                            }
                        }
                        (CaptureSink::Pcm(tx), _) => match tx.try_send(resampled) {
                            Ok(()) => {}
                            Err(tokio_mpsc::error::TrySendError::Full(_)) => {
                                tracing::warn!("[AudioCapture] Channel full, dropping frame");
                            }
                            Err(tokio_mpsc::error::TrySendError::Closed(_)) => {
                                tracing::warn!("[AudioCapture] Receiver closed, stopping capture");
                                break 'capture;
                            }
                        },
                        (CaptureSink::Opus(_), None) => unreachable!("opus sink without encoder"),
                    }
                }
                Err(std_mpsc::RecvTimeoutError::Timeout) => {
                    // Normal timeout
                }
                Err(std_mpsc::RecvTimeoutError::Disconnected) => {
                    tracing::warn!("[AudioCapture] Channel disconnected");
                    break;
                }
            }

            if last_device_check.elapsed() >= std::time::Duration::from_secs(1) {
                last_device_check = std::time::Instant::now();
                if let Some((_, new_name)) = resolve_input_device(&host, preferred_device) {
                    if new_name != current_device_name {
                        tracing::info!(
                            "[AudioCapture] Microphone changed from '{}' to '{}'. Restarting stream...",
                            current_device_name,
                            new_name
                        );
                        device_changed = true;
                    }
                }
            }
        }

        let _ = stream.pause();
    }

    let total = frame_counter.load(Ordering::SeqCst);
    INPUT_LEVEL.store(0, Ordering::Relaxed);
    tracing::info!(
        "[AudioCapture] Stopped. {} frames ({:.1}s)",
        total,
        total as f32 * 0.02
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::resample_mono_i16;

    #[test]
    fn integer_ratio_downsampling_averages_groups() {
        // 48k -> 16k means every 3 samples become 1.
        let input = vec![300, 600, 900, -300, -600, -900];
        assert_eq!(resample_mono_i16(&input, 2), vec![600, -600]);
    }

    #[test]
    fn fractional_ratio_interpolates_linearly() {
        let input = vec![0, 1000];
        let output = resample_mono_i16(&input, 3);
        assert_eq!(output, vec![0, 500, 1000]);
    }

    #[test]
    fn identity_passthrough_keeps_samples() {
        let input = vec![1, 2, 3];
        assert_eq!(resample_mono_i16(&input, 3), input);
    }

    #[test]
    fn empty_input_yields_silence() {
        assert_eq!(resample_mono_i16(&[], 4), vec![0, 0, 0, 0]);
    }
}
