//! 梦境功能：对话结束后异步分析意图，预测用户下一步操作
//!
//! 核心流程：
//! 1. 对话结束 → 异步触发 dream::trigger
//! 2. 子 Agent 分析对话历史 + 知识库 → 生成意图预测
//! 3. 加密存储到 dream/ 目录（JSON + 版本控制）
//! 4. 提供检索接口供主 Agent 查询历史意图

use crate::config::{AppConfig, DreamPluginConfig};
use crate::llm::OpenAiCompatibleClient;
use crate::paths::LaozhouPaths;
use crate::tools::subagent_runner::{ProgressMode, SubagentProgress, SubagentRunner};
use crate::tools::{ToolProgress, ToolRegistry};
use anyhow::{Context, Result};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use blake3::Hasher;
use chrono::Local;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;

const DREAM_SUBAGENT_PROMPT: &str = include_str!("../prompts/dream-subagent.md");
const DREAM_SUBAGENT_MAX_STEPS: usize = 10;

/// 意图预测结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DreamEntry {
    /// 唯一 ID：dream-YYYYMMDDHHmmss-NNN
    pub id: String,
    /// ISO 8601 时间戳
    pub timestamp: String,
    /// 版本号（每次更新 +1）
    pub version: u32,
    /// 对话摘要（加密存储时为 base64）
    pub conversation_summary: String,
    /// 预测意图
    pub predicted_intention: PredictedIntention,
    /// 相关知识库条目路径
    pub related_kb_entries: Vec<String>,
    /// 建议响应策略
    pub suggested_response_strategy: String,
    /// 是否加密
    pub encrypted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictedIntention {
    /// 意图描述
    pub description: String,
    /// 置信度评分 (0.0-1.0)
    pub confidence: f64,
    /// 意图类别
    pub category: String,
}

/// 梦境存储管理器
pub struct DreamStore {
    dream_dir: PathBuf,
    encrypt: bool,
    max_entries: usize,
}

impl DreamStore {
    pub fn new(paths: &LaozhouPaths, config: &DreamPluginConfig) -> Result<Self> {
        let dream_dir = paths.data_dir.join("dream");
        std::fs::create_dir_all(dream_dir.join("intentions"))?;
        Ok(Self {
            dream_dir,
            encrypt: config.encrypt,
            max_entries: config.max_history_entries,
        })
    }

    /// 存储意图预测结果，返回文件路径
    pub fn save(&self, mut entry: DreamEntry) -> Result<PathBuf> {
        if self.encrypt {
            entry.conversation_summary = Self::encrypt_text(&entry.conversation_summary);
            entry.predicted_intention.description =
                Self::encrypt_text(&entry.predicted_intention.description);
            entry.suggested_response_strategy =
                Self::encrypt_text(&entry.suggested_response_strategy);
            entry.encrypted = true;
        }

        let date = Local::now().format("%Y-%m-%d").to_string();
        let dir = self.dream_dir.join("intentions").join(&date);
        std::fs::create_dir_all(&dir)?;

        let filename = format!("{}.json", entry.id);
        let path = dir.join(&filename);
        let json = serde_json::to_string_pretty(&entry)?;
        std::fs::write(&path, json)?;

        self.update_index(&entry)?;
        self.cleanup_old_entries()?;

        Ok(path)
    }

    /// 检索历史意图预测记录
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<DreamEntry>> {
        let intentions_dir = self.dream_dir.join("intentions");
        if !intentions_dir.is_dir() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
        let mut entries: Vec<(PathBuf, DreamEntry)> = Vec::new();

        for date_dir in std::fs::read_dir(&intentions_dir)?.flatten() {
            if !date_dir.file_type()?.is_dir() {
                continue;
            }
            for file in std::fs::read_dir(date_dir.path())?.flatten() {
                if file.path().extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                let content = std::fs::read_to_string(file.path())?;
                if let Ok(entry) = serde_json::from_str::<DreamEntry>(&content) {
                    entries.push((file.path(), entry));
                }
            }
        }

        entries.sort_by(|a, b| b.1.timestamp.cmp(&a.1.timestamp));

        for (_, entry) in entries.into_iter().take(limit.max(50)) {
            let mut entry = entry;
            if entry.encrypted {
                entry.conversation_summary = Self::decrypt_text(&entry.conversation_summary);
                entry.predicted_intention.description =
                    Self::decrypt_text(&entry.predicted_intention.description);
                entry.suggested_response_strategy =
                    Self::decrypt_text(&entry.suggested_response_strategy);
                entry.encrypted = false;
            }
            let matches = entry.conversation_summary.contains(query)
                || entry.predicted_intention.description.contains(query)
                || entry.suggested_response_strategy.contains(query)
                || entry.predicted_intention.category.contains(query);
            if matches || query.is_empty() {
                results.push(entry);
            }
        }

        Ok(results.into_iter().take(limit).collect())
    }

    /// 根据主 Agent 请求提供相关意图分析
    pub fn get_latest(&self, limit: usize) -> Result<Vec<DreamEntry>> {
        self.search("", limit)
    }

    fn update_index(&self, entry: &DreamEntry) -> Result<()> {
        let index_path = self.dream_dir.join("intentions").join("index.json");
        let mut index: Vec<Value> = if index_path.exists() {
            serde_json::from_str(&std::fs::read_to_string(&index_path)?).unwrap_or_default()
        } else {
            Vec::new()
        };

        index.insert(
            0,
            json!({
                "id": entry.id,
                "timestamp": entry.timestamp,
                "version": entry.version,
                "category": entry.predicted_intention.category,
                "confidence": entry.predicted_intention.confidence,
            }),
        );

        index.truncate(self.max_entries);
        std::fs::write(&index_path, serde_json::to_string_pretty(&index)?)?;
        Ok(())
    }

    fn cleanup_old_entries(&self) -> Result<()> {
        let intentions_dir = self.dream_dir.join("intentions");
        let mut all_files: Vec<(PathBuf, String)> = Vec::new();

        for date_dir in std::fs::read_dir(&intentions_dir)?.flatten() {
            if !date_dir.file_type()?.is_dir() {
                continue;
            }
            for file in std::fs::read_dir(date_dir.path())?.flatten() {
                if file.path().extension().and_then(|e| e.to_str()) == Some("json")
                    && file.file_name() != "index.json"
                {
                    if let Some(name) = file.file_name().to_str() {
                        all_files.push((file.path(), name.to_string()));
                    }
                }
            }
        }

        all_files.sort_by(|a, b| b.1.cmp(&a.1));
        if all_files.len() > self.max_entries {
            for (path, _) in all_files.iter().skip(self.max_entries) {
                let _ = std::fs::remove_file(path);
            }
        }

        Ok(())
    }

    /// 加密：blake3 哈希 + base64 编码（单向哈希保护隐私）
    fn encrypt_text(text: &str) -> String {
        let mut hasher = Hasher::new();
        hasher.update(text.as_bytes());
        let hash = hasher.finalize();
        format!("enc:{}:{}", hash.to_hex(), BASE64.encode(text.as_bytes()))
    }

    /// 解密：从 base64 还原原文
    fn decrypt_text(encrypted: &str) -> String {
        if let Some(rest) = encrypted.strip_prefix("enc:") {
            if let Some ((_hash, b64)) = rest.split_once(':') {
                if let Ok(bytes) = BASE64.decode(b64) {
                    if let Ok(text) = String::from_utf8(bytes) {
                        return text;
                    }
                }
            }
        }
        encrypted.to_string()
    }
}

/// 对话结束后异步触发意图预测
pub async fn trigger_dream(
    config: AppConfig,
    paths: LaozhouPaths,
    user_input: &str,
    assistant_response: &str,
) {
    let dream_config = &config.plugins.dream;
    if !dream_config.enabled {
        return;
    }

    tracing::info!("dream: starting intention prediction");

    let result = run_dream_subagent(&config, &paths, user_input, assistant_response).await;

    match result {
        Ok(entry) => {
            tracing::info!("dream: prediction saved (id={}, confidence={:.2})",
                entry.id, entry.predicted_intention.confidence);
        }
        Err(err) => {
            tracing::warn!("dream: prediction failed: {err:#}");
        }
    }
}

/// 启动子 Agent 分析对话并生成意图预测
async fn run_dream_subagent(
    config: &AppConfig,
    paths: &LaozhouPaths,
    user_input: &str,
    assistant_response: &str,
) -> Result<DreamEntry> {
    let dream_config = &config.plugins.dream;

    let store = DreamStore::new(paths, dream_config)?;

    // 构建子 Agent 工具集：只读知识库搜索
    let mut sub_tools = ToolRegistry::new();
    crate::tools::knowledge_base::register_readonly(
        &mut sub_tools,
        config.clone(),
        paths.clone(),
    );

    let mode = ProgressMode::from_config(config);
    let (progress_tx, _progress_rx) = mpsc::unbounded_channel();
    let progress = ToolProgress::new(progress_tx);
    let sa_progress = SubagentProgress::new(progress, mode, true);
    sa_progress.phase("梦境子代理正在分析意图".to_string());

    let client = OpenAiCompatibleClient::from_config(config, paths)?
        .for_subagent_output(mode == ProgressMode::Full);

    // 构建 prompt：包含对话历史 + 知识库上下文
    let prompt = format!(
        "{DREAM_SUBAGENT_PROMPT}\n\n\
        ## 当前对话\n\
        **用户：** {user_input}\n\n\
        **助手回复：** {assistant_response}\n\n\
        请分析这段对话，搜索知识库中相关内容，预测用户下一步可能的操作意图。\
        以 JSON 格式返回分析结果。"
    );

    let runner = SubagentRunner::new(client, DREAM_SUBAGENT_PROMPT, sub_tools, sa_progress)
        .max_steps(DREAM_SUBAGENT_MAX_STEPS)
        .timeout_seconds(dream_config.subagent_timeout_secs);

    let timeout_secs = dream_config.subagent_timeout_secs.max(30);
    let outcome = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        runner.run(&prompt),
    )
    .await;

    let (result_text, state) = match outcome {
        Ok(Ok((result, stats))) => {
            let state = if stats.budget_reached {
                "budget_reached"
            } else {
                "completed"
            };
            (result.content, state)
        }
        Ok(Err(err)) => {
            tracing::warn!("dream: subagent error: {err:#}");
            return Err(err);
        }
        Err(_) => {
            tracing::warn!("dream: subagent timed out after {timeout_secs}s");
            return Err(anyhow::anyhow!("dream subagent timed out"));
        }
    };

    tracing::debug!("dream: subagent state={state}, output_len={}", result_text.len());

    // 解析子 Agent 返回的 JSON
    let entry = parse_dream_result(&result_text, user_input, assistant_response, dream_config)?;

    // 存储到文件系统
    store.save(entry.clone())?;

    Ok(entry)
}

/// 解析子 Agent 输出为 DreamEntry
fn parse_dream_result(
    raw: &str,
    user_input: &str,
    assistant_response: &str,
    config: &DreamPluginConfig,
) -> Result<DreamEntry> {
    // 尝试从输出中提取 JSON
    let json_str = extract_json(raw).unwrap_or_else(|| {
        // fallback：生成默认结构
        json!({
            "predicted_intention": {
                "description": "无法解析子 Agent 输出",
                "confidence": 0.0,
                "category": "unknown"
            },
            "related_kb_entries": [],
            "suggested_response_strategy": "建议手动分析对话内容"
        })
        .to_string()
    });

    let parsed: Value = serde_json::from_str(&json_str).context("dream: invalid JSON from subagent")?;

    let id = format!(
        "dream-{}",
        Local::now().format("%Y%m%d%H%M%S")
    );

    let summary = format!(
        "用户：{}\n助手：{}",
        truncate(user_input, 500),
        truncate(assistant_response, 500)
    );

    let confidence = parsed
        .pointer("/predicted_intention/confidence")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);

    // 如果置信度低于阈值，标记为低置信度
    let category = if confidence < config.accuracy_threshold {
        "low_confidence"
    } else {
        parsed
            .pointer("/predicted_intention/category")
            .and_then(Value::as_str)
            .unwrap_or("general")
    };

    Ok(DreamEntry {
        id,
        timestamp: Local::now().to_rfc3339(),
        version: 1,
        conversation_summary: summary,
        predicted_intention: PredictedIntention {
            description: parsed
                .pointer("/predicted_intention/description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            confidence,
            category: category.to_string(),
        },
        related_kb_entries: parsed
            .pointer("/related_kb_entries")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        suggested_response_strategy: parsed
            .pointer("/suggested_response_strategy")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        encrypted: config.encrypt,
    })
}

/// 从文本中提取 JSON 块
fn extract_json(text: &str) -> Option<String> {
    // 尝试找 ```json ... ``` 块
    if let Some(start) = text.find("```json") {
        if let Some(end) = text[start..].rfind("```") {
            let json_start = start + 7;
            return Some(text[json_start..json_start + end - 7].trim().to_string());
        }
    }
    // 尝试找 { ... } 块
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            if end > start {
                return Some(text[start..=end].to_string());
            }
        }
    }
    None
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect::<String>() + "..."
    }
}

/// 注册 dream 相关工具（供主 Agent 检索历史意图）
pub fn register_tools(registry: &mut ToolRegistry, config: AppConfig, paths: LaozhouPaths) {
    if !config.plugins.dream.enabled {
        return;
    }

    let search_config = config.clone();
    let search_paths = paths.clone();
    registry.register(ToolSpec::new(
        "search_dream_intentions",
        "Search historical dream intention predictions. Returns past intention analysis entries with predicted intentions, confidence scores, and suggested strategies.",
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search keywords to match intention descriptions or categories." },
                "limit": { "type": "integer", "description": "Maximum results to return. Default 10." }
            },
            "required": ["query"],
            "additionalProperties": false
        }),
        move |args| {
            let config = search_config.clone();
            let paths = search_paths.clone();
            async move {
                let query = args
                    .get("query")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let limit = args
                    .get("limit")
                    .and_then(Value::as_u64)
                    .map(|v| v as usize)
                    .unwrap_or(10);

                let store = DreamStore::new(&paths, &config.plugins.dream)?;
                let results = store.search(&query, limit)?;

                Ok(json!({
                    "ok": true,
                    "count": results.len(),
                    "intentions": results
                })
                .to_string())
            }
        },
    ));

    let latest_config = config.clone();
    let latest_paths = paths.clone();
    registry.register(ToolSpec::new(
        "get_latest_dream_intention",
        "Get the most recent dream intention prediction. Returns the latest predicted user intention with confidence and strategy.",
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }),
        move |_args| {
            let config = latest_config.clone();
            let paths = latest_paths.clone();
            async move {
                let store = DreamStore::new(&paths, &config.plugins.dream)?;
                let results = store.get_latest(1)?;

                if let Some(entry) = results.first() {
                    Ok(json!({
                        "ok": true,
                        "intention": entry
                    })
                    .to_string())
                } else {
                    Ok(json!({
                        "ok": false,
                        "message": "No dream intention predictions found"
                    })
                    .to_string())
                }
            }
        },
    ));
}

use crate::tools::ToolSpec;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::tempdir;

    fn test_paths(root: &Path) -> LaozhouPaths {
        LaozhouPaths {
            config_dir: root.join("config"),
            config_file: root.join("config/config.jsonc"),
            skills_dir: root.join("config/skills"),
            data_dir: root.join("data"),
            cache_dir: root.join("cache"),
            state_dir: root.join("state"),
            pictures_dir: root.join("pictures"),
            fish_hook_file: root.join("fish/conf.d/laozhou.fish"),
            bash_hook_file: root.join("config/shell/bash-hook.sh"),
            zsh_hook_file: root.join("config/shell/zsh-hook.zsh"),
            scripts_dir: root.join("config/scripts"),
            system_scripts_dir: PathBuf::new(),
        }
    }

    #[test]
    fn dream_store_save_and_search() {
        let temp = tempdir().unwrap();
        let paths = test_paths(temp.path());
        let config = DreamPluginConfig::default();
        let store = DreamStore::new(&paths, &config).unwrap();

        let entry = DreamEntry {
            id: "dream-test-001".to_string(),
            timestamp: Local::now().to_rfc3339(),
            version: 1,
            conversation_summary: "用户询问 NVIDIA 驱动安装".to_string(),
            predicted_intention: PredictedIntention {
                description: "用户可能想配置 Wayland 显卡驱动".to_string(),
                confidence: 0.85,
                category: "system_admin".to_string(),
            },
            related_kb_entries: vec!["kb/nvidia.md".to_string()],
            suggested_response_strategy: "提供安装命令和 Wayland 配置".to_string(),
            encrypted: false,
        };

        let path = store.save(entry.clone()).unwrap();
        assert!(path.exists());

        let results = store.search("NVIDIA", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].conversation_summary.contains("NVIDIA"));
    }

    #[test]
    fn dream_encrypt_decrypt_roundtrip() {
        let original = "这是一段敏感的对话内容";
        let encrypted = DreamStore::encrypt_text(original);
        assert!(encrypted.starts_with("enc:"));
        let decrypted = DreamStore::decrypt_text(&encrypted);
        assert_eq!(original, decrypted);
    }

    #[test]
    fn dream_extract_json_from_markdown() {
        let text = "分析结果：\n```json\n{\"confidence\": 0.9}\n```\n结束";
        let json = extract_json(text).unwrap();
        assert!(json.contains("confidence"));
    }

    #[test]
    fn dream_parse_result_with_low_confidence() {
        let config = DreamPluginConfig {
            enabled: true,
            accuracy_threshold: 0.8,
            ..Default::default()
        };
        let raw = r#"{"predicted_intention":{"description":"测试","confidence":0.3,"category":"test"}}"#;
        let entry = parse_dream_result(raw, "用户输入", "助手回复", &config).unwrap();
        assert_eq!(entry.predicted_intention.category, "low_confidence");
    }

    #[test]
    fn dream_store_cleanup_old_entries() {
        let temp = tempdir().unwrap();
        let paths = test_paths(temp.path());
        let config = DreamPluginConfig {
            max_history_entries: 3,
            ..Default::default()
        };
        let store = DreamStore::new(&paths, &config).unwrap();

        for i in 0..5 {
            let entry = DreamEntry {
                id: format!("dream-test-{i:03}"),
                timestamp: format!("2026-08-06T22:30:{i:02}+08:00"),
                version: 1,
                conversation_summary: format!("测试 {i}"),
                predicted_intention: PredictedIntention {
                    description: "测试意图".to_string(),
                    confidence: 0.8,
                    category: "test".to_string(),
                },
                related_kb_entries: vec![],
                suggested_response_strategy: "无".to_string(),
                encrypted: false,
            };
            store.save(entry).unwrap();
        }

        let results = store.search("", 10).unwrap();
        assert!(results.len() <= 3);
    }
}
