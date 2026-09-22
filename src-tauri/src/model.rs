// PingBoard 数据结构定义（serde 序列化，字段名与前端 TS 类型逐一对齐）
use serde::{Deserialize, Serialize};

use crate::events::{
    default_event_level, default_log_keep, deserialize_event_level, EventLevel,
};

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
    /// 是否记录该主机的事件（运行时单主机开关，默认开启）
    pub events_on: bool,
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
            events_on: true,
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
    /// 是否启用 max_threads 并发上限（关闭后不再按该值拦截，仅保留 4096 硬保护）
    ///
    /// ⚠️ 字段级 `#[serde(default = "default_true")]` 为硬性要求：
    /// v1.0.0 的旧配置文件不含该字段，缺省时必须为 `true`，否则整份配置会回落默认、
    /// 丢失用户已保存的主机列表。
    #[serde(default = "default_true")]
    pub limit_max_threads: bool,
    pub beep_on_fail: bool,
    pub auto_start: bool,
    pub history_len: usize,
    /// 事件日志总开关（关闭后不再生成任何事件，省内存/文件）
    ///
    /// ⚠️ 字段级默认值为硬性要求：旧配置缺该字段时必须为 `true`，
    /// 否则整份配置回落默认、丢失用户主机列表。
    #[serde(default = "default_true")]
    pub events_on: bool,
    /// 事件展示等级（仅故障 / 标准 / 详细）
    ///
    /// ⚠️ 必须走自定义 `deserialize_with`：未知值降级为 `Standard`，
    /// 否则未知枚举值会让整份配置反序列化失败（设计稿风险 #8）。
    #[serde(
        default = "default_event_level",
        deserialize_with = "deserialize_event_level"
    )]
    pub events_level: EventLevel,
    /// 是否把事件持久化到文件（默认开启）
    #[serde(default = "default_true")]
    pub events_persist: bool,
    /// 事件保存目录（None 表示使用默认的 app_config_dir）
    #[serde(default)]
    pub events_dir: Option<String>,
    /// 每主机保留条数（50 / 200 / 1000）
    #[serde(default = "default_log_keep")]
    pub events_keep: usize,
}

impl Default for PingSettings {
    fn default() -> Self {
        Self {
            interval_ms: 1000,
            timeout_ms: 2000,
            payload_size: 32,
            ttl: 128,
            max_threads: 256,
            limit_max_threads: true,
            beep_on_fail: false,
            auto_start: false,
            history_len: 60,
            events_on: true,
            events_level: default_event_level(),
            events_persist: true,
            events_dir: None,
            events_keep: default_log_keep(),
        }
    }
}

/// `limit_max_threads` 的缺省值（true）：保证旧配置加载后仍默认启用并发上限
fn default_true() -> bool {
    true
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
    /// 本次运行的会话标识（= 进程启动时刻，纪元毫秒）。
    /// 前端用它判断日志行是否属于「本次运行」，从而给旧会话加上日期前缀。
    pub session: u64,
    /// 自上次快照以来新增的事件（增量；始终序列化，空闲为 `[]`）
    pub events: Vec<crate::events::LogEvent>,
}

/// 持久化的目标配置（不含运行时统计与 id）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetConfig {
    pub name: String,
    pub host: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// 是否记录该主机的事件（字段级默认 true，保证旧配置兼容）
    #[serde(default = "default_true")]
    pub events_on: bool,
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
            limit_max_threads: false,
            beep_on_fail: true,
            auto_start: true,
            history_len: 120,
            events_on: false,
            events_level: EventLevel::Detail,
            events_persist: false,
            events_dir: Some("D:\\pb-events".to_string()),
            events_keep: 1000,
        };
        let js = serde_json::to_string(&s).unwrap();
        let back: PingSettings = serde_json::from_str(&js).unwrap();
        assert_eq!(back.interval_ms, s.interval_ms);
        assert_eq!(back.timeout_ms, s.timeout_ms);
        assert_eq!(back.payload_size, s.payload_size);
        assert_eq!(back.ttl, s.ttl);
        assert_eq!(back.max_threads, s.max_threads);
        assert_eq!(back.limit_max_threads, s.limit_max_threads);
        assert_eq!(back.beep_on_fail, s.beep_on_fail);
        assert_eq!(back.auto_start, s.auto_start);
        assert_eq!(back.history_len, s.history_len);
        assert_eq!(back.events_on, s.events_on);
        assert_eq!(back.events_level, s.events_level);
        assert_eq!(back.events_persist, s.events_persist);
        assert_eq!(back.events_dir, s.events_dir);
        assert_eq!(back.events_keep, s.events_keep);
    }

    /// 旧配置兼容（v1.1 新增字段）：不含 limit_max_threads 的 JSON 必须能加载且缺省为 true
    #[test]
    fn qa_old_config_without_limit_field_defaults_true() {
        // 完整的 v1.0.0 设置 JSON（无 limit_max_threads）
        let old = r#"{"interval_ms":800,"timeout_ms":1500,"payload_size":32,"ttl":128,"max_threads":256,"beep_on_fail":false,"auto_start":false,"history_len":60}"#;
        let s: PingSettings = serde_json::from_str(old).unwrap();
        assert_eq!(s.interval_ms, 800, "旧字段必须被保留");
        assert_eq!(s.max_threads, 256);
        assert!(s.limit_max_threads, "旧配置缺省必须为 true（默认启用上限）");

        // 整份 AppConfig（含旧 targets）也必须完好加载，不丢用户数据
        let old_cfg = r#"{"version":1,"settings":{"interval_ms":1000,"timeout_ms":2000,"payload_size":32,"ttl":128,"max_threads":256,"beep_on_fail":false,"auto_start":true,"history_len":60},"targets":[{"name":"阿里 DNS","host":"223.5.5.5","enabled":true}]}"#;
        let cfg: AppConfig = serde_json::from_str(old_cfg).unwrap();
        assert!(cfg.settings.limit_max_threads, "缺省应为 true");
        assert_eq!(cfg.targets.len(), 1, "旧配置的主机列表必须保留");
        assert_eq!(cfg.targets[0].host, "223.5.5.5");
        assert!(cfg.settings.auto_start);
    }

    /// 新增字段可被显式覆盖为 false 并正确往返
    #[test]
    fn qa_limit_field_can_be_set_false() {
        let s: PingSettings =
            serde_json::from_str(r#"{"limit_max_threads":false}"#).unwrap();
        assert!(!s.limit_max_threads);
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
                TargetConfig { name: "阿里 DNS".into(), host: "223.5.5.5".into(), enabled: true, events_on: true },
                TargetConfig { name: "百度".into(), host: "www.baidu.com".into(), enabled: false, events_on: true },
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
        assert!(back.targets[0].events_on, "events_on 缺省应为 true");
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
        assert!(tc.events_on, "events_on 缺省应为 true");
    }

    /* ===================== 事件日志：旧配置兼容（不丢 targets） ===================== */

    /// v1.0.0 旧配置（无 limit_max_threads、无 events_*）：必须完好加载，
    /// 主机列表逐字保留，新增字段取正确默认。
    #[test]
    fn qa_v100_config_keeps_targets_and_defaults_events() {
        // 完整的 v1.0.0 配置：settings 无 limit_max_threads / events_*，target 无 events_on
        let old = r#"{"version":1,"settings":{"interval_ms":800,"timeout_ms":1500,"payload_size":32,"ttl":128,"max_threads":256,"beep_on_fail":false,"auto_start":false,"history_len":60},"targets":[{"name":"阿里 DNS","host":"223.5.5.5","enabled":true},{"name":"百度","host":"www.baidu.com","enabled":false}]}"#;
        let cfg: AppConfig = serde_json::from_str(old).unwrap();
        assert_eq!(cfg.targets.len(), 2, "旧配置的 2 个主机必须保留");
        assert_eq!(cfg.targets[0].host, "223.5.5.5");
        assert_eq!(cfg.targets[1].name, "百度");
        assert!(!cfg.targets[1].enabled);
        assert_eq!(cfg.settings.interval_ms, 800, "既有字段必须保留");
        // 事件相关字段缺省
        assert!(cfg.settings.limit_max_threads, "缺省应为 true");
        assert!(cfg.settings.events_on, "events_on 缺省应为 true");
        assert_eq!(cfg.settings.events_level, EventLevel::Standard);
        assert!(cfg.settings.events_persist, "events_persist 缺省应为 true");
        assert!(cfg.settings.events_dir.is_none(), "events_dir 缺省应为 None");
        assert_eq!(cfg.settings.events_keep, 200, "events_keep 缺省应为 200");
        for t in &cfg.targets {
            assert!(t.events_on, "target.events_on 缺省应为 true");
        }
    }

    /// v1.1.2 旧配置（含 limit_max_threads，无 events_*）：同上且保留 limit_max_threads
    #[test]
    fn qa_v112_config_keeps_targets_and_limit_field() {
        let old = r#"{"version":1,"settings":{"interval_ms":1000,"timeout_ms":2000,"payload_size":32,"ttl":128,"max_threads":100,"limit_max_threads":false,"beep_on_fail":true,"auto_start":true,"history_len":120},"targets":[{"name":"a","host":"1.1.1.1","enabled":true}]}"#;
        let cfg: AppConfig = serde_json::from_str(old).unwrap();
        assert_eq!(cfg.targets.len(), 1);
        assert_eq!(cfg.targets[0].host, "1.1.1.1");
        assert!(!cfg.settings.limit_max_threads, "limit_max_threads 原值必须保留");
        assert_eq!(cfg.settings.max_threads, 100);
        assert!(cfg.settings.beep_on_fail);
        assert_eq!(cfg.settings.events_keep, 200);
        assert_eq!(cfg.settings.events_level, EventLevel::Standard);
    }

    /// 高危防护（风险 #8）：events_level 为未知串时，整份配置**不得**反序列化失败，
    /// 主机列表必须保留，等级降级为 Standard。
    #[test]
    fn qa_unknown_events_level_does_not_drop_targets() {
        let raw = r#"{"version":1,"settings":{"events_level":"verbose","events_on":true,"events_keep":50},"targets":[{"name":"a","host":"9.9.9.9","enabled":true}]}"#;
        let cfg: AppConfig = serde_json::from_str(raw).unwrap();
        assert_eq!(cfg.settings.events_level, EventLevel::Standard, "未知等级应降级为标准");
        assert_eq!(cfg.targets.len(), 1, "未知等级不得导致主机列表丢失");
        assert_eq!(cfg.targets[0].host, "9.9.9.9");

        // 数字等非字符串值也不能让整份配置失败
        let raw_num = r#"{"version":1,"settings":{"events_level":123},"targets":[{"name":"b","host":"8.8.8.8"}]}"#;
        let cfg2: AppConfig = serde_json::from_str(raw_num).unwrap();
        assert_eq!(cfg2.settings.events_level, EventLevel::Standard);
        assert_eq!(cfg2.targets.len(), 1);
    }

    /// 非法 events_keep 由 normalize_settings 归一（在 stats.rs 覆盖），此处仅锁定合法值往返
    #[test]
    fn qa_events_keep_roundtrip_values() {
        for k in [50usize, 200, 1000] {
            let s: PingSettings = serde_json::from_str(&format!("{{\"events_keep\":{}}}", k)).unwrap();
            assert_eq!(s.events_keep, k);
        }
    }
}
