// 事件日志核心：等级化事件模型、状态迁移纯函数、增量缓冲与 JSONL 持久化。
//
// 设计要点（见 docs/events-log-design.md）：
//   * 事件在**后端状态迁移点**生成（不再由前端 diff 快照），根治「漏记」根因。
//   * 三档等级：仅故障 / 标准（默认）/ 详细；`level` 冗余随事件存储，JSONL 自描述。
//   * 传输采用**增量 + 单调 seq**：`emit` 推入 pending，快照发射器每 500ms `drain_pending`。
//   * 持久化为 JSONL，由独立写线程串行落盘，**绝不阻塞探测循环**。
use std::collections::{HashMap, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;

use serde::{Deserialize, Deserializer, Serialize};

use crate::model::Status;

/// 事件持久化文件名
pub const EVENTS_FILE_NAME: &str = "pingboard-events.jsonl";
/// 归档文件名（轮转时使用）
pub const EVENTS_ARCHIVE_NAME: &str = "pingboard-events.1.jsonl";
/// 临时文件名（清空某主机时整文件重写的中间文件）
const EVENTS_TEMP_NAME: &str = "pingboard-events.tmp.jsonl";
/// 文件轮转阈值（8 MiB）
pub const EVENTS_FILE_MAX_BYTES: usize = 8 * 1024 * 1024;
/// 启动时只读文件末尾的最大字节数（4 MiB）
pub const EVENTS_FILE_TAIL_BYTES: usize = 4 * 1024 * 1024;
/// `events_keep` 的默认值（每主机内存环 / 展示上限）
pub const DEFAULT_EVENTS_KEEP: usize = 200;

/* ----------------------------- 事件模型 ----------------------------- */

/// 事件类型（`snake_case` 序列化，前端 TS 联合类型与之逐一对齐）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// 故障：ok -> timeout/failed
    Fault,
    /// 无法连通：idle/resolving -> timeout/failed（从未成功）
    Unreachable,
    /// 解析失败：resolving -> failed（解析源）
    DnsFail,
    /// 恢复：失败 -> ok
    Recover,
    /// 首次连通：idle/resolving -> ok
    FirstOk,
    /// 开始探测
    Start,
    /// 已停止
    Stop,
    /// 配置变更
    ConfigChange,
}

impl EventKind {
    /// 事件类型的中文名（用于 CSV「事件类型」列）
    pub fn label(self) -> &'static str {
        match self {
            EventKind::Fault => "故障",
            EventKind::Unreachable => "无法连通",
            EventKind::DnsFail => "解析失败",
            EventKind::Recover => "已恢复",
            EventKind::FirstOk => "首次连通",
            EventKind::Start => "开始探测",
            EventKind::Stop => "已停止",
            EventKind::ConfigChange => "配置变更",
        }
    }
}

/// 事件等级（三档）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventLevel {
    /// 仅故障
    Fault,
    /// 标准（默认）
    Standard,
    /// 详细
    Detail,
}

impl EventLevel {
    /// 等级排序：值越小越「严重」。过滤规则为 `rank(event.level) <= rank(events_level)`。
    /// （实际过滤在前端进行；后端保留该口径供测试与自描述使用。）
    #[allow(dead_code)]
    pub fn rank(self) -> u8 {
        match self {
            EventLevel::Fault => 0,
            EventLevel::Standard => 1,
            EventLevel::Detail => 2,
        }
    }

    /// 常量表：由事件类型推导其归属等级。
    ///
    /// 三档集合：
    ///   * 仅故障 = {fault, unreachable, dns_fail, recover}
    ///   * 标准   = 上述 + {first_ok, start, stop}
    ///   * 详细   = 上述 + {config_change}
    pub fn for_kind(kind: EventKind) -> Self {
        match kind {
            EventKind::Fault
            | EventKind::Unreachable
            | EventKind::DnsFail
            | EventKind::Recover => EventLevel::Fault,
            EventKind::FirstOk | EventKind::Start | EventKind::Stop => EventLevel::Standard,
            EventKind::ConfigChange => EventLevel::Detail,
        }
    }

    /// 等级中文名（用于 CSV「等级」列）
    pub fn label(self) -> &'static str {
        match self {
            EventLevel::Fault => "仅故障",
            EventLevel::Standard => "标准",
            EventLevel::Detail => "详细",
        }
    }
}

impl Default for EventLevel {
    fn default() -> Self {
        EventLevel::Standard
    }
}

/// `PingSettings.events_level` 的缺省值（Standard）
pub fn default_event_level() -> EventLevel {
    EventLevel::Standard
}

/// `PingSettings.events_keep` 的缺省值（200）
pub fn default_log_keep() -> usize {
    DEFAULT_EVENTS_KEEP
}

/// `events_level` 的自定义反序列化：**未知值一律降级为 `Standard`，绝不失败**。
///
/// 这是高危防护（设计稿风险 #8）：若该字段被写成未知串（例如手工改配置 / 未来版本写了
/// 当前版本不认识的值），普通 `Deserialize` 会让整份 `AppConfig` 反序列化失败，
/// 进而回落默认、**清空用户主机列表**。此处无论如何都返回一个合法等级。
pub fn deserialize_event_level<'de, D>(deserializer: D) -> Result<EventLevel, D::Error>
where
    D: Deserializer<'de>,
{
    // 用 Value 作为中间载体：任意 JSON 值都能被接受，解析失败也退化为 None。
    let value = Option::<serde_json::Value>::deserialize(deserializer).unwrap_or(None);
    let text = value.as_ref().and_then(|v| v.as_str());
    Ok(match text {
        Some("fault") => EventLevel::Fault,
        Some("detail") => EventLevel::Detail,
        Some("standard") => EventLevel::Standard,
        _ => EventLevel::Standard,
    })
}

/// 失败来源：区分「解析阶段失败」与「探测阶段失败」，用于产出 dns_fail / unreachable。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailSource {
    /// 主机名解析阶段
    Resolve,
    /// ICMP / ping.exe 探测阶段
    Probe,
}

/// 单条事件（JSONL 一行）
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LogEvent {
    /// 会话内单调自增序号，用于快照去重 / 排序。
    ///
    /// ⚠️ 口径是**「本次运行会话内单调」**，不是全局唯一 —— 每次启动由 `load_hosts`
    /// 把计数器顶到读回历史的最大 seq 之上，因此同一文件内 seq 仍严格递增；
    /// 但跨会话比较无意义，要切批次请用 `session`。
    pub seq: u64,
    /// 本事件所属运行批次的标识 = 该次进程启动时刻（纪元毫秒）。
    ///
    /// 每次启动一个新值（单调递增，天然可排序），写进该次运行产生的每一条事件。
    /// 用于在单一文件内区分「哪一次运行」，导出 CSV 时渲染为本地时间。
    /// 该字段出现之前的旧行反序列化为 `0`（导出时显示为空）。
    pub session: u64,
    /// 纪元毫秒
    pub ts: u64,
    /// 运行时目标 id（实时路由用）
    pub target_id: u64,
    /// 冗余：导出 / 重启后展示自足
    pub target_name: String,
    /// 冗余：重启后按 host 归属（运行时 id 会重排）
    pub target_host: String,
    /// 事件类型
    pub kind: EventKind,
    /// 事件等级（冗余存储，JSONL 自描述）
    pub level: EventLevel,
    /// 已本地化文案
    pub text: String,
}

impl<'de> Deserialize<'de> for LogEvent {
    /// 反序列化：容忍旧行缺 `level` / `text`，缺 `level` 时由 `EventLevel::for_kind` 推导。
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Raw {
            seq: u64,
            /// 旧行（本字段引入之前写的）缺此项 → 0，代表「更早的运行」
            #[serde(default)]
            session: u64,
            #[serde(default)]
            ts: u64,
            #[serde(default)]
            target_id: u64,
            #[serde(default)]
            target_name: String,
            #[serde(default)]
            target_host: String,
            kind: EventKind,
            #[serde(default)]
            level: Option<EventLevel>,
            #[serde(default)]
            text: String,
        }
        let raw = Raw::deserialize(deserializer)?;
        let level = raw
            .level
            .unwrap_or_else(|| EventLevel::for_kind(raw.kind));
        Ok(LogEvent {
            seq: raw.seq,
            session: raw.session,
            ts: raw.ts,
            target_id: raw.target_id,
            target_name: raw.target_name,
            target_host: raw.target_host,
            kind: raw.kind,
            level,
            text: raw.text,
        })
    }
}

/* ----------------------------- 状态迁移纯函数 ----------------------------- */

/// 依据「写入前状态 prev → 写入后状态 next」判定应产生的事件类型。
///
/// **去重内建**：`timeout/failed -> timeout/failed` 一律返回 `None`，
/// 从而根治「每轮重复记失败」的问题。
pub fn classify_transition(prev: Status, next: Status, src: FailSource) -> Option<EventKind> {
    use Status::*;
    match (prev, next) {
        (_, Ok) if prev == Timeout || prev == Failed => Some(EventKind::Recover),
        (_, Ok) if prev == Idle || prev == Resolving => Some(EventKind::FirstOk),
        (Ok, Timeout) | (Ok, Failed) => Some(EventKind::Fault),
        (Idle, Timeout) | (Idle, Failed) | (Resolving, Timeout) | (Resolving, Failed) => {
            Some(if matches!(src, FailSource::Resolve) {
                EventKind::DnsFail
            } else {
                EventKind::Unreachable
            })
        }
        (Timeout, Timeout) | (Timeout, Failed) | (Failed, Timeout) | (Failed, Failed) => None,
        _ => None,
    }
}

/// 成功类事件的本地化文案
pub fn success_text(kind: EventKind, rtt: f64) -> String {
    match kind {
        EventKind::Recover => format!("已恢复（{:.1} ms）", rtt),
        EventKind::FirstOk => format!("首次连通（{:.1} ms）", rtt),
        _ => "探测成功".to_string(),
    }
}

/// 失败类事件的本地化文案
pub fn failure_text(kind: EventKind, consecutive: u32, err: Option<&str>) -> String {
    match kind {
        EventKind::Fault => match err {
            Some(e) if !e.is_empty() => format!("探测失败：{}", e),
            _ => format!("探测超时（连续 {} 次）", consecutive),
        },
        EventKind::Unreachable => match err {
            Some(e) if !e.is_empty() => format!("无法连通：{}", e),
            _ => "无法连通".to_string(),
        },
        EventKind::DnsFail => match err {
            // resolve 侧（state.rs）构造的错误文案已自带「解析失败：」前缀，
            // 先剥掉再拼一次，避免出现「解析失败：解析失败：…」。
            Some(e) if !e.is_empty() => {
                format!("解析失败：{}", e.strip_prefix("解析失败：").unwrap_or(e))
            }
            _ => "解析失败".to_string(),
        },
        _ => "未知事件".to_string(),
    }
}

/* ----------------------------- 写线程消息 ----------------------------- */

/// 写线程消息：串行化文件操作，worker 探测循环只做「发送」，绝不阻塞。
enum WriterMsg {
    /// 配置（开关 / 目录）变更，重开句柄
    Configure { enabled: bool, dir: Option<PathBuf> },
    /// 追加一行
    Write(String),
    /// 清空某主机（整文件重写：读 → 过滤 → 临时文件 → 原子替换）
    ClearHost(String),
}

/* ----------------------------- 事件存储 ----------------------------- */

/// 事件存储：增量 pending + 每主机内存环 + 单调 seq + JSONL 写线程。
pub struct EventStore {
    /// 本次运行的会话标识（启动时刻，纪元毫秒）；创建后不变
    session: u64,
    /// 单调自增序号
    seq: AtomicU64,
    /// 每主机内存环 cap（= 详情面板展示上限）
    keep: AtomicUsize,
    /// 持久化是否启用（目录可用且开关打开）
    persist_enabled: AtomicBool,
    /// 运行期持久化是否出错（供 UI 提示；不影响主流程）
    persist_error: AtomicBool,
    /// 自上次快照以来新增的事件（快照发射器 drain）
    pending: Mutex<Vec<LogEvent>>,
    /// 每主机内存环（id -> 事件队列，按 seq 升序）
    per_host: Mutex<HashMap<u64, VecDeque<LogEvent>>>,
    /// 写线程发送端
    tx: Sender<WriterMsg>,
}

impl EventStore {
    /// 创建事件存储并启动写线程。
    ///
    /// 会话标识取**本次进程启动时刻**（纪元毫秒）：同一毫秒内不可能启动两次进程，
    /// 因此它在单一事件文件内天然唯一且单调递增，可直接用于排序与分组。
    pub fn new() -> Self {
        Self::with_session(crate::state::now_ms())
    }

    /// 指定会话标识构造（测试注入用；生产走 `new()`）
    pub fn with_session(session: u64) -> Self {
        let (tx, rx) = mpsc::channel::<WriterMsg>();
        spawn_writer(rx);
        Self {
            session,
            seq: AtomicU64::new(0),
            keep: AtomicUsize::new(DEFAULT_EVENTS_KEEP),
            persist_enabled: AtomicBool::new(false),
            persist_error: AtomicBool::new(false),
            pending: Mutex::new(Vec::new()),
            per_host: Mutex::new(HashMap::new()),
            tx,
        }
    }

    /// 本次运行的会话标识（启动时刻，纪元毫秒）
    pub fn session(&self) -> u64 {
        self.session
    }

    /// 设置每主机内存环 cap（来自 `events_keep`）
    pub fn set_keep(&self, keep: usize) {
        self.keep.store(keep.max(1), Ordering::SeqCst);
    }

    /// 应用持久化配置：目录与开关变化时重开写句柄
    pub fn apply_persist(&self, enabled: bool, dir: Option<PathBuf>) {
        let effective = enabled && dir.is_some();
        self.persist_enabled.store(effective, Ordering::SeqCst);
        self.persist_error.store(false, Ordering::SeqCst);
        let _ = self.tx.send(WriterMsg::Configure {
            enabled: effective,
            dir,
        });
    }

    /// 持久化是否处于启用状态
    #[allow(dead_code)]
    pub fn persist_enabled(&self) -> bool {
        self.persist_enabled.load(Ordering::SeqCst)
    }

    /// 发起一条事件：分配 seq、推入 pending、更新每主机环、（可选）落盘。
    pub fn emit(&self, kind: EventKind, text: String, id: u64, name: &str, host: &str) {
        let seq = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        let event = LogEvent {
            seq,
            session: self.session,
            ts: crate::state::now_ms(),
            target_id: id,
            target_name: name.to_string(),
            target_host: host.to_string(),
            kind,
            level: EventLevel::for_kind(kind),
            text,
        };

        // 1) 增量缓冲
        self.pending.lock().unwrap().push(event.clone());

        // 2) 每主机内存环（按 keep 截断）
        {
            let keep = self.keep.load(Ordering::SeqCst).max(1);
            let mut map = self.per_host.lock().unwrap();
            let ring = map.entry(id).or_insert_with(VecDeque::new);
            ring.push_back(event.clone());
            while ring.len() > keep {
                ring.pop_front();
            }
        }

        // 3) 持久化（异步，绝不阻塞调用方）
        if self.persist_enabled.load(Ordering::SeqCst) {
            if let Ok(line) = serde_json::to_string(&event) {
                if self.tx.send(WriterMsg::Write(line)).is_err() {
                    self.persist_error.store(true, Ordering::SeqCst);
                }
            }
        }
    }

    /// 取走并清空自上次以来新增的事件（供快照发射器）
    pub fn drain_pending(&self) -> Vec<LogEvent> {
        let mut guard = self.pending.lock().unwrap();
        std::mem::take(&mut *guard)
    }

    /// 读取某主机的最近 `limit` 条事件，按**新→旧**返回（详情面板展示顺序）
    pub fn list_host(&self, id: u64, limit: usize) -> Vec<LogEvent> {
        let map = self.per_host.lock().unwrap();
        match map.get(&id) {
            Some(ring) => ring.iter().rev().take(limit).cloned().collect(),
            None => Vec::new(),
        }
    }

    /// 汇总全部事件（按 seq 升序），用于「导出全部事件」
    pub fn all_events(&self) -> Vec<LogEvent> {
        let map = self.per_host.lock().unwrap();
        let mut events: Vec<LogEvent> = map
            .values()
            .flat_map(|ring| ring.iter().cloned())
            .collect();
        events.sort_by_key(|e| e.seq);
        events
    }

    /// 清空某主机：内存环清空 + 请求写线程整文件重写剔除该主机
    pub fn clear_host(&self, id: u64, host: &str) {
        self.per_host.lock().unwrap().remove(&id);
        if self.persist_enabled.load(Ordering::SeqCst) {
            let _ = self.tx.send(WriterMsg::ClearHost(host.to_string()));
        }
    }

    /// 启动读回：按 host 匹配当前 target 建立 id 映射，历史标记为已读（不入 pending）。
    ///
    /// `host_to_id` 的 key 必须已是「trim + 小写」口径。
    pub fn load_hosts(&self, events: Vec<LogEvent>, host_to_id: &HashMap<String, u64>, keep: usize) {
        let cap = keep.max(1);
        let mut max_seq = 0u64;
        {
            let mut map = self.per_host.lock().unwrap();
            for mut event in events {
                max_seq = max_seq.max(event.seq);
                let key = normalize_host(&event.target_host);
                if let Some(&id) = host_to_id.get(&key) {
                    // 重启后 id 会重排，按 host 归属后回填当前 id
                    event.target_id = id;
                    let ring = map.entry(id).or_insert_with(VecDeque::new);
                    ring.push_back(event);
                }
                // 匹配不到的目标当孤儿丢弃
            }
            // 每主机只保留最近 cap 条
            for ring in map.values_mut() {
                while ring.len() > cap {
                    ring.pop_front();
                }
            }
        }
        // 让后续实时事件的 seq 严格大于历史，避免去重口径冲突
        let mut current = self.seq.load(Ordering::SeqCst);
        while current < max_seq {
            match self
                .seq
                .compare_exchange(current, max_seq, Ordering::SeqCst, Ordering::SeqCst)
            {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
    }

    /// 运行期持久化是否出错
    #[allow(dead_code)]
    pub fn persist_error(&self) -> bool {
        self.persist_error.load(Ordering::SeqCst)
    }
}

impl Default for EventStore {
    fn default() -> Self {
        Self::new()
    }
}

/// host 归一化：trim + 小写（用于跨重启匹配）
pub fn normalize_host(host: &str) -> String {
    host.trim().to_lowercase()
}

/* ----------------------------- 写线程 ----------------------------- */

/// 启动事件写线程（串行落盘，失败只记日志，绝不影响主流程）
fn spawn_writer(rx: Receiver<WriterMsg>) {
    std::thread::Builder::new()
        .name("pingboard-events-writer".to_string())
        .spawn(move || {
            let mut writer = EventWriter {
                rx,
                dir: None,
                enabled: false,
                file: None,
            };
            writer.run();
        })
        .ok();
}

/// 文件写入器：持有当前目录与句柄，处理配置变更 / 追加 / 清空
struct EventWriter {
    rx: Receiver<WriterMsg>,
    dir: Option<PathBuf>,
    enabled: bool,
    file: Option<File>,
}

impl EventWriter {
    fn run(&mut self) {
        while let Ok(msg) = self.rx.recv() {
            match msg {
                WriterMsg::Configure { enabled, dir } => self.configure(enabled, dir),
                WriterMsg::Write(line) => self.append(&line),
                WriterMsg::ClearHost(host) => self.clear_host(&host),
            }
        }
    }

    /// 应用配置：关闭旧句柄并按需重开
    fn configure(&mut self, enabled: bool, dir: Option<PathBuf>) {
        self.file = None;
        self.enabled = enabled;
        self.dir = dir;
        if !self.enabled {
            return;
        }
        let Some(dir) = self.dir.clone() else {
            self.enabled = false;
            return;
        };
        if let Err(e) = fs::create_dir_all(&dir) {
            eprintln!("[pingboard] 无法创建事件目录 {}：{}", dir.display(), e);
            self.enabled = false;
            return;
        }
        self.open_file();
    }

    /// 打开（或新建）追加句柄
    fn open_file(&mut self) {
        let Some(dir) = &self.dir else {
            self.file = None;
            return;
        };
        let path = dir.join(EVENTS_FILE_NAME);
        match OpenOptions::new().create(true).append(true).open(&path) {
            Ok(f) => self.file = Some(f),
            Err(e) => {
                eprintln!("[pingboard] 打开事件文件失败 {}：{}", path.display(), e);
                self.file = None;
            }
        }
    }

    /// 追加一行（必要时先轮转）
    fn append(&mut self, line: &str) {
        if !self.enabled {
            return;
        }
        if self.file.is_none() {
            self.open_file();
        }
        let too_big = self
            .file
            .as_ref()
            .and_then(|f| f.metadata().ok())
            .map(|m| m.len() as usize > EVENTS_FILE_MAX_BYTES)
            .unwrap_or(false);
        if too_big {
            self.rotate();
        }
        if let Some(file) = self.file.as_mut() {
            let mut buf = String::with_capacity(line.len() + 1);
            buf.push_str(line);
            buf.push('\n');
            if let Err(e) = file.write_all(buf.as_bytes()) {
                eprintln!("[pingboard] 写事件失败：{}", e);
            } else {
                let _ = file.flush();
            }
        }
    }

    /// 文件轮转：当前文件改名归档（覆盖旧归档）后新建空文件
    fn rotate(&mut self) {
        let Some(dir) = &self.dir else {
            return;
        };
        let path = dir.join(EVENTS_FILE_NAME);
        let archive = dir.join(EVENTS_ARCHIVE_NAME);
        self.file = None;
        let _ = fs::remove_file(&archive);
        let _ = fs::rename(&path, &archive);
        self.open_file();
    }

    /// 清空某主机：读整文件 → 过滤该主机 → 写临时文件 → 原子替换。
    ///
    /// ⚠️ **主文件与归档都要处理**。只清主文件的话，配合「归档也读回」会出现
    /// 「清空了又自己回来」的怪象（2026-09-23 修复）。
    fn clear_host(&mut self, host: &str) {
        if !self.enabled {
            return;
        }
        let Some(dir) = self.dir.clone() else {
            return;
        };
        self.file = None; // 先释放句柄，便于替换
        let key = normalize_host(host);
        for name in [EVENTS_FILE_NAME, EVENTS_ARCHIVE_NAME] {
            filter_out_host(&dir, name, &key);
        }
        self.open_file();
    }
}

/// 就地剔除文件中属于 `key`（已归一化的 host）的行；文件不存在时静默跳过。
///
/// 解析失败的行（坏行 / 写到一半的残行）一律**保留**，避免误删。
fn filter_out_host(dir: &Path, name: &str, key: &str) {
    let path = dir.join(name);
    let Ok(content) = fs::read_to_string(&path) else {
        return;
    };
    let mut out = String::with_capacity(content.len());
    for line in content.lines() {
        let keep = serde_json::from_str::<LogEvent>(line)
            .map(|e| normalize_host(&e.target_host) != key)
            .unwrap_or(true);
        if keep {
            out.push_str(line);
            out.push('\n');
        }
    }
    // 过滤后为空：直接删除文件，避免在磁盘上留下 0 字节残file
    // （下次 open_file 会按需重新创建）
    if out.is_empty() {
        let _ = fs::remove_file(&path);
        return;
    }
    let tmp = dir.join(EVENTS_TEMP_NAME);
    if fs::write(&tmp, out.as_bytes()).is_ok() && fs::rename(&tmp, &path).is_err() {
        let _ = fs::remove_file(&tmp);
    }
}

/* ----------------------------- 启动读回 ----------------------------- */

/// 从文件读取历史事件：**归档与主文件都读**，合并后按 `ts` 升序返回。
///
/// 修复（2026-09-23）：此前只读主文件，一旦发生轮转，归档里的历史就在界面上
/// 「消失」（数据仍在磁盘上，只是读不回来）。两侧各只取末尾 `EVENTS_FILE_TAIL_BYTES`，
/// 真正的每主机截断交给 `load_hosts` 按 `keep` 处理。
pub fn load_history(dir: &Path) -> Vec<LogEvent> {
    let mut events = Vec::new();
    // 归档在前（更老），主文件在后（更新）。任一文件不存在都静默跳过。
    for name in [EVENTS_ARCHIVE_NAME, EVENTS_FILE_NAME] {
        if let Ok(bytes) = fs::read(dir.join(name)) {
            events.extend(parse_jsonl_tail(&bytes, EVENTS_FILE_TAIL_BYTES));
        }
    }
    // `load_hosts` 靠「后进先出截断」保留最新，因此这里必须按时间升序。
    // 稳定排序：同一毫秒内保持原插入顺序（归档 → 主文件）。
    events.sort_by_key(|e| e.ts);
    events
}

/// 解析 JSONL 字节流（只取末尾 `max_tail` 字节）；被截断时从首个换行之后开始；
/// 坏行跳过。
pub fn parse_jsonl_tail(bytes: &[u8], max_tail: usize) -> Vec<LogEvent> {
    let truncated = bytes.len() > max_tail;
    let slice = if truncated {
        &bytes[bytes.len() - max_tail..]
    } else {
        bytes
    };
    let text = String::from_utf8_lossy(slice);
    let body: &str = if truncated {
        match text.find('\n') {
            Some(idx) => &text[idx + 1..],
            None => "",
        }
    } else {
        &text
    };
    body.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return None;
            }
            serde_json::from_str::<LogEvent>(trimmed).ok()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用会话标识（启动时刻）
    const SAMPLE_SESSION: u64 = 1_700_000_000_000;

    fn sample_event(seq: u64, id: u64, host: &str, kind: EventKind) -> LogEvent {
        LogEvent {
            seq,
            session: SAMPLE_SESSION,
            ts: SAMPLE_SESSION + seq,
            target_id: id,
            target_name: format!("主机{}", id),
            target_host: host.to_string(),
            kind,
            level: EventLevel::for_kind(kind),
            text: kind.label().to_string(),
        }
    }

    /* ===================== classify_transition 全迁移用例 ===================== */

    /// 全量迁移矩阵：对 5x5 状态对 + 2 种来源逐一断言。
    #[test]
    fn classify_transition_full_matrix() {
        use FailSource::*;
        use Status::*;

        let all = [Idle, Resolving, Ok, Timeout, Failed];
        for &prev in &all {
            for &next in &all {
                let probe = classify_transition(prev, next, Probe);
                let resolve = classify_transition(prev, next, Resolve);

                // 断言口径（与设计稿 1.4 完全一致）
                let expect_probe = match (prev, next) {
                    (_, Ok) if prev == Timeout || prev == Failed => Some(EventKind::Recover),
                    (_, Ok) if prev == Idle || prev == Resolving => Some(EventKind::FirstOk),
                    (Ok, Timeout) | (Ok, Failed) => Some(EventKind::Fault),
                    (Idle, Timeout) | (Idle, Failed) | (Resolving, Timeout) | (Resolving, Failed) => {
                        Some(EventKind::Unreachable)
                    }
                    _ => None,
                };
                let expect_resolve = match (prev, next) {
                    (_, Ok) if prev == Timeout || prev == Failed => Some(EventKind::Recover),
                    (_, Ok) if prev == Idle || prev == Resolving => Some(EventKind::FirstOk),
                    (Ok, Timeout) | (Ok, Failed) => Some(EventKind::Fault),
                    (Idle, Timeout) | (Idle, Failed) | (Resolving, Timeout) | (Resolving, Failed) => {
                        Some(EventKind::DnsFail)
                    }
                    _ => None,
                };
                assert_eq!(
                    probe, expect_probe,
                    "Probe 迁移 {:?}->{:?} 结果不符",
                    prev, next
                );
                assert_eq!(
                    resolve, expect_resolve,
                    "Resolve 迁移 {:?}->{:?} 结果不符",
                    prev, next
                );
            }
        }
    }

    /// 去重：timeout/failed 之间来回切换一律不再产生事件（根治「每轮重复记」）
    #[test]
    fn classify_transition_dedup_no_repeat() {
        use FailSource::*;
        use Status::*;
        for &prev in &[Timeout, Failed] {
            for &next in &[Timeout, Failed] {
                assert_eq!(classify_transition(prev, next, Probe), None);
                assert_eq!(classify_transition(prev, next, Resolve), None);
            }
        }
    }

    /// 关键场景固化
    #[test]
    fn classify_transition_key_cases() {
        use FailSource::*;
        use Status::*;
        // 首次连通
        assert_eq!(
            classify_transition(Idle, Ok, Probe),
            Some(EventKind::FirstOk)
        );
        assert_eq!(
            classify_transition(Resolving, Ok, Probe),
            Some(EventKind::FirstOk)
        );
        // 恢复
        assert_eq!(
            classify_transition(Timeout, Ok, Probe),
            Some(EventKind::Recover)
        );
        assert_eq!(
            classify_transition(Failed, Ok, Probe),
            Some(EventKind::Recover)
        );
        // 故障
        assert_eq!(classify_transition(Ok, Timeout, Probe), Some(EventKind::Fault));
        assert_eq!(classify_transition(Ok, Failed, Probe), Some(EventKind::Fault));
        // 启动即失败
        assert_eq!(
            classify_transition(Idle, Timeout, Probe),
            Some(EventKind::Unreachable)
        );
        assert_eq!(
            classify_transition(Resolving, Failed, Probe),
            Some(EventKind::Unreachable)
        );
        // 解析失败
        assert_eq!(
            classify_transition(Idle, Failed, Resolve),
            Some(EventKind::DnsFail)
        );
        assert_eq!(
            classify_transition(Resolving, Failed, Resolve),
            Some(EventKind::DnsFail)
        );
        // ok -> ok / resolving -> resolving 不产生事件
        assert_eq!(classify_transition(Ok, Ok, Probe), None);
    }

    /* ===================== 等级推导 ===================== */

    #[test]
    fn level_for_kind_and_rank() {
        assert_eq!(EventLevel::for_kind(EventKind::Fault), EventLevel::Fault);
        assert_eq!(
            EventLevel::for_kind(EventKind::Unreachable),
            EventLevel::Fault
        );
        assert_eq!(EventLevel::for_kind(EventKind::DnsFail), EventLevel::Fault);
        assert_eq!(EventLevel::for_kind(EventKind::Recover), EventLevel::Fault);
        assert_eq!(EventLevel::for_kind(EventKind::FirstOk), EventLevel::Standard);
        assert_eq!(EventLevel::for_kind(EventKind::Start), EventLevel::Standard);
        assert_eq!(EventLevel::for_kind(EventKind::Stop), EventLevel::Standard);
        assert_eq!(
            EventLevel::for_kind(EventKind::ConfigChange),
            EventLevel::Detail
        );
        assert!(EventLevel::Fault.rank() < EventLevel::Standard.rank());
        assert!(EventLevel::Standard.rank() < EventLevel::Detail.rank());
    }

    /* ===================== 内存缓冲 / seq / drain ===================== */

    #[test]
    fn emit_buffers_seq_and_respects_keep() {
        let store = EventStore::new();
        store.set_keep(3);
        for i in 1..=5u64 {
            store.emit(
                EventKind::Fault,
                format!("e{}", i),
                if i <= 3 { 1 } else { 2 },
                "n",
                "h",
            );
        }
        // seq 单调自增
        let pending = store.drain_pending();
        assert_eq!(pending.len(), 5, "pending 应累计 5 条");
        assert_eq!(pending[0].seq, 1);
        assert_eq!(pending[4].seq, 5);
        // 每主机环最多保留 keep 条（新→旧）
        let h1 = store.list_host(1, 100);
        assert_eq!(h1.len(), 3, "主机 1 环应被截断到 3 条");
        assert!(h1[0].seq > h1[2].seq, "list_host 应新→旧返回");
        let h2 = store.list_host(2, 100);
        assert_eq!(h2.len(), 2, "主机 2 仅 2 条");
        // drain 后 pending 清空
        assert!(store.drain_pending().is_empty());
    }

    #[test]
    fn emit_sets_level_redundantly() {
        let store = EventStore::new();
        store.emit(EventKind::ConfigChange, "x".into(), 7, "n", "h");
        let ev = &store.list_host(7, 1)[0];
        assert_eq!(ev.level, EventLevel::Detail, "level 应冗余随事件存储");
    }

    /* ===================== JSONL 序列化 / 反序列化 ===================== */

    #[test]
    fn jsonl_serde_roundtrip_and_snake_case() {
        let ev = sample_event(42, 3, "223.5.5.5", EventKind::DnsFail);
        let line = serde_json::to_string(&ev).unwrap();
        assert!(line.contains("\"kind\":\"dns_fail\""), "kind 应为 snake_case");
        assert!(line.contains("\"level\":\"fault\""), "level 应为 snake_case");
        let back: LogEvent = serde_json::from_str(&line).unwrap();
        assert_eq!(back, ev);
    }

    /// 旧行缺 `level` 时由 `for_kind` 推导；缺 ts/text 时用默认值
    #[test]
    fn jsonl_missing_level_is_derived_from_kind() {
        let line = r#"{"seq":9,"target_id":1,"target_name":"a","target_host":"h","kind":"first_ok"}"#;
        let ev: LogEvent = serde_json::from_str(line).unwrap();
        assert_eq!(ev.level, EventLevel::Standard, "first_ok 缺 level 应推导为标准");
        assert_eq!(ev.kind, EventKind::FirstOk);
        assert_eq!(ev.ts, 0);
        assert_eq!(ev.text, "");
    }

    /// 追加 + 尾部读回：写 3 行 → load_history 读回 3 条，坏行跳过
    #[test]
    fn jsonl_append_and_tail_readback() {
        let dir = std::env::temp_dir().join("pingboard_qa_events_append");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(EVENTS_FILE_NAME);

        let events = vec![
            sample_event(1, 1, "1.1.1.1", EventKind::FirstOk),
            sample_event(2, 1, "1.1.1.1", EventKind::Fault),
            sample_event(3, 2, "2.2.2.2", EventKind::Start),
        ];
        {
            let mut f = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .unwrap();
            for ev in &events {
                f.write_all(serde_json::to_string(ev).unwrap().as_bytes()).unwrap();
                f.write_all(b"\n").unwrap();
            }
            f.write_all(b"{ this is a corrupt line\n").unwrap();
        }

        let back = load_history(&dir);
        assert_eq!(back.len(), 3, "坏行应被跳过，保留 3 条有效事件");
        assert_eq!(back[0].seq, 1);
        assert_eq!(back[2].target_host, "2.2.2.2");
    }

    /// 尾部读回在截断时应丢弃首个残行，只解析完整行
    #[test]
    fn jsonl_tail_skips_partial_first_line() {
        let dir = std::env::temp_dir().join("pingboard_qa_events_tail");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(EVENTS_FILE_NAME);

        let e1 = sample_event(1, 1, "h", EventKind::Start);
        let e2 = sample_event(2, 1, "h", EventKind::Stop);
        let mut content = String::new();
        content.push_str("PARTIAL-GARBAGE-HEAD\n");
        content.push_str(&serde_json::to_string(&e1).unwrap());
        content.push('\n');
        content.push_str(&serde_json::to_string(&e2).unwrap());
        content.push('\n');
        std::fs::write(&path, content.as_bytes()).unwrap();

        // 以「从第 5 字节开始」触发截断分支（起点落在首个残行内部）
        let parsed = parse_jsonl_tail(content.as_bytes(), content.len() - 5);
        assert_eq!(parsed.len(), 2, "截断后应只解析 2 条完整行");
        assert_eq!(parsed[0].seq, 1);
        assert_eq!(parsed[1].seq, 2);
    }

    /* ===================== 启动读回：按 host 归属 + 孤儿丢弃 ===================== */

    #[test]
    fn load_hosts_maps_by_host_and_drops_orphans() {
        let store = EventStore::new();
        store.set_keep(2);

        let mut host_to_id = HashMap::new();
        host_to_id.insert("1.1.1.1".to_string(), 10u64);

        let events = vec![
            sample_event(1, 1, "1.1.1.1", EventKind::FirstOk),
            sample_event(2, 1, "1.1.1.1", EventKind::Fault),
            sample_event(3, 1, "1.1.1.1", EventKind::Recover),
            // 孤儿：当前 targets 无此 host
            sample_event(4, 9, "9.9.9.9", EventKind::FirstOk),
        ];
        store.load_hosts(events, &host_to_id, 2);

        let loaded = store.list_host(10, 100);
        assert_eq!(loaded.len(), 2, "每主机只保留最近 keep=2 条");
        assert_eq!(loaded[0].seq, 3, "应保留最新两条（seq 3、2）");
        assert_eq!(loaded[0].target_id, 10, "重启后 id 应回填为当前 id");
        assert!(store.list_host(9, 100).is_empty(), "孤儿事件应被丢弃");

        // 历史不应进入 pending（历史标记为已读）
        assert!(store.drain_pending().is_empty(), "读回历史不应产生未读增量");

        // 后续实时事件 seq 应大于文件中的历史最大 seq（含孤儿事件的 seq=4）
        store.emit(EventKind::Start, "开始探测".into(), 10, "n", "1.1.1.1");
        let fresh = store.drain_pending();
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0].seq, 5, "实时事件 seq 应接续历史最大 seq(4)");
    }

    /// 回归：解析失败文案不得出现重复前缀。
    ///
    /// `state.rs` 构造的解析错误已自带「解析失败：」前缀（如
    /// `format!("解析失败：{}", e)`），若此处再拼一次就会显示成
    /// 「解析失败：解析失败：不知道这样的主机。」（真实报障，已修）。
    #[test]
    fn qa_dns_fail_text_has_no_duplicated_prefix() {
        // 上游已带前缀 —— 不得重复
        assert_eq!(
            failure_text(EventKind::DnsFail, 1, Some("解析失败：不知道这样的主机。(os error 11001)")),
            "解析失败：不知道这样的主机。(os error 11001)"
        );
        // 上游未带前缀 —— 仍应补上
        assert_eq!(
            failure_text(EventKind::DnsFail, 1, Some("不知道这样的主机。")),
            "解析失败：不知道这样的主机。"
        );
        // 无错误详情 —— 退化为纯类型名
        assert_eq!(failure_text(EventKind::DnsFail, 1, None), "解析失败");
        assert_eq!(failure_text(EventKind::DnsFail, 1, Some("")), "解析失败");
    }

    /// 其余失败类型的前缀不受影响（防止改一个坏一个）。
    #[test]
    fn qa_other_failure_texts_still_prefixed() {
        assert_eq!(
            failure_text(EventKind::Fault, 1, Some("IOP 状态码 11013")),
            "探测失败：IOP 状态码 11013"
        );
        assert_eq!(failure_text(EventKind::Fault, 3, None), "探测超时（连续 3 次）");
        assert_eq!(failure_text(EventKind::Unreachable, 1, Some("请求超时")), "无法连通：请求超时");
        assert_eq!(failure_text(EventKind::Unreachable, 1, None), "无法连通");
    }

    /* ===================== 会话标记（session） ===================== */

    /// 每条事件都带上「本次运行的会话标识」，且创建后不变
    #[test]
    fn emit_stamps_current_session() {
        let store = EventStore::with_session(1_756_000_000_000);
        assert_eq!(store.session(), 1_756_000_000_000);
        store.emit(EventKind::Start, "开始探测".into(), 1, "n", "h");
        store.emit(EventKind::Fault, "故障".into(), 1, "n", "h");
        let pending = store.drain_pending();
        assert_eq!(pending.len(), 2);
        assert!(
            pending.iter().all(|e| e.session == 1_756_000_000_000),
            "每条事件都应带会话标识"
        );
        assert!(
            store
                .list_host(1, 10)
                .iter()
                .all(|e| e.session == 1_756_000_000_000),
            "内存环里同样带会话标识"
        );
    }

    /// 旧行（无 `session` 字段）反序列化为 0；新行序列化必须含该字段并可往返
    #[test]
    fn jsonl_missing_session_defaults_to_zero() {
        let line =
            r#"{"seq":9,"ts":5,"target_id":1,"target_name":"a","target_host":"h","kind":"start"}"#;
        let ev: LogEvent = serde_json::from_str(line).unwrap();
        assert_eq!(ev.session, 0, "旧行缺 session 应默认为 0");

        let ev2 = sample_event(3, 1, "h", EventKind::Start);
        let out = serde_json::to_string(&ev2).unwrap();
        assert!(out.contains("\"session\":"), "序列化必须含 session 字段：{}", out);
        let back: LogEvent = serde_json::from_str(&out).unwrap();
        assert_eq!(back, ev2);
    }

    /* ===================== 归档读回与清空（两处缺陷回归） ===================== */

    /// 缺陷①回归：`load_history` 必须**同时读归档与主文件**，并按 `ts` 升序返回。
    ///
    /// 此前只读主文件，一旦轮转发生，归档里的历史就在界面上「消失」。
    #[test]
    fn load_history_merges_archive_and_main_by_ts() {
        let dir = std::env::temp_dir().join("pingboard_qa_events_archive");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // 刻意让归档里的 seq（500/501）大于主文件的 seq（1），
        // 以证明合并顺序依据的是 ts 而不是 seq。
        let archived = vec![
            LogEvent {
                seq: 500,
                ts: 2_000,
                ..sample_event(1, 1, "1.1.1.1", EventKind::FirstOk)
            },
            LogEvent {
                seq: 501,
                ts: 3_000,
                ..sample_event(2, 1, "1.1.1.1", EventKind::Fault)
            },
        ];
        let main = vec![LogEvent {
            seq: 1,
            ts: 9_000,
            ..sample_event(3, 1, "1.1.1.1", EventKind::Recover)
        }];
        let write = |name: &str, evs: &[LogEvent]| {
            let mut s = String::new();
            for e in evs {
                s.push_str(&serde_json::to_string(e).unwrap());
                s.push('\n');
            }
            std::fs::write(dir.join(name), s.as_bytes()).unwrap();
        };
        write(EVENTS_ARCHIVE_NAME, &archived);
        write(EVENTS_FILE_NAME, &main);

        let back = load_history(&dir);
        assert_eq!(back.len(), 3, "归档 2 条 + 主文件 1 条都应读回（此前只能读到 1 条）");
        assert_eq!(
            back.iter().map(|e| e.ts).collect::<Vec<_>>(),
            vec![2_000, 3_000, 9_000],
            "必须按 ts 升序合并（load_hosts 靠后进先出截断保留最新）"
        );
        assert_eq!(back[2].kind, EventKind::Recover, "最新的一条来自主文件");
    }

    /// 只有归档、主文件不存在时也要能读回（刚轮转完的瞬间）
    #[test]
    fn load_history_works_with_archive_only() {
        let dir = std::env::temp_dir().join("pingboard_qa_events_archive_only");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let ev = sample_event(1, 1, "h", EventKind::Start);
        std::fs::write(
            dir.join(EVENTS_ARCHIVE_NAME),
            format!("{}\n", serde_json::to_string(&ev).unwrap()).as_bytes(),
        )
        .unwrap();

        assert_eq!(load_history(&dir).len(), 1, "仅有归档时也应读回");
    }

    /// 缺陷②回归：清空某主机必须**同时**剔除主文件与归档里的该主机行。
    ///
    /// 只清主文件的话，配合缺陷① 的修复会出现「清空了又自己回来」。
    #[test]
    fn filter_out_host_clears_both_files() {
        let dir = std::env::temp_dir().join("pingboard_qa_events_clear_both");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let keep_ev = sample_event(1, 2, "keep.example.com", EventKind::Start);
        let del_ev = sample_event(2, 1, "223.5.5.5", EventKind::Fault);
        let content = format!(
            "{}\n{}\n",
            serde_json::to_string(&keep_ev).unwrap(),
            serde_json::to_string(&del_ev).unwrap()
        );
        for name in [EVENTS_FILE_NAME, EVENTS_ARCHIVE_NAME] {
            std::fs::write(dir.join(name), content.as_bytes()).unwrap();
        }

        let key = normalize_host("223.5.5.5");
        for name in [EVENTS_FILE_NAME, EVENTS_ARCHIVE_NAME] {
            filter_out_host(&dir, name, &key);
        }

        for name in [EVENTS_FILE_NAME, EVENTS_ARCHIVE_NAME] {
            let text = std::fs::read_to_string(dir.join(name)).unwrap();
            assert!(!text.contains("223.5.5.5"), "{} 仍残留被清主机：{}", name, text);
            assert!(text.contains("keep.example.com"), "{} 误删了其他主机：{}", name, text);
            assert_eq!(
                text.lines().filter(|l| !l.trim().is_empty()).count(),
                1,
                "{} 应恰好剩 1 行",
                name
            );
        }
    }

    /// 归档不存在时清空必须静默跳过（不 panic、不创建空文件）
    #[test]
    fn filter_out_host_tolerates_missing_file() {
        let dir = std::env::temp_dir().join("pingboard_qa_events_clear_missing");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        filter_out_host(&dir, EVENTS_ARCHIVE_NAME, "h");
        assert!(
            !dir.join(EVENTS_ARCHIVE_NAME).exists(),
            "文件不存在时不应顺手创建空文件"
        );
    }

    /// 过滤后一条不剩时应删除文件，而不是留一个 0 字节残file
    #[test]
    fn filter_out_host_removes_file_when_all_cleared() {
        let dir = std::env::temp_dir().join("pingboard_qa_events_clear_all");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let ev = sample_event(1, 1, "1.1.1.1", EventKind::Fault);
        let content = format!("{}\n", serde_json::to_string(&ev).unwrap());
        std::fs::write(dir.join(EVENTS_FILE_NAME), content.as_bytes()).unwrap();

        filter_out_host(&dir, EVENTS_FILE_NAME, &normalize_host("1.1.1.1"));
        assert!(
            !dir.join(EVENTS_FILE_NAME).exists(),
            "全部清空后应删除文件，不留 0 字节残file"
        );
    }

    /// 坏行（写到一半的残行）在清空时必须保留，避免误删用户数据
    #[test]
    fn filter_out_host_keeps_corrupt_lines() {
        let dir = std::env::temp_dir().join("pingboard_qa_events_clear_corrupt");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let del_ev = sample_event(1, 1, "1.1.1.1", EventKind::Fault);
        let content = format!("{{坏行\n{}\n", serde_json::to_string(&del_ev).unwrap());
        std::fs::write(dir.join(EVENTS_FILE_NAME), content.as_bytes()).unwrap();

        filter_out_host(&dir, EVENTS_FILE_NAME, &normalize_host("1.1.1.1"));
        let text = std::fs::read_to_string(dir.join(EVENTS_FILE_NAME)).unwrap();
        assert!(text.contains("{坏行"), "坏行必须保留：{}", text);
        assert!(!text.contains("1.1.1.1"), "目标主机应被剔除：{}", text);
    }

    /* ===================== seq 口径核查 ===================== */

    /// 核查「seq 每次启动从 0 开始」是否会与历史撞号。
    ///
    /// 结论：**不构成可达缺陷** —— `load_hosts` 会把计数器顶到**所有读回事件**（含孤儿）的
    /// 最大 seq 之上，而读的又是文件末尾，最新的一定在读回范围内。
    /// 本测试固化该行为，防止后续重构（例如改成「只对匹配到目标的事件计数」）把它破坏。
    #[test]
    fn qa_seq_resumes_above_history_max_even_with_orphans() {
        let store = EventStore::with_session(1_756_000_000_000);
        let mut host_to_id = HashMap::new();
        host_to_id.insert("1.1.1.1".to_string(), 7u64);

        let events = vec![
            sample_event(10, 7, "1.1.1.1", EventKind::Start),
            // 孤儿：当前 targets 里没有它，但 seq 更大 —— 计数器同样必须被顶起来
            sample_event(99, 3, "orphan.example.com", EventKind::Fault),
        ];
        store.load_hosts(events, &host_to_id, 200);

        store.emit(EventKind::Stop, "已停止".into(), 7, "n", "1.1.1.1");
        let fresh = store.drain_pending();
        assert_eq!(fresh.len(), 1);
        assert!(
            fresh[0].seq > 99,
            "新事件 seq({}) 必须大于历史最大 seq(99)，否则前端 seq 去重会丢事件",
            fresh[0].seq
        );
        assert_eq!(fresh[0].session, 1_756_000_000_000, "新事件应带当前会话标识");
    }
}
