use crate::config::VoicePluginConfig;
use anyhow::{bail, Context, Result};
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordBackend {
    Auto,
    Parec,
    PwRecord,
    Arecord,
}

impl RecordBackend {
    pub fn from_config(config: &VoicePluginConfig) -> Self {
        match config.record_backend.trim().to_ascii_lowercase().as_str() {
            "parec" | "parecord" => Self::Parec,
            "pw-record" | "pwrecord" => Self::PwRecord,
            "arecord" => Self::Arecord,
            _ => Self::Auto,
        }
    }

    pub fn resolve(self) -> Result<Self> {
        if self != Self::Auto {
            return Ok(self);
        }
        for candidate in [Self::PwRecord, Self::Parec, Self::Arecord] {
            if candidate.binary_available() {
                return Ok(candidate);
            }
        }
        bail!(
            "{}",
            crate::i18n::text_owned(
                "no audio recorder found; install pipewire (pw-record), pulseaudio (parec), or alsa-utils (arecord)".to_string(),
                "未找到录音工具；请安装 pipewire (pw-record)、pulseaudio (parec) 或 alsa-utils (arecord)".to_string(),
            )
        )
    }

    pub fn binary_available(self) -> bool {
        let name = self.binary_name();
        which(name).is_some()
    }

    pub fn binary_name(self) -> &'static str {
        match self {
            Self::Auto => "pw-record",
            Self::Parec => "parec",
            Self::PwRecord => "pw-record",
            Self::Arecord => "arecord",
        }
    }
}

fn which(name: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

struct RecorderChild {
    child: Child,
}

impl RecorderChild {
    fn spawn(backend: RecordBackend, config: &VoicePluginConfig) -> Result<Self> {
        let backend = backend.resolve()?;
        ensure_denoised_source(config);
        let (program, args) = recorder_command(backend, config);
        let child = Command::new(program)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("failed to start {program}"))?;
        Ok(Self { child })
    }
}

/// If the configured input device is the noise-cancelled PipeWire source and it
/// is not present, create it on the fly (WebRTC noise suppression). This keeps
/// the voice pipeline working even if the persistent PipeWire config did not
/// load (e.g. after a reboot).
fn ensure_denoised_source(config: &VoicePluginConfig) {
    let device = config.input_device.trim();
    if device != "denoised_source" {
        return;
    }
    if source_exists("denoised_source") {
        return;
    }
    let _ = Command::new("pactl")
        .args([
            "load-module",
            "module-echo-cancel",
            "source_name=denoised_source",
            "source_properties=device.description=Noise_cancelled_Mic",
            "use_master_format=1",
            "aec_method=webrtc",
            "aec_args=analog_gain_control=0 noise_suppression=1 voice_detection=1",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn source_exists(name: &str) -> bool {
    let Ok(output) = Command::new("pactl")
        .args(["list", "sources", "short"])
        .output()
    else {
        return false;
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|line| line.contains(name))
}

fn recorder_command(backend: RecordBackend, config: &VoicePluginConfig) -> (&'static str, Vec<String>) {
    let device = config.input_device.trim();
    match backend {
        RecordBackend::PwRecord => {
            let mut args = vec![
                "--rate=16000".to_string(),
                "--channels=1".to_string(),
                "--format=s16".to_string(),
                "--raw".to_string(),
            ];
            if !device.is_empty() {
                args.push(format!("--target={device}"));
            }
            args.push("-".to_string());
            ("pw-record", args)
        }
        RecordBackend::Parec => {
            let mut args = vec![
                "--raw".to_string(),
                "--format=s16le".to_string(),
                "--rate=16000".to_string(),
                "--channels=1".to_string(),
            ];
            if !device.is_empty() {
                args.push(format!("--device={device}"));
            }
            args.push("-".to_string());
            ("parec", args)
        }
        RecordBackend::Arecord => {
            let mut args = vec![
                "-f".to_string(),
                "S16_LE".to_string(),
                "-r".to_string(),
                "16000".to_string(),
                "-c".to_string(),
                "1".to_string(),
                "-t".to_string(),
                "raw".to_string(),
            ];
            if !device.is_empty() {
                args.push(format!("-D{device}"));
            }
            args.push("-".to_string());
            ("arecord", args)
        }
        RecordBackend::Auto => unreachable!("auto recorder resolved before use"),
    }
}

const SAMPLE_RATE: u32 = 16_000;
const CHANNELS: u16 = 1;
const SAMPLE_BYTES: usize = 2;

/// Continuously listen to the microphone and capture one short utterance for
/// wake-word detection. Waits for voice activity (with a small pre-roll), then
/// records a fixed short window so the captured audio stays clean and short,
/// which greatly improves ASR accuracy for a two-syllable wake word.
pub fn listen_for_speech(config: &VoicePluginConfig) -> Result<std::path::PathBuf> {
    const FRAME_SAMPLES: usize = 400; // 25ms at 16kHz

    let backend = RecordBackend::from_config(config).resolve()?;
    let pre_roll_ms = 300u64;
    let capture_ms = config.wake_window_ms.max(600).min(3000);
    let mut child = RecorderChild::spawn(backend, config)?;
    let stdout = child
        .child
        .stdout
        .take()
        .context("recorder stdout unavailable")?;
    let mut reader = std::io::BufReader::new(stdout);

    let mut pre_roll: Vec<i16> = Vec::new();
    let mut utterance: Vec<i16> = Vec::new();
    let mut started = false;
    let mut buf = [0u8; 4096];

    let pre_roll_samples = (pre_roll_ms * SAMPLE_RATE as u64 / 1000) as usize;
    let capture_samples = (capture_ms * SAMPLE_RATE as u64 / 1000) as usize;

    // Calibrate the noise floor from the first second of ambient audio.
    let calibration_samples = SAMPLE_RATE as usize;
    let mut noise_rms_samples: Vec<f32> = Vec::new();
    let mut calibrated = false;
    let mut collected = 0usize;
    // Consecutive speech frames needed to start an utterance (rejects clicks).
    let start_confirm_frames = 2u32;

    let mut pending: Vec<i16> = Vec::new();
    let mut loud_frames = 0u32;

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        let chunk_samples = n / SAMPLE_BYTES;
        let bytes = &buf[..chunk_samples * SAMPLE_BYTES];
        for pair in bytes.chunks_exact(SAMPLE_BYTES) {
            let sample = i16::from_le_bytes([pair[0], pair[1]]);
            pending.push(sample);
        }

        // Process complete 25ms frames.
        let frame_samples = FRAME_SAMPLES * SAMPLE_BYTES;
        let frame_count = pending.len() * SAMPLE_BYTES / frame_samples;
        for f in 0..frame_count {
            let start = f * FRAME_SAMPLES;
            let frame = &pending[start..start + FRAME_SAMPLES];
            let frame_rms = rms_i16(frame);

            if !calibrated {
                collected += FRAME_SAMPLES;
                if collected <= calibration_samples {
                    noise_rms_samples.push(frame_rms);
                }
                if collected > calibration_samples && !noise_rms_samples.is_empty() {
                    calibrated = true;
                }
            }

            let threshold = voice_threshold(calibrated, &noise_rms_samples);
            if frame_rms >= threshold {
                loud_frames = loud_frames.saturating_add(1);
            } else {
                loud_frames = 0;
            }

            if !started {
                // Keep a rolling pre-roll buffer.
                pre_roll.extend_from_slice(frame);
                if pre_roll.len() > pre_roll_samples {
                    let overflow = pre_roll.len() - pre_roll_samples;
                    pre_roll.drain(0..overflow);
                }
                if loud_frames >= start_confirm_frames {
                    started = true;
                    utterance.extend_from_slice(&pre_roll);
                }
            } else {
                utterance.extend_from_slice(frame);
                if utterance.len() >= capture_samples {
                    let _ = child.child.kill();
                    let _ = child.child.wait();
                    return write_wav(&utterance, &temp_wav_path());
                }
            }
        }

        // Drop processed samples.
        let processed = frame_count * FRAME_SAMPLES;
        if processed > 0 {
            pending.drain(0..processed);
        }
    }

    let _ = child.child.kill();
    let _ = child.child.wait();

    if !started || utterance.is_empty() {
        bail!(
            "{}",
            crate::i18n::text(
                "no voice activity detected",
                "未检测到语音活动",
            )
        );
    }
    write_wav(&utterance, &temp_wav_path())
}

fn rms_i16(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: i64 = samples.iter().map(|s| i64::from(*s) * i64::from(*s)).sum();
    ((sum as f64 / samples.len() as f64).sqrt()) as f32
}

/// Adaptive voice-activity threshold relative to the calibrated noise floor.
/// Speech must be several times louder than the measured background noise so a
/// noisy USB microphone's own hum does not trigger false wake-ups. The absolute
/// floor protects quiet microphones that report near-zero ambient RMS.
fn voice_threshold(calibrated: bool, noise_rms: &[f32]) -> f32 {
    if calibrated && !noise_rms.is_empty() {
        let mut sorted = noise_rms.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let p95_index = (sorted.len() as f32 * 0.95) as usize;
        let p95 = sorted[p95_index.min(sorted.len() - 1)];
        let median = sorted[sorted.len() / 2];
        // Trigger when speech is clearly louder than the ambient noise floor.
        // For noisy microphones this needs a comfortable margin above the hum;
        // for quiet microphones a low absolute floor suffices.
        let noise_based = (p95 * 1.8).max(median + 8000.0);
        return noise_based.max(1200.0);
    }
    // Uncalibrated fallback: a conservative fixed threshold.
    0.05 * i16::MAX as f32
}

/// Record a single utterance, ending automatically after a trailing silence
/// gap (adaptive to the mic noise floor) or the max duration. Captures the
/// utterance from the first detected voice activity with a short pre-roll.
pub fn record_utterance(config: &VoicePluginConfig) -> Result<std::path::PathBuf> {
    const FRAME_SAMPLES: usize = 400; // 25ms at 16kHz

    let backend = RecordBackend::from_config(config).resolve()?;
    let max_seconds = config.max_record_seconds.max(1);
    let silence_ms = config.silence_ms;
    let pre_roll_ms = 250u64;
    let mut child = RecorderChild::spawn(backend, config)?;
    let stdout = child
        .child
        .stdout
        .take()
        .context("recorder stdout unavailable")?;
    let mut reader = std::io::BufReader::new(stdout);

    let mut pre_roll: Vec<i16> = Vec::new();
    let mut utterance: Vec<i16> = Vec::new();
    let mut started = false;
    let mut buf = [0u8; 4096];

    let pre_roll_samples = (pre_roll_ms * SAMPLE_RATE as u64 / 1000) as usize;
    let end_silent_frames = (silence_ms.max(100) / 25).max(1) as u32;

    // Calibrate the noise floor from the first second of ambient audio.
    let calibration_samples = SAMPLE_RATE as usize;
    let mut noise_rms_samples: Vec<f32> = Vec::new();
    let mut calibrated = false;
    let mut collected = 0usize;
    let start_confirm_frames = 2u32;

    let mut pending: Vec<i16> = Vec::new();
    let mut loud_frames = 0u32;
    let mut silent_frames = 0u32;
    let started_at = Instant::now();

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        let chunk_samples = n / SAMPLE_BYTES;
        let bytes = &buf[..chunk_samples * SAMPLE_BYTES];
        for pair in bytes.chunks_exact(SAMPLE_BYTES) {
            let sample = i16::from_le_bytes([pair[0], pair[1]]);
            pending.push(sample);
        }

        let frame_count = pending.len() * SAMPLE_BYTES / (FRAME_SAMPLES * SAMPLE_BYTES);
        for f in 0..frame_count {
            let start = f * FRAME_SAMPLES;
            let frame = &pending[start..start + FRAME_SAMPLES];
            let frame_rms = rms_i16(frame);

            if !calibrated {
                collected += FRAME_SAMPLES;
                if collected <= calibration_samples {
                    noise_rms_samples.push(frame_rms);
                }
                if collected > calibration_samples && !noise_rms_samples.is_empty() {
                    calibrated = true;
                }
            }

            let threshold = voice_threshold(calibrated, &noise_rms_samples);
            if frame_rms >= threshold {
                loud_frames = loud_frames.saturating_add(1);
                silent_frames = 0;
            } else {
                loud_frames = 0;
                silent_frames = silent_frames.saturating_add(1);
            }

            if !started {
                pre_roll.extend_from_slice(frame);
                if pre_roll.len() > pre_roll_samples {
                    let overflow = pre_roll.len() - pre_roll_samples;
                    pre_roll.drain(0..overflow);
                }
                if loud_frames >= start_confirm_frames {
                    started = true;
                    utterance.extend_from_slice(&pre_roll);
                }
            } else {
                utterance.extend_from_slice(frame);
                if silent_frames >= end_silent_frames
                    && utterance.len() > pre_roll_samples + FRAME_SAMPLES
                {
                    // Trailing silence after real speech: done.
                    let _ = child.child.kill();
                    let _ = child.child.wait();
                    return write_wav(&utterance, &temp_wav_path());
                }
                if started_at.elapsed() >= Duration::from_secs(max_seconds) {
                    let _ = child.child.kill();
                    let _ = child.child.wait();
                    return write_wav(&utterance, &temp_wav_path());
                }
            }
        }

        let processed = frame_count * FRAME_SAMPLES;
        if processed > 0 {
            pending.drain(0..processed);
        }
    }

    let _ = child.child.kill();
    let _ = child.child.wait();

    if !started || utterance.is_empty() {
        bail!(
            "{}",
            crate::i18n::text(
                "no audio captured from microphone",
                "未从麦克风捕获到音频",
            )
        );
    }
    write_wav(&utterance, &temp_wav_path())
}

fn temp_wav_path() -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "laozhou_voice_{}_{}.wav",
        std::process::id(),
        n
    ))
}

fn write_wav(samples: &[i16], path: &Path) -> Result<std::path::PathBuf> {
    let data_len = (samples.len() * SAMPLE_BYTES) as u32;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&CHANNELS.to_le_bytes());
    bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(SAMPLE_RATE * CHANNELS as u32 * SAMPLE_BYTES as u32).to_le_bytes());
    bytes.extend_from_slice(&(CHANNELS * SAMPLE_BYTES as u16).to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    std::fs::write(path, &bytes)?;
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rms_detects_loud_audio() {
        let loud: Vec<i16> = vec![30000, -30000, 30000, -30000];
        assert!(rms_i16(&loud) > 0.5 * i16::MAX as f32);
    }

    #[test]
    fn rms_detects_silence() {
        let quiet = vec![0i16; 32];
        assert!(rms_i16(&quiet) < 1.0);
    }

    #[test]
    fn writes_valid_wav_header() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.wav");
        let samples = vec![100i16; 1000];
        let written = write_wav(&samples, &path).unwrap();
        assert_eq!(written, path);
        let data = std::fs::read(&path).unwrap();
        assert_eq!(&data[0..4], b"RIFF");
        assert_eq!(&data[8..12], b"WAVE");
        assert_eq!(&data[12..16], b"fmt ");
        assert_eq!(data[40..44], 2000u32.to_le_bytes());
    }
}
