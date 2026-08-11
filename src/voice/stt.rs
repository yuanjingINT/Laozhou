use crate::config::VoicePluginConfig;
use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;

pub fn transcribe(config: &VoicePluginConfig, wav_path: &Path) -> Result<String> {
    match config.stt_backend.trim().to_ascii_lowercase().as_str() {
        "whisper-cli" => whisper_cli(config, wav_path),
        "xiaomi" => crate::voice::xiaomi::transcribe(config, wav_path),
        "command" => custom_command(config, wav_path),
        "none" => bail!(
            "{}",
            crate::i18n::text(
                "speech-to-text is disabled in configuration",
                "配置中已禁用语音转文字",
            )
        ),
        other => bail!(
            "{}",
            crate::i18n::text_owned(
                format!("unsupported STT backend: {other}"),
                format!("不支持的语音转文字后端: {other}"),
            )
        ),
    }
}

fn whisper_cli(config: &VoicePluginConfig, wav_path: &Path) -> Result<String> {
    let bin = if config.stt_command.trim().is_empty() {
        "whisper-cli"
    } else {
        config.stt_command.trim()
    };
    let model = expand_tilde(config.stt_model.trim());
    if model.is_empty() {
        bail!(
            "{}",
            crate::i18n::text(
                "plugins.voice.stt_model is required for whisper-cli (e.g. /usr/share/whisper/models/ggml-base.bin)",
                "whisper-cli 需要配置 plugins.voice.stt_model（如 /usr/share/whisper/models/ggml-base.bin）",
            )
        );
    }    let language = config.stt_language.trim();
    let mut command = Command::new(bin);
    command
        .arg("-m")
        .arg(model)
        .arg("-f")
        .arg(wav_path)
        .arg("-nt")
        .arg("-np")
        .arg("-otxt")
        .arg("-of")
        .arg(transcript_base_path(wav_path));
    if !language.is_empty() && language != "auto" {
        command.arg("-l").arg(language);
    }
    // 简体中文引导：whisper 中文模型默认输出繁体，用初始 prompt 强制简体，
    // 配合 carry-initial-prompt 只作为上下文引导而不拼入转写结果。
    if language.eq_ignore_ascii_case("zh") || language.eq_ignore_ascii_case("cmn") {
        command
            .arg("--prompt")
            .arg("简体中文")
            .arg("--carry-initial-prompt");
    }
    let output = command
        .output()
        .with_context(|| format!("failed to run {bin}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "{}",
            crate::i18n::text_owned(
                format!("whisper-cli failed: {}", stderr.trim()),
                format!("whisper-cli 执行失败: {}", stderr.trim()),
            )
        );
    }
    let transcript = std::fs::read_to_string(transcript_path(wav_path)).unwrap_or_default();
    Ok(clean_transcript(&transcript))
}

fn custom_command(config: &VoicePluginConfig, wav_path: &Path) -> Result<String> {
    let command_line = config.stt_command.trim();
    if command_line.is_empty() {
        bail!(
            "{}",
            crate::i18n::text(
                "plugins.voice.stt_command is empty",
                "plugins.voice.stt_command 为空",
            )
        );
    }
    let expanded = command_line.replace("{file}", &wav_path.display().to_string());
    let parts = shell_words(&expanded);
    let (program, args) = parts
        .split_first()
        .context("empty stt_command")?;
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("failed to run {program}"))?;
    if !output.status.success() {
        bail!(
            "{}",
            crate::i18n::text(
                "custom STT command failed",
                "自定义语音转文字命令执行失败",
            )
        );
    }
    Ok(clean_transcript(&String::from_utf8_lossy(&output.stdout)))
}

fn transcript_base_path(wav_path: &Path) -> String {
    wav_path
        .to_str()
        .map(|s| s.trim_end_matches(".wav").to_string())
        .unwrap_or_else(|| "laozhou_voice".to_string())
}

fn transcript_path(wav_path: &Path) -> std::path::PathBuf {
    std::path::PathBuf::from(format!("{}.txt", transcript_base_path(wav_path)))
}

fn clean_transcript(raw: &str) -> String {
    raw.lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
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

fn shell_words(input: &str) -> Vec<String> {
    input
        .split_whitespace()
        .map(|word| word.trim_matches('"').trim_matches('\'').to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_transcript_lines() {
        assert_eq!(clean_transcript("  你好  \n  老周\n\n"), "你好 老周");
        assert_eq!(clean_transcript(""), "");
    }

    #[test]
    fn expands_file_placeholder() {
        let config = VoicePluginConfig {
            stt_command: "my-stt -i {file} -o out.txt".to_string(),
            ..VoicePluginConfig::default()
        };
        let parts = shell_words(&config.stt_command.replace("{file}", "/tmp/x.wav"));
        assert_eq!(parts, vec!["my-stt", "-i", "/tmp/x.wav", "-o", "out.txt"]);
    }
}
