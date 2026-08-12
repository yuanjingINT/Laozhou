use crate::config::VoicePluginConfig;
use anyhow::{bail, Context, Result};
use std::io::Cursor;
use std::path::Path;
use std::process::Command;

/// Speak `text`; `on_tick` is invoked periodically during playback so the UI
/// can animate the orb while the assistant is speaking.
pub fn speak_with_tick(
    config: &VoicePluginConfig,
    text: &str,
    on_tick: &mut dyn FnMut(u64),
) -> Result<()> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(());
    }
    match config.tts_backend.trim().to_ascii_lowercase().as_str() {
        "espeak-ng" => espeak_ng(config, text, on_tick),
        "piper" => piper(config, text, on_tick),
        "xiaomi" => xiaomi(config, text, on_tick),
        "command" => custom_command(config, text),
        "none" => Ok(()),
        other => bail!(
            "{}",
            crate::i18n::text_owned(
                format!("unsupported TTS backend: {other}"),
                format!("不支持的语音合成后端: {other}"),
            )
        ),
    }
}

fn output_audio_path(extension: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "laozhou_tts_{}.{}",
        std::process::id(),
        extension
    ))
}

fn espeak_ng(
    config: &VoicePluginConfig,
    text: &str,
    on_tick: &mut dyn FnMut(u64),
) -> Result<()> {
    let bin = if config.tts_command.trim().is_empty() {
        "espeak-ng"
    } else {
        config.tts_command.trim()
    };
    let wav = output_audio_path("wav");
    let voice = config.tts_voice.trim();
    let mut command = Command::new(bin);
    command.arg("-w").arg(&wav);
    if !voice.is_empty() {
        command.arg("-v").arg(voice);
    }
    command.arg(text);
    let output = command
        .output()
        .with_context(|| format!("failed to run {bin}"))?;
    if !output.status.success() {
        bail!(
            "{}",
            crate::i18n::text(
                "espeak-ng failed to synthesize speech",
                "espeak-ng 语音合成失败",
            )
        );
    }
    play_file(&wav, on_tick)
}

fn piper(
    config: &VoicePluginConfig,
    text: &str,
    on_tick: &mut dyn FnMut(u64),
) -> Result<()> {
    let bin = expand_tilde(config.tts_command.trim());
    let bin = if bin.is_empty() {
        "piper".to_string()
    } else {
        bin
    };
    let wav = output_audio_path("wav");
    let model = expand_tilde(config.tts_voice.trim());
    let mut command = Command::new(bin.clone());
    if !model.is_empty() {
        command.arg("-m").arg(model);
    }
    command.arg("-f").arg(&wav);
    let mut child = command
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .with_context(|| format!("failed to run {bin}"))?;
    if let Some(stdin) = child.stdin.as_mut() {
        use std::io::Write;
        let _ = stdin.write_all(text.as_bytes());
    }
    let status = child.wait()?;
    if !status.success() {
        bail!(
            "{}",
            crate::i18n::text(
                "piper failed to synthesize speech",
                "piper 语音合成失败",
            )
        );
    }
    play_file(&wav, on_tick)
}

fn xiaomi(config: &VoicePluginConfig, text: &str, on_tick: &mut dyn FnMut(u64)) -> Result<()> {
    let mp3 = output_audio_path("mp3");
    crate::voice::xiaomi::synthesize(config, text, &mp3)?;
    play_file(&mp3, on_tick)
}

fn custom_command(config: &VoicePluginConfig, text: &str) -> Result<()> {
    let command_line = config.tts_command.trim();
    if command_line.is_empty() {
        bail!(
            "{}",
            crate::i18n::text(
                "plugins.voice.tts_command is empty",
                "plugins.voice.tts_command 为空",
            )
        );
    }
    let out = output_audio_path("wav");
    let text_path = output_audio_path("txt");
    std::fs::write(&text_path, text).with_context(|| "writing tts text file")?;
    let expanded = command_line
        .replace("{text}", &quote_shell(text))
        .replace("{text_file}", &text_path.display().to_string())
        .replace("{file}", &out.display().to_string());
    let parts: Vec<String> = shell_words(&expanded)
        .into_iter()
        .map(|w| expand_tilde(&w))
        .collect();
    let (program, args) = parts
        .split_first()
        .context("empty tts_command")?;
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("failed to run {program}"))?;
    if !output.status.success() {
        bail!(
            "{}",
            crate::i18n::text(
                "custom TTS command failed",
                "自定义语音合成命令执行失败",
            )
        );
    }
    if out.exists() {
        let mut noop = |_: u64| {};
        return play_file(&out, &mut noop);
    }
    // Custom commands may play audio themselves.
    Ok(())
}

/// Expand a leading `~` in a path to the user's home directory.
fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf()) {
            return home.join(rest).display().to_string();
        }
    }
    path.to_string()
}

fn quote_shell(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

fn shell_words(input: &str) -> Vec<String> {
    input
        .split_whitespace()
        .map(|word| word.trim_matches('"').trim_matches('\'').to_string())
        .collect()
}

fn play_file(path: &Path, on_tick: &mut dyn FnMut(u64)) -> Result<()> {
    let audio = std::fs::read(path).with_context(|| {
        format!(
            "{}: {}",
            crate::i18n::text("failed to read audio file", "读取音频文件失败"),
            path.display()
        )
    })?;
    let (_stream, handle) = rodio::OutputStream::try_default()?;
    let cursor = Cursor::new(audio);
    let sink = rodio::Sink::try_new(&handle)?;
    let source = rodio::Decoder::new(cursor)?;
    sink.append(source);
    let mut tick = 0u64;
    while !sink.empty() {
        on_tick(tick);
        std::thread::sleep(std::time::Duration::from_millis(50));
        tick = tick.wrapping_add(1);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_shell_text() {
        assert_eq!(quote_shell("it's fine"), "'it'\\''s fine'");
    }

    #[test]
    fn splits_command_words() {
        assert_eq!(
            shell_words("my-tts -v zh '你好'"),
            vec!["my-tts", "-v", "zh", "你好"]
        );
    }
}
