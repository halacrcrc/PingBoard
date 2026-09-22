// 全局状态：目标表、设置、工作线程管理与聚合快照发射任务
//
// 并发模型：
//   * 每个运行中的目标对应一个独立工作线程，线程内复用探测引擎句柄
//   * 目标状态用 Arc<Mutex<TargetState>>，线程各自加锁更新，互不阻塞
//   * 控制标志用 AtomicBool，休眠按 100ms 分片检查，避免长休眠阻塞停止指令
//   * 由独立任务每 500ms 主动 emit 一次聚合快照，前端只做整体替换
use std::collections::HashMap;
use std::net::{IpAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter, Manager};

use crate::events::{self, EventKind, FailSource, LogEvent};
use crate::model::{AppConfig, PingSettings, Snapshot, Status, TargetConfig, TargetEntry, TargetState};
use crate::pinger::{fallback, Engine};
use crate::{config, stats};

/// 停止原因：只有「用户主动停止」才产生 `stop` 事件。
///
/// * `User` —— 用户点「停止全部 / 停止选中」；**打点**
/// * `Internal` —— 删除目标 / 关闭监控 / `set_enabled(false)`；**不打点**（改由 `config_change` 表达）
///
/// 程序退出（`WindowEvent::CloseRequested`）不会调用 `stop`，线程随进程结束，故也不产生 `stop`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// 用户主动停止
    User,
    /// 内部原因（删除 / 关闭监控 / 禁用）
    Internal,
}

/// 共享的目标句柄
pub type SharedTarget = Arc<Mutex<TargetState>>;

/// 快照事件名
pub const SNAPSHOT_EVENT: &str = "ping-snapshot";

/// 快照发射间隔（毫秒）
const SNAPSHOT_INTERVAL_MS: u64 = 500;

/// 关闭「线程数上限」后仍保留的硬保护上限，防止误加入海量目标拖垮系统
pub const HARD_MAX_THREADS: usize = 4096;

/// 工作线程句柄
struct WorkerHandle {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

/// 内部共享状态
pub struct Inner {
    /// 有序目标列表（保持添加顺序）
    targets: Mutex<Vec<SharedTarget>>,
    settings: RwLock<PingSettings>,
    next_id: AtomicU64,
    running: AtomicBool,
    workers: Mutex<HashMap<u64, WorkerHandle>>,
    started_at: Mutex<Option<u64>>,
    /// 一旦 ICMP 不可用即置位，之后所有线程直接走 ping.exe
    icmp_failed: AtomicBool,
    /// 事件存储（增量缓冲 + 每主机环 + JSONL 持久化）
    events: events::EventStore,
    /// 事件默认目录（app_config_dir；由 setup 注入）
    events_base_dir: Mutex<Option<PathBuf>>,
}

/// 应用状态（由 Tauri manage）
pub struct AppState {
    inner: Arc<Inner>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                targets: Mutex::new(Vec::new()),
                settings: RwLock::new(PingSettings::default()),
                next_id: AtomicU64::new(1),
                running: AtomicBool::new(false),
                workers: Mutex::new(HashMap::new()),
                started_at: Mutex::new(None),
                icmp_failed: AtomicBool::new(false),
                events: events::EventStore::new(),
                events_base_dir: Mutex::new(None),
            }),
        }
    }

    /* ----------------------------- 读取 ----------------------------- */

    /// 生成一份聚合快照
    pub fn snapshot(&self) -> Snapshot {
        let targets: Vec<TargetState> = {
            let list = self.inner.targets.lock().unwrap();
            list.iter().map(|t| t.lock().unwrap().clone()).collect()
        };

        let total_sent: u64 = targets.iter().map(|t| t.sent).sum();
        let total_received: u64 = targets.iter().map(|t| t.received).sum();
        let total_failed: u64 = targets.iter().map(|t| t.failed).sum();
        let active_threads = self.inner.workers.lock().unwrap().len();
        let settings = self.inner.settings.read().unwrap().clone();
        let started_at = *self.inner.started_at.lock().unwrap();

        Snapshot {
            targets,
            settings,
            running: self.inner.running.load(Ordering::SeqCst),
            active_threads,
            total_sent,
            total_received,
            total_failed,
            loss_pct: stats::loss_pct(total_sent, total_failed),
            started_at,
            updated_at: now_ms(),
            // 本次运行的会话标识（启动时刻），前端用于区分「本次运行」与历史批次
            session: self.inner.events.session(),
            // 增量事件：取走并清空 pending，空闲时为 []
            events: self.inner.events.drain_pending(),
        }
    }

    /// 导出当前配置（用于持久化）
    pub fn config(&self) -> AppConfig {
        let targets: Vec<TargetConfig> = {
            let list = self.inner.targets.lock().unwrap();
            list.iter()
                .map(|t| {
                    let g = t.lock().unwrap();
                    TargetConfig {
                        name: g.name.clone(),
                        host: g.host.clone(),
                        enabled: g.enabled,
                        events_on: g.events_on,
                    }
                })
                .collect()
        };
        AppConfig {
            version: 1,
            settings: self.inner.settings.read().unwrap().clone(),
            targets,
        }
    }

    /// 保存配置到磁盘
    pub fn save_config(&self, app: &AppHandle) -> Result<(), String> {
        config::save(app, &self.config())
    }

    /* ----------------------------- 初始化 ----------------------------- */

    /// 用配置初始化：写入设置并按顺序重建目标（id 自增、不持久化）
    pub fn init_from_config(&self, cfg: &AppConfig) {
        {
            let mut s = self.inner.settings.write().unwrap();
            *s = cfg.settings.clone();
            stats::normalize_settings(&mut s);
        }
        let mut list = self.inner.targets.lock().unwrap();
        list.clear();
        for tc in &cfg.targets {
            if tc.host.trim().is_empty() {
                continue;
            }
            let id = self.inner.next_id.fetch_add(1, Ordering::SeqCst);
            let mut state = TargetState::new(id, tc.name.clone(), tc.host.clone());
            state.enabled = tc.enabled;
            state.events_on = tc.events_on;
            list.push(Arc::new(Mutex::new(state)));
        }
    }

    /* ----------------------------- 事件日志 ----------------------------- */

    /// 注入事件默认目录（`app_config_dir`）并应用一次持久化配置
    pub fn configure_events(&self, base_dir: PathBuf) {
        *self.inner.events_base_dir.lock().unwrap() = Some(base_dir);
        self.refresh_events_persist();
    }

    /// 依据当前设置刷新事件存储（keep + 持久化开关 + 目录）
    fn refresh_events_persist(&self) {
        let (keep, persist, dir_opt) = {
            let s = self.inner.settings.read().unwrap();
            (s.events_keep, s.events_persist, s.events_dir.clone())
        };
        let base = self.inner.events_base_dir.lock().unwrap().clone();
        let dir = match dir_opt {
            Some(d) => Some(PathBuf::from(d)),
            None => base,
        };
        self.inner.events.set_keep(keep);
        self.inner.events.apply_persist(persist, dir);
    }

    /// 启动读回：从事件文件加载历史，按 host 归属到当前目标（历史标记为已读）
    pub fn load_events_from_file(&self) {
        let (persist, keep, dir_opt) = {
            let s = self.inner.settings.read().unwrap();
            (s.events_persist, s.events_keep, s.events_dir.clone())
        };
        if !persist {
            return;
        }
        let dir = match dir_opt {
            Some(d) => PathBuf::from(d),
            None => match self.inner.events_base_dir.lock().unwrap().clone() {
                Some(b) => b,
                None => return,
            },
        };
        let history = events::load_history(&dir);
        if history.is_empty() {
            return;
        }
        // host（trim + 小写）-> 当前 id
        let mut host_to_id: HashMap<String, u64> = HashMap::new();
        {
            let list = self.inner.targets.lock().unwrap();
            for t in list.iter() {
                let g = t.lock().unwrap();
                host_to_id.insert(events::normalize_host(&g.host), g.id);
            }
        }
        self.inner.events.load_hosts(history, &host_to_id, keep);
    }

    /// 读取某主机的最近 `limit` 条事件（新→旧）
    pub fn list_events(&self, target_id: u64, limit: usize) -> Vec<LogEvent> {
        self.inner.events.list_host(target_id, limit)
    }

    /// 清空某主机的事件（内存 + 文件重写）
    pub fn clear_events(&self, target_id: u64) {
        let host = self
            .find(target_id)
            .map(|t| t.lock().unwrap().host.clone());
        match host {
            Some(h) => self.inner.events.clear_host(target_id, &h),
            None => self.inner.events.clear_host(target_id, ""),
        }
    }

    /// 收集事件用于导出：`Some(id)` 只取该主机，`None` 取全部（按 seq 升序）
    pub fn collect_events(&self, target_id: Option<u64>) -> Vec<LogEvent> {
        match target_id {
            Some(id) => {
                let mut v = self.inner.events.list_host(id, usize::MAX);
                v.reverse(); // 转回升序，便于导出呈时间正序
                v
            }
            None => self.inner.events.all_events(),
        }
    }

    /// 设置单主机「记录事件」开关（仅改该字段，避免全量更新副作用）
    pub fn set_target_events(&self, id: u64, events_on: bool) -> Result<(), String> {
        let target = self
            .find(id)
            .ok_or_else(|| format!("目标 {} 不存在", id))?;
        target.lock().unwrap().events_on = events_on;
        Ok(())
    }

    /* ----------------------------- 目标管理 ----------------------------- */

    fn find(&self, id: u64) -> Option<SharedTarget> {
        let list = self.inner.targets.lock().unwrap();
        list.iter().find(|t| t.lock().unwrap().id == id).cloned()
    }

    /// 新增目标，返回新分配的 id 列表；若全局正在运行则自动开始 ping
    pub fn add_targets(&self, entries: Vec<TargetEntry>) -> Result<Vec<u64>, String> {
        let mut ids: Vec<u64> = Vec::new();
        {
            let mut list = self.inner.targets.lock().unwrap();
            for e in entries {
                let host = e.host.trim().to_string();
                if host.is_empty() {
                    continue;
                }
                let name_raw = e.name.trim();
                let name = if name_raw.is_empty() {
                    host.clone()
                } else {
                    name_raw.to_string()
                };
                let id = self.inner.next_id.fetch_add(1, Ordering::SeqCst);
                list.push(Arc::new(Mutex::new(TargetState::new(id, name, host))));
                ids.push(id);
            }
        }
        if ids.is_empty() {
            return Err("没有可添加的有效目标".to_string());
        }
        if self.inner.running.load(Ordering::SeqCst) {
            // 运行中新目标自动开始；若超过线程上限则忽略本次自动启动
            if let Err(e) = self.start(Some(ids.clone())) {
                eprintln!("[pingboard] 新目标自动启动被拒绝：{}", e);
            }
        }
        Ok(ids)
    }

    /// 删除目标（会先停止其工作线程）
    pub fn remove_targets(&self, ids: &[u64]) {
        self.stop(Some(ids.to_vec()), StopReason::Internal);
        let mut list = self.inner.targets.lock().unwrap();
        list.retain(|t| !ids.contains(&t.lock().unwrap().id));
    }

    /// 修改目标（备注名 / 主机名 / 启用状态）
    pub fn update_target(
        &self,
        id: u64,
        name: String,
        host: String,
        enabled: bool,
    ) -> Result<(), String> {
        let target = self.find(id).ok_or_else(|| format!("目标 {} 不存在", id))?;
        let new_host = host.trim().to_string();
        if new_host.is_empty() {
            return Err("主机名不能为空".to_string());
        }
        let name_trim = name.trim().to_string();
        let global_on = self.inner.settings.read().unwrap().events_on;
        {
            let mut g = target.lock().unwrap();
            let old_name = g.name.clone();
            let old_host = g.host.clone();
            let old_enabled = g.enabled;

            let new_name = if name_trim.is_empty() {
                new_host.clone()
            } else {
                name_trim
            };
            let host_changed = g.host != new_host;
            g.name = new_name;
            if host_changed {
                g.host = new_host;
                g.resolved_ip = None; // 主机名变更后需重新解析
                g.status = Status::Idle;
                g.last_error = None;
            }
            g.enabled = enabled;

            // 配置变更事件（详细级）：仅记录真正发生变化的字段
            if global_on && g.events_on {
                if old_name != g.name {
                    let text = format!("配置变更：备注名 -> {}", g.name);
                    self.inner
                        .events
                        .emit(EventKind::ConfigChange, text, id, &g.name, &g.host);
                }
                if old_host != g.host {
                    let text = format!("配置变更：主机名 -> {}", g.host);
                    self.inner
                        .events
                        .emit(EventKind::ConfigChange, text, id, &g.name, &g.host);
                }
                if old_enabled != g.enabled {
                    let text = format!(
                        "配置变更：启用状态 -> {}",
                        if g.enabled { "开启" } else { "关闭" }
                    );
                    self.inner
                        .events
                        .emit(EventKind::ConfigChange, text, id, &g.name, &g.host);
                }
            }
        }
        if !enabled {
            self.stop(Some(vec![id]), StopReason::Internal);
        } else if self.inner.running.load(Ordering::SeqCst) {
            let _ = self.start(Some(vec![id]));
        }
        Ok(())
    }

    /// 批量设置启用状态
    pub fn set_enabled(&self, ids: &[u64], enabled: bool) {
        for id in ids {
            if let Some(t) = self.find(*id) {
                t.lock().unwrap().enabled = enabled;
            }
        }
        if enabled {
            if self.inner.running.load(Ordering::SeqCst) {
                let _ = self.start(Some(ids.to_vec()));
            }
        } else {
            self.stop(Some(ids.to_vec()), StopReason::Internal);
        }
    }

    /* ----------------------------- 运行控制 ----------------------------- */

    /// 开始 Ping；`ids` 为 None 表示启动全部已启用目标
    pub fn start(&self, ids: Option<Vec<u64>>) -> Result<(), String> {
        let (max_threads, limit_max_threads) = {
            let s = self.inner.settings.read().unwrap();
            (s.max_threads, s.limit_max_threads)
        };

        // 选出候选目标
        let candidates: Vec<SharedTarget> = {
            let list = self.inner.targets.lock().unwrap();
            match &ids {
                Some(wanted) => list
                    .iter()
                    .filter(|t| wanted.contains(&t.lock().unwrap().id))
                    .cloned()
                    .collect(),
                None => list
                    .iter()
                    .filter(|t| t.lock().unwrap().enabled)
                    .cloned()
                    .collect(),
            }
        };

        // 过滤掉已在运行的
        let active: Vec<u64> = self.inner.workers.lock().unwrap().keys().cloned().collect();
        let to_start: Vec<SharedTarget> = candidates
            .into_iter()
            .filter(|t| !active.contains(&t.lock().unwrap().id))
            .collect();

        let planned = active.len() + to_start.len();
        if limit_max_threads {
            // 开启上限（默认）：维持原有行为与错误文案
            if planned > max_threads {
                return Err(format!(
                    "线程数将超过上限 {}（当前运行 {}，待启动 {}），请减少目标或提高「最大线程数」设置",
                    max_threads,
                    active.len(),
                    to_start.len()
                ));
            }
        } else {
            // 关闭上限：不再按 max_threads 拦截，仅保留 4096 硬保护
            if planned > HARD_MAX_THREADS {
                return Err(format!(
                    "并发线程数超过硬上限 {}（当前运行 {}，待启动 {}），请减少目标数量",
                    HARD_MAX_THREADS,
                    active.len(),
                    to_start.len()
                ));
            }
        }

        for t in to_start {
            spawn_worker(self.inner.clone(), t);
        }

        if !self.inner.workers.lock().unwrap().is_empty() {
            self.inner.running.store(true, Ordering::SeqCst);
            let mut started = self.inner.started_at.lock().unwrap();
            if started.is_none() {
                *started = Some(now_ms());
            }
        }
        Ok(())
    }

    /// 停止 Ping；`ids` 为 None 表示停止全部，`Some` 只停指定 id
    ///
    /// 三阶段实现：先置位、立即更新可见状态、后台回收线程。
    /// 命令本身**不 join 工作线程**——否则耗时 ≈ Σ(各线程退出时间)，N 台会累加到十几秒；
    /// 现在无论目标多少都在毫秒级返回，不再随目标数增长。
    pub fn stop(&self, ids: Option<Vec<u64>>, reason: StopReason) {
        let global_on = self.inner.settings.read().unwrap().events_on;

        // 1. 先把句柄从表中摘出（持锁时间极短，不在锁内 join）
        let handles: Vec<(u64, WorkerHandle)> = {
            let mut map = self.inner.workers.lock().unwrap();
            let keys: Vec<u64> = match &ids {
                Some(list) => list.clone(),
                None => map.keys().cloned().collect(),
            };
            let mut out = Vec::new();
            for k in keys {
                if let Some(h) = map.remove(&k) {
                    out.push((k, h));
                }
            }
            out
        };

        // 2. 置位停止标志，并**立即**把可见状态改为已停止（不等线程真正退出）
        //    仅「用户主动停止」才产生 stop 事件（标准级）。
        for (id, h) in &handles {
            h.stop.store(true, Ordering::SeqCst);
            if let Some(t) = self.find(*id) {
                let mut g = t.lock().unwrap();
                g.running = false;
                if reason == StopReason::User && global_on && g.events_on {
                    self.inner.events.emit(
                        EventKind::Stop,
                        "已停止".to_string(),
                        *id,
                        &g.name,
                        &g.host,
                    );
                }
            }
        }

        // 3. 原有全局 running / started_at 逻辑（基于本函数移除后的 workers 表）
        let all_stopped = self.inner.workers.lock().unwrap().is_empty();
        if ids.is_none() || all_stopped {
            self.inner.running.store(false, Ordering::SeqCst);
        }
        if ids.is_none() {
            *self.inner.started_at.lock().unwrap() = None;
        }

        // 4. 交给后台「回收线程」去 join，命令不阻塞在这上面
        if !handles.is_empty() {
            std::thread::Builder::new()
                .name("pingboard-reaper".to_string())
                .spawn(move || {
                    for (_, mut h) in handles {
                        if let Some(j) = h.join.take() {
                            let _ = j.join();
                        }
                    }
                })
                .ok();
        }
    }

    /// 重置统计；`ids` 为 None 表示重置全部
    pub fn reset_stats(&self, ids: Option<Vec<u64>>) {
        let list = self.inner.targets.lock().unwrap();
        for t in list.iter() {
            let mut g = t.lock().unwrap();
            let hit = match &ids {
                Some(v) => v.contains(&g.id),
                None => true,
            };
            if !hit {
                continue;
            }
            g.sent = 0;
            g.received = 0;
            g.failed = 0;
            g.last_rtt_ms = None;
            g.min_rtt_ms = None;
            g.max_rtt_ms = None;
            g.avg_rtt_ms = None;
            g.sum_rtt_ms = 0.0;
            g.ttl = None;
            g.consecutive_fail = 0;
            g.loss_pct = 0.0;
            g.last_success_ts = None;
            g.history.clear();
            g.last_error = None;
            g.status = Status::Idle;
        }
    }

    /* ----------------------------- 设置 ----------------------------- */

    /// 更新设置（归一化后写入；工作线程每轮循环读取，立即生效）
    pub fn update_settings(&self, mut settings: PingSettings) -> Result<(), String> {
        stats::normalize_settings(&mut settings);
        if settings.timeout_ms >= settings.interval_ms.saturating_mul(10) && settings.interval_ms > 0
        {
            // 仅做温和提示，不阻断
            eprintln!(
                "[pingboard] 提示：超时时间（{}ms）远大于探测间隔（{}ms）",
                settings.timeout_ms, settings.interval_ms
            );
        }
        *self.inner.settings.write().unwrap() = settings;
        // 事件目录 / 开关 / keep 可能变化：刷新持久化配置
        self.refresh_events_persist();
        Ok(())
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/* ----------------------------- 工作线程 ----------------------------- */

/// 启动一个目标的工作线程
fn spawn_worker(inner: Arc<Inner>, target: SharedTarget) {
    let id = { target.lock().unwrap().id };
    let global_on = inner.settings.read().unwrap().events_on;
    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = stop.clone();
    let inner_clone = inner.clone();
    let target_clone = target.clone();

    let join = std::thread::Builder::new()
        .name(format!("pingboard-worker-{}", id))
        .spawn(move || worker_loop(inner_clone, target_clone, stop_clone))
        .ok();

    inner.workers.lock().unwrap().insert(id, WorkerHandle { stop, join });
    {
        let mut g = target.lock().unwrap();
        g.running = true;
        // 每个 worker 启动记 1 条 start（标准级）
        if global_on && g.events_on {
            inner
                .events
                .emit(EventKind::Start, "开始探测".to_string(), id, &g.name, &g.host);
        }
    }
}

/// 判断给定 stop 标志是否仍是该目标「当前注册」的 worker。
///
/// 用于收敛 `running` 的写入：只有当前注册的 worker 才有权改该目标的 `running`，
/// 避免「旧 worker 覆盖新 worker 状态」——后台回收后旧 worker 可能还卡在 `probe()`
/// 里等满 timeout，此期间同 id 已被重新 spawn，旧 worker 退出时不得再写 `running`。
fn is_current_worker(inner: &Inner, id: u64, stop: &Arc<AtomicBool>) -> bool {
    let map = inner.workers.lock().unwrap();
    match map.get(&id) {
        Some(h) => Arc::ptr_eq(&h.stop, stop),
        // 已被 stop() 摘除，说明自己不是当前 worker（stop 已负责写 false）
        None => false,
    }
}

/// 工作线程主循环：解析 → 探测 → 更新统计 → 分片休眠
fn worker_loop(inner: Arc<Inner>, target: SharedTarget, stop: Arc<AtomicBool>) {
    let id = { target.lock().unwrap().id };
    let mut v4_engine: Option<Engine> = None;
    let mut v6_engine: Option<fallback::PingEngine> = None;

    while !stop.load(Ordering::SeqCst) {
        // 每轮读取最新设置（运行中修改立即生效）
        let (interval_ms, timeout_ms, payload_size, ttl, history_len, events_on) = {
            let s = inner.settings.read().unwrap();
            (
                s.interval_ms,
                s.timeout_ms,
                s.payload_size,
                s.ttl,
                s.history_len,
                s.events_on,
            )
        };

        let host = { target.lock().unwrap().host.clone() };
        if host.is_empty() {
            break;
        }

        // 本轮写入前的状态（上一次已完成结果），用于状态迁移打点。
        // 刻意在「置 Resolving 之前」读取：解析每轮都会把状态临时置为 Resolving，
        // 若以解析后的状态为 prev，持久性 DNS 失败会每轮重复产出 dns_fail，
        // 违反「连续失败不重复记」（设计稿 1.4 / 8-T02 验收）。
        let prev_status = { target.lock().unwrap().status };

        // ---------- 解析（带缓存） ----------
        let cached = { target.lock().unwrap().resolved_ip.clone() };
        let mut addr: Option<IpAddr> = cached.as_deref().and_then(|c| c.parse::<IpAddr>().ok());

        if addr.is_none() {
            {
                let mut g = target.lock().unwrap();
                g.status = Status::Resolving;
            }
            match resolve_host_blocking(&host) {
                Ok(a) => {
                    {
                        let mut g = target.lock().unwrap();
                        g.resolved_ip = Some(a.to_string());
                        g.last_error = None;
                    }
                    addr = Some(a);
                }
                Err(e) => {
                    // 解析可能阻塞数秒，期间若收到停止信号则丢弃本次失败统计
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    {
                        let mut g = target.lock().unwrap();
                        g.last_error = Some(e);
                        stats::apply_failure(&mut g, history_len, Status::Failed);
                        // 解析失败：resolving -> failed（来源 Resolve）
                        if events_on && g.events_on {
                            if let Some(kind) =
                                events::classify_transition(prev_status, Status::Failed, FailSource::Resolve)
                            {
                                let text = events::failure_text(
                                    kind,
                                    g.consecutive_fail,
                                    g.last_error.as_deref(),
                                );
                                inner.events.emit(kind, text, id, &g.name, &g.host);
                            }
                        }
                    }
                    sleep_interruptible(&stop, interval_ms);
                    continue;
                }
            }
        }

        let ip = match addr {
            Some(a) => a,
            None => {
                sleep_interruptible(&stop, interval_ms);
                continue;
            }
        };

        // ---------- 探测 ----------
        let outcome = if ip.is_ipv6() {
            if v6_engine.is_none() {
                v6_engine = Some(fallback::PingEngine::new());
            }
            v6_engine
                .as_mut()
                .unwrap()
                .probe(&ip.to_string(), timeout_ms, true)
        } else {
            if v4_engine.is_none() {
                v4_engine = Some(Engine::new(&inner.icmp_failed));
            }
            v4_engine
                .as_mut()
                .unwrap()
                .probe(&ip.to_string(), timeout_ms, payload_size, ttl)
        };

        // 期间若收到停止信号，丢弃本次探测结果直接退出，
        // 避免「停止后 sent 仍在增长」（后台回收期间旧线程仍可能写回统计）
        if stop.load(Ordering::SeqCst) {
            break;
        }

        // ---------- 更新统计 ----------
        let now = now_ms();
        // 仅当自己仍是该 id 当前注册的 worker 时才写 running，
        // 否则会与同 id 的新 worker 竞态（旧 worker 可能还卡在 probe 里等满 timeout）。
        let still_current = is_current_worker(&inner, id, &stop);
        {
            let mut g = target.lock().unwrap();
            match outcome.status {
                Status::Ok => {
                    let rtt = outcome.rtt_ms.unwrap_or(0.0);
                    stats::apply_success(&mut g, rtt, outcome.ttl, now, history_len);
                    // 成功：recover / first_ok（来源 Probe）
                    if events_on && g.events_on {
                        if let Some(kind) =
                            events::classify_transition(prev_status, Status::Ok, FailSource::Probe)
                        {
                            let text = events::success_text(kind, rtt);
                            inner.events.emit(kind, text, id, &g.name, &g.host);
                        }
                    }
                }
                Status::Timeout => {
                    stats::apply_failure(&mut g, history_len, Status::Timeout);
                    // 超时：fault / unreachable（来源 Probe）
                    if events_on && g.events_on {
                        if let Some(kind) = events::classify_transition(
                            prev_status,
                            Status::Timeout,
                            FailSource::Probe,
                        ) {
                            let text = events::failure_text(kind, g.consecutive_fail, None);
                            inner.events.emit(kind, text, id, &g.name, &g.host);
                        }
                    }
                }
                _ => {
                    if outcome.error.is_some() {
                        g.last_error = outcome.error.clone();
                    }
                    stats::apply_failure(&mut g, history_len, Status::Failed);
                    // 系统错误：fault / unreachable（来源 Probe）
                    if events_on && g.events_on {
                        if let Some(kind) =
                            events::classify_transition(prev_status, Status::Failed, FailSource::Probe)
                        {
                            let text = events::failure_text(
                                kind,
                                g.consecutive_fail,
                                g.last_error.as_deref(),
                            );
                            inner.events.emit(kind, text, id, &g.name, &g.host);
                        }
                    }
                }
            }
            if still_current {
                g.running = true;
            }
        }

        // ---------- 休眠（分片检查停止标志） ----------
        sleep_interruptible(&stop, interval_ms);
    }

    // 线程退出：只在「自己仍是该目标当前注册的 worker」时才清 running。
    // 否则会与同 id 的新 worker 竞态：旧 worker（可能还卡在 probe 里等满 timeout）
    // 退出时会把新 worker 刚写上的 running=true 覆盖回 false，导致 UI 短暂显示「已停止」。
    // （被 stop() 摘除的情况 map 查不到自己 → 不写，running=false 已由 stop() 负责。）
    if is_current_worker(&inner, id, &stop) {
        target.lock().unwrap().running = false;
    }
}

/// 可被停止标志打断的休眠：每 100ms 检查一次
fn sleep_interruptible(stop: &AtomicBool, total_ms: u64) {
    let mut remaining = total_ms;
    while remaining > 0 {
        if stop.load(Ordering::SeqCst) {
            return;
        }
        let chunk = remaining.min(100);
        std::thread::sleep(Duration::from_millis(chunk));
        remaining -= chunk;
    }
}

/* ----------------------------- 工具 ----------------------------- */

/// 解析主机名为 IP：优先 IPv4，其次 IPv6；失败返回中文错误信息
pub fn resolve_host_blocking(host: &str) -> Result<IpAddr, String> {
    let trimmed = host.trim();
    if let Ok(ip) = trimmed.parse::<IpAddr>() {
        return Ok(ip);
    }
    match (trimmed, 0u16).to_socket_addrs() {
        Ok(iter) => {
            let addrs: Vec<_> = iter.collect();
            if let Some(v4) = addrs.iter().find(|a| a.is_ipv4()) {
                return Ok(v4.ip());
            }
            addrs
                .first()
                .map(|a| a.ip())
                .ok_or_else(|| format!("无法解析主机名：{}", trimmed))
        }
        Err(e) => Err(format!("解析失败：{}", e)),
    }
}

/// 当前纪元毫秒
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 启动快照发射任务：每 500ms 主动 emit 一次聚合快照
pub fn spawn_snapshot_emitter(app: AppHandle) {
    std::thread::Builder::new()
        .name("pingboard-snapshot".to_string())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_millis(SNAPSHOT_INTERVAL_MS));
            let state = app.state::<AppState>();
            let snap = state.snapshot();
            if let Err(e) = app.emit(SNAPSHOT_EVENT, &snap) {
                eprintln!("[pingboard] 快照事件发送失败：{}", e);
            }
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AppConfig, PingSettings, TargetConfig, TargetEntry};
    use std::time::Duration;

    fn build_cfg(
        targets: Vec<(&str, &str)>,
        auto_start: bool,
        max_threads: usize,
        interval_ms: u64,
    ) -> AppConfig {
        AppConfig {
            version: 1,
            settings: PingSettings {
                interval_ms,
                timeout_ms: 1000,
                payload_size: 32,
                ttl: 128,
                max_threads,
                limit_max_threads: true,
                beep_on_fail: false,
                auto_start,
                history_len: 60,
                events_on: true,
                events_level: events::EventLevel::Standard,
                events_persist: false,
                events_dir: None,
                events_keep: 200,
            },
            targets: targets
                .into_iter()
                .map(|(n, h)| TargetConfig {
                    name: n.into(),
                    host: h.into(),
                    enabled: true,
                    events_on: true,
                })
                .collect(),
        }
    }

    /// 构造带指定目标数与上限开关的配置（用于并发开关测试）
    fn build_cfg_limited(
        target_count: usize,
        max_threads: usize,
        limit_max_threads: bool,
    ) -> AppConfig {
        let mut cfg = AppConfig {
            version: 1,
            settings: PingSettings {
                interval_ms: 1000,
                timeout_ms: 1000,
                payload_size: 32,
                ttl: 128,
                max_threads,
                limit_max_threads,
                beep_on_fail: false,
                auto_start: false,
                history_len: 60,
                events_on: true,
                events_level: events::EventLevel::Standard,
                events_persist: false,
                events_dir: None,
                events_keep: 200,
            },
            targets: Vec::new(),
        };
        for i in 0..target_count {
            // 全部使用回环地址，避免依赖外网
            cfg.targets.push(TargetConfig {
                name: format!("loop-{}", i),
                host: "127.0.0.1".to_string(),
                enabled: true,
                events_on: true,
            });
        }
        cfg
    }

    /* ===================== QA 新增：主机名解析 ===================== */

    #[test]
    fn qa_resolve_host_blocking_cases() {
        assert_eq!(
            resolve_host_blocking("127.0.0.1").unwrap().to_string(),
            "127.0.0.1"
        );
        assert_eq!(resolve_host_blocking("::1").unwrap().to_string(), "::1");
        let err = resolve_host_blocking("no-such-host-xyz.invalid").unwrap_err();
        eprintln!("[qa] 无效主机名错误信息：{}", err);
        assert!(!err.is_empty());

        match resolve_host_blocking("www.baidu.com") {
            Ok(ip) => {
                eprintln!("[qa] www.baidu.com -> {}", ip);
                assert!(ip.is_ipv4(), "应优先返回 IPv4");
            }
            Err(e) => eprintln!("[qa-skip] 域名解析失败（环境限制）：{}", e),
        }
    }

    /* ===================== QA 新增：并发上限 / 幂等 / 停止清理 ===================== */

    /// 超过 max_threads 必须返回可读错误，且不启动任何线程
    #[test]
    fn qa_max_threads_limit_returns_readable_error() {
        let st = AppState::new();
        st.init_from_config(&build_cfg(
            vec![("a", "127.0.0.1"), ("b", "127.0.0.2"), ("c", "127.0.0.3")],
            false,
            2,
            1000,
        ));
        let err = st.start(None).expect_err("超过线程上限应返回错误");
        eprintln!("[qa] 线程上限错误信息：{}", err);
        assert!(err.contains("上限"), "错误信息应包含「上限」：{}", err);
        assert_eq!(st.snapshot().active_threads, 0, "报错后不应残留线程");
    }

    /// 并发压力：50 个目标（回环 + 外网）运行 5 秒；重复 start 幂等；stop 后清理干净
    #[test]
    fn qa_concurrency_50_targets_idempotent_and_clean_stop() {
        let mut hosts: Vec<(&str, &str)> = Vec::new();
        for _ in 0..40 {
            hosts.push(("loop", "127.0.0.1"));
        }
        for _ in 0..10 {
            hosts.push(("dns", "223.5.5.5"));
        }
        let st = AppState::new();
        st.init_from_config(&build_cfg(hosts, false, 256, 500));

        st.start(None).unwrap();
        let n1 = st.snapshot().active_threads;
        st.start(None).unwrap(); // 幂等：不得新增线程
        st.start(None).unwrap();
        let n2 = st.snapshot().active_threads;
        assert_eq!(n1, 50, "首次 start 应启动 50 个线程，实际 {}", n1);
        assert_eq!(n2, 50, "重复 start 不得新增线程（幂等），实际 {}", n2);

        std::thread::sleep(Duration::from_millis(5000));

        let snap = st.snapshot();
        eprintln!(
            "[qa] 5s 后：running={} threads={} sent={} recv={} fail={}",
            snap.running, snap.active_threads, snap.total_sent, snap.total_received, snap.total_failed
        );
        assert!(snap.running);
        assert_eq!(snap.active_threads, 50);
        assert!(snap.total_sent >= 50, "5 秒内总发包应 >= 50，实际 {}", snap.total_sent);
        for t in &snap.targets {
            assert!(t.sent >= 1, "目标 {} 应至少发包 1 次", t.host);
            assert!(t.running, "目标 {} 应处于运行态", t.host);
        }

        st.stop(None, StopReason::Internal);
        let after = st.snapshot();
        assert_eq!(after.active_threads, 0, "stop 后线程应全部退出（无泄漏）");
        assert!(!after.running);
        for t in &after.targets {
            assert!(!t.running, "目标 {} 应被标记为停止", t.host);
        }
    }

    /// 空目标被拒绝；运行中新增目标会自动启动
    #[test]
    fn qa_add_targets_empty_rejected_and_autostart() {
        let st = AppState::new();
        st.init_from_config(&build_cfg(vec![("loop", "127.0.0.1")], false, 256, 500));

        assert!(
            st.add_targets(vec![TargetEntry {
                name: "".into(),
                host: "   ".into()
            }])
            .is_err(),
            "空主机名应被拒绝"
        );

        st.start(None).unwrap();
        assert_eq!(st.snapshot().active_threads, 1);

        let ids = st
            .add_targets(vec![TargetEntry {
                name: "dns".into(),
                host: "223.5.5.5".into(),
            }])
            .unwrap();
        assert_eq!(ids.len(), 1);
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(
            st.snapshot().active_threads,
            2,
            "运行中新增目标应自动启动新线程"
        );
        st.stop(None, StopReason::Internal);
        assert_eq!(st.snapshot().active_threads, 0);
    }

    /* ===================== 新增：并发上限开关 ===================== */

    /// 关闭「线程数上限」后：目标数超过 max_threads 但未超 4096 → 不报错且确实全部启动
    #[test]
    fn qa_limit_disabled_allows_exceeding_max_threads() {
        let st = AppState::new();
        st.init_from_config(&build_cfg_limited(3, 2, false));

        // 3 > max_threads(2)，但上限已关闭 → 应正常启动
        st.start(None).expect("关闭上限后不应因 max_threads 报错");
        let n = st.snapshot().active_threads;
        eprintln!("[qa] 关闭上限后活跃线程数：{}", n);
        assert_eq!(n, 3, "关闭上限后应启动全部 3 个线程");

        st.stop(None, StopReason::Internal);
        assert_eq!(st.snapshot().active_threads, 0, "stop 后应清理干净");
    }

    /// 开启上限（默认）时：同样超过 max_threads 必须报错且不启动
    #[test]
    fn qa_limit_enabled_still_blocks_before_start() {
        let st = AppState::new();
        st.init_from_config(&build_cfg_limited(3, 2, true));
        let err = st.start(None).expect_err("开启上限时应报错");
        assert!(err.contains("上限"), "错误信息应含「上限」：{}", err);
        assert_eq!(st.snapshot().active_threads, 0);
    }

    /// 关闭上限后仍保留 4096 硬保护：4097 个目标必须返回可读错误且不启动任何线程
    #[test]
    fn qa_limit_disabled_still_enforces_hard_4096() {
        let st = AppState::new();
        st.init_from_config(&build_cfg_limited(4097, 8, false));
        let err = st.start(None).expect_err("超过 4096 硬上限应报错");
        eprintln!("[qa] 硬上限错误信息：{}", err);
        assert!(err.contains("4096"), "错误信息应包含硬上限 4096：{}", err);
        assert_eq!(st.snapshot().active_threads, 0, "报错后不得启动线程");
    }

    /// 硬上限常量口径固定为 4096（前端文案与后端一致）
    #[test]
    fn qa_hard_max_threads_constant_is_4096() {
        assert_eq!(HARD_MAX_THREADS, 4096);
    }

    /* ===================== 新增：选中项单独启停 ===================== */

    /// 需求 5：start(ids) 只启动选中目标；stop(ids) 只停选中项，不影响其它运行中的目标
    #[test]
    fn qa_selective_start_and_stop_by_ids() {
        let st = AppState::new();
        st.init_from_config(&build_cfg_limited(3, 256, true));
        let ids: Vec<u64> = st.snapshot().targets.iter().map(|t| t.id).collect();
        assert_eq!(ids.len(), 3);

        // 只启动前两个
        st.start(Some(vec![ids[0], ids[1]])).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        let snap = st.snapshot();
        assert_eq!(snap.active_threads, 2, "只应启动选中的 2 个目标");
        let running: Vec<u64> = snap.targets.iter().filter(|t| t.running).map(|t| t.id).collect();
        assert!(running.contains(&ids[0]) && running.contains(&ids[1]), "选中的目标应运行");
        assert!(!running.contains(&ids[2]), "未选中的目标不应运行");

        // 只停第 1 个，未选中的第 2 个应继续运行
        st.stop(Some(vec![ids[0]]), StopReason::Internal);
        std::thread::sleep(Duration::from_millis(200));
        let snap2 = st.snapshot();
        assert_eq!(snap2.active_threads, 1, "只停 1 个后应剩 1 个线程");
        let t1 = snap2.targets.iter().find(|t| t.id == ids[0]).unwrap();
        let t2 = snap2.targets.iter().find(|t| t.id == ids[1]).unwrap();
        assert!(!t1.running, "被选停的目标应停止");
        assert!(t2.running, "未被选停的目标必须继续运行");

        st.stop(None, StopReason::Internal);
        assert_eq!(st.snapshot().active_threads, 0);
    }

    /* ============== QA v1.1.0 追加：并发开关真实行为 + 选中启停强化 ============== */

    /// C1：limit=false + max_threads=2 + 3 回环目标 → 不报错，且 3 个都“真的开始”
    /// 用 sent 计数增长证明（而不是只看 start 返回值）
    #[test]
    fn qa_v11_limit_disabled_three_targets_actually_send() {
        let st = AppState::new();
        st.init_from_config(&build_cfg_limited(3, 2, false));
        st.start(None).expect("关闭上限后不应因 max_threads 报错");
        std::thread::sleep(Duration::from_millis(2500));
        let snap = st.snapshot();
        eprintln!(
            "[qa] limit=false: threads={} sent={}",
            snap.active_threads, snap.total_sent
        );
        assert_eq!(snap.active_threads, 3, "3 个线程都应启动");
        for t in &snap.targets {
            assert!(t.sent >= 1, "目标 {} 应已发包（sent={}）", t.host, t.sent);
            assert!(t.running, "目标 {} 应处于运行态", t.host);
        }
        st.stop(None, StopReason::Internal);
        assert_eq!(st.snapshot().active_threads, 0, "停止后无残留线程");
    }

    /// C2：limit=true（默认）+ max_threads=2 + 3 目标 → 可读中文错误，且「没有任何线程启动」
    /// 等待后确认 active_threads 恒为 0 且 sent 不增长
    #[test]
    fn qa_v11_limit_enabled_blocks_and_no_thread_started() {
        let st = AppState::new();
        st.init_from_config(&build_cfg_limited(3, 2, true));
        let err = st.start(None).expect_err("超过上限应报错");
        eprintln!("[qa] limit=true 错误信息：{}", err);
        assert!(err.contains("上限"), "错误信息应含「上限」：{}", err);
        std::thread::sleep(Duration::from_millis(1200));
        let snap = st.snapshot();
        assert_eq!(snap.active_threads, 0, "报错后不得启动任何线程");
        for t in &snap.targets {
            assert!(!t.running, "目标 {} 不应处于运行态", t.host);
            assert_eq!(t.sent, 0, "目标 {} 不应发包（sent={}）", t.host, t.sent);
        }
    }

    /// C3：4096 硬保护（limit=false）—— 4097 目标必须返回含 4096 的可读错误，不启动线程
    /// 说明：不真实创建 4097 个线程，只触发 state.start 的拦截分支（构造 4097 个回环目标配置）
    #[test]
    fn qa_v11_hard_4096_rejects_4097_without_spawning() {
        let st = AppState::new();
        st.init_from_config(&build_cfg_limited(4097, 8, false));
        let err = st.start(None).expect_err("超过 4096 硬上限应报错");
        eprintln!("[qa] 硬上限错误信息：{}", err);
        assert!(err.contains("4096"), "错误信息应含硬上限 4096：{}", err);
        assert_eq!(st.snapshot().active_threads, 0, "报错后不得启动线程");
        // 恰好 4096 的边界：不报 max_threads 相关错（但会真的启动 4096 线程，风险高，故不实际启动）
        assert_eq!(HARD_MAX_THREADS, 4096);
    }

    /// D5 强化：起 4 个目标全跑 → stop(ids[0]) → 仅 id0 停，其余 3 个仍在 run 且 sent 继续增长
    #[test]
    fn qa_v11_stop_one_of_four_keeps_others_sending() {
        let st = AppState::new();
        st.init_from_config(&build_cfg_limited(4, 256, true));
        let ids: Vec<u64> = st.snapshot().targets.iter().map(|t| t.id).collect();
        assert_eq!(ids.len(), 4);

        st.start(None).unwrap();
        std::thread::sleep(Duration::from_millis(2200));
        assert_eq!(st.snapshot().active_threads, 4, "4 个目标应全部运行");

        st.stop(Some(vec![ids[0]]), StopReason::Internal);
        // stop 现在「先置位、立即返回、后台回收」；(b) 的停止检查保证旧线程不再写回统计，
        // 故此处取到的 sent 已稳定
        let after_stop = st.snapshot();
        let s0 = after_stop.targets.iter().find(|t| t.id == ids[0]).unwrap().sent;
        let others_before: Vec<u64> = ids[1..]
            .iter()
            .map(|id| after_stop.targets.iter().find(|t| t.id == *id).unwrap().sent)
            .collect();
        assert_eq!(after_stop.active_threads, 3, "只停 1 个后应剩 3 个线程");

        std::thread::sleep(Duration::from_millis(2200));
        let after = st.snapshot();
        let t0 = after.targets.iter().find(|t| t.id == ids[0]).unwrap();
        assert!(!t0.running, "被选停目标应停止");
        assert_eq!(t0.sent, s0, "被选停目标 sent 不应再增长（{} → {}）", s0, t0.sent);
        for (i, id) in ids[1..].iter().enumerate() {
            let t = after.targets.iter().find(|t| t.id == *id).unwrap();
            assert!(t.running, "其余目标 {} 应继续运行", id);
            assert!(
                t.sent > others_before[i],
                "其余目标 {} 的 sent 应继续增长（{} → {}）",
                id,
                others_before[i],
                t.sent
            );
        }
        eprintln!("[qa] stop-one: active={} total_sent={}", after.active_threads, after.total_sent);
        st.stop(None, StopReason::Internal);
        assert_eq!(st.snapshot().active_threads, 0);
    }

    /* ============== v1.1.2 追加：stop 性能（不再随目标数增长） ============== */

    /// 100 个回环目标时 stop(None) 必须立即返回：
    /// 修复前 ≈ Σ(各线程退出时间)，100 台接近 10 秒；修复后应 < 300ms。
    #[test]
    fn qa_v112_stop_returns_fast_with_many_targets() {
        let st = AppState::new();
        st.init_from_config(&build_cfg_limited(100, 256, true));
        st.start(None).expect("启动 100 个回环目标应成功");
        std::thread::sleep(Duration::from_millis(300));

        let before = st.snapshot();
        eprintln!("[qa-v112] stop 前：threads={} sent={}", before.active_threads, before.total_sent);

        let t0 = std::time::Instant::now();
        st.stop(None, StopReason::Internal);
        let elapsed = t0.elapsed();
        eprintln!("[qa-v112] stop(None) 100 目标耗时：{} ms", elapsed.as_millis());
        assert!(
            elapsed < Duration::from_millis(300),
            "stop 应立即返回（< 300ms），实际 {} ms",
            elapsed.as_millis()
        );

        // 可见状态应立即变为已停止
        let snap = st.snapshot();
        assert!(!snap.running, "stop(None) 后 running 应为 false");
        assert_eq!(snap.active_threads, 0, "stop(None) 后 workers 表应已清空");
        for t in &snap.targets {
            assert!(!t.running, "目标 {} 应被标记为已停止", t.host);
        }

        // 再等 300ms，后台回收期间 sent 不应再增长（验证 (b) 丢弃迟到结果）
        let sent_at_stop: u64 = snap.total_sent;
        std::thread::sleep(Duration::from_millis(300));
        let sent_later = st.snapshot().total_sent;
        eprintln!("[qa-v112] stop 后 total_sent：{} → {}", sent_at_stop, sent_later);
        assert_eq!(sent_later, sent_at_stop, "stop 后 sent 不应再增长");
    }

    /* ============== v1.1.3 追加：stop→立即 start 的旧 worker 竞态回归 ============== */

    /// 回归：反复「stop → 立即 start」后，旧 worker 退出不得把同 id 新 worker 的
    /// `running=true` 覆盖回 false（修复前旧 worker 在 probe/休眠结束后无条件写 false，
    /// 会在约 1s 内造成「enabled 且 running=false」的假象 —— QA 用 100 目标复现过）。
    ///
    /// interval_ms=1000：新 worker 在 spawn 时即写 running=true 并每轮探测再写一次，
    /// 旧 worker 则在被停后 ≤100ms（分片边界）退出并（旧代码）写 false，
    /// 于是 start 后约 100ms~1000ms 窗口内可稳定采样到 running=false。
    #[test]
    fn qa_v113_restart_keeps_running_true_no_stale_worker_race() {
        let st = AppState::new();
        st.init_from_config(&build_cfg_limited(4, 256, true));
        let ids: Vec<u64> = st.snapshot().targets.iter().map(|t| t.id).collect();
        assert_eq!(ids.len(), 4);

        for round in 0..8 {
            st.start(None).unwrap();
            std::thread::sleep(Duration::from_millis(50)); // 让 worker 进入 probe/休眠
            // 停全部后立即重新启动全部：旧 worker 与新 worker 会短暂重叠
            st.stop(None, StopReason::Internal);
            st.start(None).unwrap();

            // 以 60ms 粒度采样，任一时刻都不应出现「enabled 且 running=false」
            let mut worst = 0usize;
            for _ in 0..12 {
                std::thread::sleep(Duration::from_millis(60));
                let snap = st.snapshot();
                let bad = snap
                    .targets
                    .iter()
                    .filter(|t| t.enabled && !t.running)
                    .count();
                if bad > worst {
                    worst = bad;
                }
            }
            eprintln!("[qa-v113] round {} 最坏「enabled 且 running=false」= {}", round, worst);
            assert_eq!(
                worst, 0,
                "第 {} 轮：stop→立即 start 后不应出现「enabled 且 running=false」（旧 worker 覆盖新 worker 的竞态）",
                round
            );
        }

        st.stop(None, StopReason::Internal);
    }

    /* ===================== v1.2.0 新增：事件日志端到端 ===================== */

    /// 配置变更：改备注名应产生 config_change（详细级），且文本带新值
    #[test]
    fn qa_events_config_change_recorded() {
        let st = AppState::new();
        st.init_from_config(&build_cfg(vec![("orig", "127.0.0.1")], false, 256, 200));
        let id = st.snapshot().targets[0].id;

        st.update_target(id, "新备注".to_string(), "127.0.0.1".to_string(), true)
            .unwrap();
        let snap = st.snapshot();
        assert!(
            snap.events
                .iter()
                .any(|e| e.kind == EventKind::ConfigChange && e.text.contains("新备注")),
            "改备注名应产生含新值的 config_change：{:?}",
            snap.events
        );
        // 无变化时不应产生事件
        st.update_target(id, "新备注".to_string(), "127.0.0.1".to_string(), true)
            .unwrap();
        let snap2 = st.snapshot();
        assert!(
            !snap2.events.iter().any(|e| e.kind == EventKind::ConfigChange),
            "无变化不应产生 config_change"
        );
    }

    /// 启动即失败：真实探测保留网段 192.0.2.1 → 记 unreachable
    #[test]
    fn qa_events_unreachable_on_startup_failure() {
        let st = AppState::new();
        st.init_from_config(&build_cfg(vec![("dead", "192.0.2.1")], false, 256, 200));
        st.start(None).unwrap();
        std::thread::sleep(Duration::from_millis(2600));
        let snap = st.snapshot();
        eprintln!(
            "[qa-events] 启动即失败事件：{:?}",
            snap.events.iter().map(|e| (e.kind, e.text.clone())).collect::<Vec<_>>()
        );
        assert!(
            snap.events.iter().any(|e| e.kind == EventKind::Unreachable),
            "启动即失败应记录 unreachable：{:?}",
            snap.events
        );
        st.stop(None, StopReason::Internal);
    }

    /// DNS 失败：不存在的域名 → dns_fail；且连续失败不重复记；用户停止 → stop
    #[test]
    fn qa_events_dns_fail_dedup_and_user_stop() {
        let st = AppState::new();
        st.init_from_config(&build_cfg(
            vec![("bad", "no-such-host-xyz.invalid")],
            false,
            256,
            200,
        ));
        st.start(None).unwrap();

        // 第 1 段：应至少记录 1 条 dns_fail（同时含 1 条 start）
        std::thread::sleep(Duration::from_millis(1500));
        let snap1 = st.snapshot();
        let dns1 = snap1
            .events
            .iter()
            .filter(|e| e.kind == EventKind::DnsFail)
            .count();
        eprintln!("[qa-events] DNS 第 1 段事件：{:?}", snap1.events);
        assert!(dns1 >= 1, "DNS 失败应记录 dns_fail");
        assert!(
            snap1.events.iter().any(|e| e.kind == EventKind::Start),
            "worker 启动应记录 start"
        );

        // 第 2 段：持续 DNS 失败不应重复记录（去重）
        std::thread::sleep(Duration::from_millis(1200));
        let snap2 = st.snapshot();
        let dns2 = snap2
            .events
            .iter()
            .filter(|e| e.kind == EventKind::DnsFail)
            .count();
        assert_eq!(dns2, 0, "连续 DNS 失败不应重复记录：{:?}", snap2.events);

        // 用户停止 → 1 条 stop
        st.stop(None, StopReason::User);
        let snap3 = st.snapshot();
        let stops = snap3
            .events
            .iter()
            .filter(|e| e.kind == EventKind::Stop)
            .count();
        assert_eq!(stops, 1, "用户停止应记录恰好 1 条 stop：{:?}", snap3.events);
    }

    /// 单主机开关关闭时不产生事件；set_target_events 状态可读回
    #[test]
    fn qa_events_per_target_switch_and_persist_flag() {
        let st = AppState::new();
        st.init_from_config(&build_cfg(vec![("bad", "no-such-host-xyz.invalid")], false, 256, 200));
        let id = st.snapshot().targets[0].id;

        st.set_target_events(id, false).unwrap();
        assert!(!st.snapshot().targets[0].events_on, "单主机开关应被写入");
        assert!(st.set_target_events(9999, true).is_err(), "不存在的主机应报错");

        // 关闭后启动探测：不应产生任何事件
        st.start(None).unwrap();
        std::thread::sleep(Duration::from_millis(1200));
        let snap = st.snapshot();
        assert!(
            snap.events.is_empty(),
            "单主机开关关闭时不应产生事件：{:?}",
            snap.events
        );
        st.stop(None, StopReason::Internal);
    }
}
