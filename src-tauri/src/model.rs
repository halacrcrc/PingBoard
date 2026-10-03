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
    /// 所属文件夹 id；`None` = 临时区（未分类区域）。
    ///
    /// 必须带 `default`（红线 R1）：v1.1.9 写出的配置**没有**这个字段，
    /// 缺省必须是 `None`（= 全部落在临时区），否则旧配置整份反序列化失败、
    /// 回落默认值 → **丢用户全部主机列表**。这是零迁移的前提。
    ///
    /// 指向**不存在**的文件夹时**不得**跳过该主机，必须回落为 `None`：
    /// 见 `config::finalize` 的孤儿回落。红线 R1 的教训是「宁可降级一个字段」。
    #[serde(default, deserialize_with = "deserialize_folder_id")]
    pub folder_id: Option<u64>,
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
            folder_id: None,
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
    /// 目标文件夹 id（v1.1.10 新增）；`None` 或缺省 = 临时区。
    ///
    /// 用容错反序列化：非法值降级为 `None`，绝不让「文件夹字段写错」
    /// 导致整个添加请求失败（那会让用户以为添加失败，实际是字段问题）。
    #[serde(default, deserialize_with = "deserialize_folder_id")]
    pub folder_id: Option<u64>,
}

/// 文件夹 + 台数（`list_folders` 的返回项，供侧边栏直接渲染）
#[derive(Debug, Clone, Serialize)]
pub struct FolderEntry {
    pub folder: FolderConfig,
    pub count: usize,
}

/* ============ 配置字段类型容错（红线 R1 防护，详见 docs/code-review.md） ============ */

/// 生成数值型字段的容错反序列化器：**任意 JSON 值都绝不返回 Err**。
///
/// 背景（红线 R1 / 批次 2 A3）：某个设置字段被写成非法类型（字符串 / null / 布尔 /
/// 越界数字）时，serde 默认派生会让**整份** `AppConfig` 反序列化失败 → 回落
/// `AppConfig::default()` → 用户主机列表被静默清空，随后又被 `save` 覆盖，连手工
/// 恢复的机会都没有。此处遇到任何非法值都退化为该字段的缺省值，具体取值边界交给
/// `stats::normalize_settings` 兜底。
macro_rules! tolerant_number {
    ($fn_name:ident, $ty:ty, $fallback:expr) => {
        pub fn $fn_name<'de, D>(deserializer: D) -> Result<$ty, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            // 用 Value 作为中间载体：任意 JSON 值都能被接受，解析失败退化为 None。
            let value = Option::<serde_json::Value>::deserialize(deserializer).unwrap_or(None);
            Ok(match value.as_ref().and_then(|v| v.as_u64()) {
                // 超出目标类型上界的数字先钳制再转换，避免 `as` 静默回绕
                Some(n) => n.min(<$ty>::MAX as u64) as $ty,
                None => $fallback,
            })
        }
    };
}

/// 生成布尔型字段的容错反序列化器：**任意 JSON 值都绝不返回 Err**（同 `tolerant_number`）。
///
/// ⚠️ **字符串要按字面语义解析，不能一律回落 fallback**：手改配置最常见的错误就是
/// 给布尔值加上引号（`"enabled": "false"`）。若一律回落，`"false"` 会被读成 `true`——
/// 用户想关掉的主机反而被打开，这是比「整份解析失败」更隐蔽的错误。
/// 只有无法判读的字符串（`"abc"`）才回落到 fallback。
macro_rules! tolerant_bool {
    ($fn_name:ident, $fallback:expr) => {
        pub fn $fn_name<'de, D>(deserializer: D) -> Result<bool, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            let value = Option::<serde_json::Value>::deserialize(deserializer).unwrap_or(None);
            Ok(match value.as_ref() {
                Some(serde_json::Value::Bool(b)) => *b,
                // 加引号的布尔/开关写法：按字面语义解析（trim + 小写，容忍 " YES " 之类）
                Some(serde_json::Value::String(s)) => match s.trim().to_ascii_lowercase().as_str() {
                    "true" | "1" | "yes" | "on" => true,
                    "false" | "0" | "no" | "off" => false,
                    _ => $fallback,
                },
                _ => $fallback,
            })
        }
    };
}

/// 生成字符串型字段的容错反序列化器：**任意 JSON 值都绝不返回 Err**。
///
/// 与 `tolerant_opt_string` 的区别：这里返回 `String` 而不是 `Option<String>`，
/// 用于 `TargetConfig` 的 `name` / `host`（缺失时由字段级 `#[serde(default)]` 兜底为
/// 空串，类型不符时由本宏兜底）。
///
/// ⚠️ **保形优先于退化**：能被转成可见文本的（数字 / 布尔）一律转成字符串，
/// 而不是退化为空串。因为这些字段退化成空串会被 `AppState::init_from_config`
/// 直接跳过 —— 结果和「整条消失」完全一样，那这个修复就是空转。
/// 宁可让用户在界面上看到一台看得见、能改正的坏主机。
macro_rules! tolerant_string {
    // 通用臂：String / Number / Bool 都保形（用于 name 这类纯显示文本）
    ($fn_name:ident) => {
        pub fn $fn_name<'de, D>(deserializer: D) -> Result<String, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            let value = Option::<serde_json::Value>::deserialize(deserializer).unwrap_or(None);
            Ok(match value.as_ref() {
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(serde_json::Value::Number(n)) => n.to_string(),
                Some(serde_json::Value::Bool(b)) => b.to_string(),
                _ => String::new(),
            })
        }
    };
    // host 专用臂：只收 String / Number，Bool 与其余一律退化为空串
    //
    // 理由：host 会被真的拿去做 DNS 解析和发 ICMP，而 `"true"` / `"false"` 作为地址
    // 没有任何 plausible 来源（没人会用布尔表示一个地址）；忘加引号的数字才是那个
    // 真实形态。空串会被 init_from_config 跳过，等价于「这台主机不存在」——
    // 对 host 而言这比「拿一个假地址去 ping」更可接受。
    ($fn_name:ident, host) => {
        pub fn $fn_name<'de, D>(deserializer: D) -> Result<String, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            let value = Option::<serde_json::Value>::deserialize(deserializer).unwrap_or(None);
            Ok(match value.as_ref() {
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(serde_json::Value::Number(n)) => n.to_string(),
                _ => String::new(),
            })
        }
    };
}

/// 生成 `Option<String>` 字段的容错反序列化器：**非字符串一律退化为 None**。
///
/// 空串本身保留（`stats::normalize_settings` 负责把它归一为 None），
/// 与既有 `deserialize_ui_scale` / `deserialize_event_level` 口径一致。
macro_rules! tolerant_opt_string {
    ($fn_name:ident) => {
        pub fn $fn_name<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            let value = Option::<serde_json::Value>::deserialize(deserializer).unwrap_or(None);
            Ok(value
                .as_ref()
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()))
        }
    };
}

/// `interval_ms` 的缺省值（1000）
fn default_interval_ms() -> u64 {
    1000
}

/// `timeout_ms` 的缺省值（2000）
fn default_timeout_ms() -> u64 {
    2000
}

/// `payload_size` 的缺省值（32）
fn default_payload_size() -> u16 {
    32
}

/// `ttl` 的缺省值（128）
fn default_ttl() -> u8 {
    128
}

/// `max_threads` 的缺省值（256）
fn default_max_threads() -> usize {
    256
}

/// `history_len` 的缺省值（60）
fn default_history_len() -> usize {
    60
}

tolerant_number!(deserialize_interval_ms, u64, default_interval_ms());
tolerant_number!(deserialize_timeout_ms, u64, default_timeout_ms());
tolerant_number!(deserialize_payload_size, u16, default_payload_size());
tolerant_number!(deserialize_ttl, u8, default_ttl());
tolerant_number!(deserialize_max_threads, usize, default_max_threads());
tolerant_bool!(deserialize_limit_max_threads, default_true());
tolerant_bool!(deserialize_beep_on_fail, false);
tolerant_bool!(deserialize_auto_start, false);
tolerant_number!(deserialize_history_len, usize, default_history_len());
tolerant_bool!(deserialize_events_on, default_true());
tolerant_bool!(deserialize_events_persist, default_true());
tolerant_opt_string!(deserialize_events_dir);
tolerant_number!(deserialize_events_keep, usize, default_log_keep());
tolerant_opt_string!(deserialize_ui_font_family);
tolerant_number!(deserialize_version, u32, default_version());
tolerant_string!(deserialize_target_name);
tolerant_string!(deserialize_target_host, host);
tolerant_bool!(deserialize_target_enabled, default_enabled());
tolerant_bool!(deserialize_target_events_on, default_true());

/// Ping 设置项
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PingSettings {
    /// 探测间隔（毫秒）
    ///
    /// ⚠️ 必须走自定义 `deserialize_with`：被写成 `"1000"` / `null` / 布尔时，
    /// 一律退化到缺省值 1000，绝不让整份配置回落默认而清空主机列表（红线 R1）。
    #[serde(
        default = "default_interval_ms",
        deserialize_with = "deserialize_interval_ms"
    )]
    pub interval_ms: u64,
    #[serde(
        default = "default_timeout_ms",
        deserialize_with = "deserialize_timeout_ms"
    )]
    pub timeout_ms: u64,
    #[serde(
        default = "default_payload_size",
        deserialize_with = "deserialize_payload_size"
    )]
    pub payload_size: u16,
    #[serde(default = "default_ttl", deserialize_with = "deserialize_ttl")]
    pub ttl: u8,
    #[serde(
        default = "default_max_threads",
        deserialize_with = "deserialize_max_threads"
    )]
    pub max_threads: usize,
    /// 是否启用 max_threads 并发上限（关闭后不再按该值拦截，仅保留 4096 硬保护）
    ///
    /// ⚠️ 字段级 `#[serde(default = "default_true")]` 为硬性要求：
    /// v1.0.0 的旧配置文件不含该字段，缺省时必须为 `true`，否则整份配置会回落默认、
    /// 丢失用户已保存的主机列表。
    #[serde(default = "default_true", deserialize_with = "deserialize_limit_max_threads")]
    pub limit_max_threads: bool,
    #[serde(default, deserialize_with = "deserialize_beep_on_fail")]
    pub beep_on_fail: bool,
    #[serde(default, deserialize_with = "deserialize_auto_start")]
    pub auto_start: bool,
    #[serde(
        default = "default_history_len",
        deserialize_with = "deserialize_history_len"
    )]
    pub history_len: usize,
    /// 事件日志总开关
    ///
    /// ⚠️ 字段级默认值为硬性要求：旧配置缺该字段时必须为 `true`，
    /// 否则整份配置回落默认、丢失用户主机列表。
    #[serde(default = "default_true", deserialize_with = "deserialize_events_on")]
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
    #[serde(
        default = "default_true",
        deserialize_with = "deserialize_events_persist"
    )]
    pub events_persist: bool,
    /// 事件保存目录（None 表示使用默认的 app_config_dir）
    #[serde(default, deserialize_with = "deserialize_events_dir")]
    pub events_dir: Option<String>,
    /// 每主机保留条数（50 / 200 / 1000）
    #[serde(
        default = "default_log_keep",
        deserialize_with = "deserialize_events_keep"
    )]
    pub events_keep: usize,
    /// 界面缩放（百分比，合法区间 50..=200 由 normalize_settings 钳制；100 = 不缩放）
    ///
    /// ⚠️ 必须走自定义 `deserialize_with`：非法值（字符串 / 越界数字等）一律降级为 100，
    /// 绝不让整份配置反序列化失败（否则丢用户主机列表，同 events_level 的高危防护）。
    #[serde(
        default = "default_ui_scale",
        deserialize_with = "deserialize_ui_scale"
    )]
    pub ui_scale: u8,
    /// 界面字体族名（None 或空串 = 系统默认，仿 events_dir 的模式）
    #[serde(default, deserialize_with = "deserialize_ui_font_family")]
    pub ui_font_family: Option<String>,
}

impl Default for PingSettings {
    /// ⚠️ 所有取值必须与字段级 `default = "..."` 的缺省函数保持一致（此处直接复用它们，
    /// 避免两处字面量漂移）。批次 2 未改动任何默认取值。
    fn default() -> Self {
        Self {
            interval_ms: default_interval_ms(),
            timeout_ms: default_timeout_ms(),
            payload_size: default_payload_size(),
            ttl: default_ttl(),
            max_threads: default_max_threads(),
            limit_max_threads: default_true(),
            beep_on_fail: false,
            auto_start: false,
            history_len: default_history_len(),
            events_on: true,
            events_level: default_event_level(),
            events_persist: true,
            events_dir: None,
            events_keep: default_log_keep(),
            ui_scale: default_ui_scale(),
            ui_font_family: None,
        }
    }
}

/// `limit_max_threads` 的缺省值（true）：保证旧配置加载后仍默认启用并发上限
fn default_true() -> bool {
    true
}

/// `ui_scale` 的缺省值（100，即不缩放）
fn default_ui_scale() -> u8 {
    100
}

/// `ui_scale` 的自定义反序列化：**非法值一律降级为 100，绝不失败**。
///
/// 高危防护（同 `events_level`）：该字段被写成字符串（如手工改配置）/ 越界数字
/// （u8 最大 255，缩放无理由超过）时，普通 `Deserialize` 会让整份 `AppConfig`
/// 反序列化失败 → 回落默认、清空用户主机列表。此处无论遇到什么都返回合法值。
pub fn deserialize_ui_scale<'de, D>(deserializer: D) -> Result<u8, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // 用 Value 作为中间载体：任意 JSON 值都能被接受，解析失败也退化为 None。
    let value = Option::<serde_json::Value>::deserialize(deserializer).unwrap_or(None);
    let n = value.as_ref().and_then(|v| v.as_u64());
    Ok(match n {
        Some(n) if n <= u8::MAX as u64 => n as u8,
        _ => default_ui_scale(),
    })
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
    /// ⚠️ 必须带字段级 `default` + 类型容错：手改配置漏写 `name` 或把它写成数字时，
    /// 整份配置不得反序列化失败（红线 R1）。非字符串形态**保形**为可见文本，
    /// 而不是退化成空串——否则该条目会被上层丢弃，修复等于空转。
    #[serde(default, deserialize_with = "deserialize_target_name")]
    pub name: String,
    /// ⚠️ 同上。只收 `String` / `Number`（`223555` → `"223555"` 可见可改），
    /// `Bool` 与其余形态退化为空串，由 `AppState::init_from_config` 跳过该条目。
    #[serde(default, deserialize_with = "deserialize_target_host")]
    pub host: String,
    /// ⚠️ 类型容错：`"enabled": "yes"`（手改时忘了去掉引号）不得让整份配置失败，
    /// 回落 `default_enabled()`（true）。
    #[serde(
        default = "default_enabled",
        deserialize_with = "deserialize_target_enabled"
    )]
    pub enabled: bool,
    /// 是否记录该主机的事件（字段级默认 true，保证旧配置兼容）
    #[serde(
        default = "default_true",
        deserialize_with = "deserialize_target_events_on"
    )]
    pub events_on: bool,
    /// 所属文件夹 id；`None` = 临时区（未分类区域）。
    ///
    /// 必须带 `default`（红线 R1）：v1.1.9 写出的配置**没有**这个字段，
    /// 缺省必须是 `None`（= 全部落在临时区），否则旧配置整份反序列化失败、
    /// 回落默认值 → **丢用户全部主机列表**。这是零迁移的前提。
    ///
    /// 指向**不存在**的文件夹时**不得**跳过该主机，必须回落为 `None`：
    /// 见 `config::finalize` 的孤儿回落。红线 R1 的教训是「宁可降级一个字段」。
    #[serde(default, deserialize_with = "deserialize_folder_id")]
    pub folder_id: Option<u64>,
}
/// 文件夹（v1.1.10 新增）：主机分组，用于把「临时区」与长期管理的 IP 分开。
///
/// 🩸 每个字段都带 serde 默认值 + 类型容错（红线 R1）：
/// 手改配置时把 `id` 写成字符串、或 `name` 漏写，都**不得**让整份配置解析失败。
/// `color` 未知值降级为 `slate`（见 `normalize_folder_color`）。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FolderConfig {
    #[serde(default, deserialize_with = "deserialize_folder_id_field")]
    pub id: u64,
    #[serde(default, deserialize_with = "deserialize_folder_name")]
    pub name: String,
    /// 预设色板 key（slate/sky/emerald/amber/red/violet/pink/teal），**不是自由 hex**
    #[serde(default = "default_folder_color", deserialize_with = "deserialize_folder_color")]
    pub color: String,
}

fn default_folder_color() -> String {
    FOLDER_COLOR_SLATE.to_string()
}

/// 预设色板全部合法取值；未知值一律降级为 `slate`（不报错，与项目枚举容错口径一致）
pub const FOLDER_COLORS: [&str; 8] = [
    "slate", "sky", "emerald", "amber", "red", "violet", "pink", "teal",
];
pub const FOLDER_COLOR_SLATE: &str = "slate";

/// 未知 / 缺失的 color 归一为 `slate`。**纯函数**，便于单测锁定。
pub fn normalize_folder_color(c: &str) -> String {
    let k = c.trim().to_ascii_lowercase();
    if FOLDER_COLORS.contains(&k.as_str()) {
        k
    } else {
        FOLDER_COLOR_SLATE.to_string()
    }
}

/// `FolderConfig.id` 的容错：收 Number（可转 u64）或数字字符串；其余形态回落 0。
/// ⚠️ 回落 0 意味着该文件夹**不可被引用**（没有主机能指向它），
/// 但它本身仍会出现在列表里 —— 宁可显示一个空文件夹，也不要静默丢数据。
fn deserialize_folder_id_field<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = Option::<serde_json::Value>::deserialize(deserializer).unwrap_or(None);
    Ok(match v.as_ref() {
        Some(serde_json::Value::Number(n)) => n.as_u64().unwrap_or(0),
        Some(serde_json::Value::String(s)) => s.trim().parse::<u64>().unwrap_or(0),
        _ => 0,
    })
}

/// `TargetConfig.folder_id` 的容错：只收 Number 与数字字符串；
/// `null` / 缺失 / 布尔 / 其他 → `None`（= 临时区）。
///
/// ⚠️ 刻意**不**把无法解析的形态变成 `Some(0)`：那会让主机被归到一个
/// 名为「未分类」的假文件夹里，比留在临时区更让人困惑。
fn deserialize_folder_id<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = Option::<serde_json::Value>::deserialize(deserializer).unwrap_or(None);
    Ok(match v.as_ref() {
        Some(serde_json::Value::Number(n)) => n.as_u64(),
        Some(serde_json::Value::String(s)) => s.trim().parse::<u64>().ok(),
        _ => None,
    })
}

/// `FolderConfig.name` 的容错：保形优先（数字/布尔转字符串），无法保形才回落空串。
/// 空名会在界面上显示为「(未命名)」，但文件夹本身与其中的主机都不丢。
fn deserialize_folder_name<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = Option::<serde_json::Value>::deserialize(deserializer).unwrap_or(None);
    Ok(match v.as_ref() {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        Some(serde_json::Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    })
}

/// `FolderConfig.color` 的容错：任何形态都不失败，未知值降级为 `slate`
fn deserialize_folder_color<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = Option::<serde_json::Value>::deserialize(deserializer).unwrap_or(None);
    let s = match v.as_ref() {
        Some(serde_json::Value::String(s)) => s.clone(),
        _ => String::new(),
    };
    Ok(normalize_folder_color(&s))
}

/// `AppConfig.folders` 的容错：**逐元素**跳过非法项，数组本身无法解析时回落空数组。
///
/// ⚠️ 与 `AppConfig.targets` 的关键区别（见 `docs/folder-design.md` 4.2）：
/// `targets` 是**承重结构**，绝不能静默跳过非法元素（那会让备份+抢救的触发器消失）；
/// 而 `folders` 不承担该职责 —— 这里跳过坏元素，使「文件夹写坏了」**不连带影响主机列表**。
pub fn deserialize_folders<'de, D>(deserializer: D) -> Result<Vec<FolderConfig>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = Option::<serde_json::Value>::deserialize(deserializer).unwrap_or(None);
    let arr = match v {
        Some(serde_json::Value::Array(a)) => a,
        // null / 缺失 / 类型不符 → 空数组（绝不返回 Err）
        _ => return Ok(Vec::new()),
    };
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        // 逐元素用 serde_json 自行解析：非法元素直接跳过，不影响其余
        if let Ok(f) = serde_json::from_value::<FolderConfig>(item) {
            out.push(f);
        }
    }
    Ok(out)
}

fn default_enabled() -> bool {
    true
}

/// `AppConfig.version` 的缺省值（1）
fn default_version() -> u32 {
    1
}

/// `AppConfig.settings` 的自定义反序列化：**任意值（含缺失 / null / 字符串 / 数组）
/// 都绝不返回 Err**，非法形态一律回落 `PingSettings::default()`。
///
/// 背景（红线 R1 / 批次 2 A2-1）：容器级 `#[serde(default)]` 只在字段**缺失**时生效，
/// `"settings": null` 会让整份配置反序列化失败 → 清空用户主机列表。此处用
/// `Option<Value>` 做中间载体把这一条补上（口径与 `deserialize_ui_scale` 一致）。
pub fn deserialize_settings<'de, D>(deserializer: D) -> Result<PingSettings, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer).unwrap_or(None);
    Ok(match value {
        Some(v) => serde_json::from_value::<PingSettings>(v).unwrap_or_default(),
        None => PingSettings::default(),
    })
}

/// 持久化的应用配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_version", deserialize_with = "deserialize_version")]
    pub version: u32,
    #[serde(default, deserialize_with = "deserialize_settings")]
    pub settings: PingSettings,
    #[serde(default)]
    pub targets: Vec<TargetConfig>,
    /// 文件夹列表（v1.1.10 新增）。缺省 = 空数组 = 行为与 v1.1.9 完全一致。
    #[serde(default, deserialize_with = "deserialize_folders")]
    pub folders: Vec<FolderConfig>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: 1,
            settings: PingSettings::default(),
            targets: Vec::new(),
            folders: Vec::new(),
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
            ui_scale: 125,
            ui_font_family: Some("Consolas".to_string()),
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
        assert_eq!(back.ui_scale, s.ui_scale);
        assert_eq!(back.ui_font_family, s.ui_font_family);
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
                TargetConfig { name: "阿里 DNS".into(), host: "223.5.5.5".into(), enabled: true, events_on: true, folder_id: None },
                TargetConfig { name: "百度".into(), host: "www.baidu.com".into(), enabled: false, events_on: true, folder_id: None },
            ],
            folders: Vec::new(),
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

    /// 把某个设置字段渲染成字符串形态（单元测试用）。
    /// 用于「非法值 → 回落默认值」的逐字段比对，避免 14 个字段各写一遍断言。
    fn field_value(s: &PingSettings, f: &str) -> String {
        match f {
            "interval_ms" => s.interval_ms.to_string(),
            "timeout_ms" => s.timeout_ms.to_string(),
            "payload_size" => s.payload_size.to_string(),
            "ttl" => s.ttl.to_string(),
            "max_threads" => s.max_threads.to_string(),
            "limit_max_threads" => s.limit_max_threads.to_string(),
            "beep_on_fail" => s.beep_on_fail.to_string(),
            "auto_start" => s.auto_start.to_string(),
            "history_len" => s.history_len.to_string(),
            "events_on" => s.events_on.to_string(),
            "events_persist" => s.events_persist.to_string(),
            "events_dir" => format!("{:?}", s.events_dir),
            "events_keep" => s.events_keep.to_string(),
            "ui_font_family" => format!("{:?}", s.ui_font_family),
            other => panic!("测试用到了未知字段：{}", other),
        }
    }

    /// 高危防护（批次 2 A2-3）：settings 段缺失 / 为 null / 为标量时，
    /// **整份配置不得解析失败**，主机列表必须逐字保住，设置回落默认。
    ///
    /// ⚠️ 本测试取代了原先的 `qa_missing_required_field_is_err`：把「缺 settings 应报错」
    /// 锁成既定策略，正是导致用户主机被静默清空的错误策略（红线 R1）。
    #[test]
    fn qa_missing_settings_block_keeps_targets() {
        const TARGETS: &str = r#"[{"name":"阿里 DNS","host":"223.5.5.5","enabled":true}]"#;
        let default_settings = PingSettings::default();

        // 1) 缺 settings（字段完全不存在）
        let raw = format!(r#"{{"version":1,"targets":{}}}"#, TARGETS);
        let cfg: AppConfig = serde_json::from_str(&raw)
            .unwrap_or_else(|e| panic!("缺 settings 时不得解析失败：{}", e));
        assert_eq!(cfg.targets.len(), 1, "缺 settings 时主机列表必须保住");
        assert_eq!(cfg.targets[0].host, "223.5.5.5", "主机详情必须逐字保留");
        assert_eq!(cfg.settings.interval_ms, default_settings.interval_ms);
        assert_eq!(cfg.settings.events_keep, default_settings.events_keep);
        assert_eq!(cfg.settings.ui_scale, 100, "settings 应整体回落默认");

        // 2) settings 为 null（容器级 #[serde(default)] 救不了，必须靠 deserialize_with）
        let raw_null = format!(r#"{{"version":1,"settings":null,"targets":{}}}"#, TARGETS);
        let cfg_null: AppConfig = serde_json::from_str(&raw_null)
            .unwrap_or_else(|e| panic!("settings=null 时不得解析失败：{}", e));
        assert_eq!(cfg_null.targets.len(), 1, "settings=null 时主机列表必须保住");
        assert_eq!(cfg_null.targets[0].name, "阿里 DNS");
        assert!(cfg_null.settings.events_on, "settings=null 时应回落默认设置");

        // 3) settings 为字符串（手改配置常见形态）
        let raw_str = format!(r#"{{"version":1,"settings":"abc","targets":{}}}"#, TARGETS);
        let cfg_str: AppConfig = serde_json::from_str(&raw_str)
            .unwrap_or_else(|e| panic!("settings=字符串时不得解析失败：{}", e));
        assert_eq!(cfg_str.targets.len(), 1, "settings 为字符串时主机列表必须保住");
        assert_eq!(cfg_str.settings.history_len, default_settings.history_len);

        // 4) 某条 target 缺 host：该条退化为「空 host」并交给
        //    `AppState::init_from_config` 跳过，绝不能升级为整份配置失败
        let one_missing_host = r#"{"version":1,"targets":[{"name":"x"}]}"#;
        let cfg_hostless: AppConfig = serde_json::from_str(one_missing_host)
            .unwrap_or_else(|e| panic!("target 缺 host 不得让整份配置失败：{}", e));
        assert!(cfg_hostless.targets[0].host.is_empty(), "缺 host 的条目应退化为空白 host");
        // init_from_config 会跳过空 host 条目（state.rs），此处用同口径断言其最终效果
        let kept: Vec<_> = cfg_hostless
            .targets
            .iter()
            .filter(|t| !t.host.trim().is_empty())
            .collect();
        assert!(kept.is_empty(), "空 host 条目会被 init_from_config 跳过");

        // 5) 缺 version：默认取 1，主机列表保住
        let no_version = r#"{"targets":[{"host":"1.1.1.1"}]}"#;
        let cfg_nv: AppConfig = serde_json::from_str(no_version)
            .unwrap_or_else(|e| panic!("缺 version 不得解析失败：{}", e));
        assert_eq!(cfg_nv.version, 1, "version 缺省应为 1");
        assert_eq!(cfg_nv.targets.len(), 1);
        assert_eq!(cfg_nv.targets[0].host, "1.1.1.1");
    }

    /// 高危防护（批次 2 收尾）：version 写成字符串 / null / 布尔等非法形态时，
    /// 整份 `AppConfig` **不得**反序列化失败——连 A2 的 settings 兜底都够不着，
    /// 用户主机列表和已保存的设置会一起丢失（A4 的抢救只能救回 targets，救不回 settings）。
    ///
    /// ⚠️ 哨兵字段（`settings.history_len: 333`）必须存活：若只断言 targets 不丢，
    /// A4 的抢救路径会把这条测试「救」过去，测不出真正的危害（同上一轮 A3 的教训）。
    #[test]
    fn qa_invalid_version_never_drops_settings_or_targets() {
        const BAD_VERSIONS: [&str; 6] = ["\"1\"", "null", "true", "1.5", "[]", "{}"];
        for v in BAD_VERSIONS {
            let raw = format!(
                r#"{{"version":{},"settings":{{"history_len":333}},"targets":[{{"name":"必须保住","host":"223.5.5.5","enabled":true}}]}}"#,
                v
            );
            let cfg: AppConfig = match serde_json::from_str(&raw) {
                Ok(c) => c,
                Err(e) => panic!(
                    "version={} 让整份配置解析失败（settings 与 targets 会一起丢）：{}",
                    v, e
                ),
            };
            assert_eq!(cfg.version, 1, "version={} 应回落为 1", v);
            assert_eq!(cfg.targets.len(), 1, "version={} 时主机列表必须保住", v);
            assert_eq!(cfg.targets[0].host, "223.5.5.5", "version={} 时主机详情必须完整", v);
            assert_eq!(
                cfg.settings.history_len, 333,
                "version={} 时 settings 块不得被连带丢弃",
                v
            );
        }

        // 合法值不受影响
        let ok: AppConfig = serde_json::from_str(r#"{"version":2,"targets":[]}"#).unwrap();
        assert_eq!(ok.version, 2, "合法的 version=2 必须原样保留");
        // 越界 version 钳制到 u32 上界，不得 panic / 回绕
        let big: AppConfig =
            serde_json::from_str(r#"{"version":99999999999,"targets":[]}"#).unwrap();
        assert_eq!(big.version, u32::MAX, "越界 version 应钳制到 u32::MAX");
    }

    /// 高危防护（批次 2 A3）：14 个设置字段被写成任意非法形态时，
    /// 整份配置**必须仍能解析**、主机列表必须保住、该字段回落默认值。
    ///
    /// ⚠️ 若把某个字段的 `deserialize_with` 去掉，本测试会红（这正是它的价值）。
    #[test]
    fn qa_invalid_settings_field_never_drops_targets() {
        const FIELDS: [&str; 14] = [
            "interval_ms",
            "timeout_ms",
            "payload_size",
            "ttl",
            "max_threads",
            "limit_max_threads",
            "beep_on_fail",
            "auto_start",
            "history_len",
            "events_on",
            "events_persist",
            "events_dir",
            "events_keep",
            "ui_font_family",
        ];
        /// 布尔字段：`true` / `false` 是合法值，只对它们测「非布尔形态」
        const BOOL_FIELDS: [&str; 5] = [
            "limit_max_threads",
            "beep_on_fail",
            "auto_start",
            "events_on",
            "events_persist",
        ];
        /// 字符串字段：`"..."` 是合法值，只对它们测「非字符串形态」
        const OPT_STR_FIELDS: [&str; 2] = ["events_dir", "ui_font_family"];
        /// 对所有字段类型都不合法的形态：null / 数组 / 对象 / 浮点 / 负数 / 越界整数
        const BAD_VALUES: [&str; 6] = ["null", "[]", "{}", "1.5", "-1", "99999999999999999999"];

        let baseline = PingSettings::default();

        for f in FIELDS {
            let is_bool = BOOL_FIELDS.contains(&f);
            let is_opt_str = OPT_STR_FIELDS.contains(&f);
            // 非法形态清单：按类型的「合法值」做减法——布尔字段不加 `true`，
            // 字符串字段不加 `"abc"`（这两个对该类型本来就是合法配置值）。
            let mut cases: Vec<&str> = BAD_VALUES.to_vec();
            if !is_bool {
                cases.push("true");
            }
            if !is_opt_str {
                cases.push("\"abc\"");
            }

            for v in cases {
                // 哨兵字段：同一个 settings 块里的**其它合法字段**必须存活。
                // 若某字段的容错被去掉，罪魁是「整个 settings 块被丢弃」——
                // 哨兵随之丢失，本测试立刻变红（否则很容易被 A2 的整体兜底掩盖）。
                let sentinel = if f == "history_len" {
                    "\"beep_on_fail\":true"
                } else {
                    "\"history_len\":333"
                };
                let raw = format!(
                    r#"{{"version":1,"settings":{{"{}":{}, {}}},"targets":[{{"name":"必须保住","host":"223.5.5.5","enabled":true}}]}}"#,
                    f, v, sentinel
                );
                let cfg: AppConfig = match serde_json::from_str(&raw) {
                    Ok(c) => c,
                    Err(e) => panic!(
                        "字段 {}={} 让整份配置解析失败（会清空用户主机列表）：{}",
                        f, v, e
                    ),
                };
                assert_eq!(cfg.targets.len(), 1, "字段 {}={} 时主机列表必须保住", f, v);
                assert_eq!(cfg.targets[0].host, "223.5.5.5", "字段 {}={} 时主机详情必须完整", f, v);
                assert_eq!(
                    field_value(&cfg.settings, f),
                    field_value(&baseline, f),
                    "字段 {}={} 应回落到默认值",
                    f,
                    v
                );
                if f == "history_len" {
                    assert!(
                        cfg.settings.beep_on_fail,
                        "字段 {}={} 不得连带丢掉 settings 块里的其它字段",
                        f,
                        v
                    );
                } else {
                    assert_eq!(
                        cfg.settings.history_len, 333,
                        "字段 {}={} 不得连带丢掉 settings 块里的其它字段",
                        f,
                        v
                    );
                }
            }
        }
    }

    /// 高危防护（批次 2b 🔴-1）：`TargetConfig` 的 4 个字段被写成任意非法形态时，
    /// 整份配置必须仍能解析、**邻座主机不得消失**、**settings 块不得被连带丢弃**。
    ///
    /// ⚠️ 哨兵套路（同上）：只断言「坏条目没让配置崩」是不够的——
    /// 真正要防的是「一个坏字符 → 其它用户数据一起没」，所以必须断言哨兵存活。
    #[test]
    fn qa_invalid_target_field_never_drops_neighbors_or_settings() {
        const FIELDS: [&str; 4] = ["name", "host", "enabled", "events_on"];
        const BAD_VALUES: [&str; 8] = [
            "\"abc\"",
            "null",
            "true",
            "42",
            "1.5",
            "[]",
            "{}",
            "99999999999999999999",
        ];

        for f in FIELDS {
            for v in BAD_VALUES {
                // 逐字段构造坏条目：**替换**被测字段的值，其余字段取合法值
                // （不能用追加，否则同一字段出现两次 → duplicate field）
                let bad_item = format!(
                    "{{{}}}",
                    FIELDS
                        .iter()
                        .map(|k| format!(
                            "\"{}\":{}",
                            k,
                            if *k == f {
                                v.to_string()
                            } else {
                                match *k {
                                    "name" => "\"坏条目\"".to_string(),
                                    "host" => "\"10.0.0.1\"".to_string(),
                                    _ => "true".to_string(),
                                }
                            }
                        ))
                        .collect::<Vec<_>>()
                        .join(",")
                );
                // 坏条目 + 邻座哨兵主机 + settings 哨兵字段，三者同框
                let raw = format!(
                    r#"{{"version":1,"settings":{{"history_len":333}},"targets":[{},{{"name":"哨兵","host":"223.5.5.5","enabled":false,"events_on":false}}]}}"#,
                    bad_item
                );
                let cfg: AppConfig = match serde_json::from_str(&raw) {
                    Ok(c) => c,
                    Err(e) => panic!(
                        "TargetConfig.{}={} 让整份配置解析失败（会清空用户主机列表）：{}",
                        f, v, e
                    ),
                };
                assert_eq!(cfg.targets.len(), 2, "字段 {}={} 时邻座主机必须保住", f, v);
                assert_eq!(cfg.targets[1].host, "223.5.5.5", "字段 {}={} 时哨兵主机必须完整", f, v);
                assert!(
                    !cfg.targets[1].enabled,
                    "字段 {}={} 时哨兵的 enabled=false 必须保留",
                    f,
                    v
                );
                assert_eq!(
                    cfg.settings.history_len, 333,
                    "字段 {}={} 时 settings 块不得被连带丢弃",
                    f,
                    v
                );
            }
        }
    }

    /// 字符串布尔必须按**字面语义**解析（🟡-3）：`"false"` 绝不能被读成 true，
    /// 否则用户想关掉的主机反而被打开——这比整份解析失败更隐蔽。
    #[test]
    fn qa_string_bool_semantics_are_honored() {
        // TargetConfig.enabled
        let t: TargetConfig =
            serde_json::from_str(r#"{"name":"a","host":"1.1.1.1","enabled":"false"}"#).unwrap();
        assert!(!t.enabled, "\"false\" 必须解析为 false");
        let t2: TargetConfig =
            serde_json::from_str(r#"{"name":"a","host":"1.1.1.1","enabled":"yes"}"#).unwrap();
        assert!(t2.enabled, "\"yes\" 应解析为 true");
        let t3: TargetConfig =
            serde_json::from_str(r#"{"name":"a","host":"1.1.1.1","enabled":"abc"}"#).unwrap();
        assert!(t3.enabled, "无法判读的字符串应回落 default_enabled()==true");

        // 大小写与空格容忍
        for s in ["\"TRUE\"", "\" Yes \"", "\"on\"", "\"1\""] {
            let v: TargetConfig =
                serde_json::from_str(&format!(r#"{{"name":"a","host":"1.1.1.1","enabled":{}}}"#, s))
                    .unwrap();
            assert!(v.enabled, "{} 应解析为 true", s);
        }
        for s in ["\"FALSE\"", "\" No \"", "\"off\"", "\"0\""] {
            let v: TargetConfig =
                serde_json::from_str(&format!(r#"{{"name":"a","host":"1.1.1.1","enabled":{}}}"#, s))
                    .unwrap();
            assert!(!v.enabled, "{} 应解析为 false", s);
        }

        // PingSettings 的布尔字段同样适用（默认 true 的字段被显式关掉时必须生效）
        let s: PingSettings = serde_json::from_str(r#"{"events_on":"false"}"#).unwrap();
        assert!(!s.events_on, "\"false\" 必须真的关掉事件记录");
        let s2: PingSettings = serde_json::from_str(r#"{"auto_start":"true"}"#).unwrap();
        assert!(s2.auto_start);

        // 坏值不得让邻座主机与 settings 消失（哨兵仍须存活）
        let raw = r#"{"version":1,"settings":{"history_len":333},"targets":[{"name":"坏条目","host":"10.0.0.1","enabled":"false"},{"name":"哨兵","host":"223.5.5.5","enabled":true}]}"#;
        let cfg: AppConfig = serde_json::from_str(raw).unwrap();
        assert_eq!(cfg.targets.len(), 2);
        assert!(!cfg.targets[0].enabled, "\"false\" 必须生效");
        assert_eq!(cfg.settings.history_len, 333);
    }

    /// 正向断言（架构师点名保留的 fixture）：`"enabled":"yes"` 现在应**解析成功**，
    /// `enabled` 回落 `true`，邻座主机与 settings 都保住。
    ///
    /// 这条曾经是「损坏配置」的触发器；🔴-1 落地后它变成合法形态，锁住这个新行为。
    #[test]
    fn qa_target_enabled_string_falls_back_to_true() {
        let raw = r#"{"version":1,"settings":{"history_len":333},"targets":[{"name":"坏条目","host":"10.0.0.1","enabled":"yes"},{"name":"哨兵","host":"223.5.5.5","enabled":false}]}"#;
        let cfg: AppConfig = serde_json::from_str(raw)
            .unwrap_or_else(|e| panic!("\"enabled\":\"yes\" 现在应解析成功：{}", e));
        assert_eq!(cfg.targets.len(), 2, "邻座主机必须保住");
        assert!(
            cfg.targets[0].enabled,
            "\"yes\" 不是合法布尔，应回落 default_enabled()==true"
        );
        assert_eq!(cfg.targets[0].host, "10.0.0.1", "坏条目的 host 必须保住");
        assert_eq!(cfg.settings.history_len, 333, "settings 不得被连带丢弃");
    }

    /// host 的保形规则：**Number 保形**（忘加引号的数字是真实形态），
    /// **Bool 退化为空串**（没有人会用布尔表示一个地址，空串会被 init_from_config 跳过）。
    #[test]
    fn qa_target_host_number_kept_bool_dropped() {
        // 数字：保形为可见文本，用户在界面上看得到、能改正
        let n: TargetConfig = serde_json::from_str(r#"{"name":"a","host":223555}"#).unwrap();
        assert_eq!(n.host, "223555", "数字的 host 应保形为可见字符串");
        let n2: TargetConfig = serde_json::from_str(r#"{"name":"a","host":42}"#).unwrap();
        assert_eq!(n2.host, "42");
        // 布尔：退化为空串（由 init_from_config 跳过该条目）
        let b: TargetConfig = serde_json::from_str(r#"{"name":"a","host":true}"#).unwrap();
        assert!(b.host.is_empty(), "布尔的 host 应退化为空串并被跳过");
        let b2: TargetConfig = serde_json::from_str(r#"{"name":"a","host":false}"#).unwrap();
        assert!(b2.host.is_empty());
        // name 是纯显示文本，布尔保形无害
        let nb: TargetConfig = serde_json::from_str(r#"{"name":true,"host":"1.1.1.1"}"#).unwrap();
        assert_eq!(nb.name, "true", "name 的布尔值应保形（不影响探测）");
        // 合法字符串原样保留
        let ok: TargetConfig =
            serde_json::from_str(r#"{"name":"阿里 DNS","host":"223.5.5.5"}"#).unwrap();
        assert_eq!(ok.name, "阿里 DNS");
        assert_eq!(ok.host, "223.5.5.5");
    }

    /// 容错不得误伤合法值：边界值必须逐字保留（否则上述容错会反过来吞掉用户设置）
    #[test]
    fn qa_valid_settings_values_survive_tolerant_deserialize() {
        let raw = r#"{"version":1,"settings":{"interval_ms":300,"timeout_ms":1000,"payload_size":65500,"ttl":255,"max_threads":1024,"limit_max_threads":false,"beep_on_fail":true,"auto_start":true,"history_len":600,"events_on":false,"events_persist":false,"events_dir":"D:\\pb-events","events_keep":1000,"ui_font_family":"Consolas"},"targets":[{"name":"x","host":"1.1.1.1"}]}"#;
        let cfg: AppConfig = serde_json::from_str(raw).unwrap();
        let s = &cfg.settings;
        assert_eq!(s.interval_ms, 300);
        assert_eq!(s.timeout_ms, 1000);
        assert_eq!(s.payload_size, 65500, "u16 上界内的合法值必须保留");
        assert_eq!(s.ttl, 255, "u8 上界内的合法值必须保留");
        assert_eq!(s.max_threads, 1024);
        assert!(!s.limit_max_threads, "显式 false 必须保留");
        assert!(s.beep_on_fail);
        assert!(s.auto_start);
        assert_eq!(s.history_len, 600);
        assert!(!s.events_on, "显式 false 必须保留");
        assert!(!s.events_persist, "显式 false 必须保留");
        assert_eq!(s.events_dir.as_deref(), Some("D:\\pb-events"));
        assert_eq!(s.events_keep, 1000);
        assert_eq!(s.ui_font_family.as_deref(), Some("Consolas"));
        assert_eq!(cfg.targets.len(), 1);
    }

    /// 超出目标类型上界的数字应先钳制再转换（不得因 `as` 回绕变成垃圾值）
    #[test]
    fn qa_out_of_range_numbers_are_clamped_not_wrapped() {
        let s: PingSettings = serde_json::from_str(r#"{"payload_size":70000,"ttl":300}"#).unwrap();
        assert_eq!(s.payload_size, u16::MAX, "超 u16 的值应钳制到 65535");
        assert_eq!(s.ttl, u8::MAX, "超 u8 的值应钳制到 255");
        // 其余业务边界（65500 / 1..255）由 stats::normalize_settings 兜底
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

    /* ===================== 界面缩放 / 字体：旧配置兼容（不丢 targets） ===================== */

    /// 旧配置（v1.1.6 及更早，无 ui_scale / ui_font_family）：必须完好加载且缺省正确。
    #[test]
    fn qa_old_config_without_ui_fields_defaults() {
        let old = r#"{"version":1,"settings":{"interval_ms":800,"timeout_ms":1500,"payload_size":32,"ttl":128,"max_threads":256,"beep_on_fail":false,"auto_start":false,"history_len":60},"targets":[{"name":"阿里 DNS","host":"223.5.5.5","enabled":true}]}"#;
        let cfg: AppConfig = serde_json::from_str(old).unwrap();
        assert_eq!(cfg.settings.ui_scale, 100, "ui_scale 缺省应为 100");
        assert!(cfg.settings.ui_font_family.is_none(), "ui_font_family 缺省应为 None");
        assert_eq!(cfg.targets.len(), 1, "旧配置的主机列表必须保留");
        assert_eq!(cfg.targets[0].host, "223.5.5.5");

        // 空设置对象同样缺省正确
        let s: PingSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(s.ui_scale, 100);
        assert!(s.ui_font_family.is_none());
    }

    /// 高危防护（同 events_level）：ui_scale 非法值一律降级 100，绝不整份失败。
    #[test]
    fn qa_invalid_ui_scale_falls_back_to_100() {
        // 字符串值（手工改配置的典型形态）
        let s: PingSettings =
            serde_json::from_str(r#"{"ui_scale":"abc"}"#).unwrap();
        assert_eq!(s.ui_scale, 100, "字符串值应降级为 100");
        // 越界数字（u8 装不下的 300）
        let s2: PingSettings = serde_json::from_str("{\"ui_scale\":300}").unwrap();
        assert_eq!(s2.ui_scale, 100, "越界数字应降级为 100");
        // null / 布尔 / 浮点同样不报错
        for raw in ["{\"ui_scale\":null}", "{\"ui_scale\":true}", "{\"ui_scale\":1.5}"] {
            let s3: PingSettings = serde_json::from_str(raw).unwrap();
            assert_eq!(s3.ui_scale, 100, "非法值应降级：{}", raw);
        }
        // 合法范围内的值原样保留
        let ok: PingSettings = serde_json::from_str("{\"ui_scale\":125}").unwrap();
        assert_eq!(ok.ui_scale, 125);
    }

    /// ui_font_family 仿 events_dir：None / 空串都表示「系统默认」；显式值可往返。
    #[test]
    fn qa_ui_font_family_roundtrip() {
        let s: PingSettings = serde_json::from_str(r#"{"ui_font_family":"Consolas"}"#).unwrap();
        assert_eq!(s.ui_font_family.as_deref(), Some("Consolas"));
        // 空串能加载（归一为 None 由 stats::normalize_settings 处理）
        let s2: PingSettings = serde_json::from_str(r#"{"ui_font_family":""}"#).unwrap();
        assert_eq!(s2.ui_font_family.as_deref(), Some(""));
    }
}
