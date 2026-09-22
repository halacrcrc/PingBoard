# PingBoard 增量设计：事件日志（等级化 / 持久化 / 单主机开关）

> 架构设计稿（架构师 高见远 产出，主理人 齐活林 汇编并追加决策）。**实现以本文为准。**
> 工作区 `D:/pinginfo`（构建工作区）。改动只做在这里，源码由主理人同步到 git 仓库。

---

## 0. 需求与根因

### 0.1 用户可见问题
右侧详情面板「最近日志（失败 / 恢复事件）」**几乎永远为空**（加主机、暂停、开始都没内容）。

### 0.2 根因（已核实）
事件原本在**前端**用前后两次快照 diff 生成（`src/App.tsx:159-207` 旧代码），规则写死为：

- `ok -> (timeout|failed)` ⇒ 记「失败」
- `(timeout|failed) -> ok` ⇒ 记「恢复」

而后端真实状态迁移是 `idle -> resolving -> ok/timeout/failed`（`state.rs:494` 置 `Resolving`；`:513`、`:569-577` 置失败）。因此：

| 场景 | 真实迁移 | 旧规则 | 结果 |
|---|---|---|---|
| 启动时就连不上 | `idle/resolving -> timeout/failed` | 要求 `prev == ok` | **漏记** |
| DNS 解析失败 | `resolving -> failed` | 同上；且每轮重试使相邻快照同为 `failed`（`prev == next` 直接跳过） | **漏记** |
| 暂停 / 开始 | `idle <-> ok` | 不属于 fail/recover | 不记（与「日志」预期不符） |

另有半成品证据：`src/types.ts:74` 定义了 `kind: "info"`，`DetailPanel.tsx:133` 为它写了灰色样式，但**全项目从无一处产出 `info` 事件**。

### 0.3 本期范围（用户逐条确认）
1. 修复事件漏记 —— 事件生成从「前端 diff 快照」迁到「后端状态迁移点打点」
2. 三档事件等级 + 全局总开关
3. 事件持久化到文件，**默认开启**；支持用系统文件夹选择对话框自定义目录
4. 详情面板每主机独立的「记录事件」开关
5. 日志区增强：标题显示条数、未读故障小红点、复制 / 导出 CSV / 清空
6. 每主机保留条数可配：50 / 200 / 1000

---

## 1. 事件模型

### 1.1 Rust 结构（新增 `src-tauri/src/events.rs`，`model.rs` 扩展）

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Fault,        // 故障：ok -> timeout/failed
    Unreachable,  // 无法连通：idle/resolving -> timeout/failed（从未成功）
    DnsFail,      // 解析失败
    Recover,      // 恢复：失败 -> ok
    FirstOk,      // 首次连通：idle/resolving -> ok
    Start,        // 开始探测
    Stop,         // 已停止
    ConfigChange, // 配置变更
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventLevel { Fault, Standard, Detail }

impl EventLevel {
    pub fn rank(self) -> u8 { match self { EventLevel::Fault => 0, EventLevel::Standard => 1, EventLevel::Detail => 2 } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEvent {
    pub seq: u64,            // 会话内单调自增，用于快照去重/排序
    pub ts: u64,             // 纪元毫秒
    pub target_id: u64,      // 运行时 id（实时路由）
    pub target_name: String, // 冗余：导出/重启后展示自足
    pub target_host: String, // 冗余：重启后按 host 归属（id 会重排）
    pub kind: EventKind,
    pub level: EventLevel,   // 冗余存，见 1.3
    pub text: String,        // 已本地化文案
}
```

### 1.2 事件类型 -> 等级映射（过滤规则：`rank(level) <= rank(当前等级)`）

| kind | 中文文案模板 | 触发迁移 | 等级 |
|---|---|---|---|
| `fault` | `探测失败：{原因}` / `探测超时（连续 N 次）` | ok -> timeout/failed | 仅故障 |
| `unreachable` | `无法连通：{原因}` | idle/resolving -> timeout/failed | 仅故障 |
| `dns_fail` | `解析失败：{错误}` | resolving -> failed（解析源） | 仅故障 |
| `recover` | `已恢复（{rtt} ms）` | timeout/failed -> ok | 仅故障 |
| `first_ok` | `首次连通（{rtt} ms）` | idle/resolving -> ok | 标准 |
| `start` | `开始探测` | worker 启动 | 标准 |
| `stop` | `已停止` | 用户主动停止 | 标准 |
| `config_change` | `配置变更：{字段} -> {新值}` | 备注名 / 主机名 / 启用状态被改 | 详细 |

三档集合：
- **仅故障** = {fault, unreachable, dns_fail, recover}
- **标准（默认）** = 上述 + {first_ok, start, stop}
- **详细** = 上述 + {config_change}

### 1.3 冗余存 `level`：**要存**
- 每事件固定归属唯一等级，存下来让 JSONL 自描述，跨版本/口径变化仍可正确显示。
- 读回时若旧行缺 `level`，由 `EventLevel::for_kind(kind)` 常量表推导。

### 1.4 去重是内建的（根治「每轮重复记」）
**只在「从非失败态进入失败态」时产生失败事件；`timeout/failed -> timeout/failed` 一律不产生。**
把边沿触发写进纯函数：

```rust
pub enum FailSource { Resolve, Probe }

/// prev = 写入前状态, next = 写入后状态
pub fn classify_transition(prev: Status, next: Status, src: FailSource) -> Option<EventKind> {
    use Status::*;
    match (prev, next) {
        (_, Ok) if prev == Timeout || prev == Failed => Some(EventKind::Recover),
        (_, Ok) if prev == Idle || prev == Resolving => Some(EventKind::FirstOk),
        (Ok, Timeout) | (Ok, Failed) => Some(EventKind::Fault),
        (Idle, Timeout) | (Idle, Failed) | (Resolving, Timeout) | (Resolving, Failed) => {
            Some(if matches!(src, FailSource::Resolve) { EventKind::DnsFail } else { EventKind::Unreachable })
        }
        (Timeout, Timeout) | (Timeout, Failed) | (Failed, Timeout) | (Failed, Failed) => None, // 去重
        _ => None,
    }
}
```

### 1.5 TS 侧对称类型（`src/types.ts`）

```ts
export type EventKind = "fault" | "unreachable" | "dns_fail" | "recover" | "first_ok" | "start" | "stop" | "config_change";
export type EventLevel = "fault" | "standard" | "detail";

export interface LogEvent {
  seq: number;
  ts: number;
  target_id: number;
  target_name: string;
  target_host: string;
  kind: EventKind;
  level: EventLevel;
  text: string;
}
```

> 旧 `LogEntry`（`kind: "fail" | "recover" | "info"`）**整体废弃并删除**，一次迁完不留半改。

---

## 2. 打点位置（Rust，含行号）

| # | 位置 | 行号 | 打点 | 产出 |
|---|---|---|---|---|
| A | `worker_loop` 成功写回 `apply_success` | `state.rs:559-568` | 写入前 `let prev = g.status;`，写入后 `classify_transition(prev, Ok, Probe)` | `recover` / `first_ok` |
| B | `worker_loop` 超时分支 | `state.rs:569-571` | `classify_transition(prev, Timeout, Probe)` | `fault` / `unreachable` |
| C | `worker_loop` 系统错误分支 | `state.rs:572-577` | `classify_transition(prev, Failed, Probe)` | `fault` / `unreachable` |
| D | `worker_loop` DNS Err 分支 | `state.rs:505-517`（写点 `:513`） | `classify_transition(prev, Failed, Resolve)` | `dns_fail` |
| E | `spawn_worker`（唯一 spawn 汇聚点） | `state.rs:433-447` | `running` 置 true 后 emit | `start` |
| F | `AppState::stop` | `state.rs:327`（置位 `:345-350`） | **仅 reason == User** 时对每个被停目标 emit | `stop` |
| G | `AppState::update_target` | `state.rs:199-234` | 检测 name/host/enabled 与原值差异后 emit | `config_change` |

打点统一收在 `EventStore::emit(kind, text, id, name, host)`；worker 线程通过已持有的 `Arc<Inner>` 访问 `inner.events`，不需要 `AppHandle`。

### 2.1 「已停止」如何区分「用户主动停止」与「程序退出」

| 触发 | 是否调用 `stop()` | 是否产生 `stop` 事件 |
|---|---|---|
| 用户点「停止全部 / 停止选中」 | 是（reason = **User**） | **是** |
| 程序退出（`lib.rs:55-63` `WindowEvent::CloseRequested`） | **否**（仅 `save_config`，线程随进程结束） | **否**（期望行为） |
| 删除目标 / 关闭监控 / `set_enabled(false)` | 是（reason = **Internal**） | **否**（改由 `config_change` 表达） |

做法：`stop(&self, ids, reason: StopReason)` 增加参数（`User | Internal`），仅 `User` 打点。**同步改 4 处调用点**（state 内部 3 处 + `commands.rs:78`）。

---

## 3. 传输：`Snapshot.events`

**选型：增量（自上次快照以来的新事件）+ 单调 `seq` + 选中主机懒回填。**

| 方案 | 241 台 payload | 结论 |
|---|---|---|
| A 增量 + seq | 空闲 `[]`；仅状态迁移时非空 | **采用** |
| B 每主机最近 N 条全量 | 241 x 20 x ~150B ≈ **700KB / 500ms** | 否决 |
| C 全量 + 前端 diff | 更大，且旧方案已有根因 | 否决 |

精确语义：
- `EventStore.pending: Mutex<Vec<LogEvent>>`；`emit()` 时 `seq = AtomicU64++`，推入 pending + per_host 环。
- 快照发射器（`state.rs:642`，每 500ms）调 `drain_pending()` 取走并清空 -> 塞进 `Snapshot.events`。
- `Snapshot` 新增 `events: Vec<LogEvent>`，始终序列化，空为 `[]`。TS `Snapshot` 与 `App.tsx` 的 `EMPTY_SNAPSHOT` 同步补 `events: []`。
- 前端用 `seq` 严格去重（`e.seq <= lastSeq` 丢弃）。
- 切换选中主机：若该 id 尚未加载 -> `list_events(id, keep)` 回填。
- 快照丢失时实时增量会漏该 tick，但 (a) 选中时从 per_host 环补齐；(b) 文件是最终事实源，重启回读。
- 241 台开销：现有每 500ms 已全量序列化 241 个 `TargetState`（主要开销），新增多为空数组，**可忽略**。

---

## 4. 持久化

| 项 | 决策 |
|---|---|
| 格式 | **JSONL**（一行一个 `LogEvent` JSON）。追加 O(1)、尾部残行读时丢弃 |
| 文件名 | `pingboard-events.jsonl` |
| 默认目录 | `app_config_dir()`（`%APPDATA%\com.pingboard.desktop\`，与 `pingboard-config.json` 同级） |
| 自定义目录 | `events_dir`（`Option<String>`） |

- **追加**：`OpenOptions::new().create(true).append(true)`，整行 `write_all(line + "\n")`。
- **写入线程**：`EventStore` 持 `mpsc::Sender`，由独立线程 `pingboard-events-writer` 串行写；目录/开关变更发控制消息重开句柄。**IO 不阻塞 worker 探测循环。**
- **容量控制**：内存 per_host 环按 `events_keep` 截断；文件按大小轮转 —— `> EVENTS_FILE_MAX_BYTES`（建议 **8 MiB**）时改名 `pingboard-events.1.jsonl`（覆盖旧归档）后新建空文件，稳态最多 2 个文件。
- **启动读回**：只读文件末尾最大 `EVENTS_FILE_TAIL_BYTES`（建议 **4 MiB**）并从首个换行后开始解析 -> 逐行 `serde_json::from_str::<LogEvent>`，坏行跳过 -> 按 `target_host`（小写 trim）分组，每组保留最近 `events_keep` 条 -> 与当前 targets **按 host 匹配**建立 id 映射（重启后 id 重排，不可靠），匹配不到的当孤儿丢弃 -> 历史**标记为已读**（不计未读红点）。
- **写入失败绝不影响主流程**：所有文件操作包裹，错误 `eprintln!` 限频一次 + 置运行期 `persist_error`，**绝不 `?` 上传**到命令或 worker。

---

## 5. 配置与数据结构

### 5.1 `PingSettings` 新增字段
（`model.rs:91-110`，已有容器级 `#[serde(default)]`，**仍按项目硬约定逐个加字段级默认**）

| 字段 | 类型 | serde | 默认 |
|---|---|---|---|
| `events_on` | `bool` | `#[serde(default = "default_true")]` | true |
| `events_level` | `EventLevel` | `#[serde(default = "default_event_level")]` + **自定义 deserialize** | Standard |
| `events_persist` | `bool` | `#[serde(default = "default_true")]` | true |
| `events_dir` | `Option<String>` | `#[serde(default)]` | None |
| `events_keep` | `usize` | `#[serde(default = "default_log_keep")]` | **200** |

`PingSettings::default()` 补这 5 个字段。

**`normalize_settings`（`stats.rs:71`）新增规则**：
```rust
s.events_keep = match s.events_keep { 50 | 200 | 1000 => s.events_keep, _ => 200 };
s.events_dir = s.events_dir.as_ref().map(|d| d.trim().to_string()).filter(|d| !d.is_empty());
// 不校验路径存在性；不可写时运行期降级
```

### 5.2 单主机开关
| 结构 | 新增 | serde |
|---|---|---|
| `TargetConfig`（`model.rs:149`） | `events_on: bool` | `#[serde(default = "default_true")]` |
| `TargetState`（`model.rs:27`） | `events_on: bool` | 无（运行时），`new()` 里设 `true` |

`config()`（`state.rs:103`）映射带上 `events_on`；`init_from_config`（`:132`）从 `tc.events_on` 回填。排查所有 `TargetState` 结构体字面量构造点（含 `tests/`）同步补字段。

### 5.3 旧配置兼容性（逐字段）
`AppConfig` 无容器默认（`version` + `settings` 必填，老配置都有）；`targets` 有字段默认；`PingSettings` 有容器默认 + 新增字段级默认；`TargetConfig.events_on` 有字段默认。

| 来源配置 | 读入结果 |
|---|---|
| v1.0.0（无 `limit_max_threads`、无 events_*） | `limit_max_threads=true`；`events_on=true`；`events_level=standard`；`events_persist=true`；`events_dir=null`；`events_keep=200`；各 target `events_on=true`。**既有字段与主机列表逐字保留** |
| v1.1.2（有 `limit_max_threads`） | 同上，且 `limit_max_threads` 保留原值 |
| 新版全字段 | 原样往返 |

**结论：不会整份回落，主机列表安全。**

### 5.4 `events_dir` 用 `Option<String>`
语义显式（JSON `null` / TS `string | null`）；空串归一为 `None`。
降级：重开句柄时 `create_dir_all` + append open，失败 -> 句柄置 None + 置 `persist_error` + 一次性提示，**静默禁用持久化但不回写默认目录**（尊重用户选择）；内存事件与 UI 不受影响。

---

## 6. 命令与前端 API

| 命令 | 签名 | 说明 |
|---|---|---|
| `list_events` | `(state, target_id: u64, limit: usize) -> Result<Vec<LogEvent>, String>` | 选中主机回填（读 per_host 环） |
| `clear_events` | `(state, target_id: u64) -> Result<(), String>` | 清空该主机（内存 + 重写文件） |
| `export_events` | `(state, path: String, target_id: Option<u64>, tz_offset_minutes: i64) -> Result<(), String>` | 导出事件 CSV |
| `set_target_events` | `(app, state, id: u64, events_on: bool) -> Result<(), String>` | 单主机开关 + `save_config` |

- 命令注册 `lib.rs:18-32`：**13 -> 17**（新增 4 个，全部注册）。
- 单主机开关**走新命令 `set_target_events`**，不复用 `update_target`（后者是全量更新且带副作用，仅为日志开关传全量易被陈旧 name/host 覆盖）。

`src/lib/api.ts` 新增（多词参数走 Tauri 默认 camelCase）：
```ts
export const listEvents      = (targetId: number, limit: number) => invoke<LogEvent[]>("list_events", { targetId, limit });
export const clearEvents     = (targetId: number) => invoke<void>("clear_events", { targetId });
export const exportEvents    = (path: string, targetId: number | null, tzOffsetMinutes: number) =>
  invoke<void>("export_events", { path, targetId, tzOffsetMinutes });
export const setTargetEvents = (id: number, eventsOn: boolean) => invoke<void>("set_target_events", { id, eventsOn });
```

**导出 CSV**：新增 `export::to_events_csv`，**复用 `esc_csv`（`export.rs:35`，已处理 `,` `"` CRLF）与 UTF-8 BOM 约定（`to_csv:81`）**。
列：`序号,时间(本地),主机,备注,事件类型,等级,说明`。编码沿用 BOM（Excel 中文正常），**不引入 GBK**。

---

## 7. UI 改动清单

### 7.1 设置对话框 `SettingsDialog.tsx`（新增「事件日志」分组，置于「并发线程数」块之后）
| 项 | 控件 | 交互 |
|---|---|---|
| 启用事件日志 | checkbox (`events_on`) | 总开关 |
| 事件等级 | **三选一 radio**（仅故障 / 标准 / 详细） | 需带说明文字，说明各档包含哪些事件；默认标准 |
| 保存事件到文件 | checkbox (`events_persist`) | — |
| 事件保存目录 | 只读文本（截断）+「选择…」+「恢复默认」 | 「选择…」-> `dialog.open({ directory: true })`；「恢复默认」-> 置 null |
| 每主机保留条数 | 三选一（50 / 200 / 1000） | 默认 200 |

- **必须沿用 `wasOpenRef` 模式**（`SettingsDialog.tsx:48-59`，**不加依赖数组**），否则草稿会被每 500ms 的快照重置（v1.1.2 才修好的 bug，别改回去）。
- `save()` 校验追加 `events_keep ∈ {50,200,1000}`。
- 对话框体已有 `overflow-auto`（`:108`），新增内容溢出可滚动，页脚固定。

### 7.2 详情面板 `DetailPanel.tsx`（日志区 `:114-143`）
| 位置 | 改动 |
|---|---|
| 日志区标题行 | `最近日志（N）`（N = 按当前等级过滤后的可见条数）+ 未读小红点（`unread > 0`）+ 右侧「复制 / 导出 CSV / 清空」三个独立按钮 + 「记录事件」单主机开关（自绘 `role="switch"`，参照 `:52-67` 监控开关写法，**不放进顶部标题栏**以免拥挤） |
| 日志列表 | 每条按 kind 配色：`fault`/`unreachable`/`dns_fail` -> 红；`recover`/`first_ok` -> 绿；`start`/`stop`/`config_change` -> 灰 |
| 空态 | `events_on === false` -> 「已关闭事件记录」；否则「暂无事件」 |

**项目硬约定**：所有文字 `whitespace-nowrap shrink-0`；**≥1200px 观感冻结不得改动**（新增按钮在固定 `w-[420px]` 面板内，必要时用 `max-[1199px]:` 紧凑化）；**功能入口不合并不隐藏**；清空走既有 `ConfirmDialog`（**禁用 `window.confirm`**）。

### 7.3 前端数据接线 `App.tsx`
- **删除**旧 `:159-207` 前端 diff 逻辑与 `prevStatusRef`。
- `logs` 类型改 `Record<number, LogEvent[]>`；新增 `unread: Record<number, number>`、`lastSeqRef`、`loadedIdsRef`。
- 订阅回调：遍历 `snapshot.events`（`seq > lastSeq` 去重）-> `logs[target_id]` 头插并裁到 `settings.events_keep`；若 `target_id !== primaryId` 且 kind 属故障级 -> `unread[id]++`。
- 选中（`primaryId` 变化）-> 未加载则 `listEvents(id, keep)` 回填 + `unread[id] = 0`（**选中即清零**）。
- 删除目标（旧 `:278`）/ 清空列表（旧 `:388`）同步清理 `logs` 与 `unread`。

---

## 8. 任务分解（有序，供工程师）

### T01 — 后端事件核心与契约冻结 [P0]
- **文件**：`src-tauri/src/model.rs`、`src-tauri/src/events.rs`（新）、`src-tauri/src/stats.rs`、`src-tauri/src/lib.rs`
- **内容**：`LogEvent`/`EventKind`/`EventLevel` + `for_kind`/`rank`；`classify_transition` 纯函数；`EventStore`（pending 增量 + per_host 环 + seq + JSONL 读写骨架 + 写线程）；`Snapshot.events`；`PingSettings.events_*` / `TargetConfig.events_on` / `TargetState.events_on`；`normalize_settings` 扩展；**冻结跨端契约**。
- **验收**：`cargo test` 新增 `classify_transition` 全迁移用例全过；v1.0.0/v1.1.2 旧配置反序列化**不丢 targets**且新字段取正确默认；JSONL 追加 + 尾部读回单测过；编译绿。

### T02 — 后端打点接入与 4 个新命令 [P0]
- **文件**：`src-tauri/src/state.rs`、`src-tauri/src/commands.rs`、`src-tauri/src/export.rs`、`src-tauri/src/config.rs`
- **内容**：A–G 七处打点；`StopReason { User, Internal }`；4 个新命令 + 注册（13 -> 17）；事件 CSV（BOM + 本地时间）；启动 `load_from_file`。
- **依赖**：T01
- **验收**：启动即失败 -> 记 `unreachable`；DNS 失败 -> `dns_fail`；`ok` <-> 失败双向；**连续失败不重复记**；用户停止 -> `stop`；**程序退出不产生 `stop`**；改 name/host/enabled -> `config_change`；**文件不可写时快照仍正常**。

### T03 — 前端数据层与实时订阅 [P0]
- **文件**：`src/types.ts`、`src/lib/api.ts`、`src/App.tsx`
- **内容**：`LogEvent` 等类型；4 个 invoke 封装；移除旧 diff，改为消费 `snapshot.events`（seq 去重）；选中主机 `listEvents` 回填 + 未读清零；删除/清空同步清理；cap 用 `events_keep`。
- **依赖**：T01（契约；联调需 T02）
- **验收**：**根因两类（启动即失败 / DNS 失败）能记入**；无重复/无丢失；切换主机回填正确；`tsc` 0 error。

### T04 — UI（设置分组 + 详情面板增强 + 集成）[P0]
- **文件**：`src/components/SettingsDialog.tsx`、`src/components/DetailPanel.tsx`、`src/lib/format.ts`、`src/App.tsx`
- **内容**：设置「事件日志」分组（7.1）；详情面板开关/条数/红点/复制/导出/清空/配色（7.2）；props 与事件接线。
- **依赖**：T03
- **验收**：条数正确；红点未读则显、**选中即清零**；复制成功（含降级）；导出 CSV 落盘可打开；清空仅清当前主机；单主机开关持久化；**1200px 无回退、任意视口文字不折行**。

```
T01 -> T02
T01 -> T03 -> T04
```

---

## 9. 风险与决策

| # | 风险 / 事项 | 结论 |
|---|---|---|
| 1 | **WebView2 剪贴板** `navigator.clipboard` | 可能抛 `NotAllowedError`。**降级链**：`navigator.clipboard.writeText` -> 隐藏 `<textarea>` + `document.execCommand('copy')`（用户手势内可靠）-> 仍失败则 toast「复制失败，请手动选择」。**不引入新 Rust 依赖**；须真机（非 headless）验证 |
| 2 | **CSV 转义 / 编码** | 复用 `esc_csv`；编码沿用 **UTF-8 BOM**；**不用 GBK** |
| 3 | **清空某主机**：从 JSONL 剔除行 | **重写整文件**：读 -> 过滤 -> 写同目录临时文件 -> `fs::rename` 原子替换（Windows 上 Rust 用 `MoveFileExW` + `REPLACE_EXISTING`）。失败则内存照清 + 提示 |
| 4 | **`keep` 语义** | = **每主机**内存环形 cap **且** 详情面板显示上限（两者相等）。**文件不受 keep 限制**（仅受 8 MiB 轮转）；启动读回时每主机只取最近 keep |
| 5 | **未读故障红点** | 仅**故障级**（fault/unreachable/dns_fail）计入；仅当 `target_id != primaryId` 时 `unread++`；**选中即清零**；重启读回的历史标记已读 |
| 6 | **241 台增量开销** | 空闲 payload `[]`，可忽略；否决"每主机最近 N 全量"（≈1.4 MB/s） |
| 7 | **过滤时机** | **总开关 + 单主机开关 -> 生成时**（关闭则不产生，省内存/文件；事后打开不回补历史）。**等级 -> 展示时**（后端存全等级，前端按 `rank(level) <= rank(events_level)` 过滤，**改等级历史事件立即显隐**）；`list_events` 返回全等级 |
| 8 | **枚举非法值导致整份配置回落（高危）** | `events_level` 若被写成未知串会让整份 `AppConfig` 反序列化失败 -> **丢主机列表**。**必须用自定义 `deserialize_with`（未知 -> Standard）** |
| 9 | **写入阻塞探测循环** | 用独立写线程 + channel；若内联写须置于短锁内 |
| 10 | **`TargetState` 新增字段构造点** | `new()` 设 `events_on = true`；排查所有结构体字面量构造（含 `tests/`） |
| 11 | **无新增权限 / 依赖** | 复用 `dialog:allow-open` 选目录（**须核对 `capabilities/default.json`**）；`tauri.conf.json` **无需改动**（md5 保持基线 `6a8f489a1893166821ae001c199bdafa`）；`Cargo.toml` / `package.json` **无新增依赖** |
| 12 | **旧 `LogEntry` 全面替换** | `logs` 与 DetailPanel props 一次性迁移，避免半改残留 |
| 13 | **命令数** | 新增 4 个 -> **13 + 4 = 17**（设计稿正文有一处笔误写作 16，以本节为准） |

### 主理人已拍板的四项
1. **`events_keep` 默认 = 200**（原前端硬编码 50 太少，配合持久化读回后 200 更实用）。
2. **每主机各记 1 条 `start` / `stop`**（标准级）：同意。在 241 台规模下点一次「开始全部」会一次性产生 241 条 `start` + 241 条 `first_ok`，属预期；如嫌吵可把等级切到「仅故障」。
3. **事件 CSV 时间列用本地时间**，列名标注 `时间(本地)`。实现方式：前端传 `tz_offset_minutes = -new Date().getTimezoneOffset()`，后端在既有 UTC 民用历算法上加偏移，**不引入 chrono / time 等新依赖**。
4. **`classify_transition` 的 `prev` 取「本轮循环开始、置 `Resolving` 之前」的状态**（engineer 提出、主理人核准）。
   理由：`resolve_host_blocking` 每轮都会把状态临时置为 `Resolving`。若以「写入前状态」为 `prev`，则 **持久性 DNS 失败会每轮重复产出 `dns_fail`**，违背 §1.4 的去重目标；以循环起点状态为 `prev` 才能得到
   `迭代1: (Idle, Failed, Resolve) -> dns_fail`、`迭代2: (Failed, Failed, Resolve) -> None`。
   **`classify_transition` 函数体仍严格按 §1.4 实现，未作改动**，仅调用点传参口径如此。已由端到端测试 `state::tests::qa_events_dns_fail_dedup_and_user_stop` 覆盖。
