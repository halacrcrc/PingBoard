# PingBoard 交接文档

> 版本：v1.3.0 · 最后更新 2026-10-24 · 面向「新会话 / 新 agent 接手」
>
> 本文是**接手时的第一个读点**，只交代状态与索引；细节一律引用专题文档，不在此重复。
> 阅读顺序建议：§二 状态表 → §四 硬约束 → §五 待办 → 其余按需。

---

## 一、一句话定位

Windows 多主机 ICMP Ping 监视器，对标 NirSoft PingInfoView。
Rust + Tauri 2 + React 18 + TS + Tailwind v3.4（**无组件库、无图表库**，趋势图原生 SVG）。界面简体中文、默认浅色。

---

## 二、当前状态（接手先核对这些没变）

| 项 | 值 |
|---|---|
| 线上最新 Release | **v1.2.1**（GitHub Release 仍停在上一版，三包齐全） |
| 当前版本 | **v1.3.0** —— 代码与文档已对齐、x64 在线引导包已构建，**待打 tag / 上传 Release** |
| 当前 HEAD | **以 `git log -1` 为准**（本文不写死 SHA）。🩸 本地与远程 **SHA 不同属正常**，一致性**只看 tree 哈希**，见 §三 |
| 测试 | `cargo test --lib` = **179 passed / 0 failed**；`npm test` = **174 passed / 0 failed**；`npx tsc --noEmit` = **0 error** |
| Tauri 命令数 | **24**（`commands.rs` 23 + `import.rs` 1；注册清单 `src-tauri/src/lib.rs`） |
| 版本声明位置 | **五处**：`package.json`、`package-lock.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock`。🩸 `package-lock.json` 是 v1.1.9 时漏掉的第 5 处；另需同步 README 安装包名 |
| 代码规模 | 约 **12,900 行**（`src-tauri/src/**/*.rs` 与 `src/**/*.ts{,x}` 实测）：Rust **7,997 行 / 15 文件**（`state.rs` 1994、`model.rs` 1246、`events.rs` 1178）+ 前端 **4,882 行 / 18 文件**（`App.tsx` 1135、`format.ts` 584） |
| 平台 | **Windows x64 为主，最低 Windows 10**；ARM64 为历史实验包（**未经真机测试**，v1.3.0 未构建） |
| 唯一外部运行时依赖 | WebView2 Evergreen Runtime（**不需要** VC++ 运行库 / .NET / `WebView2Loader.dll`） |
| 🩸 本环境 cargo 限制 | 沙箱拦 rustup shim（`C:\Users\22534\.cargo\bin\cargo.exe`）。用工具链真 cargo：<br>`$env:PATH = "C:\Users\22534\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin;" + $env:PATH`<br>（`node` / `npm` / `npx tsc` 不受影响） |

**v1.3.0 已构建包**（`src-tauri/target/release/bundle/nsis/`）：

| 附件 | 体积 | SHA-256 | 说明 |
|---|---|---|---|
| `PingBoard_1.3.0_x64-setup.exe` | 2,149,339 B | `8C69A2B60DAC16410372F3424D2779A818656B45E4E3721AFE43B7CFC827153F` | 在线引导（默认） |

> 本版**只构建 x64 在线引导版**；内置引导版与 ARM64 实验包需自行切换 `webviewInstallMode` 打包。
> 目录里残留的 `PingBoard_1.2.1_*.exe`（10-06）是上一版产物，**发布前别误传**。

---

## 三、工作目录与推送

### 3.1 只有一个位置（2026-10-23 起）

旧的「双位置约定」**已作废**。原仓库侧 `C:\Users\22534\WorkBuddy\pinginfo` 已被删除，
`D:\pinginfo` 原本没有 `.git`，现已就地恢复（`git init` + `remote add` + `fetch` + `reset origin/main`，工作区文件未动）。

| 位置 | 作用 | `.git` |
|---|---|---|
| `D:\pinginfo` | **唯一工作区 + 唯一仓库**（源码、`node_modules`、`target`、`dist` 都在这里） | ✅ |

- 不再需要「D 盘改完同步回 C 盘」。
- ⚠️ `qa-artifacts/`（探针、校验脚本）与 `build-v1*/`、`*-extract-*/`、`icon-work/` 都在 `.gitignore` 里，
  **别往里放需要入库的文档**；长期文档放 `docs/`。
- 工作区若再次丢失：`git clone https://github.com/halacrcrc/PingBoard.git` 即完整恢复（含 tag 与文档）；
  `node_modules` 与 `src-tauri/target` 需重装 / 重编。

### 3.2 🩸 本地与远程 SHA 不同是正常的

本环境 **git 传输层不稳定**（`push` / `fetch` 常报 `Recv failure: Connection was reset` 或超时），
历次推送多走 **GitHub API 兜底**（`qa-artifacts/push_via_api.mjs`）。
API 创建的 commit 与本地 commit **内容相同但 SHA 不同**，看起来像分叉，其实是同一棵树。

**判断同步只看 tree 哈希：**

```
本地：git rev-parse 'HEAD^{tree}'
远程：GET /repos/halacrcrc/PingBoard/commits/main  ->  commit.tree.sha
两者相同 = 文件内容逐字节一致
```

网络恢复后对齐：`git fetch origin && git reset origin/main`（工作区已一致，不丢文件）。

### 3.3 基线 md5 与发版禁忌

- `src-tauri/tauri.conf.json` **基线 md5 = `9377624eb871a28b58b3d99cb5196660`（v1.3.0 起）**。
  🩸 **提版本后基线必然变化，必须重记**（v1.2.1 的旧基线 `1abf882e…`、v1.1.8 的 `2c0a8cf0…` 均已作废）。
- 测 `webviewInstallMode` / `additionalBrowserArgs` 时，只改构建侧配置，**测完立刻还原并核对 md5**
  （`json.dump` 会重排格式，md5 必变）。
- 🩸 `.gitignore` 必须排除 `交付包/` 与 `build-v1*/`：里面是 200 MB+ 的安装包，
  **超过 GitHub 单文件 100 MB 硬上限**，误 `git add -A` 会被直接拒收。安装包只走 Release 附件。
  推文件时用 `git add <具体路径>`，**永远不要在仓库根裸跑 `git add -A`**。

---

## 四、不可协商的硬约束（改动前必读）

1. **ICMP 只能用 Win32 IP Helper `IcmpSendEcho`**（`windows` 0.61）。禁 raw socket / surge-ping。
   注意 `pinger/icmp.rs` 无 `cfg(windows)` 门控 → **代码库不可跨平台编译**。
2. 所有 spawn 子进程必须带 `CREATE_NO_WINDOW (0x0800_0000)`（全仓唯一处在 `pinger/fallback.rs`）。
3. **契约**：**24 个 Tauri 命令**（注册清单 `src-tauri/src/lib.rs`）+ 事件 `ping-snapshot`
   （500ms 聚合推送，前端整体替换状态）。
4. `Status` serde 必须 lowercase：`idle / resolving / ok / timeout / failed`。
5. **配色语义 绿 = 正常、红 = 失败**（监控语义，**不要**套股票涨红跌绿）。改徽标 / 按钮底色观感前**必须先问用户**。
6. **禁用 `window.confirm` / `window.alert`**（Tauri WebView 不可靠）→ 一律自绘 `ConfirmDialog`。
7. **新增配置字段必须逐字段带 serde 默认值，枚举必须自定义 `deserialize_with`**（未知值降级、不报错）。
   否则旧配置整份反序列化失败 → 回落默认值 → **丢用户全部主机列表**。这是本项目最贵的教训。
8. **空范围就是空范围**：侧边栏一个都不勾 → 主表 0 台、导出 0 台。
   任何「空集回落成全部」都会让用户以为自己删了数据。

---

## 五、当前真正的待办

### 5.1 批次 4：行为类 🟡（唯一遗留，未开始）

来源：`docs/reviews/baseline-review-2026-09-29.md`（5 🔴 / 25 🟡 / 16 💭）。🔴 已全部清零，余下的 🟡 属行为类：

| 编号 | 内容 |
|---|---|
| B1 | 事件被旁路消费（导出/快照路径吃掉 `drain_pending()` 的增量） |
| B2 | 清空操作未清 pending |
| B3 | 幽灵 worker（停止后线程残留判定） |
| B9 | 删除目标时未同步清理其事件 |

**验收要求**：每条修复都要配「回滚到旧写法就失败」的测试（`docs/code-review.md` §6）；
R1 相关改动走**全量通道**（L1 自检 + L2 多轮至 🔴🟡 清零 + L3 发布前走查）。

### 5.2 已知「假测试」（需要重写的那条）

`tests/frontend_pure.test.mjs` 里那条「单个添加绕过查重」的回归测试，
**把修复回滚后依然全绿** —— 因为它测的是纯函数，而坏的是接线。
修复方向：抽出 `decideAdd()` 决策纯函数，让测试打在真实决策点上。

> 测试基建本身已补齐（`package.json` 有 `test` 脚本，`esbuild` 已声明为 devDependency，当前 174 项）。

---

## 六、已完成批次（历史归档，仅供追溯）

### 6.1 版本进度总览

| 批次 / 版本 | 内容 | 状态 |
|---|---|---|
| 批次 1 | `add_targets` 吞保存失败 + Delete 键穿透弹层 | ✅ `b1a828b` |
| 批次 2 / 2b / 2c | R1 主线：结构体默认值 / 字段类型容错 / 备份+抢救 / notice 可观测 | ✅ `59f22d5` |
| 批次 3 | 测试基建 + 让测试真正能失败（抽纯函数） | ✅ 前端测试 57→81 |
| 批次 5 | 报表导出筛选（零丢包 / 全部未成功） | ✅ 随 v1.1.9 发布 |
| v1.2.0 A–D | 文件夹数据模型 / 状态与命令层 / 统计持久化 / 侧边栏与导出联动 | ✅ 已发布 |
| v1.2.0 E–F | 清空列表只清临时区 + 添加/导入选文件夹 + 导出范围跟随侧边栏 | ✅ 已发布 |
| v1.2.1 | 拖拽归入 + 右键通道 + 三区域宽度 + 判重限同文件夹 | ✅ 已发布 |
| v1.3.0 | 标题栏显示版本号 + 分隔条细化（1px 线）+ 详情区双竖线修复 | ✅ 代码/文档完成，待发版 |
| **批次 4** | 行为类 🟡（见 §5.1） | ⏳ **未开始** |

### 6.2 v1.2.x 期间修掉的 9 个用户可见缺陷（2026-10-05 ~ 10-06）

**全部是「纯函数测试全绿但功能是坏的」**，记录在此以防重犯：

| # | 现象 | 根因 |
|---|---|---|
| 1 | 添加主机选了文件夹却进临时区 | `add_targets` 调 `TargetState::new`，而它**硬编码 `folder_id: None`**，把前端传来的值吞了 |
| 2 | 侧边栏「取消勾选」后反而只剩临时区 | 写成 `folders.forEach(f => onToggle(f.id))`，**逐个 toggle** 把没勾的勾上了 |
| 3 | 清空/删除 IP 后侧边栏台数不变 | 台数取自 `listFolders().count`，它**只在增删文件夹时刷新**（`refreshFolders` 全项目仅 1 处调用） |
| 4 | 打开应用看不到文件夹 | 启动 effect 里**没有加载 folders**，`folders=[]` → 台数全被当孤儿算进临时区 |
| 5 | 删除文件夹功能不存在 | `askDelete` state + ConfirmDialog + `onDelete` **全部实现完毕，却没有任何入口** |
| 6 | 冻结的台数：删到 0 仍显示 1 | `counts?.get(id) ?? fallback.get(id) ?? 0`，**`??` 穿透**到滞后的 fallback |
| 7 | 「归入」下拉显示旧台数 | 侧边栏改了实时统计，下拉仍直接渲染 `f.count`（只修一个消费点，没做源头覆盖） |
| 8 | 复选框勾一个会取消之前的 | 表头全选用 `new Set(filtered)` **替换**整个选中集合，清掉范围外的选中 |
| 9 | 拖 IP 到文件夹完全无反应 | 原生 HTML5 拖放在 **WebView2 + `<table>` 下 `dragstart` 不触发**；已改鼠标事件自实现 |

**共性教训**：这些缺陷**没有一条能被纯函数测试覆盖** —— 坏的是后端 wiring、React state 连线、DOM 事件链路。
以后改动涉及这三处，**必须在纯函数之外做一次端到端实点**。

### 6.3 R1 闭合记录（2026-09-29 ~ 10-02，已全部闭合）

四条判据全满足：①字段级容错修复 + 哨兵测试 ②`take_startup_notice` 通道打通且用户真读得到
③`load` 与 `parse_with_recovery` 合一 ④真实旧配置端到端人工 QA。

关键实现（与原设想不同，备查）：notice **没有**塞进 `Snapshot` 字段，而是新增第 19 个命令
`take_startup_notice`。理由：`snapshot()` 有 3 个调用方（`get_state` / 500ms 发射器 / 导出），
one-shot notice 会被竞争消费者吞掉，且发射器启动早于前端 mount。

**人工 QA 复现要点（下次端到端 QA 直接照抄）：**

1. `cargo build` 后**必须**先 `npm run dev`（debug exe 走 `devUrl=http://localhost:1420`，否则白屏）。
2. 后台起进程要用后台任务，**`cmd &` 起的子进程会在工具返回时被杀**。
3. 坐标点击不准（缩放 + `ui_scale` 影响）→ **改用 CDP**：`additionalBrowserArgs` 加
   `--remote-debugging-port=9333`，`GET /json/list` 取 `webSocketDebuggerUrl`，
   `Runtime.evaluate` 读 DOM，还能 `window.__TAURI_INTERNALS__.invoke('get_state')` 直问后端。
4. Tauri 命令参数是**扁平**的（`update_target` 收 `{id, name, host, enabled}`，**不是 patch 对象**）。
5. 改 `tauri.conf.json` 测完**必须还原并核对基线 md5**。
6. ⚠️ 「未开始」徽标是 `Status`（探测状态 `idle`），**与 `enabled` 无关** ——
   v1.1.2 起表格已无 `enabled` 列，别拿徽标判断是否启用，要调 `get_state`。

---

## 七、必须记住的两个承重点

### 7.1 `AppConfig.targets` 的窄路是承重结构

**不要给 `targets` 加 `deserialize_with` 容错，也不要在抢救里把非对象元素「静默跳过」。**
经过全字段容错后，「targets 数组里出现非对象元素」（裸字符串 / `null` / 数字）是**唯一**还能让整份配置
解析失败、从而触发「备份 + 提示」的形态。**谁顺手堵上它，触发器就永久消失** →
原文件在退出时被静默覆盖，**比现在更糟**，而且所有测试依然全绿（测试验的是解析成功，不是验有没有触发备份）。

理想形态是**解耦**（启动时无条件留滚动备份 + 一旦发生任何丢弃/降级就发 notice），属未来改进方向。

### 7.2 方法论（改配置 / 写防护性代码时必用）

1. **哨兵字段套路**：只断言「targets 不丢」的测试会**假绿** —— 整块兜底会掩盖单点失效。
   必须在同一个 settings 块里塞一个合法字段（如 `history_len: 333`）并断言它存活。
2. **测试前提会随修复过期**：加容错后，原本用它制造「配置损坏」的用例会失效。
   **加容错时先检查哪些测试在拿它当损坏触发器。**
3. **「删掉它，哪个用例会红？」必须给实测输出**：防护性改动都要回答，且是**实测红色输出**，不能只推理。
4. **变异必须打在被测行为本身上**：打在错的地方会出现假绿（例如给字符串布尔做变异却改 `Value::Bool` 臂）。
   **变异还是绿时，先怀疑变异打错了地方，再怀疑测试无效。**
5. **抢救/损坏类用例必须带前提守卫**：`assert!(serde_json::from_str::<AppConfig>(text).is_err(), "前提：这段文本必须真的解析失败")`。
   没有它，一旦字段被加容错，「损坏」形态变合法，用例会**静默失效**而不是响亮失败。

---

## 八、专题文档索引（改对应模块前必读）

记忆库 `.workbuddy/memory/`（随仓库入库）：

| 文件 | 覆盖内容 |
|---|---|
| `MEMORY.md` | 项目定位、硬约束、三大陷阱、UI 铁律、功能口径速查、基线 md5 |
| `MEMORY-UI.md` | 界面 / 样式 / 配色 / `SettingsDialog`、缩放 zoom 与弹层定位 |
| `MEMORY-EVENTS.md` | 事件日志：枚举分级、去重口径、增量与 `seq`、持久化轮转、清空一致性、未读红点 |
| `MEMORY-RELEASE.md` | 打包 / 发版 / 交付 / 安装测试 / QA 脚本、三包构建与 digest 校验 |

代码侧文档 `docs/`：

| 文件 | 用途 |
|---|---|
| `code-review.md` | **审查的唯一裁决依据**：三级分级（🔴/🟡/💭）、红线 R1–R14、三通道、分语言清单、反模式、意见模板 |
| `tech-assessment-brief.md` | **新功能技术评估包**（给外部模型的只读交接）：架构地图、24 命令契约、硬约束、改造面成本、评估输出模板 |
| `folder-design.md` | **主机文件夹 + 统计持久化**（v1.2.0 设计定稿）：数据模型、侧边栏交互、不变量 C1–C3、勾选语义 |
| `export-filter-design.md` | **报表导出筛选**：判定标准、15 列字段结构、兼容性保证、变异验证记录 |
| `events-log-design.md` | 事件日志设计稿 |
| `release-notes-1.3.0.md` | v1.3.0 与 v1.2.1 的逐条差异（标题栏版本号 / 分隔条细化 / 双竖线修复） |
| `release-notes-1.2.1.md` | v1.2.1 与 v1.2.0 的逐条差异（拖拽 / 宽度 / 判重口径） |
| `release-notes-1.2.0.md` | v1.2.0 与 v1.1.9 的逐条差异（文件夹 / 统计持久化 / 行为变更） |
| `release-notes-1.1.9.md` | v1.1.9 与 v1.1.8 的逐条差异 |
| `reviews/baseline-review-2026-09-29.md` | v1.1.8 基线复审（Rust + 前端 + 汇总） |

---

## 九、发版流程要点

完整流程见技能 **`github-push-and-release`**（本机**未装 gh CLI**，走 SSH push + GitHub API，凭据在 `~/.my-credentials`）；
打包 / 白屏 / CDP / 图标 / PE 校验见技能 **`tauri2-windows-package-and-verify`**。

**三个必记的坑：**

- 🩸 `tauri build` 前**必须用 PowerShell 清 `dist`**（vite 的 `rmSync` 会被沙箱 safe-delete 守护拦 →
  `beforeBuildCommand failed exit 1`）。v1.1.8 发版时命中 3 次。
- 自 v1.1.8 起**不再构建「完整离线版」**（217 MB，性价比低）；改为随版发布 **ARM64 包**（标注未经真机测试）。
- **两种 x64 变体产物同名**，每构建完一个必须**立刻改名另存**，否则被下一次覆盖。
  校验「包内是不是新版」：在**包内解出来的 exe** 里搜 UTF-16LE 版本串，应命中 **2 次**且不含旧版本号
  （用 `bytes.count` 精确匹配，**别用正则**）。

---

## 十、本机环境坑（会浪费时间的那些）

- rustc 1.98.1 (msvc) · VS2022 + SDK 10.0.26100 · Node 22.22.2 · WebView2 153.x。
- `wmic` 在沙箱黑名单 → 查进程用 Python ctypes。
- 删 `%APPDATA%` 下文件只能用 PowerShell `Remove-Item -LiteralPath`（`rm` 被 safe-delete 守护拦）。
- 本机 6 个 `msedgewebview2.exe` 属 `SearchHost.exe`，与本项目无关（别误判为残留）。
- curl 访问 GitHub 需 `--ssl-no-revoke`（schannel 报 `CRYPT_E_REVOCATION_OFFLINE`）；
  Python / Node 调 GitHub API 需 `NO_PROXY='*'`。
- 🩸 **不要用 PowerShell 数组 / ArrayList 拼装后重写源文件**（本项目三次写坏过 `.md` 与 `.tsx`）：
  编辑已存在文件一律用定点替换；新建文件用 `write_to_file`。

---

## 十一、接手时建议的第一步

1. `git log --oneline -5` + `git status`，确认工作区干净、基线是 v1.3.0（tag `v1.2.1` 之后）。
2. 跑一遍基线：`cd src-tauri && PATH=<工具链bin>: $PATH cargo test --lib`（应 179 绿）、`npm test`（应 174 绿）、`npx tsc --noEmit`（应 0 error）。
3. 读 `.workbuddy/memory/MEMORY.md`（系统会自动注入主文件，**专题三份要自己读**）。
4. 确认待办状态：只剩 §5.1 的批次 4 与 §5.2 的假测试。
5. 动手前按 `docs/code-review.md` §4.1 过一遍提交前自检清单。
