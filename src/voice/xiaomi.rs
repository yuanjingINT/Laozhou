use crate::config::VoicePluginConfig;
use anyhow::{bail, Context, Result};
use base64::Engine;
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;

/// Shared OpenAI-compatible client for Xiaomi MiMo TTS / ASR.
///
/// Xiaomi's audio models are served through `/chat/completions`:
/// - ASR: model `mimo-v2.5-asr`, user message with `input_audio`, returns `content` text.
/// - TTS: model `mimo-v2.5-tts`, assistant message with the text to speak,
///   `modalities: ["text", "audio"]` and `audio.voice`, returns base64 audio in `audio.data`.
pub fn client(config: &VoicePluginConfig) -> Result<Client> {
    let base = base_url(config);
    if base.is_empty() {
        bail!(
            "{}",
            crate::i18n::text(
                "plugins.voice.xiaomi_base_url is empty",
                "plugins.voice.xiaomi_base_url 为空",
            )
        );
    }
    resolved_api_key(config)?;
    Client::builder()
        .timeout(Duration::from_secs(90))
        .build()
        .with_context(|| "building xiaomi HTTP client".to_string())
}

fn resolved_api_key(config: &VoicePluginConfig) -> Result<String> {
    let raw = config.xiaomi_api_key.trim();
    let value = if let Some(env_name) = raw.strip_prefix("$env:") {
        std::env::var(env_name)
            .with_context(|| format!("environment variable {env_name} is not set"))?
    } else {
        raw.to_string()
    };
    let value = value.trim().to_string();
    if value.is_empty() {
        bail!(
            "{}",
            crate::i18n::text(
                "plugins.voice.xiaomi_api_key is empty; get one from the Xiaomi MiMo platform",
                "plugins.voice.xiaomi_api_key 为空；请到小米 MiMo 开放平台获取",
            )
        );
    }
    Ok(value)
}

/// The bearer token used for requests, after resolving `$env:` variables.
pub fn bearer(config: &VoicePluginConfig) -> Result<String> {
    resolved_api_key(config)
}

pub fn base_url(config: &VoicePluginConfig) -> String {
    config.xiaomi_base_url.trim().trim_end_matches('/').to_string()
}

/// Transcribe audio using the Xiaomi ASR model (`/chat/completions` with input_audio).
pub fn transcribe(config: &VoicePluginConfig, wav_path: &Path) -> Result<String> {
    let model = config.xiaomi_stt_model.trim();
    if model.is_empty() {
        bail!(
            "{}",
            crate::i18n::text(
                "plugins.voice.xiaomi_stt_model is empty",
                "plugins.voice.xiaomi_stt_model 为空",
            )
        );
    }
    let client = client(config)?;
    let audio = std::fs::read(wav_path)?;
    let data = base64::engine::general_purpose::STANDARD.encode(&audio);
    let payload = json!({
        "model": model,
        "messages": [{
            "role": "user",
            "content": [{
                "type": "input_audio",
                "input_audio": { "data": data, "format": "wav" }
            }]
        }],
        "max_tokens": 512
    });
    let response = client
        .post(format!("{}/chat/completions", base_url(config)))
        .bearer_auth(bearer(config)?)
        .json(&payload)
        .send()?;
    let status = response.status();
    let body = response.text()?;
    if !status.is_success() {
        bail!(
            "{}",
            crate::i18n::text_owned(
                format!("Xiaomi STT failed ({status}): {body}"),
                format!("小米语音识别失败（{status}）: {body}"),
            )
        );
    }
    let value: Value = serde_json::from_str(&body)?;
    let text = value
        .get("choices")
        .and_then(|choices| choices.get(0))
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .map(|text| text.trim().to_string())
        .unwrap_or_default();
    if text.is_empty() {
        bail!(
            "{}",
            crate::i18n::text_owned(
                format!("no transcript in Xiaomi STT response: {body}"),
                format!("小米语音识别响应中没有文本: {body}"),
            )
        );
    }
    Ok(text)
}

/// Synthesize speech using the Xiaomi TTS model (`/chat/completions` with audio modality).
/// Writes the returned audio to `out_path`.
pub fn synthesize(config: &VoicePluginConfig, text: &str, out_path: &Path) -> Result<()> {
    let model = config.xiaomi_tts_model.trim();
    if model.is_empty() {
        bail!(
            "{}",
            crate::i18n::text(
                "plugins.voice.xiaomi_tts_model is empty",
                "plugins.voice.xiaomi_tts_model 为空",
            )
        );
    }
    let client = client(config)?;
    let voice = if config.xiaomi_tts_voice.trim().is_empty() {
        "mimo_default"
    } else {
        config.xiaomi_tts_voice.trim()
    };
    let payload = json!({
        "model": model,
        "messages": [{
            "role": "assistant",
            "content": text
        }],
        "max_tokens": 4096,
        "modalities": ["text", "audio"],
        "audio": {
            "voice": voice,
            "format": "mp3"
        }
    });
    let response = client
        .post(format!("{}/chat/completions", base_url(config)))
        .bearer_auth(bearer(config)?)
        .json(&payload)
        .send()?;
    let status = response.status();
    let body = response.text()?;
    if !status.is_success() {
        bail!(
            "{}",
            crate::i18n::text_owned(
                format!("Xiaomi TTS failed ({status}): {body}"),
                format!("小米语音合成失败（{status}）: {body}"),
            )
        );
    }
    let value: Value = serde_json::from_str(&body)?;
    let data = value
        .get("choices")
        .and_then(|choices| choices.get(0))
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("audio"))
        .and_then(|audio| audio.get("data"))
        .and_then(Value::as_str)
        .with_context(|| format!("no audio data in Xiaomi TTS response: {body}"))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .context("failed to decode Xiaomi TTS audio")?;
    if bytes.is_empty() {
        bail!(
            "{}",
            crate::i18n::text(
                "Xiaomi TTS returned empty audio",
                "小米语音合成返回空音频",
            )
        );
    }
    std::fs::write(out_path, &bytes)?;
    Ok(())
}

/// Available Xiaomi TTS voices (discovered from the MiMo platform).
pub const XIAOMI_TTS_VOICES: &[&str] = &[
    "mimo_default",
    "冰糖",
    "茉莉",
    "苏打",
    "白桦",
    "Mia",
    "Chloe",
    "Milo",
    "Dean",
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    fn spawn_echo_server() -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let requests_clone = requests.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = match stream {
                    Ok(s) => s,
                    Err(_) => break,
                };
                let mut buf = Vec::new();
                let mut chunk = [0u8; 2048];
                loop {
                    let n = stream.read(&mut chunk).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }
                let text = String::from_utf8_lossy(&buf).to_string();
                requests_clone.lock().unwrap().push(text.clone());
                let (body, mime) = if text.contains("\"model\": \"mimo-v2.5-asr\"")
                    || text.contains("mimo-v2.5-asr")
                {
                    ("{\"choices\":[{\"message\":{\"content\":\"你好老周\"}}]}".to_string(), "application/json")
                } else {
                    let fake = base64::engine::general_purpose::STANDARD.encode("ID3FAKEAUDIO");
                    (format!("{{\"choices\":[{{\"message\":{{\"audio\":{{\"data\":\"{fake}\"}}}}}}]}}"), "application/json")
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nContent-Length: {}\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(body.as_bytes());
            }
        });
        (format!("http://{addr}/v1"), requests)
    }

    fn test_config(base_url: &str) -> VoicePluginConfig {
        let mut config = VoicePluginConfig::default();
        config.xiaomi_base_url = base_url.to_string();
        config.xiaomi_api_key = "test-key".to_string();
        config.xiaomi_stt_model = "mimo-v2.5-asr".to_string();
        config.xiaomi_tts_model = "mimo-v2.5-tts".to_string();
        config.xiaomi_tts_voice = "冰糖".to_string();
        config
    }

    #[test]
    fn normalizes_base_url() {
        let mut config = VoicePluginConfig::default();
        config.xiaomi_base_url = "https://api.xiaomimimo.com/v1/".to_string();
        assert_eq!(base_url(&config), "https://api.xiaomimimo.com/v1");
    }

    #[test]
    fn resolves_env_api_key() {
        std::env::set_var("LAOZHOU_TEST_XIAOMI_KEY", "env-secret");
        let mut config = VoicePluginConfig::default();
        config.xiaomi_api_key = "$env:LAOZHOU_TEST_XIAOMI_KEY".to_string();
        assert_eq!(bearer(&config).unwrap(), "env-secret");
        std::env::remove_var("LAOZHOU_TEST_XIAOMI_KEY");
    }

    #[test]
    fn stt_sends_input_audio_and_returns_text() {
        let (base, requests) = spawn_echo_server();
        let config = test_config(&base);
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("in.wav");
        std::fs::write(&wav, b"RIFF....WAVE fake").unwrap();
        let result = transcribe(&config, &wav).unwrap();
        assert_eq!(result, "你好老周");
        let req = requests.lock().unwrap().first().cloned().unwrap();
        assert!(req.contains("POST /v1/chat/completions"));
        assert!(req.to_lowercase().contains("authorization: bearer test-key"));
        assert!(req.contains("mimo-v2.5-asr"));
        assert!(req.contains("input_audio"));
        assert!(req.contains("format"));
    }

    #[test]
    fn tts_sends_audio_modality_and_saves_audio() {
        let (base, requests) = spawn_echo_server();
        let config = test_config(&base);
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out.mp3");
        synthesize(&config, "你好", &out).unwrap();
        let data = std::fs::read(&out).unwrap();
        assert!(data.starts_with(b"ID3"));
        let req = requests.lock().unwrap().first().cloned().unwrap();
        assert!(req.contains("POST /v1/chat/completions"));
        assert!(req.to_lowercase().contains("authorization: bearer test-key"));
        assert!(req.contains("mimo-v2.5-tts"));
        assert!(req.contains("modalities"));
        assert!(req.contains("audio"));
    }
}
