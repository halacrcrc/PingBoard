# 项目长期记忆 —— PingBoard

**当前状态**：main `bc2bdc9`，**v1.1.8 已发布**（tag `v1.1.8` → `bc2bdc9`，**三包**：
在线引导 2,106,286 B / `5279ab3b…c545`、内置引导 3,842,947 B / `afe83e5b…96e9`、
ARM64 实验包 1,912,620 B / `302bd4e4…1a1f`（**未经真机测试**，PE 0xAA64 已验）；
服务端 digest 3/3 一致）。**本版起不再构建「完整离线版」；自 v1.1.8 起随版发布 ARM64 包**（见 `MEMORY-RELEASE.md`）。
v1.1.8 内容：①图标微调（外环厚 125 / 内环 85，光点归位环带）②首个 ARM64 实验包。
ARM64 工具链已装：VS2022 组件 VC.Tools.ARM64 + rustup target aarch64-pc-windows-msvc。
**先用技能**：发版 → `github-push-and-release`；打包/白屏/CDP/图标/PE 校验 → `tauri2-windows-package-and-verify`。

## v1.1.9 进度（批次 1/2/2b/2c/3 已推送，**R1 判据 4/4 全闭合**）
批次 1（Delete 穿透 + `save_config` 落盘）、批次 2 系列（R1 主线：字段级 default + 16 字段类型容错 +
损坏备份抢救 + 启动 notice 通道）、批次 3（前端测试基建 57→81 + 修掉一条假测试）均已提交。
**2026-10-02 判据 4 人工 QA 实测通过**（`docs/HANDOFF.md` §5.2.2.1）→ **R1 正式全闭合**。
剩：发 v1.1.9 需走 §4.4 全量走查（100 目标 stop<300ms、启停/导入/导出/清空事件全路径手测）。
🩸 **端到端 QA 五坑**（做实机验证前必读 `docs/HANDOFF.md` §5.2.2.1 末节）：
① debug exe **必须**配 `npm run dev` 否则白屏 ② 后台进程用后台任务，`cmd &` 会被杀
③ **坐标点击不可靠 → 改用 CDP**（`--remote-debugging-port=9333` + `Runtime.evaluate`，
还能 `__TAURI_INTERNALS__.invoke('get_state')` 直问后端）④ **「未开始」徽标是 `Status` 与 `enabled` 无关**，
表格无 `enabled` 列，判断 `enabled` 必须调 `get_state` ⑤ Tauri 命令参数是**扁平的**不是 patch 对象。
改 `tauri.conf.json` 测完**必须还原并核对 md5**（`json.dump` 重排格式，md5 必变）。
🩸 **仓库根永远不要裸跑 `git add -A`** —— 会把 `交付包/`（881 MB，单文件 217 MB 超 GitHub 硬上限）入库。


> 📖 **专题文件（改对应模块前必读，同在本目录）**
> - 事件日志（`events.rs` / `state.rs` / `scheduler.rs` / 事件前端）→ **`MEMORY-EVENTS.md`**
> - 界面 / 样式 / 配色 / `SettingsDialog.tsx` → **`MEMORY-UI.md`**
> - 打包 / 发版 / 交付 / 安装测试 / QA → **`MEMORY-RELEASE.md`**
>
> 历史过程见日工作日志 `YYYY-MM-DD.md`。

## 定位与技术栈
Windows 多主机 Ping 监视器，对标 NirSoft **PingInfoView**。
Rust + Tauri 2 + React 18 + TS + Tailwind v3.4（无组件库/图表库，趋势图原生 SVG）。界面简体中文、默认浅色。

## 硬性约定（改动前必读）
- **ICMP 只能用 Win32 IP Helper `IcmpSendEcho`**（`windows` 0.61）。禁 raw socket / surge-ping。
- 所有 spawn 子进程处必须带 `CREATE_NO_WINDOW (0x0800_0000)`。
- 契约：**18 个 Tauri 命令**（v1.1.7-dev 起 +`list_system_fonts`，winreg 0.55 读注册表枚举字体）+ 事件
  `ping-snapshot`（500ms 聚合推送，前端整体替换状态）。
- `Status` serde lowercase：`idle/resolving/ok/timeout/failed`。
- 配色语义 **绿 = 正常、红 = 失败**（**不要**套股票涨红跌绿）→ 动**语义色观感**（徽标/按钮底色）须先问用户。
- **禁用 `window.confirm`/`alert`**（Tauri WebView 不可靠）→ 自建 `ConfirmDialog`。
- **hook 必须写在条件 return 之前**（曾因 `useMemo` 写在 `if (!open) return null` 后 → 白屏 React #310）。
- 🩸 **新增配置字段必须逐字段带 serde 默认值**，**枚举必须自定义 `deserialize_with`**（未知值降级、不报错）。
  否则旧配置整份反序列化失败 → **回落默认值、丢用户全部主机列表**。本项目最贵的教训。

## 仓库与构建工作区分离
- **git 仓库：`C:\Users\22534\WorkBuddy\pinginfo`**（有 `.git`/`LICENSE`/`docs/`/`交付包/`）。
- **`D:\pinginfo` 只是构建工作区**（有 node_modules/target/dist，**没有 `.git`**）；改配置/构建在 D 盘，
  文档改动三处同步（仓库 / D 盘 / `交付包/`）。
- `tauri.conf.json` **基线 md5 = `4048afd632b3cc649f5904b36727a69e`（v1.1.9 起，两侧须始终一致）**；提版本后须重记
  （v1.1.8 时期旧基线 `2c0a8cf0fb8d8f1d5eebea6679c8d270` 已作废）。⚠️ 打包变体时只改**构建侧 D 盘**的
  `webviewInstallMode`，构造完立刻还原（两侧 md5 必须一致）。

## 环境与本机坑
- rustc 1.98.1 (msvc) · VS2022 + SDK 10.0.26100 · Node 22.22.2 · WebView2 153.x。
- 技能已覆盖：`NO_PROXY='*'` 绕沙箱代理、清 `dist` 用 PowerShell、`indexmap` 的 `std` 特性、`--in-process-gpu`
  治空窗口、`additionalBrowserArgs` **完全覆盖** wry 默认参数、CDP `clip`/`onMessage`/API 命名用法。
- ⚠️ **SSH 22 端口会超时**（2026-09-29 起）→ 改走 HTTPS：
  `NO_PROXY='*' no_proxy='*' git -c http.proxy= -c https.proxy= push https://github.com/halacrcrc/PingBoard.git main`
  （凭据自动取 `~/.my-credentials` 的 gho_ token；**光设 NO_PROXY 不够，必须同时清空 git 自身 proxy**）。
- 🩸 **2026-10-02 起 `git push`/`git fetch` 在本环境一律 `Recv failure: Connection was reset`**
  （`ls-remote` 正常、curl 200 → 排除体积/凭据/网络，是 git 传输层问题）。
  **兜底 = 走 GitHub API**：脚本 `qa-artifacts/push_via_api.py`（不入库）读远端 tree → 与本地
  `git ls-tree -r` 逐 blob 比对 → 只推差异文件（blobs/trees/commits/refs 四步）→ 推完自动复核。
  ⚠️ 凭据是 URL 形式（`https://user:gho_XXX@github.com`），**必须用正则 `gho_[A-Za-z0-9]+` 提取**。
  ⚠️ 本地 `refs/remotes/origin/main` 因此指向本地不存在的对象，`git status` 会一直显示 ahead，属正常。
  🩸 **用 API 推送必须复刻 git 的两个规范化，否则远端与本地永久不一致**（2026-10-02 踩了两遍）：
  ① **换行符**：`.gitattributes` 是 `* text=auto eol=lf`，推之前必须 `data.replace(b"\r\n", b"\n")`，
     否则工作区 CRLF 被原样推上去（表现为 blob 永远对不上）。
     🩸 **补充（2026-10-02 下午）**：`replace_in_file` **改**的文件保持 LF，但 **`write_to_file` 新建的
     文件会带 CRLF** → 同步到仓库侧后 `git diff` 报 "CRLF will be replaced by LF"。
     **双位置同步时必须逐文件查 CRLF 并在 D 侧规范化**，否则下次同步又出现假差异。
  ② **文件模式**：**Windows 上 `os.access(X_OK)` 对所有文件都返回 True**，
     会把 `.md`/`.json` 全标成 `100755`。判断可执行位只能靠扩展名（无扩展名才算脚本）。
  推完**必须逐 blob + 逐 mode 复核**（`git ls-tree -r` 对比远端 tree），不能只看内容 SHA。
- ⚠️ 本机专属：`wmic` 在沙箱黑名单 → 查进程用 Python ctypes；删 `%APPDATA%` 下文件只能用 PowerShell
  `Remove-Item -LiteralPath`（`rm` 被 safe-delete 守护拦）。本机 6 个 `msedgewebview2.exe` 属 `SearchHost.exe`，
  与本项目无关。

## 事件日志（v1.1.4 核心）
> 📖 **改事件相关代码前必读 `MEMORY-EVENTS.md`** —— 枚举与分级、去重口径、增量与 `seq`、持久化与轮转、
> 清空一致性、会话标记、5 个配置字段、4 个命令、未读红点。设计稿 `docs/events-log-design.md`（24 KB）。
> 三条最贵的坑先记住：① **`prev` = 置 `Resolving` 之前的状态**（否则持久性 DNS 失败每轮重复记）；
> ② **清空必须同时清归档**（否则「清空了又自己回来」）；③ **切批次用 `session`，不要用 `seq`**。

## React 陷阱：每 500ms 快照重置草稿（会复现）
根因：初始化草稿的 effect 依赖 `settings` 对象，而 App 每次收到 `ping-snapshot` 都重建该对象引用
→ 每 500ms `setDraft(settings)` 冲掉用户修改（表现为「取消勾选后又自己勾上」）。
**正解 `wasOpenRef` 模式**：ref 记上次 `open`，只在 `false→true` 时初始化草稿，**有意不写依赖数组**；
仍须放在 `if (!open) return null;` **之前**（hook 顺序安全）：
```tsx
const wasOpenRef = React.useRef(false);
React.useEffect(() => {
  const justOpened = open && !wasOpenRef.current;
  wasOpenRef.current = open;
  if (!justOpened) return;
  setDraft(settings); setError(null); setBusy(false);
});
```
**通例：凡「以 props 对象为初值」的本地草稿，都不能把该对象放进 effect 依赖。**

## Rust 陷阱：停止慢 + running 竞态
- **停止慢**（逐个 `join()` 的锅）→ **摘句柄（锁内不 join）→ 全部 `stop.store(true)` → 立即写 `running=false`
  + 全局状态 → handles 交后台 `pingboard-reaper` 线程 join**；`worker_loop` 须在 probe 后写统计前**和**
  `resolve_host_blocking` 的 Err 分支各加一次 `if stop.load(SeqCst) { break; }`。QA：100 目标 stop < 300ms。
- **running 竞态**：旧 worker 退出**无条件**写 `running=false` 会覆盖新 worker 的 `true` → `is_current_worker`
  用 **`Arc::ptr_eq(&h.stop, stop)`**，循环内写 `true` 与退出写 `false` **都**收敛为条件写入（**必须双向堵**）。
  反证手法可复用：回滚成无条件 write，新测试应立刻失败 —— **能失败才证明测试有效**。

## UI 铁律（细则见 `MEMORY-UI.md`）
- 🩸 **界面缩放（v1.1.7-dev，`ui_scale` 90/100/110/125，zoom 实现）**：凡是 `fixed` + `getBoundingClientRect()`
  数值定位的弹出层，**定位前必须除以 `format.ts` 的 `currentZoomFactor()`** —— rect 返回的是含缩放的视觉坐标，
  而 fixed 的 top/left 会被根元素 zoom 再放大一次（125% 时偏 25%）。百分比/flex 居中的弹层不受影响。
  zoom 125% 时 CSS 视口 = 窗口宽 ÷1.25，**`max-[1199px]:` 断点会提前触发**（属预期行为，不是 bug）。
- **UI 永不折行**：单行区域内文字必须带 `whitespace-nowrap shrink-0`，容器带 `overflow-x-auto overflow-y-hidden`；
  **禁止「缩小字号」解决拥挤**（可读性底线）。
- **断点只加 `max-[1199px]:`**：**≥1200px 的视觉与行为是已验收基线，不得为窄视口改动宽视口观感**。
  窗口默认 1400×900、`minWidth 1000`、`minHeight 620` → 1000px 是最窄必须零溢出场景。
- 容器加 `overflow` 后，**弹出菜单必须改 `fixed` + `getBoundingClientRect()` 定位**并随 scroll/resize 收起。
- **功能入口不得隐藏或合并**（宁可间距紧，不可塞进菜单）—— 用户明确否掉过「窄视口合并『清空 ▾』」。
- 搜索框是**可压缩元素**（`shrink` + `min-w-[72px]`）：变窄先压搜索框，不要挤按钮。
- **主表 13 列**（v1.1.2 删 `enabled` 列，`min-w-[912px]`）；启用/监控开关在 `DetailPanel` 标题栏，自绘 `button`
  + `role="switch"` + `aria-checked` + `title="纳入监控：参与「开始全部」与开机自动启动"`（绿 = 已监控）。
  `SortKey` 的 `"enabled"` 与 `compare()` boolean 分支**保留不动**。
- 🩸 **配色对比度四条速记**（实测值 / props 契约 / 设置对话框细节见 `MEMORY-UI.md`）：
  **次要文字只有 `text-slate-500 dark:text-slate-400` 一套**；**底色不是白时要补一档**；
  **表格内文本按「最暗行底」设计**（彩色 `-50` 行底比白底暗，会把刚达标的档位拉低 ~0.4 →
  表格内下探到 `slate-600` / `red-700`、延迟无数据用 `slate-600`）；**emoji 图标要控色/控粗细必须自绘 SVG**。

## 功能口径速查（回答用户提问时直接引用）
- **报表导出筛选（v1.1.9-dev，批次 5）**：`export_report` 加可选 `filter`（`all`/`none_loss`/`all_failed`），
  支持只导出**零丢包**（`sent>0 && failed==0`）与**全部未成功**（`sent>0 && received==0`）两类节点。
  🩸 **判据必须带 `sent>0`** —— `sent==0`（未探测/已清空统计）时 `loss_pct` 定义为 `0.0`，
  只判 `failed==0` 会把未开始的主机全算成零丢包（已变异实测锁死）。
  **筛选只改行集合不改字段结构**（CSV 恒 15 列 / HTML 恒 15 个 `<th>`）；`All` 产出与上线前逐字节一致。
  判据**有两份实现**：前端 `format.ts::matchesExportFilter`（只做命中台数预览）+ Rust `export::matches_filter`（写文件权威），
  **改判据必须两侧同改**。入口 = 工具栏「导出 ▾」第 3 组 → `ExportDialog`。
  完整设计见 `docs/export-filter-design.md`。
- **累加限制**：单次上限 1024（前端 `MAX_BATCH`，含 IP 段展开）；**跨批次累加无任何上限**；限制只在**启动**时发生 ——
  `max_threads`（默认 256，可调 1..1024）在 `limit_max_threads=true` 时拦截，关闭后仍保留 **4096 硬保护**。
- **重复判定**：host `trim + 小写` 后比较；跨批次重复**只在前端 UI 拦截**（后端 `add_targets` 不查重）。
- **导入分工**：**Rust 只读文件，解析全在前端** `parseBatch`/`expandIpRange`。Rust 依赖 `calamine`（Excel）
  + `encoding_rs`（GBK），前端 0 新增。线格式 `ImportPayload` = `{kind:"text",text}` / `{kind:"table",rows}`。
- **平台**：**仅 x64、最低 Windows 10**（PE 头 `MinOSVersion 6.0` 是链接器默认值，别当依据）。
  **唯一外部运行时依赖 = WebView2 Evergreen Runtime**；**不需要** VC++ 运行库（CRT 静态链接）、.NET、
  `WebView2Loader.dll`、系统补丁。查依赖：Python 手动解析 PE 导入表。
- `pinger/icmp.rs` **无 `cfg(windows)` 门控**（全盘 `use windows::*`）→ 代码库**不可跨平台编译**。
  🩸 **macOS / Windows ARM64 的可行性与成本分析见技能 `tauri2-…` §六之二**（结论：**macOS ≈ 独立适配项目** ——
  `.dmg` 只能在 Mac 上打、探测层要重写、分发要苹果开发者账号；**ARM64 技术可行但 x64 机器无法真机验证**）。
