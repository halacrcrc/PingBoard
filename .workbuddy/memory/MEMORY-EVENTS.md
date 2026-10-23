# PingBoard 事件日志 —— 细节备忘（v1.1.4 引入）

> 从 `MEMORY.md` 拆出：这节是**改动 `src-tauri/src/events.rs` / `state.rs` / `scheduler.rs` 或事件相关前端前必读**。
> 起因报障「详情页最近日志一直没内容」→ 根因：事件原先在**前端**用两次快照 diff 生成、规则写死 `ok→fail`/`fail→ok`，
> 而后端真实迁移是 `idle → resolving → 失败` → 启动就连不上、DNS 失败永不记录。
> 修复 = **迁移到后端状态迁移点打点**（根因修复）。
> 📖 完整设计（类型定义 / 字段表 / 时序图 / 轮转与读回）见仓库 **`docs/events-log-design.md`**（24 KB）。

## 枚举与分级
- **`EventKind` 8 种**（serde snake_case）：`fault` / `unreachable` / `dns_fail` / `recover` / `first_ok` /
  `start` / `stop` / `config_change`。
- **`EventLevel` 3 档**：`Fault` / `Standard`（默认）/ `Detail`。
  **全量入库，展示时按 `rank()` 过滤**（不是按档位写库）。

## 🩸 边沿触发去重（最容易写错的一处）
- 纯函数 `classify_transition(prev, next, src) -> Option<EventKind>`；`timeout→timeout` 返回 `None`。
- **`prev` 口径 = 本轮循环开始、置 `Resolving` 之前的状态**。
  拿 `Resolving` 当 `prev` 会让**持久性 DNS 失败每轮重复记一条**。
- `FailSource { Resolve, Probe }` 区分 DNS 失败（`DnsFail`）与探测失败（`Unreachable`）。
- `StopReason { User, Internal }`：用户主动停止记 `stop`；程序退出 / 删除目标**不记**。
  命令 `stop_pinging` 传 `User`。

## 增量推送与 seq 口径
- `Snapshot.events` 是**增量**，前端只处理 `seq > lastSeq`（严格去重）；全空闲时 payload = `[]`。
- ⚠️ `seq` 是**会话内单调**、非全局唯一，但**不构成可达缺陷**：`load_hosts` 会把计数器顶到
  **所有读回事件（含孤儿事件）的最大 seq 之上**，而读的又是文件末尾 → 最新事件必在读回范围内。已补测试固化。
- **切批次 / 判断是否本轮启动，请用 `session`，不要用 `seq`。**

## 持久化
- `pingboard-events.jsonl`（与配置文件同目录，除非设了 `events_dir`），**追加写**、
  **8 MiB 轮转**为 `pingboard-events.1.jsonl`。
- 启动时**归档与主文件都读**（各取末尾 4 MiB），按 `ts` 升序合并后交 `load_hosts` 按 `keep` 截断。
- 🩸 **清空一致性**：`filter_out_host` 必须对**主文件与归档都执行**；过滤后全空则**删除文件**（不留 0 字节残件）；
  坏行保留。⚠️ 只清主文件会与「归档读回」叠加出**「清空了又自己回来」** —— 二者必须一起改。

## 会话标记与展示
- `session` = 每次启动时刻（纪元毫秒），写进每条事件；`Snapshot.session` 给前端。
- 非本次会话的日志显示「月-日 时:分:秒」，标题旁加「含历史」；本次会话显示「时:分:秒」。
- 导出 CSV 增「会话（启动于）」列（**7 → 8 列**）；旧行缺该字段 → 反序列化为 `0`、导出为空列。

## 配置字段与开关
- **5 个新 `PingSettings` 字段**：
  `events_on`（总开关，默认 true）/ `events_level`（默认 `Standard`，**自定义 `deserialize_with`**）/
  `events_persist`（默认 true）/ `events_dir`（`Option<String>`，空 = 默认目录，前端调资源管理器选径）/
  `events_keep`（收敛为 `{50,200,1000}`，默认 200）。
- **两级开关**：设置内总开关 + `DetailPanel` 单主机开关
  （`TargetConfig.events_on` / 运行时 `TargetState.events_on`，`new()` 置 true）。
- **新增 4 个命令**：`list_events` / `clear_events` / `export_events`（CSV，**本地时间**）/ `set_target_events`。

## 🩸 未读故障红点（用户拍板方案 A）
- 标在**主机列表行的备注名右侧**：`TargetTable` name 单元格改 flex +
  `w-2 h-2 rounded-full bg-red-600 dark:bg-red-500 shrink-0`；点开该主机即清零。
- ⚠️ `unreadIds` 必须用 `useMemo` 保引用稳定，否则每 500ms 快照击穿 `TargetTable` 的 `React.memo`。
- 旧设计（把红点画在**详情面板**）是**死代码**：「红点只画选中主机」与「非选中主机才计未读」逻辑互斥。

## 文案前缀手法
- `state.rs` 的 resolve 错误自带「解析失败：」前缀，`events.rs` 拼接前必须
  `e.strip_prefix("解析失败：").unwrap_or(e)`，否则出现「解析失败：解析失败：…」。已补 2 个回归测试。
