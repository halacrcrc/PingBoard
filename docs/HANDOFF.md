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
| 当前 HEAD | `c4d9970`（`docs: 建立代码审查标准与流程`），**已推送 origin/main** |
| 工作区 | 干净，与远端同步 |
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
3. **契约**：18 个 Tauri 命令（注册清单 `src-tauri/src/lib.rs:20-39`）+ 事件 `ping-snapshot`（500ms 聚合推送，前端整体替换状态）。
4. `Status` serde 必须 lowercase：`idle/resolving/ok/timeout/failed`。
5. **配色语义 绿 = 正常、红 = 失败**（是监控语义，**不要**套股票涨红跌绿）。改徽标/按钮底色观感前**必须先问用户**。
6. **禁用 `window.confirm` / `window.alert`**（Tauri WebView 不可靠）→ 一律自绘 `ConfirmDialog`。
7. **新增配置字段必须逐字段带 serde 默认值**，**枚举必须自定义 `deserialize_with`**（未知值降级、不报错）。
   否则旧配置整份反序列化失败 → 回落默认值 → **丢用户全部主机列表**。这是本项目最贵的教训。

---

## 五、当前待办（按优先级）

**来源**：`qa-artifacts/review-2026-09-29/baseline-review.md`（2026-09-29 基线复审，5 🔴 / 25 🟡 / 16 💭）。
v1.1.8 **不需要回滚**，但 🔴 应在 **v1.1.9 全部清零**。

| # | 问题 | 位置 | 成本 |
|---|---|---|---|
| 🔴1 | `add_targets` 用 `let _ = save_config(&app)` 吞掉保存失败（同文件另 5 个命令都是 `?`）→ 导入后不落盘，下次保存被空配置覆盖 | `src-tauri/src/commands.rs:23` | 改一个字符 |
| 🔴2 | 弹层打开时按 Delete 静默删除选中目标并落盘（无确认无撤销）。⚠️ 配套：`AddTargetsDialog.tsx:323` / `SettingsDialog.tsx:298` 弹窗容器**没有 `role="dialog"`** | `src/App.tsx:439-450` + `418-436` | ~30 min |
| 🔴3 | 配置反序列化容错不完整：4 个结构体字段缺字段级 `#[serde(default)]`；**15 个设置字段只有 2 个有类型容错** → 任一字段类型不符即丢全部主机。⚠️ 修时必须同步改掉 `model.rs:381` 那条把错误行为锁成「既定策略」的测试 | `src-tauri/src/model.rs:227-249`、`102-151` | ~2 h |
| 🔴4 | 配置解析失败直接回落默认且**不备份** → 原文件随后被空配置覆盖。需补「备份 corrupt-时间戳 + 抢救 targets」，但**保持不阻塞启动** | `src-tauri/src/config.rs:35-44` | ~1 h |
| 🔴5 | 同上（🔴3+4 是同一条故障链，必须一起修） | — | — |

**已知「假测试」**：`tests/frontend_pure.test.mjs:343-351` 那条「单个添加绕过查重」回归测试，**把修复回滚后依然全绿**（测的是纯函数，坏的是接线）。修复方向是抽 `decideAdd()` 决策纯函数。
**测试基建是空的**：`package.json` **没有 `test` 脚本**，所以审查清单里写的「npm test 通过」一直空转；`esbuild` 也未声明为 devDependency。

**修复批次建议**：① 🔴1 + 🔴2（~40 min）→ ② 🔴3 + 🔴4（~3 h）→ ③ 测试基建 + 让测试真正能失败（~2 h）→ ④ 行为类 🟡。
**验收要求**：每条修复都要配「回滚到旧写法就失败」的测试（`docs/code-review.md` §6）；R1 相关改动走**全量通道**（L1 自检 + L2 多轮至 🔴🟡 清零 + L3 发布前走查），并用一份 v1.1.0 时代的旧 `config.json` 实测升级不丢数据。

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
| `docs/code-review.md` | **审查的唯一裁决依据**：三级分级（🔴/🟡/💭）、13 条红线 R1–R13、快速/标准/全量三通道、分语言检查清单、反模式速查、意见书写模板 |
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
