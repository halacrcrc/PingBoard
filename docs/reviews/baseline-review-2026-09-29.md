# PingBoard v1.1.8 基线复审报告（L2 AI 复审）

> 依据：`docs/code-review.md` v1.0（三级分级 + R1–R13 红线 + 三通道 + 测试有效性准则）
> 基线：`c4d9970`（v1.1.8 内容）· 工作区 `D:/pinginfo`（与 git 仓库内容一致）
> 方式：静态复审，**未修改任何代码、未执行构建/测试**
> 复审人：高见远（架构师，Rust 侧）· 严过关（QA，前端 + 测试侧）
> 汇总：齐活林（交付总监）· 2026-09-29

---

## 零、总览

| 侧 | 🔴 阻断 | 🟡 建议 | 💭 细节 | 结论 |
|---|---|---|---|---|
| Rust（`src-tauri/src/`，5904 行 / 11 模块） | **4** | 9 | 8 | **不可直接发布** |
| 前端（`src/` + `tests/`，3180 行 / 49 用例） | **1** | 16 | 8 | 有条件可合入，🔴 修复后即可 |
| **合计** | **5** | **25** | **16** | 建议随 v1.1.9 带掉 🔴，不回滚 v1.1.8 |

**5 条 🔴 的共同点：4 条是「用户主机列表丢失」（配置容错不完整），1 条是「误触静默删除」。**
R2 / R3 / R8 / R9 / R10 / R13 六条红线**实现到位且有回归测试锁死**；前端 R4 / R5 / R6 / R7 / R11 / R12 **全绿**。

### 优先处理 Top 5（主理人排序）

| # | 条目 | 位置 | 理由 | 工作量 |
|---|---|---|---|---|
| 1 | 🔴 A1 `add_targets` 静默吞掉保存失败 | `commands.rs:23` | 一行改动；同文件另 5 个命令都是 `save_config(&app)?`，只有它用 `let _ =`。导入 200 台后不落盘 → 下次保存被空配置覆盖 | ~10 min |
| 2 | 🔴 前端 Delete 键穿透弹层静默删除 | `App.tsx:439-450` + `418-436` | 对话框打开时按 Delete 直接删选中目标并落盘，无确认无撤销 | ~30 min |
| 3 | 🔴 A2 + A3 配置反序列化容错补齐 | `model.rs:227-249`、`102-151` | R1 是本项目最贵的教训；15 个字段里只有 `events_level` / `ui_scale` 受保护。需同步改掉 `model.rs:381` 那条把错误行为锁成「既定策略」的测试 | ~2 h |
| 4 | 🔴 A4 配置解析失败时备份 + 抢救 targets | `config.rs:35-55` | A2/A3 的下游兜底：JSON 结构写坏仍会清零，不备份 = 用户连手工恢复的机会都没有 | ~1 h |
| 5 | 🟡-15 Round 5 回归测试「不能失败」 | `tests:343-351` | 把修复回滚后测试仍全绿 —— 测的是纯函数，坏的是接线。虚假安全感，比没测更危险 | ~1 h |

---

# 第一部分 · Rust 侧复审（高见远）

> 范围：`src-tauri/src/` 全部 11 个模块（`state.rs` 1471、`events.rs` 1289、`import.rs` 592、`export.rs` 528、`model.rs` 512、`stats.rs` 394、`pinger/` 602、`commands.rs` 189、`fonts.rs` 186、`config.rs` 55、`lib.rs` 80），合计 5904 行。

## 一、🔴 阻断（4）

### 🔴 [A1] [错误处理] `add_targets` 静默吞掉配置保存失败

**位置**：`src-tauri/src/commands.rs:23`

**为什么**：
```rust
let ids = state.add_targets(entries)?;
let _ = state.save_config(&app);   // ← 保存失败被丢弃
Ok(ids)
```
同文件里 `remove_targets:35`、`update_target:49`、`set_enabled:61`、`update_settings:101`、`set_target_events:179` **全都是 `state.save_config(&app)?` 直接传播**，只有 `add_targets` 用了 `let _ =`。这是明显的不一致而非有意设计。

触发条件：配置目录不可写（磁盘满 / 权限变更 / `%APPDATA%` 被安全软件锁定 / 配置文件被独占占用）。后果是用户批量导入 200 台主机、界面显示成功、ping 也正常跑，但**配置从未落盘**；下一次任何触发保存的操作，或退出时 `lib.rs:73` 的保存，都会写一份不含这批目标的 `config.json`——用户主机列表永久丢失，且全程无任何提示。

**建议**：与其它 5 个命令对齐，直接传播错误：
```rust
pub async fn add_targets(
    app: AppHandle,
    state: State<'_, AppState>,
    entries: Vec<TargetEntry>,
) -> Result<Vec<u64>, String> {
    let ids = state.add_targets(entries)?;
    state.save_config(&app)?;   // 失败即上报，不吞
    Ok(ids)
}
```

### 🔴 [A2] [序列化 / R1] `AppConfig` 与 `TargetConfig` 缺**字段级** `#[serde(default)]`

**位置**：`model.rs:243-249`（`version` 245、`settings` 246）；`model.rs:227-236`（`name` 229、`host` 230）

**为什么**：`PingSettings` 有容器级 `#[serde(default)]`（`model.rs:100`），`AppConfig.targets` 有（`247`），但 `version` / `settings` / `name` / `host` 都没有。**这四个字段任何一个缺失或类型不符，整份 `AppConfig` 反序列化失败 → `config.rs:43` 回落 `AppConfig::default()` → `targets` 为空 → 主机列表全丢**，随后被 `save` 覆盖写死（见 A4）。

可达触发路径：
1. 用户手工改 `config.json` 删掉某条 target 的 `"host"`（或拼成 `"hosts"`）→ 该文件里**所有**主机消失，不只这一条。
2. 未来版本加字段后用户回退 v1.1.8 → 若新版本曾把 `host` 改名/嵌套，直接全丢。
3. `settings` 写成 `null` 或字符串 → 容器级 `#[serde(default)]` **救不了**（只在字段缺失时生效，`null` 是类型不符）→ 全丢。

注意 `model.rs:379-384` 的 `qa_missing_required_field_is_err` 把「缺 settings 应报错」锁成了「既定策略」——**这个策略对 `settings` 嵌套块是错的**：正确行为应是「设置回落默认、主机列表保住」。这条测试需要一起改掉。

**建议**：
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default, deserialize_with = "deserialize_settings")]
    pub settings: PingSettings,
    #[serde(default)]
    pub targets: Vec<TargetConfig>,
}
fn default_version() -> u32 { 1 }

pub fn deserialize_settings<'de, D>(deserializer: D) -> Result<PingSettings, D::Error>
where D: serde::Deserializer<'de> {
    let v = Option::<serde_json::Value>::deserialize(deserializer).unwrap_or(None);
    match v {
        Some(v) => serde_json::from_value::<PingSettings>(v)
            .or_else(|_| serde_json::from_str::<PingSettings>("{}").map_err(serde::de::Error::custom)),
        None => serde_json::from_str::<PingSettings>("{}").map_err(serde::de::Error::custom),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetConfig {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub host: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub events_on: bool,
}
```
`state.rs:167-170` 已有「host 为空则跳过该条」，缺 host 会退化成「跳过这一条」而非「清空全部」，正是期望行为。

配套改测试：
```rust
#[test]
fn qa_missing_settings_block_keeps_targets() {
    let cfg: AppConfig = serde_json::from_str(
        r#"{"version":1,"targets":[{"name":"a","host":"1.1.1.1"}]}"#).unwrap();
    assert_eq!(cfg.targets.len(), 1, "缺 settings 不得丢主机");
    assert_eq!(cfg.settings.interval_ms, 1000);
    let cfg2: AppConfig = serde_json::from_str(
        r#"{"version":1,"settings":null,"targets":[{"host":"2.2.2.2"}]}"#).unwrap();
    assert_eq!(cfg2.targets.len(), 1, "settings=null 不得丢主机");
    let cfg3: AppConfig = serde_json::from_str(
        r#"{"version":1,"targets":[{"name":"x"}]}"#).unwrap();
    assert_eq!(cfg3.targets.len(), 0, "缺 host 的条目应被跳过而非整份失败");
}
```

### 🔴 [A3] [序列化 / R1] 只有 `events_level` / `ui_scale` 做了类型容错，其余 13 个字段一旦类型不符即丢全部主机

**位置**：`model.rs:102-116`（`interval_ms` 102、`timeout_ms` 103、`payload_size` 104、`ttl` 105、`max_threads` 106、`history_len` 116）；`model.rs:139-140`（`events_keep`）；`model.rs:114/121/134`（`beep_on_fail`、`auto_start`、`events_persist`）；`model.rs:151`（`ui_font_family`）

**为什么**：项目已为 `events_level`（`145-158`）和 `ui_scale`（`193-204`）写了自定义 `deserialize_with`，注释写得很清楚——*「该字段被写成字符串/越界数字时，普通 Deserialize 会让整份 AppConfig 反序列化失败 → 回落默认、清空用户主机列表」*。**这个风险对其它字段完全等价，但没有一个受保护。**

最易命中 `events_keep`（`139-140`，只有 `default`）：用户在设置面板会改、也最可能被手工写坏的三档值（50/200/1000），写成 `"200"` 或 `null` 立刻全丢。其次 `interval_ms` / `timeout_ms`，手工调参写 `100000` 在 `u64` 上不越界但语义越界——`normalize_settings` 本可钳制，可惜走不到那一步。

**建议**：抽成宏，一次性覆盖全部数值与布尔字段：
```rust
macro_rules! lenient_usize {
    ($fn_name:ident, $default:expr) => {
        pub fn $fn_name<'de, D>(deserializer: D) -> Result<usize, D::Error>
        where D: serde::Deserializer<'de> {
            let v = Option::<serde_json::Value>::deserialize(deserializer).unwrap_or(None);
            Ok(v.and_then(|v| v.as_u64()).map(|n| n as usize).unwrap_or($default))
        }
    };
}
macro_rules! lenient_bool {
    ($fn_name:ident, $default:expr) => {
        pub fn $fn_name<'de, D>(deserializer: D) -> Result<bool, D::Error>
        where D: serde::Deserializer<'de> {
            let v = Option::<serde_json::Value>::deserialize(deserializer).unwrap_or(None);
            Ok(v.and_then(|v| v.as_bool()).unwrap_or($default))
        }
    };
}
lenient_usize!(deserialize_interval_ms, 1000);
lenient_usize!(deserialize_timeout_ms, 2000);
lenient_usize!(deserialize_max_threads, 256);
lenient_usize!(deserialize_history_len, 60);
lenient_usize!(deserialize_events_keep, DEFAULT_EVENTS_KEEP);
lenient_bool!(deserialize_bool_false, false);
lenient_bool!(deserialize_bool_true, true);
```
```rust
#[serde(default = "default_interval", deserialize_with = "deserialize_interval_ms")]
pub interval_ms: u64,          // 同理 timeout_ms / max_threads / history_len
#[serde(default = "default_log_keep", deserialize_with = "deserialize_events_keep")]
pub events_keep: usize,
#[serde(default, deserialize_with = "deserialize_bool_false")]
pub beep_on_fail: bool,        // auto_start 同
#[serde(default = "default_true", deserialize_with = "deserialize_bool_true")]
pub events_persist: bool,
```
`payload_size: u16` / `ttl: u8` 特别危险（**越界即失败**，`ui_scale` 当初就是因此才加保护），务必覆盖：
```rust
pub fn deserialize_payload_size<'de, D>(d: D) -> Result<u16, D::Error>
where D: serde::Deserializer<'de> {
    let v = Option::<serde_json::Value>::deserialize(d).unwrap_or(None);
    Ok(v.and_then(|v| v.as_u64()).map(|n| n.min(u16::MAX as u64) as u16).unwrap_or(32))
}
```
补一组「每个字段写成字符串/null 都不丢 targets」的参数化测试，与 `model.rs:484-501` 的 `qa_invalid_ui_scale_falls_back_to_100` 同规格。

### 🔴 [A4] [数据丢失] `config::load` 解析失败后直接回落默认，且不备份原文件

**位置**：`config.rs:35-44`（load 回落）、`config.rs:48-55`（save 覆盖）

**为什么**：
```rust
match serde_json::from_str::<AppConfig>(&text) {
    Ok(mut cfg) => { ...; cfg }
    Err(_) => AppConfig::default(),   // ← 直接清零，原文件还在但已判死刑
}
```
意味着**一个字符写坏，全部主机当场消失**。更糟：磁盘上的原文件此时仍完好，但进程内状态已空；只要随后触发任意一次 `save_config`（`remove_targets` / `update_target` / `set_enabled` / `update_settings` / `set_target_events` / 退出时 `lib.rs:73`），原文件就被**空配置静默覆盖**，用户连手工恢复的机会都没有。

`config.rs:25` 的注释「失败一律回退到默认值，绝不 panic / 阻塞启动」是对的——**不阻塞启动要保持**；但「不备份 + 不抢救」是缺的那一半。这是 A2/A3 的下游放大器。

**建议**：
```rust
Err(e) => {
    let bak = path.with_file_name(format!(
        "pingboard-config.corrupt-{}.json", crate::state::now_ms()));
    if let Err(cp) = fs::copy(&path, &bak) {
        eprintln!("[pingboard] 配置损坏且备份失败：{}", cp);
    } else {
        eprintln!("[pingboard] 配置解析失败，已备份原文件到 {}：{}", bak.display(), e);
    }
    let mut cfg = AppConfig::default();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
        if let Some(list) = value.get("targets").and_then(|t| t.as_array()) {
            cfg.targets = list.iter()
                .filter_map(|t| serde_json::from_value::<TargetConfig>(t.clone()).ok())
                .filter(|t| !t.host.trim().is_empty())
                .collect();
            eprintln!("[pingboard] 已从损坏配置中抢救 {} 个主机", cfg.targets.len());
        }
    }
    cfg
}
```
配合 A2 给 `TargetConfig.name/host` 加的默认值，抢救成功率会进一步提高。再加一个「回滚到旧写法就失败」的测试。

---

## 二、🟡 建议（9）

### 🟡 [B1] [并发/契约] 增量事件是单消费者队列，却被 `export_report` 旁路消费掉
**位置**：`state.rs:124`（`drain_pending`）、`commands.rs:123`（export_report 调 `snapshot()`）
**为什么**：`Snapshot.events` 是「取走并清空」的增量通道，唯一设计消费者是 500ms 发射器（`state.rs:868-880`）。但 `export_report` 也调 `state.snapshot()`——**用户每导出一次报表，就把当时尚未推送的最多 500ms 事件吞掉**，永远到不了前端（故障/恢复事件恰恰最不能丢）。`get_state` 初始化时调一次问题不大。
**建议**：新增不消费事件通道的旁路：
```rust
pub fn target_states(&self) -> Vec<TargetState> {
    let list = self.inner.targets.lock().unwrap();
    list.iter().map(|t| t.lock().unwrap().clone()).collect()
}
```
`commands.rs:117-133` 改用它。

### 🟡 [B2] [事件一致性] 清空事件未清 `pending` —— 「清空了又自己回来」的残留窗口
**位置**：`events.rs:452-457`（`clear_host`）、`state.rs:240-248`
**为什么**：`clear_host` 只清 `per_host` 内存环，**没清 `pending`**。点「清空此主机事件」后，上次快照（≤500ms 前）之后新产生、尚未推送的事件仍留在 `pending`，会在下一次 `ping-snapshot` 送达前端。**这正是 `events.rs:633-634` 记录过的怪象——当时修了文件侧（归档没清），内存侧这个 500ms 窗口没修。**
**建议**：
```rust
pub fn clear_host(&self, id: u64, host: &str) {
    self.per_host.lock().unwrap().remove(&id);
    let key = normalize_host(host);
    if !key.is_empty() {
        self.pending.lock().unwrap()
            .retain(|e| normalize_host(&e.target_host) != key);
    }
    if self.persist_enabled.load(Ordering::SeqCst) {
        let _ = self.tx.send(WriterMsg::ClearHost(host.to_string()));
    }
}
```
补测试：emit 两条 → 不 drain → `clear_host` → `drain_pending()` 应为空。

### 🟡 [B3] [并发/资源] `spawn_worker` 把线程创建失败吞掉，产生「幽灵 worker」
**位置**：`state.rs:607-622`
**为什么**：`.ok()` 后失败与成功走同一条路——**线程没起来，却照样入 `workers` 表、照样置 `running=true`、照样发「开始探测」事件**。后果：(a) `active_threads`（`state.rs:106` 取 `workers.len()`）虚高；(b) 目标永远显示运行中但 sent 恒为 0；(c) stop 时幽灵记录被 reaper 空转；(d) 事件日志多一条假 `start`。触发真实可达（允许 4096 个 worker，`state.rs:47`）。
**建议**：`match join { Ok(j) => {入表; running=true; emit Start}, Err(e) => {不入表; running=false; last_error=Some(...)} }`

### 🟡 [B4] [输入校验] 导入无文件大小 / 表格行数上限，大文件直接 OOM
**位置**：`import.rs:55`、`68`、`98-116`（三处全量读入内存）
**建议**：`const IMPORT_MAX_BYTES: u64 = 20 * 1024 * 1024;` + `const IMPORT_MAX_ROWS: usize = 50_000;`，`read_import_file` 先用 `fs::metadata` 卡一道并返回可读错误。

### 🟡 [B5] [错误处理] 事件持久化失败完全没有用户可见反馈，`persist_error` 是死代码
**位置**：`events.rs:565-569`、`580-586`、`499-502`（`#[allow(dead_code)]`）、`418-420`；`state.rs:575-582`
**为什么**：用户勾了「持久化到文件」，`events_dir` 指向不存在盘符/只读目录/U 盘拔出 → `configure` 静默置 `enabled=false`，之后事件只在内存、重启即失，**用户以为存了其实没存**，界面毫无异样。`persist_error` / `persist_enabled` 两个 getter 已写好却挂着 `#[allow(dead_code)]`，是「建了一半的防护」。
**建议**：`Snapshot` 新增 `events_persist_error: bool`（纯新增字段，不破坏契约），`state.rs snapshot()` 里填 `self.inner.events.persist_error()`，前端在设置面板给红字提示。

### 🟡 [B6] [性能] 500ms 快照全量克隆并序列化 `history`（每主机最多 600 点）
**位置**：`state.rs:868-880`、`98-101`；`stats.rs:77`（`history_len` 钳制 `10..=600`）
**为什么**：100 目标时每 500ms 克隆约 `100 × 600 × 16B ≈ 960 KB` 再整体序列化，合计约 **1-2 MB/s 持续开销**，而 sparkline 真正需要的通常只有几十点。
**建议**（推荐 a）：500ms 通道只下发绘制窗口（如最近 60 点），详情面板全量历史改由新命令 `get_target_history(id)` 按需取；或 (b) 脏标记 + 最长 1s 心跳。补一个「100 目标 × 600 点时 `snapshot()` 耗时」的 benchmark 基线条。

### 🟡 [B7] [安全] CSV 导出未防公式注入（Excel DDE），而字段内容来自外部导入文件
**位置**：`export.rs:46-52`（`esc_csv`）、`96-115`、`123-140`
**为什么**：`esc_csv` 只做 RFC4180 转义，**没处理以 `=` `+` `-` `@` 开头的单元格**。而「备注名 / 主机名 / 说明」来自用户导入的 txt/csv/xlsx（外部可控）。导入清单里一行 `=cmd|'/c calc'!A1` 导出后用 Excel 打开即为经典 CSV 公式注入。HTML 路径 `esc_html`（`55-60`）已正确转义——**同一份数据两种口径不一致，说明是漏而非有意**。
**建议**：`esc_csv` 对首字符为 `= + @` 的单元格前置单引号；`-` 不处理（避免误伤负数）。补测试 `esc_csv("=cmd|'/c calc'!A1")` → `'=cmd...`。

### 🟡 [B8] [健壮性] `rotate()` 重命名失败后，每个事件都会重试一次 rename（无退避）
**位置**：`events.rs:619-629`、`597-605`
**为什么**：文件被占用（杀毒扫描、用户用记事本打开事件文件——它就在 `%APPDATA%` 下）时 rename 失败，`self.file` 重新指向依旧超限的原文件 → **之后每写一条事件都走一遍 remove_file + rename**，事件越多越慢，且错误被 `let _ =` 吞掉。8MiB 轮转阈值形同虚设、文件无限增长。
**建议**：`EventWriter` 加 `rotate_failed: bool`，失败后不再重试（只打一次 stderr），`configure()` 里复位。

### 🟡 [B9] [数据一致性] 删除目标不同步清理其事件环与文件行，留下孤儿数据
**位置**：`state.rs:312-316`（`remove_targets`）
**为什么**：删主机只做「停线程 + 从列表移除」，**没调 `events.clear_host`**。后果：该文件里这个 host 的历史行永久残留（界面上主机都没了，没有入口再清空）；`per_host[id]` 内存环残留，会被 `all_events()`（`events.rs:441-449`）算进「导出全部事件」——**用户导出的 CSV 里会出现已删除的主机**。
**建议**：`remove_targets` 里先按 id 取 host，`self.inner.events.clear_host(*id, &h)` 再移除。（若产品希望「删主机保留历史」，应在导出时按当前 targets 过滤并写进文档；现状是两头不靠。）

---

## 三、💭 细节（8）

| # | 位置 | 说明 |
|---|---|---|
| C1 | `state.rs:430-434` / `459-461` / `612` | `start()` 的「查活跃 → 过滤 → 插入」不在同一把锁内。**当前不可达**（所有调用点均为无 `.await` 的同步函数体），记录备查；加固方式：整个「算 to_start + 插入」收进一次 `workers` 锁内 |
| C2 | `state.rs` 83 处 `.unwrap()`；`Cargo.toml:47` `panic="abort"` | 任一 panic 即进程退出。建议最热两处（`state.rs:99-101` 快照循环、`274-276` `find`）改 `lock().unwrap_or_else(|e| e.into_inner())` |
| C3 | `events.rs:220`（`Raw.kind`） | `kind` 无未知值降级，将来新增事件类型会让旧版本静默丢行（坏行被 `.ok()` 跳过，又被 `filter_out_host` 保留 → 永远占着文件）。建议加 `#[serde(other)] Unknown` 变体 |
| C4 | `events.rs:453` vs `643-646` | `clear_host` 内存侧按 id、文件侧按 host，同 host 多目标时口径分叉（清 A 会删掉文件里 B 的事件，但 B 的内存环还在）。建议统一按 host |
| C5 | `state.rs:754`、`630-637` | `is_current_worker` 每轮探测抢一次全局 `workers` 锁（100 目标 × 最小 100ms 间隔 ≈ 1000 次/秒）。建议 `WorkerHandle` 放 `Arc<AtomicBool> current`，语义不变 |
| C6 | `commands.rs:124-131` | `Vec::contains` 过滤 O(n·m)，换 `HashSet` 一行的事 |
| C7 | `stats.rs:33` 与 `51` | `last_rtt_ms` 重复赋值两次，无功能影响但易误导重构 |
| C8 | `events.rs:619-629`、`27` | 单代轮转是有意策略（主 8MiB + 归档 8MiB ≈ 上限 16MiB 后丢最老数据），但界面只显示「每主机保留 200 条」（内存环口径）。建议在设置面板标注磁盘上限，属文档/提示层面 |

---

## 四、已核对、**无问题**的项目（明确销项）

| 项 | 依据 |
|---|---|
| **R2** `Status` lowercase | `model.rs:9-11` `rename_all="lowercase"` + `model.rs:266-275` 测试锁死 5 个取值 |
| **R3** `CREATE_NO_WINDOW` | 全仓唯一 `Command::new` 在 `fallback.rs:54`，常量 `0x0800_0000`（`:21`）已设置（`:63`）；`fonts.rs` 无子进程 |
| **R8** 只用 `IcmpSendEcho` | `icmp.rs:12-14` 仅引 3 个 Win32 API，无 raw socket / 无第三方 ping 库 |
| **R9-a** `prev` 取置 Resolving 之前 | `state.rs:664-668`，含注释说明「否则持久性 DNS 失败会每轮重复打点」 |
| **R9-b** 清空同时清归档 | `events.rs:635-648` 遍历两个文件 + `events.rs:1179-1211` 回归测试 |
| **R9-c** 批次用 `session` 不用 `seq` | `events.rs:177-183` + `1073-1106` 测试；`seq` 口径由 `1266-1288` 固化（含孤儿事件也要顶高计数器） |
| **R10-a** 锁内不 join | `state.rs:481-495` 摘句柄 → `525-537` reaper 后台 join；`1268-1302` 有 100 目标 <300ms 性能回归 |
| **R10-b** `Arc::ptr_eq` 条件写入 | `state.rs:630-637` + `814-820` 双向堵；`1313-1350` 有 8 轮 stop→start 采样回归 |
| **R13** `tauri.conf.json` 基线 md5 | 两侧实测均 `2c0a8cf0fb8d8f1d5eebea6679c8d270`，无漂移 |
| 18 个 Tauri 命令契约 | `lib.rs:20-39` 注册 18 个，与 `commands.rs` / `api.ts` 一一对应，无签名或事件名变更 |
| 死锁 / 锁序反转 | 逐路径核对：`snapshot()` `[targets→target]`→`[pending]`，`stop()` `[target]→[pending]`，`worker_loop` `[target]→[pending]`/`[workers]`（不嵌套），`spawn_worker` 三次加锁均为语句级临时量。**无环** |
| 共享状态一律 `Arc<Mutex>` / `AtomicBool` | `state.rs:56-70` 全部合规；无裸 `static mut` / `Rc` / `RefCell` 跨线程 |
| 锁内不做 IO | 唯一锁内 IO 是 `events.emit` 的 `tx.send`（mpsc 无阻塞）；文件写入全在独立写线程（`events.rs:519-532`）；`config::save` 调用点均无持锁 |
| 用户可见路径裸 `unwrap()` | 生产路径仅 `lib.rs:79`（启动失败，合理）与 `fallback.rs:41-46`（常量正则，合理）；其余集中在 `.lock().unwrap()` 与 `#[cfg(test)]`。**无 `panic!`/`todo!`/`unreachable!` 泄漏** |
| `IcmpSendEcho` 句柄释放 | `icmp.rs:139-146` 实现 `Drop`，错误分支走 RAII；`reply_buf: Vec<u64>` 保证 8 字节对齐 |
| 导入编码探测 | `import.rs:77-93` 四级回退（BOM → 严格 UTF-8 → GBK → lossy）+ `355-392` 锁定 UTF-16 边界 |
| CSV / HTML 转义（XSS） | `export.rs:55-60` `esc_html` 覆盖 `& < > "`，HTML 报表主机名/备注名均经转义 |
| 时区偏移符号契约 | Rust `export.rs:151-155` 期望东八区 +480，前端 `App.tsx:356` 为 `-getTimezoneOffset()`，两端一致 |
| `stats.rs` 统计口径 | `avg = sum / received`（只统计成功包，超时不拉低均值，口径正确）；`reset_stats` 同步清 `sum`/`received`；`push_history` 在 `history_len` 变小后自动收敛 |
| `fonts.rs` 注册表读取 | `97-126` 失败返回空列表由前端降级；族名清洗为纯函数且有 4 组测试；`STYLE_WORDS` 刻意不收 `ui/text/math` |
| `main.rs` / `lib.rs` 装配 | 启动顺序（读配置 → 注入事件目录 → 读回历史 → 起发射器 → 自动开始）正确；`lib.rs:69-77` 退出前保存 |

## 五、正反馈 —— 写对的关键防护

1. **`Arc::ptr_eq` 双向堵**（`state.rs:630-637` + `814-820`）：不止退出时条件写入，连循环内每次探测后的 `running=true`（`805-807`）都用 `still_current` 兜住；map 里查不到自己时返回 `false`，把「被摘除」与「被替换」统一处理，逻辑闭合。配套 `1313-1350` 用 8 轮 × 12 次采样证明「回滚到无条件写就失败」。
2. **reaper 后台 join**（`state.rs:525-537`）：摘句柄 → 置位 → 立即改可见状态 → 甩给 `pingboard-reaper`。可见状态不等线程真退出就先更新（497-514），UI 响应与线程回收解耦；`1268-1302` 把 100 目标 <300ms 固化成契约。
3. **`prev` 在置 Resolving 之前读**（`state.rs:664-668`）：R9 里最易写错的一条，不但做对还把原因写成注释。
4. **配置枚举降级反序列化**（`model.rs:145-158`、`193-204`）：`Option<serde_json::Value>` 中间载体 + `unwrap_or(None)`，任意输入都返回合法值——R1 的教科书实现；`484-501` 覆盖字符串/越界/null/布尔/浮点五种非法输入。
5. **清空事件同时清归档**（`events.rs:635-648`）：`self.file = None` 先释放句柄（Windows 上必须）再遍历两个文件；`filter_out_host` 对坏行一律保留（653-663）避免误删；四个回归测试分别锁住「两侧都清」「文件不存在不创建」「全清后删文件」「坏行保留」。
6. **`load_history` 归档与主文件都读**（`events.rs:688-700`）：按 `ts` 升序合并而非 `seq`（跨会话 seq 会归零）；`1119-1121` 刻意让归档 seq 大于主文件 seq 来证明排序依据是 ts——用测试证明口径。
7. **事件写线程串行化**（`events.rs:415-422`、`519-532`）：探测循环只 `tx.send`，IO 全在 `pingboard-events-writer`，杜绝探测被磁盘阻塞；落盘失败只 `eprintln` 不回流，符合「事件日志不能影响主流程」。
8. **ICMP 引擎 RAII 与对齐**（`icmp.rs:26-31`、`139-146`）：`reply_buf: Vec<u64>` 天然 8 字节对齐、按 `div_ceil(8)` 复用、句柄走 `Drop`；`154-174` 用真实回环探测验证「普通用户权限即可」。
9. **`deserialize_event_level` 的注释质量**（`model.rs:140-144`）：把「为什么必须这么做」追溯到设计稿风险 #8 和具体后果，让红线可执行而非口号。

---

# 第二部分 · 前端与测试侧复审（严过关）

> 范围：`src/`（15 文件，3,180 行）+ `tests/frontend_pure.test.mjs`（487 行 / 49 用例）
> 附带验证：`npx tsc --noEmit` 干净通过（0 error）

## 零、结论速览

| 项 | 结果 |
|---|---|
| 🔴 / 🟡 / 💭 | **1 / 16 / 8** |
| 是否可合入 | 有条件可合入：🔴-1 修复后即可 |
| 是否可发布 | **可发布**（v1.1.8 已验收基线，无契约/并发/配置破坏） |
| 红线 | **R4 / R5 / R6 / R7 / R11 / R12 逐条核对通过** |

## 一、🔴 阻断（1）

### [🔴] [数据丢失 / 交互] 全局 Delete 键穿透弹层：对话框打开时按 Delete 会静默删除选中目标

**位置**：`src/App.tsx:439-450`（键盘监听）+ `418-436`（`deleteSelected`）+ `:582`（Toolbar `onDeleteSelected`）

**为什么**：该 `keydown` 挂在 `window` 上，只排除 `INPUT` / `TEXTAREA`（`:441-442`），**没有排除「当前有模态弹层打开」**。
复现路径：勾选若干行 → 打开「添加主机」或「设置」对话框 → 焦点落在对话框内的**按钮 / 下拉 / 滑块**（都不是 INPUT/TEXTAREA）→ 按 `Delete` → `selected.size > 0` 成立 → 直接 `api.removeTargets(ids)`，目标连同统计被删除并**立即落盘 config.json**。
后果：删除发生在遮罩之下，用户完全看不到；无提示、无确认、**无撤销**。加重情节：「清空列表」（`541-557` + `682-695`）走 `ConfirmDialog`，「删除选中」不走——更温和的操作有确认、更常用的没有，口径自相矛盾。

**建议**：
```tsx
const [confirmDelete, setConfirmDelete] = React.useState(false);
const requestDeleteSelected = React.useCallback(() => {
  if (selected.size === 0) return;
  setConfirmDelete(true);
}, [selected]);

React.useEffect(() => {
  const onKey = (e: KeyboardEvent) => {
    const tag = (e.target as HTMLElement | null)?.tagName;
    if (tag === "INPUT" || tag === "TEXTAREA") return;
    if (document.querySelector('[role="dialog"]')) return;   // 任意对话框打开时不响应
    if (e.key === "Delete" && selected.size > 0) {
      e.preventDefault();
      requestDeleteSelected();
    }
  };
  window.addEventListener("keydown", onKey);
  return () => window.removeEventListener("keydown", onKey);
}, [selected, requestDeleteSelected]);
```
再补一个 `ConfirmDialog`（放在 `App.tsx:695` 之后）：
```tsx
<ConfirmDialog
  open={confirmDelete}
  title="删除选中目标"
  danger
  confirmText={`删除 ${selected.size} 个`}
  message={<>确定要删除选中的 <b>{selected.size}</b> 个目标吗？其统计数据会一并清除，且无法撤销。</>}
  onConfirm={() => { setConfirmDelete(false); deleteSelected(); }}
  onCancel={() => setConfirmDelete(false)}
/>
```
⚠️ 配套必改：`ConfirmDialog.tsx:52` 已有 `role="dialog"`，但 **`AddTargetsDialog.tsx:323` 与 `SettingsDialog.tsx:298` 的弹窗容器没有** → 上面的守卫抓不到。二选一：给两个对话框根节点补 `role="dialog" aria-modal="true"`（推荐），或在 App 里维护 `openDialogCount` 作为守卫条件。

## 二、🟡 建议（16）

### 测试侧

- **🟡-1 [测试基建] `package.json` 没有 `test` 脚本** — `package.json:7-12`。`npm test` 会报 `Missing script`，而 `docs/code-review.md:76` 与 `:158` 都把「npm test 通过」写成必过项，等于这条自检长期空转。建议加 `"test": "node tests/frontend_pure.test.mjs"`。
- **🟡-2 [测试基建] `esbuild` 未声明为 devDependency** — `package.json:19-29`；`tests:10` 直接 `import { build } from "esbuild"`，只活在 vite 的依赖树里。建议 `npm i -D esbuild@^0.21`。
- **🟡-3 [测试基建] `check()` 吞掉异步断言失败** — `tests:75-82`。同步调用 `fn()`，若有人写 async 用例，返回 Promise 永不 throw → `pass++`。建议检测到 thenable 直接抛错。
- **🟡-4 [覆盖度] `tableToBatchText` 完全零覆盖** — `AddTargetsDialog.tsx:124-136`。承载表头识别（仅第 1 行生效）、空首列跳过、`主机\t备注` 拼接三条规则，Excel 导入全靠它。建议补 5 条最小用例（含大小写不敏感、与 `parseBatch` 串联）。
- **🟡-5 [覆盖度] `format.ts` 有 7 个导出函数零覆盖** — `fmtTime:17` / `fmtDateTimeShort:25` / `statusRowClass:75`（**R12 行底色**）/ `eventColorClass:109` / `eventKindLabel:124` / `isUnreadKind:148`（**R9 未读口径前端唯一落点**）/ `currentZoomFactor:187`（R7 另一半）。至少补 `statusRowClass` + `eventColorClass` + `isUnreadKind` 三条。
- **🟡-6 [可测性] 排序比较器 `compare()` 未导出** — `TargetTable.tsx:79-97`。内含三条不变量（null 最小 / status 走 `STATUS_ORDER` 而非字典序 / `localeCompare(..., "zh-Hans-CN")`），模块私有导致改坏不会被任何测试拦住。建议 `export function compare` 并补 3 条用例。
- **🟡-7 [测试有效性] R9 事件增量去重不变量既无测试也不可测** — `App.tsx:154-156`。前端唯一的「事件不重复消费」护栏写在组件里、依赖 `lastSeqRef` 与 `beep()`。建议抽成 `src/lib/events.ts` 的 `consumeEvents()` 纯函数，配三条**能失败**的测试（重复投递不重复入表 / seq 回退丢弃 / cap 截断）。
- **🟡-8 [测试有效性] R7 的 zoom 换算只有一半被覆盖** — `format.ts:187-190` + `Toolbar.tsx:84-85`。`zoomStyle` 有 3 条用例、`currentZoomFactor` 0 条，配对关系无断言。建议抽 `fixedTopFromRect(rectBottom, zoomFactor, gap)` 并加反向断言（不除 zoom 就偏 25%）。
- **🟡-15 [测试有效性] Round 5「跨批次重复检测」回归测试不能失败** — `tests:343-351`；修复点 `AddTargetsDialog.tsx:236-244`。断言的是 `findDuplicateHosts(...).length === 1`，但真正的 bug 是**接线**（`onSubmitSingle` 曾直接 `doAdd([entry])` 绕过 `submitEntries`）。把修复回滚成 `void doAdd([entry])`，这三条**依然全绿**——测的是纯函数，而纯函数从未坏过。建议抽 `decideAdd(entries, existingHosts): AddDecision` 纯函数，让三条路径统一调用，测试才咬得住接线。

### 前端实现侧

- **🟡-9 [性能] `React.memo(TargetTable)` 被 4 个内联回调每 500ms 击穿** — `App.tsx:385/394/516/528` 定义的箭头函数每次新引用，memo 浅比较必然失败 → 整个表格（含全量排序 + 每行 Sparkline）每 500ms 重算。`App.tsx:501` 的注释说明作者意识到了这个陷阱，但只对 `unreadIds` 做了 `useMemo`——**对象侧防住了，函数侧漏了**。`React.memo(Toolbar)`（`Toolbar.tsx:272`）同理。建议全部 `useCallback` 化 + ref 桥接。
- **🟡-10 [正确性] `showToast` 定时器从不清理** — `App.tsx:299-302`。t=0s 弹 A、t=1s 弹 B，A 的 timer 在 t=4s 触发 → **B 只显示 3 秒就被清掉**；卸载后仍 setState。建议 `toastTimerRef` + 卸载清理。
- **🟡-11 [一致性 / R4] 「删除选中」无确认** — `App.tsx:418-436` / `:582`；对照组「清空列表」`541-557` + `682-695` 走 `ConfirmDialog`。两个不可逆删除一个有一个没有，用户无法形成稳定预期。与 🔴-1 同一处修复。
- **🟡-12 [错误处理] `ping-snapshot` 订阅失败被完全静默** — `App.tsx:207-209`。`get_state` 只在挂载时调一次（`:210-213`），**不是周期性兜底**。若 `listen` 失败，界面停在初始快照、状态灯/延迟/统计/事件全部不再更新，而 UI 看上去完全正常。建议至少 `showToast` 提示 + 可选轮询兜底。
- **🟡-13 [重复代码] host 归一化实现两遍** — `AddTargetsDialog.tsx:144`（`findDuplicateHosts` 内）vs `:179-180`（`existingSet` useMemo 内）。将来改口径（去掉末尾点 / IPv6 规范化）会漏一处，出现「提示 N 个重复、实际跳过 M 个」的错位。建议 `const normHost = (h) => h.trim().toLowerCase()` 单一来源。
- **🟡-14 [输入校验] 导入文件没有大小 / 行数上限** — `AddTargetsDialog.tsx:265-287`。`readImportFile`（`api.ts:59`）全量读入内存；批量文本有 `MAX_BATCH = 1024` 兜底，文件侧零校验——几百 MB 的 xlsx 会让前端在 `parseBatch` 里长时间阻塞。建议 `MAX_IMPORT_BYTES = 20MB` 前置拒绝（与 Rust 侧 B4 配対）。
- **🟡-16 [UI] 界面缩放变化时导出菜单不收起** — `Toolbar.tsx:91-100` 只监听 `scroll` / `resize`。改缩放时 `document.documentElement.style.zoom` 变了但**不一定触发 window resize**（视口尺寸没变），菜单会停在换算错误的坐标上。建议在 `App.tsx:244-246` 的 effect 里补 `window.dispatchEvent(new Event("resize"))`。

## 三、💭 细节（8）

| # | 位置 | 说明 |
|---|---|---|
| 💭-1 | `App.tsx:213` | `showToast` 在 `:299` 才定义却在 `:213` 的 effect 里引用（靠 effect 延迟执行才不炸），属「能跑但脆」 |
| 💭-2 | `Toolbar.tsx:265-266`、`App.tsx:590`、`AddTargetsDialog.tsx:302` | 同文件 `:241-242` 注释说明「emoji 由彩色字形渲染，color 无效」并自绘了 🔍 SVG，但主题切换仍用 🌙/☀️、欢迎页 📡、重复提示 ⚠。建议统一自绘 SVG（主题切换优先） |
| 💭-3 | `format.ts:173-177` | `fontFamilyStyle` 直接把族名拼进 CSS 值，含 `"` 或 `\` 会产出非法 font-family。建议 `f.replace(/["\\]/g, "")` |
| 💭-4 | `App.tsx:528-536` / `TargetTable.tsx:124-125` | `selected` 可能残留已删除或被搜索过滤掉的 id；`allSelected` 用 `selected.size === targets.length`，残留会让全选框显示为 indeterminate |
| 💭-5 | `AddTargetsDialog.tsx:167-174` | 对话框关闭再打开不清空 `host`/`name`/`batch`/`filePath`（只在 `doAdd` 成功时清）。保留草稿可能是有意，但建议明确为特性并写注释 |
| 💭-6 | `AddTargetsDialog.tsx:38` | 正则 `(\d{1,3})` 接受 `010.1.1.1` 这类前导零八位组（会被当十进制 10）。严格 IPv4 应拒绝 |
| 💭-7 | `StatusBar.tsx:15` | `item` 缺 `shrink-0`，与 Toolbar 的 `nowrap + shrink-0` 双保险写法不一致（实际不会塌，仅一致性） |
| 💭-8 | `DetailPanel.tsx:243` | 未包 `React.memo`，每次快照重渲染整个日志列表（最多 1000 行 `<li>`）。`logs` 已用 useMemo 稳定，补一个 memo 就能吃到收益 |

## 四、正反馈：写对的关键防护点

1. **R5 hook 顺序 —— 全组件零违规。** `AddTargetsDialog` 的 3 个 `useMemo` 全排在 `if (!open) return null`（`:207`）之前，每个都带注释（`:177`/`:184`/`:192`）；`SettingsDialog` 的 `useEffect` 在 `:224` 之前；`ConfirmDialog` 在 `:42` 之前。这是「白屏 React #310」的直接免疫。
2. **R6 wasOpenRef 模式 —— 两处都用对了。** `SettingsDialog.tsx:206-222` 无依赖数组 + `wasOpenRef`，注释（204-205）直接写明事故表现；`App.tsx:239-246` 只依赖原始值 `ui_scale` / `ui_font_family`，不依赖 `settings` 对象。同一陷阱两处独立处理正确。
3. **R7 —— 唯一弹层已正确除 zoom。** 全仓 `getBoundingClientRect()` 只有 `Toolbar.tsx:81` 一处，紧接着 `:84-85` 除 `currentZoomFactor()`，注释解释了原因；`:91-100` 监听 scroll(capture)+resize 收起、`:177` 点击遮罩关闭——弹层三件套齐全。
4. **R4 —— 全盘零违规。** grep `window.confirm` / `alert` / `prompt` / 裸 `confirm(` 在 `src/` 下**只命中一条注释**（`ConfirmDialog.tsx:1`）。两处不可逆操作（清空列表、清空事件日志）都走自绘 `ConfirmDialog`，且文案写出「同时清除内存与已保存文件」——把 R9 语义传达给了用户。
5. **R11 —— 断点口径纯净。** 全仓 15 处断点样式**全部是 `max-[1199px]:`**，零 `min-[...]`、零 `sm:/md:/lg:/xl:`，宽视口基线未被污染。
6. **R12 —— 语义色无反转。** 全部绿=正常 / 红=失败；DetailPanel 对「0 次失败不着红」（`:137-138`）、「空值不着绿」（`:126-129`）做了专门处理。
7. **R9 双口径各司其职。** 去重用 `seq`（`App.tsx:154-156`）、批次归属用 `session`（`DetailPanel.tsx:215`）；另到 Rust 侧核实 `clear_events` 不回退 seq（`events.rs:388` 只有 `fetch_add`、`state.rs:240` 只重写文件、`events.rs:485-494` 顶高计数器），前端单调 `lastSeqRef` 安全——**已核对、无问题**。
8. **订阅竞态处理正确。** `App.tsx:196-205` 用 `disposed` 标志处理「订阅在清理之后才就绪」，`:214-226` 清理做了 `typeof u === "function"` 判空 + try/catch，React 18 StrictMode 双挂载下不泄漏。
9. **类型与契约干净。** `src/` 下 `any` 逃逸 **0 处**；`tsc --noEmit` **0 error**；`console.log` / `TODO` / `FIXME` 残留 0 处；`types.ts` 与 `api.ts` 的 18 个 invoke 一一对应。
10. **已有测试中真正「能失败」的部分质量很高。** `tests:306-312` 断言 `r[1024] === undefined`（回滚 `out.length < MAX_BATCH` 成 `<=` 立即失败）；`230-247` / `257-268` 把无障碍对比度结论写成可执行断言；`467-479` 跨文件比对 `src/styles.css` 与 `DEFAULT_FONT_STACK` 防漂移；`436-441` `zoomStyle` 钳制测试。
11. **剪贴板三级降级。** `App.tsx:59-82`（Clipboard API → 隐藏 textarea + `execCommand` → 抛错转 toast），注释点明 WebView2 下可能抛 `NotAllowedError`。

## 五、测试覆盖度矩阵

| 模块 / 函数 | 位置 | 当前用例 | 评价 |
|---|---|---|---|
| `expandIpRange` | AddTargetsDialog:36 | 17 | ✅ 充分且有效 |
| `parseBatch` | AddTargetsDialog:84 | 8 | ✅ 充分 |
| `findDuplicateHosts` | AddTargetsDialog:143 | 3 | ⚠️ 纯函数测了，**接线没测**（🟡-15） |
| `tableToBatchText` | AddTargetsDialog:124 | **0** | ❌ 缺失（🟡-4） |
| `defaultEventsFile` | SettingsDialog:94 | 8 | ✅ 充分 |
| `zoomStyle` / `fontFamilyStyle` / 字体栈 | format.ts:161/173/153 | 7 | ✅ 充分（含跨文件守护） |
| `fmtMs` / `fmtPct` / `fmtDuration` | format.ts:5/11/33 | 3 | ✅ 基本覆盖 |
| `rttColorClass` / `statusBadgeClass` / `statusLabel` | format.ts:50/91/58 | 3 | ✅ 充分（含无障碍断言） |
| `fmtTime` / `fmtDateTimeShort` | format.ts:17/25 | **0** | ❌ 缺失 |
| `statusRowClass` / `eventColorClass` | format.ts:75/109 | **0** | ❌ 缺失，R12 守护点 |
| `eventKindLabel` / `isUnreadKind` | format.ts:124/148 | **0** | ❌ 缺失，R9 未读口径 |
| `currentZoomFactor` | format.ts:187 | **0** | ❌ 缺失，R7 另一半 |
| `compare()` 排序 | TargetTable:79 | **0**（未导出） | ❌ 不可测（🟡-6） |
| 事件增量去重 | App.tsx:154 | **0**（在组件内） | ❌ 不可测（🟡-7） |
| 弹层 zoom 换算 | Toolbar:84 | **0** | ❌ 缺失（🟡-8） |
| 契约一致性（types.ts ↔ Rust serde） | — | **0** | ❌ 缺失，仅 Rust 侧 `model.rs` 有 roundtrip |

## 六、Rust 侧 `#[cfg(test)]` 现状与补测优先级

| 模块 | 行数 | `#[cfg(test)]` | 测试数（约） | 密度 |
|---|---|---|---|---|
| `state.rs` | 1471 | ✅ `:882` | 16 | **最低（1/92 行）** |
| `events.rs` | 1289 | ✅ `:731` | 22 | 中 |
| `import.rs` | 592 | ✅ `:153` | 15 | 良 |
| `export.rs` | 528 | ✅ `:272` | 12 | 良 |
| `model.rs` | 512 | ✅ `:261` | 15 | 优 |
| `stats.rs` | 394 | ✅ `:99` | 14 | 优 |
| `pinger/icmp.rs` | 269 | ✅ `:148` | 6 | 良 |
| `pinger/fallback.rs` | 256 | ✅ `:129` | 12 | 优 |
| `fonts.rs` | 186 | ✅ `:128` | 4 | 良 |
| **`commands.rs`** | 189 | ❌ | **0** | — |
| **`pinger/mod.rs`** | 77 | ❌ | **0** | — |
| **`config.rs`** | 55 | ❌ | **0** | — |

补测优先级：

| 优先级 | 目标 | 理由 |
|---|---|---|
| **P0** | `config.rs::load` / `save`（`26-45` / `48-55`） | **R1 事故根因路径且零测试**。`load()` 对「不存在/读失败/解析失败」一律静默回落默认；`save()` 用 `fs::write` **非原子写**，写一半崩溃留下半截 JSON → 下次启动照样全丢。建议同步改为「写临时文件 + rename」 |
| **P1** | `events.rs` 的 `drain_pending` 增量语义 | 前端 `lastSeqRef` 去重完全依赖「同一事件只下发一次」，现有 22 个测试**没有 drain 语义的测试** |
| **P1** | `pinger/mod.rs::Engine::new` 降级路径（`:57-68`） | **R10 事故区**。ICMP 失败时置 `icmp_failed` 并降级 `ping.exe`，只有一句 `eprintln!`；状态层有守护、**引擎选择层零覆盖** |
| **P1** | `state.rs` 的 `snapshot()` / `remove_targets` / `update_target` / `auto_start` | 密度最低的大文件；聚合字段、批量停止、改 host 后事件归属重映射、启动路径均未覆盖 |
| **P2** | `commands.rs` 18 个契约命令 | 建议加一条「命令清单 ↔ `api.ts` invoke 名一一对应」的静态校验测试，比逐个冒烟划算 |
| **P2** | `stats.rs::normalize_settings` 边界 | 缺口在 `events_dir` 为空串/相对路径/含 `~`，以及 `ui_font_family` 超长串/控制字符 |
| **P3** | `import.rs` / `export.rs` / `model.rs` / `icmp.rs` / `fallback.rs` / `fonts.rs` | 覆盖充分，可暂缓 |

---

# 第三部分 · 主理人结论

## 结论

- **v1.1.8 不需要回滚**：线上包没有契约破坏、并发竞态、配置损坏类的已触发故障；13 条红线逐条核对通过；`tsc --noEmit` 干净。
- **但 5 条 🔴 应在 v1.1.9 全部清零**，其中 4 条（A1/A2/A3/A4）是同一条故障链——**配置容错不完整 → 整份配置回落默认 → 主机列表静默清空并随后覆盖落盘**，正是 R1 记录过的最贵教训。只修其中一条都不够。
- **修复后必须走全量通道复核**（`docs/code-review.md` §4.2：R1 相关改动 = L1 自检 + L2 多轮至 🔴/🟡 清零 + L3 发布前走查），并按 §4.4 用一份 v1.1.0 时代的旧 `config.json` 实测升级不丢数据。
- **每条修复都要配「回滚到旧写法就失败」的测试**（§6 测试有效性准则）。

## 建议的修复批次

| 批次 | 内容 | 预估 |
|---|---|---|
| 批次 1（性价比最高） | 🔴 A1（`let _ =` → `?`）+ 🔴 前端 Delete 穿透（含「删除选中」加 ConfirmDialog + 两个对话框补 `role="dialog"`） | ~40 min |
| 批次 2（R1 主线） | 🔴 A2 + A3 + A4（配置字段级默认值 + 类型容错宏 + 备份抢救），同步改 `model.rs:381` 测试 | ~3 h |
| 批次 3（测试基建） | 🟡-1/2（npm test 脚本 + esbuild 显式依赖）、🟡-15（`decideAdd` 抽出让测试能失败）、🟡-6/7/8（compare / consumeEvents / fixedTopFromRect 抽出） | ~2 h |
| 批次 4（行为相关 🟡） | Rust B1/B2/B3/B9 + 前端 🟡-10/12/16 | ~2 h |
| 批次 5（可选） | 其余 🟡（B4/B5/B6/B7/B8、🟡-4/5/9/13/14）与 💭 | 按需 |
