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

pub struct AudioCapture {
    is_recording: Arc<AtomicBool>,
}

impl AudioCapture {
    pub fn new() -> Result<Self> {
        let host = cpal::default_host();
        match host.default_input_device() {
            Some(device) => {
                println!(
                    "[AudioCapture] Default device: {}",
                    device.name().unwrap_or_default()
                );
            }
            None => {
                println!("[AudioCapture] WARNING: No default input device found.");
            }
        }

        Ok(Self {
            is_recording: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn is_recording(&self) -> bool {
        self.is_recording.load(Ordering::SeqCst)
    }

    pub fn start(&self) -> Result<tokio_mpsc::Receiver<Vec<u8>>> {
        if self.is_recording.swap(true, Ordering::SeqCst) {
            return Err(anyhow!("Already recording"));
        }

        let (tokio_tx, tokio_rx) = tokio_mpsc::channel::<Vec<u8>>(500);
        let is_recording = self.is_recording.clone();

        thread::spawn(move || {
            #[cfg(target_os = "windows")]
            {
                use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
                unsafe {
                    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                }
                println!("[AudioCapture] COM initialized");
            }

            println!("[AudioCapture] >>> Thread spawned <<<");
            use std::io::Write;
            let _ = std::io::stdout().flush();

            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_audio_capture(tokio_tx, is_recording.clone())
            }));

            match result {
                Ok(Ok(_)) => {
                    println!("[AudioCapture] Completed normally");
                }
                Ok(Err(e)) => {
                    println!("[AudioCapture] ERROR: {}", e);
                }
                Err(panic_info) => {
                    println!("[AudioCapture] PANIC: {:?}", panic_info);
                }
            }

            is_recording.store(false, Ordering::SeqCst);
            println!("[AudioCapture] Thread exiting");
            let _ = std::io::stdout().flush();
        });

        tracing::info!("Audio capture started");
        Ok(tokio_rx)
    }

    pub fn stop(&self) {
        self.is_recording.store(false, Ordering::SeqCst);
        INPUT_LEVEL.store(0, Ordering::Relaxed);
        tracing::info!("Audio capture stopped");
    }
}

fn run_audio_capture(
    tokio_tx: tokio_mpsc::Sender<Vec<u8>>,
    is_recording: Arc<AtomicBool>,
) -> Result<()> {
    let host = cpal::default_host();
    let frame_counter = Arc::new(AtomicU64::new(0));
    // Outer loop to handle device setup / restart
    while is_recording.load(Ordering::SeqCst) {
        let device = match host.default_input_device() {
            Some(dev) => dev,
            None => {
                println!("[AudioCapture] No input device available, retrying in 1s...");
                thread::sleep(std::time::Duration::from_secs(1));
                continue;
            }
        };

        let device_name = device.name().unwrap_or_default();
        let current_device_name = device_name.clone();
        println!("[AudioCapture] Target device: {}", device_name);

        // Get the device's default config
        let supported_config = match device.default_input_config() {
            Ok(cfg) => cfg,
            Err(e) => {
                println!(
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

        // Create Opus encoder (16kHz mono)
        let mut encoder = match OpusEncoder::new(OPUS_SAMPLE_RATE, OPUS_CHANNELS) {
            Ok(enc) => enc,
            Err(e) => {
                println!("[AudioCapture] Opus encoder FAILED: {}", e);
                return Err(e);
            }
        };

        // Calculate frame sizes
        let samples_per_frame_native =
            (native_sample_rate * FRAME_DURATION_MS / 1000) as usize * native_channels as usize;
        let samples_per_frame_opus = (OPUS_SAMPLE_RATE * FRAME_DURATION_MS / 1000) as usize; // mono

        let (std_tx, std_rx) = std_mpsc::channel::<Vec<i16>>();
        let is_recording_clone = is_recording.clone();
        let err_fn = |err| {
            println!("[AudioCapture] Stream error: {}", err);
        };

        let stream = match sample_format {
            SampleFormat::I16 => {
                let mut buffer = Vec::<i16>::with_capacity(samples_per_frame_native * 2);
                match device.build_input_stream(
                    &config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        if !is_recording_clone.load(Ordering::SeqCst) {
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
                        println!(
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
                match device.build_input_stream(
                    &config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        if !is_recording_clone.load(Ordering::SeqCst) {
                            return;
                        }
                        let samples: Vec<i16> =
                            data.iter().map(|s| (*s * 32767.0) as i16).collect();
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
                        println!(
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
            println!(
                "[AudioCapture] Failed to play stream: {}, retrying in 1s...",
                e
            );
            thread::sleep(std::time::Duration::from_secs(1));
            continue;
        }

        println!("[AudioCapture] Stream playing: {}!", device_name);
        let mut last_device_check = std::time::Instant::now();
        let mut device_changed = false;

        // Inner processing loop for the current stream/device
        while is_recording.load(Ordering::SeqCst) && !device_changed {
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

                    let mono_samples_per_native_frame =
                        samples_per_frame_native / native_channels as usize;
                    let mut resampled: Vec<i16> =
                        if mono_samples_per_native_frame != samples_per_frame_opus {
                            let ratio = mono_samples_per_native_frame as f32
                                / samples_per_frame_opus as f32;
                            (0..samples_per_frame_opus)
                                .map(|i| {
                                    let src_idx =
                                        ((i as f32 * ratio) as usize).min(mono_frame.len() - 1);
                                    mono_frame[src_idx]
                                })
                                .collect()
                        } else {
                            mono_frame
                        };

                    if !resampled.is_empty() {
                        let sum_sq: f64 = resampled.iter().map(|&s| (s as f64) * (s as f64)).sum();
                        let rms = (sum_sq / resampled.len() as f64).sqrt();

                        let noise_gate_threshold = 150.0;
                        if rms < noise_gate_threshold {
                            resampled.fill(0);
                            INPUT_LEVEL.store(0, Ordering::Relaxed);
                        } else {
                            let norm = (rms / 6000.0).min(1.0);
                            INPUT_LEVEL.store((norm * 1000.0) as u32, Ordering::Relaxed);
                        }
                    }

                    let pcm_bytes: Vec<u8> =
                        resampled.iter().flat_map(|s| s.to_le_bytes()).collect();

                    match encoder.encode(&pcm_bytes) {
                        Ok(opus_frame) => {
                            let count = frame_counter.fetch_add(1, Ordering::SeqCst);
                            if count == 0 {
                                println!("[Audio] First frame captured and encoded!");
                            }
                            if count > 0 && count % 50 == 0 {
                                println!(
                                    "[AudioCapture] Frames: {} ({:.1}s)",
                                    count,
                                    count as f32 * 0.02
                                );
                            }

                            if tokio_tx.try_send(opus_frame).is_err() {
                                println!("[AudioCapture] Channel full, dropping frame");
                            }
                        }
                        Err(e) => {
                            if frame_counter.load(Ordering::SeqCst) == 0 {
                                println!("[AudioCapture] First encode error: {}", e);
                            }
                        }
                    }
                }
                Err(std_mpsc::RecvTimeoutError::Timeout) => {
                    // Normal timeout
                }
                Err(std_mpsc::RecvTimeoutError::Disconnected) => {
                    println!("[AudioCapture] Channel disconnected");
                    break;
                }
            }

            if last_device_check.elapsed() >= std::time::Duration::from_secs(1) {
                last_device_check = std::time::Instant::now();
                if let Some(new_device) = host.default_input_device() {
                    if let Ok(new_name) = new_device.name() {
                        if new_name != current_device_name {
                            println!("[AudioCapture] Default microphone changed from '{}' to '{}'. Restarting stream...", current_device_name, new_name);
                            device_changed = true;
                        }
                    }
                }
            }
        }

        let _ = stream.pause();
    }

    let total = frame_counter.load(Ordering::SeqCst);
    INPUT_LEVEL.store(0, Ordering::Relaxed);
    println!("[AudioCapture] Total frames: {}", total);
    println!(
        "[Mic] Stopped. {} frames ({:.1}s)",
        total,
        total as f32 * 0.02
    );

    Ok(())
}
