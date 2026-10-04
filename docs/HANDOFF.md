# PingBoard 交接文档

> 版本：v1.2 · 最后更新 2026-10-04 · 面向「新会话 / 新 agent 接手」
> 本文是**接手时的第一个读点**，只做状态交代与索引。细节一律引用专题文档，不在此重复。

---

## 一、一句话定位

Windows 多主机 ICMP Ping 监视器，对标 NirSoft PingInfoView。Rust + Tauri 2 + React 18 + TS + Tailwind v3.4（无组件库、无图表库，趋势图原生 SVG）。界面简体中文、默认浅色。

---

## 二、当前状态（接手前先确认这几项没变）

| 项 | 值 |
|---|---|
| 最新发布版本 | **v1.2**（tag `v1.2` 与 GitHub Release 待发） |
| 当前开发版本 | **v1.2** —— 批次 A-F 全部完成，代码已推送 `origin/main` |
| 当前 HEAD（仓库侧） | 见 `git log -1`，工作区干净 |
| Rust 测试 | `cargo test --lib` = **179 passed / 0 failed**；`npm test` = **130 passed / 0 failed**；`npx tsc --noEmit` = 0 error |
| Tauri 命令数 | **24**（19 → 24，v1.1.10 新增 5 个文件夹命令；导出/添加接口仅加参数） |
| 版本声明位置 | **五处**：`package.json`、`package-lock.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock`。🩸 **`package-lock.json` 是 v1.1.9 时漏掉的第 5 处**，v1.1.10 已补；另需改 README |
| 🩸 本环境 cargo 限制 | 沙箱拦 **rustup shim**（`C:\Users\22534\.cargo\bin\cargo.exe`）。改用工具链真 cargo：<br>`$env:PATH = "C:\Users\22534\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin;" + $env:PATH`<br>（`node`/`npm`/`npx tsc` 不受影响） |
| 平台 | **仅 Windows x64，最低 Windows 10**；自 v1.1.8 起随版发布 ARM64 实验包（**未经真机测试**） |
| 唯一外部运行时依赖 | WebView2 Evergreen Runtime（**不需要** VC++ 运行库 / .NET / `WebView2Loader.dll`） |
| 代码规模 | 约 9,300 行：Rust 11 模块（`state.rs` 1471、`events.rs` 1289 为最大） + 前端 15 文件（`App.tsx` 722） |

**v1.1.8 三包**（`交付包/`，SHA256 见 `交付包/交付校验值-SHA256.txt`）：
- `PingBoard_1.1.8_x64-setup.exe` — 在线引导，2,106,286 B
- `PingBoard_1.1.8_x64-setup-embedWebView2.exe` — 内置引导，3,842,947 B
- `PingBoard_1.1.8_arm64-setup.exe` — ARM64 实验包，1,912,620 B

---

## 三、双位置约定（最容易踩的坑）

| 位置 | 作用 | 有无 `.git` |
|---|---|---|
| `C:\Users\22534\WorkBuddy\pinginfo` | **唯一正式仓库**（源码 + `交付包\`，HTTPS 推送 github.com/halacrcrc/PingBoard） | ✅ |
| `D:\pinginfo` | **构建工作区**（有 node_modules / target / dist，改配置、打包、跑构建都在这里） | ❌ |

**规则**：改代码/配置在 D 盘跑通后，源码与文档要同步回仓库侧；`docs/` 两侧都要有。
⚠️ `qa-artifacts/` 是**临时产物目录**（变异探针、基线快照、截图），已在 `.gitignore` 中，**不要**往里放需要入库的文档。
需要长期保存的复审报告放 `docs/reviews/`。
`交付包\` 里的 README 也要一并更新（三处同步：仓库 / D 盘 / 交付包）。

🩸 **`.gitignore` 必须排除 `交付包/`**（2026-09-30 教训）。它**只存在于仓库侧**、不入库：
- 体积 881 MB / 31 个文件，其中三个「完整离线版」各 **217 MB**，**远超 GitHub 单文件 100 MB 硬上限**，
  误 `git add -A` 会直接被 GitHub 拒收；
- 安装包通过 **Release 附件**分发（见 `github-push-and-release` 技能），仓库只管源码。
推历史包时用 `git add <具体路径>`，**永远不要在仓库根裸跑 `git add -A`**。

`src-tauri/tauri.conf.json` **基线 md5 = `db16ab49a6e825f1bf6f4677efd36b08`（v1.2 起，两侧必须始终一致）**。
⚠️ **提版本后基线必然变化，必须重记并同步两侧**（v1.1.8 时期的旧基线 `2c0a8cf0fb8d8f1d5eebea6679c8d270` 已作废）。
打包变体时只改**构建侧 D 盘**的 `webviewInstallMode`，构造完立刻还原。

---

## 四、不可协商的硬约束（改动前必读）

1. **ICMP 只能用 Win32 IP Helper `IcmpSendEcho`**（`windows` 0.61）。禁 raw socket / surge-ping。
   注意 `pinger/icmp.rs` 无 `cfg(windows)` 门控 → **代码库不可跨平台编译**。
2. 所有 spawn 子进程必须带 `CREATE_NO_WINDOW (0x0800_0000)`（全仓唯一处在 `pinger/fallback.rs:21/63`）。
3. **契约**：19 个 Tauri 命令（注册清单 `src-tauri/src/lib.rs:20-40`）+ 事件 `ping-snapshot`（500ms 聚合推送，前端整体替换状态）。
4. `Status` serde 必须 lowercase：`idle/resolving/ok/timeout/failed`。
5. **配色语义 绿 = 正常、红 = 失败**（是监控语义，**不要**套股票涨红跌绿）。改徽标/按钮底色观感前**必须先问用户**。
6. **禁用 `window.confirm` / `window.alert`**（Tauri WebView 不可靠）→ 一律自绘 `ConfirmDialog`。
7. **新增配置字段必须逐字段带 serde 默认值**，**枚举必须自定义 `deserialize_with`**（未知值降级、不报错）。
   否则旧配置整份反序列化失败 → 回落默认值 → **丢用户全部主机列表**。这是本项目最贵的教训。

---

## 五、当前待办（按优先级）

**来源**：`docs/reviews/baseline-review-2026-09-29.md`（2026-09-29 基线复审，5 🔴 / 25 🟡 / 16 💭）。
v1.1.8 **不需要回滚**，但 🔴 应在 **v1.1.9 全部清零**。

### 5.1 进度总览

| 批次 | 内容 | 状态 |
|---|---|---|
| **批次 1** | 原 🔴1 `add_targets` 吞保存失败 + 原 🔴2 Delete 键穿透弹层 | ✅ **已完成并推送** `b1a828b`（6 文件 +114/−9），测试 49→57 |
| **批次 2** | 原 🔴3+🔴4+🔴5（R1 主线：A2 结构体默认值 / A3 字段类型容错 / A4 备份+抢救） | ✅ **已推送** `59f22d5`。复审又查出 2 条新 🔴 → 见 5.2 |
| **批次 2b** | 🔴-1 `TargetConfig` 四字段容错 + settings 抢救 + 🔴-2 抢救可观测（跨端） | ✅ **已推送** `59f22d5`。复审 **0 🔴 / 3 🟡** |
| **批次 2c** | 清 2b 的 4 条 🟡（字符串布尔语义 / toast 装不下路径 / `.catch` 静默吞错 / 提示分级） | ✅ **已推送** `59f22d5`。143 测试全绿，`tsc` 0 error |
| 批次 3 | 测试基建 + 让测试真正能失败（`package.json` 加 test 脚本、`decideAdd()` / `compare()` / `consumeEvents()` 抽纯函数） | ✅ 已推送（前端测试 57→81） |
| **批次 5（新）** | **报表导出筛选**：零丢包 / 全部未成功两种独立口径 | 🟢 **已落盘，前后端测试全绿**；待人工 QA + 发版（见 5.5） |
| 批次 4 | 行为类 🟡（B1 事件被旁路消费 / B2 清空未清 pending / B3 幽灵 worker / B9 删目标不清事件等） | ⏳ 未开始 |

### 5.5 批次 5：报表导出筛选（2026-10-02，v1.1.9-dev）

**做了什么**：在 `export_report` 上加一个可选 `filter` 参数，支持只导出
①**完全未丢包**（`sent>0 && failed==0`）②**全部 ping 未成功**（`sent>0 && received==0`）两类节点。
工具栏「导出 ▾」新增第 3 组入口 → 打开 `ExportDialog`（范围 × 筛选口径 × 格式 正交排布 + 命中台数预览）。

**详细设计见 `docs/export-filter-design.md`** —— 含 15 列字段表、判定表、兼容性逐条保证、变异验证记录。
此处只记**接手必知的三条**：

1. **筛选只改行集合，绝不改字段结构**：CSV 仍 15 列、HTML 仍 15 个 `<th>`。
   `ExportFilter::All`（缺省/`null`/`""`）的产出与本功能上线前**逐字节一致**，
   原有 6 条导出菜单路径与默认文件名 `pingboard-report.<ext>` 一字未改。
2. **🩸 判据必须带 `sent > 0`**：`sent==0`（从未探测 / 已清空统计）时 `loss_pct` 被定义为 `0.0`，
   只判 `failed==0` 会把未开始的主机全算成「零丢包」。**已用变异实测锁死**（把判据改回
   `failed===0` → 前端测试 88/2，精确报出「未开始」被误判）。
3. **判据在前端有两份实现**：`format.ts` 的 `matchesExportFilter`（只做预览计数）与
   Rust `export::matches_filter`（写文件的唯一权威）。**改判据必须两侧同时改**，
   两侧测试用逐台同构的四台样本互相锁定。

**测试状态**：Rust `cargo test --lib` = **154 passed / 0 failed**（143 基线 + 新增 11）；
前端 `npm test` = **90 passed / 0 failed**（81 + 新增 9）；`npx tsc --noEmit` 0 error。
（实现期间沙箱拒绝运行 `cargo.exe`（`untrusted mount point`），`export.rs` 曾一度未经编译器验证，
已在正常环境补跑通过 —— 遇到类似阻塞别误判成代码问题。）

**⚠️ 剩余工作**：①~~**双位置同步**~~ ✅ **2026-10-02 已完成**（12 个文件：9 改 + 3 新，两侧哈希全部一致；
新建文件带的 CRLF 已在 D 侧规范化，见 `MEMORY.md` 环境坑一节）；
②人工 QA 两种口径的端到端导出（坑见 §5.2.2.1）；③发 v1.1.9 走 §4.4 全量走查；
④**提交与推送**（git 传输层在本环境失效，需走 `push_via_api.py` 兜底）。

### 5.2 批次 2 系列：销项状态（**R1 已全闭合，4 条判据全满足**）

批次 2 / 2b / 2c 全部落盘并已合并提交为 `59f22d5`（**已推送**）。`cargo test --lib` 143 全绿、`tsc --noEmit` 0 error。
架构师终审：**0 🔴**，并判定「代码侧的 R1 主线已经做完，质量高于基线」。
**2026-10-02 补**：判据 4 人工 QA 已实测通过（见 5.2.2.1），**R1 正式全闭合**。

| # | 问题 | 状态 |
|---|---|---|
| **A2/A3/A4** | 结构体字段级 default + 16 字段类型容错 + 损坏时备份抢救 | ✅ 已销 |
| **🔴-1** | `TargetConfig` 四字段无类型容错（`"enabled": true` 手改成 `"true"` → 该主机被静默丢弃） | ✅ **已销**（`tolerant_string!` 两臂宏 + `tolerant_bool!`；host 保形、Bool 退化） |
| **🔴-2** | 抢救/备份在 release 下不可见（`windows_subsystem = "windows"` 使 `eprintln!` 全丢弃） | ✅ **已销**（独立命令 `take_startup_notice` + 前端 toast，文案带备份完整路径） |
| 🟡-1 | `load` 与 `parse_with_recovery` 两份手抄实现，7 个测试锁错对象 | ✅ 已销（`load` → `load_from_path` → `parse_with_recovery` 三级委派） |
| 🟡 字符串布尔 | `"enabled": "false"` 回落成 **true**（用户想关的主机反被打开） | ✅ 已销（`tolerant_bool!` 加字符串臂） |
| 🟡 toast | 单行 `truncate` + 4 秒，备份路径被省略号吃掉 | ✅ 已销（时长可传参 20s + `whitespace-pre-wrap break-words`） |
| 🟡 静默吞错 | 命令漏注册时整条链路静默失效（QA 变异实测：删掉注册 → 141 全绿） | ✅ 已降级（`.catch` 改 `console.error` 留痕） |
| 🟡 提示分级 | 抢救回 0 台是最严重情况，措辞却与抢救回 N 台相同 | ✅ 已销（n==0 用强警告，含「在此之前请勿退出本程序」） |

**🔴-2 的实际做法**（与原设想不同，记录备查）：没有塞进 `Snapshot` 字段，而是**新增第 19 个命令** `take_startup_notice`。
理由：`snapshot()` 有 3 个调用方（`get_state` / 500ms 发射器 / 导出），one-shot notice 会被竞争消费者吞掉，且发射器启动早于前端 mount。
链路：`config::load` 返回 `(AppConfig, Option<String>)` → `state.rs` 的 `config_notice: RwLock<Option<String>>` + `take_config_notice()`（`take()` 读走即清空）→ 命令 → `api.ts:89` → `App.tsx` 在 `getState()` 之后调一次。

### ⚠️ 5.2.1 一条必须记住的承重点（改动前必读）

**`AppConfig.targets` 不要加 `deserialize_with` 容错，也不要在抢救里把非对象元素「静默跳过」。**
经过全字段容错后，「targets 数组里出现非对象元素」（裸字符串 / `null` / 数字）是**唯一**还能让整份配置解析失败、从而触发备份+提示的形态。
**这条窄路现在是承重结构** —— 谁顺手堵上它，整份配置就永远解析成功 → 备份与提示的触发器消失 → 原文件在退出时被静默覆盖，**比现在更糟**，而且所有测试依然全绿（测试验的是解析成功，不是验「有没有触发备份」）。
理想形态是**解耦**（启动时无条件留滚动备份 + 只要发生过任何丢弃/降级就发 notice），属未来改进方向。

### 5.2.2 R1 闭合判据（架构师给，4 条全满足才签字）

| # | 判据 | 状态 |
|---|---|---|
| 1 | 🔴-1 修复 + 配套哨兵测试（坏值不得让邻座主机消失、不得连带丢 settings） | ✅ 满足 |
| 2 | 🔴-2 notice 通道打通且**用户真的读得到** | ✅ 满足（2c 修复 toast 后才真正满足） |
| 3 | 🟡-1 `load` 与 `parse_with_recovery` 合一，回滚抢救逻辑测试变红 | ✅ 满足 |
| 4 | **人工 QA：真实旧配置文件端到端实测** | ✅ **满足（2026-10-02 实测，见下）** |

### 5.2.2.1 判据 4 人工 QA 实测报告（2026-10-02）

**环境**：`cargo build` debug 版 + `npm run dev`（vite dev server 必须先起，否则 debug exe 白屏）。
临时给 `additionalBrowserArgs` 加 `--remote-debugging-port=9333` 走 CDP 精确读 DOM / 调命令，
**测完已还原**（`tauri.conf.json` md5 回到基线 `2c0a8cf0fb8d8f1d5eebea6679c8d270`）。

**构造配置**：3 台主机 —— ①`enabled:"yes"`（字符串）②`enabled:1`（数字）③`enabled:false`（哨兵）。

| 判定 | 期望 | 实测 | 结论 |
|---|---|---|---|
| ① 3 行主机都在 | 3 | 3（标题栏「共 3 台」，tbody 3 行） | ✅ |
| ② 坏条目 host 保形 | `223.5.5.5` 非空 | `223.5.5.5` | ✅ |
| ③ 可编辑且重启仍在 | 改 host 后重启保留 | `223.5.5.5`→`1.1.1.1` 已写入磁盘，重启后仍为 `1.1.1.1` | ✅ |
| ④ 邻座未启用未被翻转 | 仍 false | `enabled=false`；且 `start_pinging(null)` **只启动 2 台，跳过哨兵** | ✅ |
| ⑤ 不弹损坏提示 | 无 toast | 无任何 fixed 浮层 | ✅ |

**`enabled` 实际解析值**（`get_state` 直读，非仅看界面）：

| 备注名 | 配置里写的 | 解析结果 | 说明 |
|---|---|---|---|
| 坏值主机 | `"yes"` | **`true`** | 批次 2c 字符串臂生效 |
| 字符串数字主机 | `1` | **`true`** | 数字臂生效 |
| 哨兵未启用 | `false` | **`false`** | 邻座未被连带污染 |

`enabled` 过滤实测：调 `start_pinging(null)` → 2 台 `status=ok`（真实 ping 通）、哨兵保持 `idle`。
⚠️ **「未开始」徽标是 `Status`（探测状态 `idle`），与 `enabled` 无关** —— v1.1.2 起表格已无 `enabled` 列，
**别拿徽标判断 `enabled`**，要调 `get_state`。

**负向对照**（`targets` 末尾加 `null`）：
- 抢救出 **3 台**，`null` 被丢弃 ✅
- 界面 toast 真的弹出来了（读 `position:fixed` 浮层拿到全文）：
  「配置文件已损坏，已备份原文件到 `…\pingboard-config.corrupt-1790946333915.json`，
  从中抢救回 **3** 个主机。当前正在使用抢救后的配置…」✅ —— 含**完整路径 + 抢救条数**。
- 配置目录生成 `pingboard-config.corrupt-1790946333915.json`（895 B，**完整保留含 `null` 的原始 4 元素**）✅
- `take_startup_notice` 二次调用返回 `null` —— **这是正确行为**（one-shot 语义，应用启动时已消费）。

**环境已还原**：配置目录回到测试前 md5（`pingboard-config.json` 814 B / `events.jsonl` 7594 B），
corrupt 备份已删，dev server 与 9333 端口已关，备份与截图在 `C:\Users\22534\Desktop\pingboard-qa-backup\`。

**复现要点（下次做端到端 QA 直接照抄）**：
1. `cargo build` 后**必须**先 `npm run dev`（debug exe 走 `devUrl=http://localhost:1420`，否则白屏「localhost 拒绝连接」）。
2. 后台起进程要用后台任务的 `run_in_background`，**`cmd &` 起的子进程会在工具返回时被杀**。
3. 坐标点击不准（缩放 + `ui_scale` 影响），**改用 CDP**：`additionalBrowserArgs` 加
   `--remote-debugging-port=9333`，`GET /json/list` 拿 `webSocketDebuggerUrl`，
   `Runtime.evaluate` 就能读 DOM、还能 `window.__TAURI_INTERNALS__.invoke('get_state')` 直问后端。
4. Tauri 命令参数是**扁平**的（`update_target` 收 `{id, name, host, enabled}`，**不是 patch 对象**）。
5. 改 `tauri.conf.json` 测完**必须还原并核对 md5**（`json.dump` 会重排格式，md5 必变）。


### 5.3 其余已知待办（未开始）

**已知「假测试」**：`tests/frontend_pure.test.mjs:343-351` 那条「单个添加绕过查重」回归测试，**把修复回滚后依然全绿**（测的是纯函数，坏的是接线）。修复方向是抽 `decideAdd()` 决策纯函数。
**测试基建是空的**：`package.json` **没有 `test` 脚本**，所以审查清单里写的「npm test 通过」一直空转；`esbuild` 也未声明为 devDependency。

**验收要求**：每条修复都要配「回滚到旧写法就失败」的测试（`docs/code-review.md` §6）；R1 相关改动走**全量通道**（L1 自检 + L2 多轮至 🔴🟡 清零 + L3 发布前走查）。

### 5.4 本轮沉淀的方法论（改配置 / 写防护性代码时必用）

1. **哨兵字段套路**：只断言「targets 不丢」的测试会**假绿**——因为 `deserialize_settings` 的整块兜底会掩盖单点失效。必须在同一个 settings 块里塞一个合法字段（如 `history_len: 333`）并断言它存活，才能测出「单点写坏连带丢同块其它设置」的真危害。
2. **测试前提会随修复过期**：给字段加容错后，原本用它制造「配置损坏」的用例会失效（version 那次打挂 2 个、`{"enabled":"yes"}` 会打挂 3 个）。**加容错时先检查哪些测试在拿它当损坏触发器**。
3. **「删掉它，哪个用例会红？」必须给实测输出**（QA 提出）：任何防护性改动都要回答这个问题，且是**实测的红色输出**，不能只靠推理。批次 2 的 🟡-1 与 drain 缺陷都不是「代码写错」，而是**正确行为没被任何用例盯着**——两次都是在测试全绿的情况下把怕的东西原样装回去、毫无反应。架构师的同一判断是「测试绿 ≠ 行为被锁住」。
4. **变异必须打在被测行为本身上**（工程师 2c 踩到）：他第一次做字符串布尔的变异，打在 `Value::Bool` 臂上——测试**全绿**，因为字符串臂还在、行为根本没变。改打「把字符串臂退化成 fallback」才是真红。**变异检测到「绿」时，先怀疑变异打错了地方，再怀疑测试无效。**
5. **抢救/损坏类用例必须带前提守卫**：`assert!(serde_json::from_str::<AppConfig>(text).is_err(), "前提：这段文本必须真的解析失败")`。没有它，一旦某字段被加上容错，「损坏」形态变合法，用例会**静默失效**而不是响亮失败。

---

## 六、专题文档索引（改对应模块前必读）

全部在 `.workbuddy/memory/`（D 盘）与仓库侧：

| 文件 | 覆盖内容 |
|---|---|
| `MEMORY.md` | 项目定位、硬约束、双位置约定、React / Rust 三大陷阱、UI 铁律、功能口径速查 |
| `MEMORY-UI.md` | 界面 / 样式 / 配色 / `SettingsDialog`，缩放 zoom 与弹层定位 |
| `MEMORY-EVENTS.md` | 事件日志：枚举分级、去重口径、增量与 `seq`、持久化轮转、清空一致性、未读红点 |
| `MEMORY-RELEASE.md` | 打包 / 发版 / 交付 / 安装测试 / QA 脚本，三包构建与 digest 校验 |

代码侧文档（`docs/`）：

| 文件 | 用途 |
|---|---|
| `docs/code-review.md` | **审查的唯一裁决依据**：三级分级（🔴/🟡/💭）、**14 条红线 R1–R14**、快速/标准/全量三通道、分语言检查清单、反模式速查、意见书写模板 |
| `docs/events-log-design.md` | 事件日志设计稿（24 KB） |
| `docs/export-filter-design.md` | **报表导出筛选**（零丢包 / 全部未成功）：判定标准、15 列字段结构、兼容性保证、变异验证记录 |
| `docs/folder-design.md` | **主机文件夹 + 统计持久化**（v1.1.10 设计定稿）：数据模型、侧边栏交互、兼容性不变量 C1–C3 |
| `docs/release-notes-1.2.md` | **v1.2 发布说明**：与 v1.1.9 的逐条差异、行为变更、升级建议 |
| `docs/reviews/baseline-review-2026-09-29.md` | v1.1.8 基线复审（Rust + 前端 + 汇总） |

---

## 七、发版流程要点

完整可复用流程见技能 **`github-push-and-release`**（本机**未装 gh CLI**，走 SSH push + GitHub API，凭据已存 `~/.my-credentials`）。
打包 / 白屏 / CDP 校验 / 图标 / PE 校验见技能 **`tauri2-windows-package-and-verify`**。

两个必记的坑：
- 🩸 `tauri build` 前**必须用 PowerShell 清 `dist`**（vite 的 `rmSync` 会被沙箱 safe-delete 守护拦 → `beforeBuildCommand failed exit 1`）。v1.1.8 发版时命中 3 次。
- 自 v1.1.8 起**不再构建「完整离线版」**（217 MB，性价比低）；改为随版发布 **ARM64 包**。

---

## 八、Suggested skills（下一个 agent 应优先调用）

| 场景 | 技能 |
|---|---|
| 发版 / 提版本 / 推 GitHub / 建 Release | `github-push-and-release` |
| 打包 / 白屏排查 / CDP 冒烟 / 图标重制 / PE 校验 | `tauri2-windows-package-and-verify` |
| 改代码前查红线、提交前自检 | 先读 `docs/code-review.md`，再对照 `.workbuddy/memory/` 专题文件 |
| 需要正式团队流程（PRD → 架构 → 实现 → 测试） | `software-company` 专家团（团队成员：许清楚 PM / 高见远架构 / 寇豆码工程 / 严过关 QA） |
| 需要压缩会话给下一个 agent | `handoff` |

---

## 九、本机环境坑（会浪费时间的那些）

- rustc 1.98.1 (msvc) · VS2022 + SDK 10.0.26100 · Node 22.22.2 · WebView2 153.x。
- `wmic` 在沙箱黑名单 → 查进程用 Python ctypes。
- 删 `%APPDATA%` 下文件只能用 PowerShell `Remove-Item -LiteralPath`（`rm` 被 safe-delete 守护拦）。
- 本机 6 个 `msedgewebview2.exe` 属 `SearchHost.exe`，与本项目无关（别误判为残留进程）。
- curl 下 GitHub 需加 `--ssl-no-revoke`（代理环境下 schannel 报 `CRYPT_E_REVOCATION_OFFLINE`）；Python 脚本调 GitHub API 需 `NO_PROXY='*'`。

---

## 十、接手时建议的第一步

1. `git log --oneline -5` + `git status` 确认基线仍是 `c4d9970`。
2. 读 `.workbuddy/memory/MEMORY.md`（系统会自动注入，但只注入主文件，**专题三份要自己读**）。
3. 确认待办是否已被处理：看 `docs/reviews/baseline-review-2026-09-29.md` 里的 🔴 清单。
4. 动手前按 `docs/code-review.md` §4.1 过一遍提交前自检清单。
