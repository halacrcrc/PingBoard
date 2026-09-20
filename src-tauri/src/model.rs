// PingBoard 数据结构定义（serde 序列化，字段名与前端 TS 类型逐一对齐）
use serde::{Deserialize, Serialize};

/// 探测状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// 未开始
    Idle,
    /// 解析中
    Resolving,
    /// 最近一次探测成功
    Ok,
    /// 探测超时无响应
    Timeout,
    /// 主机名解析失败或系统错误
    Failed,
}

impl Default for Status {
    fn default() -> Self {
        Status::Idle
    }
}

/// 单个目标的运行时状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetState {
    pub id: u64,
    pub name: String,
    pub host: String,
    pub enabled: bool,
    pub resolved_ip: Option<String>,
    pub status: Status,
    pub sent: u64,
    pub received: u64,
    pub failed: u64,
    pub last_rtt_ms: Option<f64>,
    pub min_rtt_ms: Option<f64>,
    pub max_rtt_ms: Option<f64>,
    pub avg_rtt_ms: Option<f64>,
    pub sum_rtt_ms: f64,
    pub ttl: Option<u32>,
    pub consecutive_fail: u32,
    pub loss_pct: f64,
    pub last_success_ts: Option<u64>,
    /// 最近 N 次探测的 rtt，失败/超时记 None
    pub history: Vec<Option<f64>>,
    pub running: bool,
    pub last_error: Option<String>,
}

impl TargetState {
    /// 创建一个新的目标状态（默认启用，状态为未开始）
    pub fn new(id: u64, name: String, host: String) -> Self {
        Self {
            id,
            name,
            host,
            enabled: true,
            resolved_ip: None,
            status: Status::Idle,
            sent: 0,
            received: 0,
            failed: 0,
            last_rtt_ms: None,
            min_rtt_ms: None,
            max_rtt_ms: None,
            avg_rtt_ms: None,
            sum_rtt_ms: 0.0,
            ttl: None,
            consecutive_fail: 0,
            loss_pct: 0.0,
            last_success_ts: None,
            history: Vec::new(),
            running: false,
            last_error: None,
        }
    }
}

/// 前端提交的新目标条目
#[derive(Debug, Clone, Deserialize)]
pub struct TargetEntry {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub host: String,
}

/// Ping 设置项
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PingSettings {
    pub interval_ms: u64,
    pub timeout_ms: u64,
    pub payload_size: u16,
    pub ttl: u8,
    pub max_threads: usize,
    pub beep_on_fail: bool,
    pub auto_start: bool,
    pub history_len: usize,
}

impl Default for PingSettings {
    fn default() -> Self {
        Self {
            interval_ms: 1000,
            timeout_ms: 2000,
            payload_size: 32,
            ttl: 128,
            max_threads: 256,
            beep_on_fail: false,
            auto_start: false,
            history_len: 60,
        }
    }
}

/// 聚合快照：既作为 ping-snapshot 事件的 payload，也作为 get_state 的返回值
#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub targets: Vec<TargetState>,
    pub settings: PingSettings,
    pub running: bool,
    pub active_threads: usize,
    pub total_sent: u64,
    pub total_received: u64,
    pub total_failed: u64,
    pub loss_pct: f64,
    pub started_at: Option<u64>,
    pub updated_at: u64,
}

/// 持久化的目标配置（不含运行时统计与 id）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetConfig {
    pub name: String,
    pub host: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

/// 持久化的应用配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub version: u32,
    pub settings: PingSettings,
    #[serde(default)]
    pub targets: Vec<TargetConfig>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: 1,
            settings: PingSettings::default(),
            targets: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Status 序列化必须为 lowercase（前端 TS 联合类型依赖此口径）
    #[test]
    fn qa_status_serde_is_lowercase() {
        assert_eq!(serde_json::to_string(&Status::Idle).unwrap(), "\"idle\"");
        assert_eq!(serde_json::to_string(&Status::Resolving).unwrap(), "\"resolving\"");
        assert_eq!(serde_json::to_string(&Status::Ok).unwrap(), "\"ok\"");
        assert_eq!(serde_json::to_string(&Status::Timeout).unwrap(), "\"timeout\"");
        assert_eq!(serde_json::to_string(&Status::Failed).unwrap(), "\"failed\"");
        assert_eq!(serde_json::from_str::<Status>("\"timeout\"").unwrap(), Status::Timeout);
        assert!(serde_json::from_str::<Status>("\"OK\"").is_err(), "大写不应被接受");
    }

    /// PingSettings 序列化 → 反序列化 完全一致
    #[test]
    fn qa_settings_roundtrip_exact() {
        let s = PingSettings {
            interval_ms: 500,
            timeout_ms: 1500,
            payload_size: 56,
            ttl: 64,
            max_threads: 32,
            beep_on_fail: true,
            auto_start: true,
            history_len: 120,
        };
        let js = serde_json::to_string(&s).unwrap();
        let back: PingSettings = serde_json::from_str(&js).unwrap();
        assert_eq!(back.interval_ms, s.interval_ms);
        assert_eq!(back.timeout_ms, s.timeout_ms);
        assert_eq!(back.payload_size, s.payload_size);
        assert_eq!(back.ttl, s.ttl);
        assert_eq!(back.max_threads, s.max_threads);
        assert_eq!(back.beep_on_fail, s.beep_on_fail);
        assert_eq!(back.auto_start, s.auto_start);
        assert_eq!(back.history_len, s.history_len);
    }

    /// AppConfig（设置 + 目标列表）序列化 → 反序列化 完全一致
    #[test]
    fn qa_app_config_roundtrip_with_targets() {
        let cfg = AppConfig {
            version: 1,
            settings: PingSettings {
                interval_ms: 1000,
                timeout_ms: 2000,
                auto_start: true,
                ..PingSettings::default()
            },
            targets: vec![
                TargetConfig { name: "阿里 DNS".into(), host: "223.5.5.5".into(), enabled: true },
                TargetConfig { name: "百度".into(), host: "www.baidu.com".into(), enabled: false },
            ],
        };
        let js = serde_json::to_string_pretty(&cfg).unwrap();
        let back: AppConfig = serde_json::from_str(&js).unwrap();
        assert_eq!(back.version, 1);
        assert!(back.settings.auto_start);
        assert_eq!(back.targets.len(), 2);
        assert_eq!(back.targets[0].host, "223.5.5.5");
        assert_eq!(back.targets[0].name, "阿里 DNS");
        assert!(!back.targets[1].enabled, "enabled=false 必须被保留");
    }

    /// JSON 损坏 → 解析失败（config::load 据此回退默认值，不 panic）
    #[test]
    fn qa_corrupt_json_is_err() {
        assert!(serde_json::from_str::<AppConfig>("{ 这不是合法 JSON ").is_err());
        assert!(serde_json::from_str::<AppConfig>("").is_err());
    }

    /// 缺失必填字段 → 解析失败 → 整体回落默认（这是 config::load 的既定策略）
    #[test]
    fn qa_missing_required_field_is_err() {
        // 缺 settings（无 serde(default)）
        assert!(serde_json::from_str::<AppConfig>("{\"version\":1,\"targets\":[]}").is_err());
    }

    /// 缺失可选字段应使用默认：targets 缺省为空；TargetConfig.enabled 缺省为 true
    #[test]
    fn qa_missing_optional_fields_use_defaults() {
        let cfg: AppConfig = serde_json::from_str("{\"version\":1,\"settings\":{}}").unwrap();
        assert_eq!(cfg.targets.len(), 0);
        assert_eq!(cfg.settings.interval_ms, 1000, "PingSettings 缺省应回退默认");
        assert_eq!(cfg.settings.history_len, 60);

        let tc: TargetConfig = serde_json::from_str("{\"name\":\"x\",\"host\":\"1.1.1.1\"}").unwrap();
        assert!(tc.enabled, "enabled 缺省应为 true");
    }
}
