use crate::config::VoicePluginConfig;
use anyhow::{bail, Context, Result};
use std::io::Cursor;
use std::path::Path;
use std::process::Command;

pub fn speak(config: &VoicePluginConfig, text: &str) -> Result<()> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(());
    }
    match config.tts_backend.trim().to_ascii_lowercase().as_str() {
        "espeak-ng" => espeak_ng(config, text),
        "piper" => piper(config, text),
        "xiaomi" => xiaomi(config, text),
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

fn espeak_ng(config: &VoicePluginConfig, text: &str) -> Result<()> {
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
    play_file(&wav)
}

fn piper(config: &VoicePluginConfig, text: &str) -> Result<()> {
    let bin = if config.tts_command.trim().is_empty() {
        "piper"
    } else {
        config.tts_command.trim()
    };
    let wav = output_audio_path("wav");
    let model = config.tts_voice.trim();
    let mut command = Command::new(bin);
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
    play_file(&wav)
}

fn xiaomi(config: &VoicePluginConfig, text: &str) -> Result<()> {
    let mp3 = output_audio_path("mp3");
    crate::voice::xiaomi::synthesize(config, text, &mp3)?;
    play_file(&mp3)
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
    let expanded = command_line
        .replace("{text}", &quote_shell(text))
        .replace("{file}", &out.display().to_string());
    let parts = shell_words(&expanded);
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
        return play_file(&out);
    }
    // Custom commands may play audio themselves.
    Ok(())
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

fn play_file(path: &Path) -> Result<()> {
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
    sink.sleep_until_end();
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
