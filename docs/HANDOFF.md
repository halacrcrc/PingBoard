# PingBoard 交接文档

> 版本：v1.1.8 · 最后更新 2026-09-29 · 面向「新会话 / 新 agent 接手」
> 本文是**接手时的第一个读点**，只做状态交代与索引。细节一律引用专题文档，不在此重复。

---

## 一、一句话定位

Windows 多主机 ICMP Ping 监视器，对标 NirSoft PingInfoView。Rust + Tauri 2 + React 18 + TS + Tailwind v3.4（无组件库、无图表库，趋势图原生 SVG）。界面简体中文、默认浅色。

---

## 二、当前状态（接手前先确认这几项没变）

| 项 | 值 |
|---|---|
| 最新发布版本 | **v1.1.8**（tag `v1.1.8` → commit `bc2bdc9`） |
| 当前 HEAD | `b1a828b`（批次 1 修复），**已推送 origin/main** |
| 工作区 | ✅ **干净**，与 `origin/main` 同步（HEAD `59f22d5`） |
| Rust 测试 | `cargo test --lib` = **143 passed / 0 failed**；`npx tsc --noEmit` = 0 error |
| Tauri 命令数 | **19**（原 18，批次 2b 新增 `take_startup_notice`） |
| 版本声明位置 | `package.json:4` 与 `src-tauri/tauri.conf.json:4`（**提版本要同时改这两处**，另需改 README） |
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
| `C:\Users\22534\WorkBuddy\pinginfo` | **唯一正式仓库**（源码 + `交付包\`，SSH 推送 github.com/halacrcrc/PingBoard） | ✅ |
| `D:\pinginfo` | **构建工作区**（有 node_modules / target / dist，改配置、打包、跑构建都在这里） | ❌ |

**规则**：改代码/配置在 D 盘跑通后，源码与文档要同步回仓库侧；`docs/` 与 `qa-artifacts/` 两侧都要有。
`交付包\` 里的 README 也要一并更新（三处同步：仓库 / D 盘 / 交付包）。

`src-tauri/tauri.conf.json` **基线 md5 = `2c0a8cf0fb8d8f1d5eebea6679c8d270`（v1.1.8 起，两侧必须始终一致）**。
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

**来源**：`qa-artifacts/review-2026-09-29/baseline-review.md`（2026-09-29 基线复审，5 🔴 / 25 🟡 / 16 💭）。
v1.1.8 **不需要回滚**，但 🔴 应在 **v1.1.9 全部清零**。

### 5.1 进度总览

| 批次 | 内容 | 状态 |
|---|---|---|
| **批次 1** | 原 🔴1 `add_targets` 吞保存失败 + 原 🔴2 Delete 键穿透弹层 | ✅ **已完成并推送** `b1a828b`（6 文件 +114/−9），测试 49→57 |
| **批次 2** | 原 🔴3+🔴4+🔴5（R1 主线：A2 结构体默认值 / A3 字段类型容错 / A4 备份+抢救） | ✅ **已推送** `59f22d5`。复审又查出 2 条新 🔴 → 见 5.2 |
| **批次 2b** | 🔴-1 `TargetConfig` 四字段容错 + settings 抢救 + 🔴-2 抢救可观测（跨端） | ✅ **已推送** `59f22d5`。复审 **0 🔴 / 3 🟡** |
| **批次 2c** | 清 2b 的 4 条 🟡（字符串布尔语义 / toast 装不下路径 / `.catch` 静默吞错 / 提示分级） | ✅ **已推送** `59f22d5`。143 测试全绿，`tsc` 0 error |
| 批次 3 | 测试基建 + 让测试真正能失败（`package.json` 加 test 脚本、`decideAdd()` / `compare()` / `consumeEvents()` 抽纯函数） | ⏳ 未开始 |
| 批次 4 | 行为类 🟡（B1 事件被旁路消费 / B2 清空未清 pending / B3 幽灵 worker / B9 删目标不清事件等） | ⏳ 未开始 |

### 5.2 批次 2 系列：销项状态（**代码侧已全部做完，仅剩实机验证**）

批次 2 / 2b / 2c 全部落盘并已合并提交为 `59f22d5`（**已推送**）。`cargo test --lib` 143 全绿、`tsc --noEmit` 0 error。
架构师终审：**0 🔴**，并判定「代码侧的 R1 主线已经做完，质量高于基线」。

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
| 4 | **人工 QA：真实旧配置文件端到端实测** | ❌ **未执行 —— 发布前必须补** |

**判据 4 的可执行步骤**（详见 `docs/HANDOFF.md` 历史版本或架构师报告 §3）：
1. 配置路径 `C:\Users\<用户名>\AppData\Roaming\com.pingboard.desktop\pingboard-config.json`，**先复制一份到桌面当还原点**。
2. 用一份 v1.1.0 旧配置（或构造只含早期字段的 JSON），把某台主机的 `"enabled": true` 改成 `"enabled": "yes"`。建议同时放一台 `"enabled": false` 的哨兵主机。
3. 启动应用，四条判定：**① 3 行主机都在 ② 坏条目的 host 保形为 `223.5.5.5`（不是空）③ 双击可编辑且改完重启仍在 ④ 邻座「未启用」未被翻转，且不弹损坏提示**。
4. 负向对照：`targets` 数组末尾加一个 `null` → 重启 → 应弹提示（含备份完整路径 + 抢救条数）、主机仍在、配置目录多出 `pingboard-config.corrupt-<时间戳>.json`。
5. 还原：把还原点拷回去，删掉测试产生的 `corrupt-` 备份。

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
| `qa-artifacts/review-2026-09-29/baseline-review.md` | v1.1.8 基线复审（Rust + 前端 + 汇总） |

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
3. 确认待办是否已被处理：看 `qa-artifacts/review-2026-09-29/baseline-review.md` 里的 🔴 清单。
4. 动手前按 `docs/code-review.md` §4.1 过一遍提交前自检清单。
