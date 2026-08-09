use crate::config::VoicePluginConfig;
use crate::voice::record;
use anyhow::Result;

/// Listen for the configured wake word indefinitely until it is detected.
/// Uses continuous VAD-triggered recording: the microphone is always listened
/// to, and each complete utterance is transcribed and checked for the wake
/// word, so nothing is missed between fixed windows.
/// Ctrl+C (SIGINT) cancels from the caller side.
pub fn listen_for_wake_word(config: &VoicePluginConfig) -> Result<()> {
    if !config.wake_enabled {
        return Ok(());
    }
    let wake_word = config.wake_word.trim();
    if wake_word.is_empty() {
        return Ok(());
    }

    eprintln!(
        "{}",
        crate::i18n::text_owned(
            format!("Listening for wake word: \"{wake_word}\" ..."),
            format!("正在监听唤醒词: \"{wake_word}\" ..."),
        )
    );

    loop {
        let wav = match record::listen_for_speech(config) {
            Ok(wav) => wav,
            Err(err) => {
                eprintln!(
                    "{}",
                    crate::i18n::text_owned(
                        format!("wake-word detection skipped: {err}"),
                        format!("唤醒词检测跳过: {err}"),
                    )
                );
                continue;
            }
        };
        let text = match crate::voice::stt::transcribe(config, &wav) {
            Ok(text) => text,
            Err(err) => {
                eprintln!(
                    "{}",
                    crate::i18n::text_owned(
                        format!("wake-word transcription failed: {err}"),
                        format!("唤醒词转写失败: {err}"),
                    )
                );
                continue;
            }
        };
        eprintln!(
            "{}",
            crate::i18n::text_owned(
                format!("heard: \"{text}\""),
                format!("听到: \"{text}\""),
            )
        );
        if text_matches_wake(&text, wake_word) {
            return Ok(());
        }
    }
}

pub fn text_matches_wake(text: &str, wake_word: &str) -> bool {
    let text = normalize(text);
    let wake = normalize(wake_word);
    if wake.is_empty() {
        return false;
    }
    if text.contains(&wake) {
        return true;
    }
    // For CJK wake words, tolerate dropped/inserted punctuation or a slightly
    // different transcription by checking that the characters appear in order.
    // Require at least two characters so a single stray character never matches.
    if wake.chars().filter(|c| !c.is_ascii()).count() >= 2 {
        return contains_in_order(&text, &wake);
    }
    text.split_whitespace().any(|word| word == wake)
}

fn contains_in_order(haystack: &str, needle: &str) -> bool {
    let needle_chars: Vec<char> = needle.chars().collect();
    let mut needle_idx = 0usize;
    for c in haystack.chars() {
        if needle_idx < needle_chars.len() && c == needle_chars[needle_idx] {
            needle_idx += 1;
        }
    }
    needle_idx == needle_chars.len()
}

fn normalize(input: &str) -> String {
    input
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_ascii_punctuation())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_wake_word_in_text() {
        assert!(text_matches_wake("你好 laozhou", "laozhou"));
        assert!(text_matches_wake("LAOZHOU，查一下磁盘", "laozhou"));
        assert!(text_matches_wake("hello老周", "老周"));
        assert!(text_matches_wake("老周帮忙", "老周"));
        assert!(text_matches_wake("老周。查一下磁盘", "老周"));
        assert!(text_matches_wake("嗯老周", "老周"));
        assert!(!text_matches_wake("hello", "laozhou"));
        assert!(!text_matches_wake("", "laozhou"));
        assert!(!text_matches_wake("周末", "老周"));
        assert!(!text_matches_wake("老", "老周"));
    }
}

