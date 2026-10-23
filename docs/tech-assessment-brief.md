# PingBoard 新功能技术评估包（供外部模型使用）

> 版本基线：**v1.3.0**（2026-10-24）· 面向「另一个模型做技术评估」的只读交接材料
> 本文只交代**事实与约束**，不含结论；评估结论由外部模型自行给出。
> 配套阅读：[`HANDOFF.md`](HANDOFF.md)（状态与待办）、[`code-review.md`](code-review.md)（审查红线）

---

## 〇、怎么用这份文档

把下面这段连同本文一起交给评估模型（**只读评估，不要改代码**）：

```
你在评估 PingBoard（Windows 多主机 ICMP Ping 监视器，Rust + Tauri 2 + React 18）的一个新功能。
输入：docs/tech-assessment-brief.md（技术事实与约束）+ 我在下方给出的功能描述。
要求：
1) 先给结论（推荐 / 有条件推荐 / 不推荐），一句话理由；
2) 至少两个可比方案（含「不做」），逐项对比；
3) 明确指出要改哪些文件、哪些是破坏性契约变更；
4) 逐条核对第 6 节的硬约束，任何一条冲突都必须单独标红；
5) 按第 10 节的输出模板作答，结论必须能追溯到具体文件/符号，不要泛泛而谈。
```

> ⚠️ 评估所需的功能描述**尚未填入**，见第 1 节。没有它，本文只能支撑「可行性体检」，
> 无法支撑「工作量与风险」判断。

---

## 一、待评估功能（待填）

| 项 | 内容 |
|---|---|
| 功能名 | _（待填）_ |
| 一句话目标 | _（待填：用户想达成什么）_ |
| 使用场景 | _（待填：什么人在什么情况下用）_ |
| 明确不做 | _（待填：划清边界，避免评估跑偏）_ |
| 期望形态 | _（待填：新页面 / 新按钮 / 后台自动行为 / 新导出格式 …）_ |

---

## 二、产品定位与不可动摇的语义

- 对标 NirSoft PingInfoView 的**轻量监视器**：装完即用，**免 UAC**（`installMode = currentUser`），
  唯一外部运行时依赖是 WebView2 Evergreen。
- 界面**简体中文、默认浅色**；无组件库、无图表库（折线图是手写 SVG）。
- 三块固定骨架：左侧文件夹侧边栏 / 中部主表 / 右侧详情面板，宽度可拖（1px 视觉线 + 6px 热区）。
- 🩸 **配色语义：绿 = 正常、红 = 失败**（监控语义，**不是**股票涨红跌绿）。
  改徽标或按钮底色观感前必须先问产品负责人。
- 🩸 **空范围就是空范围**：侧边栏一个都不勾 → 主表 0 台、导出 0 台。
  任何「空集回落成全部」都会让用户误以为数据被删。

---

## 三、技术栈与代码地图

| 项 | 事实 |
|---|---|
| 后端 | Rust 2021 · Tauri 2 · `windows` 0.61（Win32 IP Helper） |
| 前端 | React 18 + TypeScript + Tailwind v3.4（**无 UI 库**） |
| 构建 | Vite 5 · `npx tauri build --bundles nsis` |
| 平台 | **仅 Windows**（`pinger/icmp.rs` 无 `cfg(windows)` 门控，代码库不可跨平台编译） |
| 后端规模 | **15 个 .rs 文件 / 7,997 行**（含 `#[cfg(test)]`） |
| 前端规模 | **18 个文件 / 4,882 行** |
| Tauri 命令 | **24 个**（`commands.rs` 23 + `import.rs` 1） |

后端文件（按体量）：

| 文件 | 行数 | 职责 |
|---|---|---|
| `src-tauri/src/state.rs` | 1,994 | `AppState` 聚合根：目标表、设置、文件夹、worker 线程表、统计落盘、快照、命令实现 |
| `src-tauri/src/model.rs` | 1,246 | 数据模型 + **全部 serde 容错反序列化器**（红线 R1 的主战场） |
| `src-tauri/src/events.rs` | 1,178 | 事件日志：分级、去重、增量 `seq`、jsonl 持久化与轮转 |
| `src-tauri/src/export.rs` | 783 | CSV / TXT / HTML 报表生成（15 列口径统一） |
| `src-tauri/src/config.rs` | 574 | 配置读写、损坏备份与抢救、`finalize` 孤儿回落 |
| `src-tauri/src/import.rs` | 526 | 批量导入解析（txt/csv/ip 段展开） |
| `pinger/icmp.rs` · `pinger/fallback.rs` · `stats.rs` · `stats_store.rs` · `fonts.rs` · `commands.rs` · `lib.rs` | 其余 | ICMP 探测 / `ping.exe` 降级 / 统计计算 / 统计持久化 / 系统字体 / 命令层 / 装配入口 |

前端文件：

| 文件 | 行数 | 职责 |
|---|---|---|
| `src/App.tsx` | 1,135 | 全部状态与编排：订阅 `ping-snapshot`、选中/范围、文件夹、导出、标题、快捷键 |
| `src/lib/format.ts` | 584 | **纯函数层**（格式化、判重、宽度钳制、导出范围、标题拼接）——测试主要打在这里 |
| `src/components/SettingsDialog.tsx` | 550 | 设置弹层（含缩放、字体、事件日志） |
| `src/components/AddTargetsDialog.tsx` | 549 | 添加 / 批量粘贴（含 IP 段展开预览） |
| `src/components/TargetTable.tsx` | 317 | 13 列主表、排序、勾选、右键菜单、拖拽起点 |
| 其余 8 个组件 | — | `Toolbar` / `FolderSidebar` / `DetailPanel` / `PaneResizer` / `ExportDialog` / `ConfirmDialog` / `StatusBar` / `Sparkline` + `LatencyChart` |
| `src/lib/api.ts` · `types.ts` · `keyboard.ts` | — | `invoke` 封装（与 24 命令一一对应） / 前端类型（与 Rust 模型镜像） / 键盘判定纯函数 |

---

## 四、运行时契约

### 4.1 命令（24 个，注册清单在 `src-tauri/src/lib.rs`）

`get_state` · `add_targets` · `remove_targets` · `update_target` · `set_enabled` · `start_pinging` ·
`stop_pinging` · `reset_stats` · `update_settings` · `resolve_host` · `export_report` · `get_config_path` ·
`read_import_file` · `list_events` · `clear_events` · `export_events` · `set_target_events` ·
`list_system_fonts` · `take_startup_notice` · `list_folders` · `create_folder` · `update_folder` ·
`delete_folder` · `move_targets`

⚠️ 参数是**扁平结构**（如 `update_target` 收 `{id, name, host, enabled}`），不是 patch 对象。

### 4.2 事件与推送节奏

| 机制 | 事实 |
|---|---|
| 唯一事件 | `ping-snapshot`：每 **500ms** 一次**聚合快照**，前端整体替换状态（不做增量合并） |
| 快照内容 | `targets / settings / running / active_threads / total_sent / total_received / total_failed / loss_pct / started_at / updated_at / session / events` |
| 事件增量 | 快照里的 `events` 是**自上次快照以来的增量**，空闲为 `[]` |
| 统计落盘 | 后台线程每 **30s** 写一次 `pingboard-stats.json`（防异常退出丢数据） |
| 启动提示 | `take_startup_notice` 是 **one-shot**，**没有**塞进快照（快照有 3 个消费方，会被竞争吞掉） |

### 4.3 探测与并发模型

- 主路径 Win32 IP Helper `IcmpSendEcho`，每线程复用句柄；句柄创建失败 → **全局降级**到 `ping.exe`
  （`CREATE_NO_WINDOW`，不闪黑框，解析兼容中英文）。
- **每个运行中的目标一个独立 `std::thread`**；`max_threads` 默认 256（可关闭，关闭后仅保留 **4096** 硬保护）。
- 停止/改间隔用 `AtomicBool` + 分片休眠（100ms），长休眠不阻塞停止指令。

---

## 五、数据模型与持久化

### 5.1 三个运行时文件（`%APPDATA%\com.pingboard.desktop\`）

| 文件 | 内容 | 关键约束 |
|---|---|---|
| `pingboard-config.json` | `AppConfig { version, settings, targets, folders }` | **用户意图**，只在增删改时写 |
| `pingboard-stats.json` | 每目标累计统计 + 趋势 | **「清空统计」必须删除本文件**，否则重启后旧统计复活 |
| `pingboard-events.jsonl` | 事件日志（逐行 JSON） | 8 MiB 轮转到 `.1.jsonl`；启动只读末尾 4 MiB |

### 5.2 字段容错（红线 R1，本项目最贵的教训）

**新增配置字段必须逐字段带 serde 默认值，枚举必须自定义 `deserialize_with`。**
否则旧配置整份反序列化失败 → 回落默认 → **用户全部主机列表消失**。

现有字段的风险等级参考：

- `PingSettings`：`interval_ms / timeout_ms / payload_size / ttl / max_threads / limit_max_threads /
  beep_on_fail / auto_start / history_len / events_on / events_level / events_persist / events_dir /
  events_keep / ui_scale / ui_font_family` —— 每个都有 `default` + 自定义反序列化。
- `TargetConfig`：`name / host / enabled / events_on / folder_id`（`folder_id` 缺省 `None` = 临时区，
  指向不存在的文件夹时**必须回落 `None`，不得丢该主机**）。
- `TargetState`（运行时）：`id / name / host / enabled / resolved_ip / status / sent / received / failed /
  last_rtt_ms / min_rtt_ms / max_rtt_ms / avg_rtt_ms / sum_rtt_ms / ttl / consecutive_fail / loss_pct /
  last_success_ts / history / running / last_error / events_on / folder_id`。
- `Status` 序列化**必须 lowercase**：`idle / resolving / ok / timeout / failed`（前端联合类型依赖）。
- `AppConfig.targets` 的「非对象元素」是**唯一**还能触发「备份 + 提示」的形态，
  🩸 **不要顺手给它加容错**（堵上后，坏配置会被静默覆盖，且所有测试依然全绿）。

---

## 六、不可协商的硬约束（评估时逐条核对）

| # | 约束 | 违反后果 |
|---|---|---|
| 1 | ICMP 只用 Win32 IP Helper `IcmpSendEcho`；禁 raw socket / 第三方 ping 库 | 需管理员权限 / 破坏免 UAC 定位 |
| 2 | 所有子进程必须带 `CREATE_NO_WINDOW (0x0800_0000)`（唯一处在 `pinger/fallback.rs`） | 控制台窗口闪烁 |
| 3 | 契约：24 个命令 + `ping-snapshot` 事件；新增命令必须同步 `lib.rs` 注册与 `api.ts` | 前端 `invoke` 直接失败 |
| 4 | `Status` serde lowercase；前端 TS 联合类型与之镜像 | 状态显示错乱 / 列颜色失效 |
| 5 | 绿 = 正常、红 = 失败；改配色/徽标观感前先问 | 语义反转，用户误判 |
| 6 | 禁用 `window.confirm` / `window.alert`（WebView 不可靠）→ 自绘 `ConfirmDialog` | 弹窗不显示，操作无反馈 |
| 7 | 新增配置字段必须容错（见 §5.2），并补「旧配置哨兵」测试 | **丢用户全部主机列表** |
| 8 | 空范围就是空范围，不许回落成全部 | 用户误以为删了数据 |
| 9 | 原生 HTML5 拖放在 WebView2 + `<table>` 下**不触发**；拖拽靠鼠标事件自实现 | 「拖过去毫无反应」 |
| 10 | 界面缩放开启时，鼠标坐标必须除以 `currentZoomFactor()` | 125% 缩放下拖拽/弹层错位 |

---

## 七、改造面与真实成本（评估工作量的锚点）

新增功能通常落进下面某一类，**连锁改动清单**如下（按经验顺序，不是建议方案）：

| 改动类型 | 必改文件 | 隐藏成本 |
|---|---|---|
| **A. 新增 Tauri 命令** | `commands.rs` → `lib.rs` 注册 → `src/lib/api.ts` → `types.ts` → 调用点 | 漏注册时报错发生在运行时；`api.ts` 与命令必须一一对应（当前 24:24） |
| **B. 新增配置字段** | `model.rs`（字段 + `default` + `deserialize_with`）→ `normalize_settings` → `SettingsDialog.tsx` → `types.ts` | **旧配置兼容测试**：必须断言「旧 JSON 解析后主机列表不丢」；枚举未知值要降级 |
| **C. 新增运行时统计/列** | `TargetState` → `Snapshot` → `stats_store.rs`（落盘结构）→ `stats.rs` → `TargetTable`/`DetailPanel` → `format.ts` 纯函数 | 统计文件结构变更要考虑「旧文件读回」；「清空统计」语义不能被破坏 |
| **D. 新增事件类型** | `events.rs`（枚举 + 分级 + 去重口径）→ 前端等级过滤 → 事件导出 | `seq` 增量与去重口径容易破坏；轮转阈值 8 MiB |
| **E. 新增导出格式/列** | `export.rs`（15 列口径统一 + 转义）→ `ExportDialog.tsx` → 测试 | CSV 转义、UTC 时间口径、HTML 转义三处都得对；筛选只改行集合不改字段 |
| **F. 新增后台周期任务** | `lib.rs` 的 `setup` → `state.rs` | 与 500ms 快照、30s 落盘竞争同一把锁；不能阻塞 UI 线程 |
| **G. 改布局/交互** | `App.tsx` → `PaneResizer.tsx` → `PANE_LIMITS` / `clampPaneWidth` → 新增 `localStorage` 键 | 必须保住「主表 ≥ 320px」与默认观感不变；缩放坐标换算 |
| **H. 协议/网络类（若做告警、远端上报）** | 全新模块 + 设置项 + 凭据存储 | 与「唯一外部依赖 WebView2」的轻量定位冲突；需评估离线行为与失败重试 |

**性能锚点**（评估容量假设时用）：100 台主机 × 1Hz 时，前端只收 2 次/秒的聚合快照；
线程数 = 运行中的目标数（默认上限 256，硬保护 4096）；趋势只保留 `history_len` 个点。

---

## 八、已知限制与技术债（评估时必须计入风险）

### 8.1 已知限制

- ICMP 主路径仅 **IPv4**；IPv6 目标走 `ping.exe -6` 降级。
- 降级路径使用系统默认 32 字节负载，`payload_size` 对降级路径**不生效**。
- 导出报表「最后成功时间」是 **UTC**；事件日志 CSV 是**本地时间**。
- CSV **未防公式注入**（`=`、`+`、`@`、`-` 开头的备注名在 Excel 里可能被当公式）。
- 文件夹**只有一层**，不支持嵌套；删除文件夹不删主机（迁到临时区）。
- ARM64 包未经真机测试；当前版本只构建 x64 在线引导版。

### 8.2 未完成的行为类缺陷（`docs/reviews/baseline-review-2026-09-29.md`）

| 编号 | 内容 |
|---|---|
| B1 | 事件被旁路消费（导出 / 快照路径吃掉 `drain_pending()` 的增量） |
| B2 | 清空操作未清 pending |
| B3 | 幽灵 worker（停止后线程残留判定） |
| B9 | 删除目标时未同步清理其事件 |

### 8.3 测试基建的坑

- 前端测试是**纯函数测试**（esbuild 转译真实 TSX → 打桩 React/Tauri），
  **对「事件不触发」「state 连线错误」完全无能为力**。历史 9 个用户可见缺陷全部属于这一类。
- 存在 1 条**假测试**：「单个添加绕过查重」的回归测试，把修复回滚后依然全绿。
- 因此新功能的评估结论里，凡涉及 DOM 事件 / React 状态连线 / 后端 wiring 的，
  **必须给出「需要人工端到端实点」的验收项**。

---

## 九、验证与交付流程（评估测试方案时的基线）

```powershell
# 后端（本机 cargo 需绕开 rustup shim）
$env:PATH = "C:\Users\22534\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin;" + $env:PATH
cd src-tauri; cargo test --lib          # 期望 179 passed / 0 failed

cd ..; npm test                          # 期望 174 passed / 0 failed
npx tsc --noEmit                         # 期望 0 error
```

- **变异验证是硬要求**：任何防护性改动都要给出「把修复回滚 → 测试变红」的实测输出。
- 打包前必须 `Remove-Item -LiteralPath "dist" -Recurse -Force`（vite 的 `rmSync` 会被沙箱拦）。
- 版本声明**五处**必须同步：`package.json`、`package-lock.json`、`src-tauri/tauri.conf.json`、
  `src-tauri/Cargo.toml`、`src-tauri/Cargo.lock`（外加 README 里的安装包文件名）。
- 交付物校验：`Get-FileHash <安装包> -Algorithm SHA256` + 字节数，写入 `docs/release-notes-*.md`。
- 当前基线：`PingBoard_1.3.0_x64-setup.exe` = 2,149,339 B ·
  `8C69A2B60DAC16410372F3424D2779A818656B45E4E3721AFE43B7CFC827153F`。

---

## 十、评估输出模板（外部模型按此作答）

```markdown
## 1. 结论
推荐 / 有条件推荐 / 不推荐 —— 一句话理由。

## 2. 方案对比
| 方案 | 做法 | 优点 | 缺点 | 主要改动文件 |
（至少两个，含「不做」）

## 3. 数据与持久化影响
- 新增/变更字段、默认值、兼容策略、是否需要迁移、旧文件能否读回。

## 4. 契约影响
- 新增/变更命令（是否破坏 24 命令契约）、事件、导出列；
- 是否有破坏性变更，前端需要同步的 `api.ts` / `types.ts` 改动点。

## 5. 前端改动面
- 组件、`App.tsx` 状态、`format.ts` 可测试纯函数；是否需要新增 localStorage 键。

## 6. 性能与并发
- 100 / 1,000 台主机下的线程、快照体积、落盘频率、锁竞争分析。

## 7. 硬约束核对
| # | 约束 | 是否冲突 | 说明 |
（逐条对照 §6 的 10 条）

## 8. 测试方案
- 纯函数用例（可自动）+ 必须人工实点的验收项；
- 给出至少一条「回滚到旧实现就失败」的用例设计。

## 9. 工作量
- 按文件列出改动点与预估（人天），标明可并行/串行。

## 10. 风险与否决条件
- 触发即放弃的条件（例如「必须管理员权限」「必须引入新运行时」）。
```

---

## 十一、参考文档索引

| 文件 | 何时读 |
|---|---|
| [`HANDOFF.md`](HANDOFF.md) | 状态表、硬约束、未完成待办、环境坑 |
| [`code-review.md`](code-review.md) | 审查裁决依据：三级分级、红线 R1–R14、三通道 |
| [`folder-design.md`](folder-design.md) | 文件夹与统计持久化的设计定稿、不变量 C1–C3 |
| [`export-filter-design.md`](export-filter-design.md) | 导出筛选口径、15 列字段、兼容性保证 |
| [`events-log-design.md`](events-log-design.md) | 事件日志分级、去重、轮转、未读红点 |
| [`release-notes-1.3.0.md`](release-notes-1.3.0.md) | 当前版本相对 v1.2.1 的差异 |
| [`reviews/baseline-review-2026-09-29.md`](reviews/baseline-review-2026-09-29.md) | 历史基线复审（5 🔴 / 25 🟡 / 16 💭） |

---

## 十二、评估环境须知

- 项目**只能在 Windows 上编译**（Win32 API + Tauri bundler）。
- 本机 cargo 被 rustup shim 挡住，需显式把工具链 `bin` 加到 `PATH`（见 §9）。
- `dist/` 与 `src-tauri/target/` 不参与版本控制，克隆后需 `npm install` 与重新编译。
- 沙箱对「删除 `dist`」有 safe-delete 拦截，PowerShell 的 `Remove-Item -LiteralPath` 可用。
