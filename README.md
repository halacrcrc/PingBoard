# PingBoard — 多主机 Ping 监视器

<img src="docs/app-icon.png" width="96" alt="PingBoard 图标">

一款面向 Windows 平台的多主机并行 Ping 监视工具。
使用 **Rust + Tauri 2 + React 18 + TypeScript + Tailwind CSS** 开发，界面语言为简体中文。

> **普通用户权限即可运行**：探测走 Win32 IP Helper 的 ICMP API（`IcmpSendEcho`），
> 不使用原始套接字（`SOCK_RAW`），因此**不需要管理员权限**，也不会触发 UAC。

---

## 系统要求

| 项目 | 要求 |
|---|---|
| 操作系统 | **Windows 10**（建议 1809 及以上）或 **Windows 11**。Windows Server 2016 / 2019 / 2022 理论可用（未实测） |
| 系统架构 | **仅 64 位（x64）**。32 位系统无法安装，暂无 ARM64 原生版本 |
| 额外组件 | **仅需 Microsoft Edge WebView2 运行时**（Windows 11 已内置；Windows 10 多数已随 Edge 一起安装）。无需 VC++ 运行库、无需 .NET Framework、无需任何系统补丁 |
| 权限 | 普通用户即可。安装到当前用户目录，运行与安装**均不触发 UAC** |
| 磁盘占用 | 程序约 5.5 MB；WebView2 用户数据目录约 10 MB（随缓存缓慢增长） |
| 网络 | 探测依赖 ICMP 回显（IPv4），需本机出站策略与目标主机放行 ping |

> **不支持 Windows 7 / 8.1**：WebView2 运行时自 109 版起已停止支持这两个系统。
>
> **如何确认本机是否已装 WebView2**：打开「设置 → 应用 → 已安装的应用」搜索 `WebView2`；
> 或查看目录 `C:\Program Files (x86)\Microsoft\EdgeWebView\Application\`（存在版本号子目录即已安装）。
> 若缺失，可从微软官网安装 **Evergreen 运行时**：
> <https://developer.microsoft.com/microsoft-edge/webview2/>

### 为什么不需要 VC++ 运行库

程序使用 Rust 编译，C 运行时（CRT）已**静态链接**进 `pingboard.exe`，导入表中不含
`vcruntime140.dll` / `msvcp140.dll`；WebView2 的加载器（`WebView2Loader.dll`）同样已静态链接，
因此安装目录下只有一个可执行文件，**不需要任何额外的可再发行组件**。

---

## 安装

提供三种安装包，**程序功能完全相同**，区别只在于「如何为缺失 WebView2 的目标机提供运行时」。

| 安装包 | 体积 | 目标机缺 WebView2 时 | 适合场景 |
|---|---|---|---|
| `PingBoard_1.1.4_x64-setup.exe`（**在线引导版**，默认） | ≈ 2.0 MB | 需联网，安装时自动下载引导程序 | 普通用户，机器可正常上网 |
| `PingBoard_1.1.3_x64-setup-embedWebView2.exe`（**内置引导版**） | ≈ 3.7 MB | 需联网下载运行时（引导程序已内置） | 网络不稳，避免「下载引导程序」这一步失败 |
| `PingBoard_1.1.3_x64-setup-offline.exe`（**完整离线版**） | 217.9 MB（207.8 MiB） | **完全不需要联网** | 内网 / 无外网 / 批量部署 |

> **v1.1.4 只发布了在线引导版**（最小体积）；内置引导版与完整离线版沿用 **v1.1.3** 所出，
> 三者的应用主体功能一致，差别仅在 WebView2 运行时的投递方式。如需 v1.1.4 的另两种变体，可自行按本文档构建。

> 三者的差异仅在于打包方式（Tauri 的 `webviewInstallMode`）：
> - 在线引导版 = `downloadBootstrapper`：安装时若检测到缺少 WebView2，才去下载约 1.8 MB 的引导程序；
> - 内置引导版 = `embedBootstrapper`：把引导程序预置在安装包里，离线环境也能「发起」安装，但仍需联网下载运行时本体；
> - 完整离线版 = `offlineInstaller`：内置 213 MB（203 MiB）的 WebView2 离线安装程序，**全程无需联网**。
>
> 只要目标机已装 WebView2（Windows 11 默认如此），三种包装出来的结果**完全一致**，
> 用最小的在线引导版即可。

### 安装步骤

1. 双击安装包，按向导完成（默认安装到 `%LOCALAPPDATA%\PingBoard`，当前用户，免 UAC）。
2. 从开始菜单或桌面快捷方式启动。
3. 卸载：通过「设置 → 应用」或安装目录下的 `uninstall.exe`。

静默安装 / 卸载（供批量部署使用）：

```bat
PingBoard_1.1.4_x64-setup.exe /S                     :: 安装到默认目录
PingBoard_1.1.4_x64-setup.exe /S /D=C:\Tools\PingBoard   :: /D= 必须是最后一个参数且用反斜杠
uninstall.exe /S                                     :: 静默卸载
```

---

## 功能特性

### 探测引擎（两级策略）
1. **主路径 — Win32 IP Helper ICMP API**：`IcmpCreateFile` / `IcmpSendEcho` / `IcmpCloseHandle`。
   每个工作线程复用一个句柄，循环收发，避免频繁创建销毁。
2. **降级路径 — 系统 `ping.exe`**：当 ICMP 句柄创建失败（权限/系统限制）时全局自动降级。
   子进程以 `CREATE_NO_WINDOW` 启动，**不闪黑框**；输出解析同时兼容中英文。

### 并发模型
- 每个运行中的目标对应一个独立工作线程（`std::thread`）。
- 每 500ms 由后端主动推送一次**聚合快照事件** `ping-snapshot`，前端只做整体替换，
  避免「100 台 × 1Hz = 100 event/s」把 WebView 拖垮。
- 停止/间隔变更通过 `AtomicBool` + 分片休眠（100ms）响应，长休眠不会阻塞停止指令。
- `max_threads` 限制并发线程上限（默认 256，可通过「启用线程数上限」开关关闭）。
  关闭后不再按该值拦截，仅保留 **4096** 硬保护；详见「设置项」中的性能影响说明。

### 界面
- 顶部工具栏：添加主机、开始/停止全部、清空统计、**清空列表**、设置、导出（CSV / TXT / HTML）、搜索、主题切换、状态指示灯。
- 选中目标后工具栏额外出现：**开始选中 / 停止选中 / 删除选中**（可对列表内的部分目标单独启停）。
- 中部主表格：选择 / 备注名 / 主机名 / IP 地址 / 状态 / 延迟 / 平均 / 最小 / 最大 /
  丢包率 / 成功·失败 / 趋势 sparkline / 最后成功时间（共 **13 列**）。支持点击表头排序、多选（`Ctrl` / `Shift` + 复选框）、
  **表头全选（仅作用于当前搜索过滤后的可见列表）**、`Delete` 键删除。
- 右侧详情面板：标题栏带**「监控」开关**（决定该目标是否纳入「开始全部」与开机自动启动）+
  最近 60 次延迟**折线图（纯 SVG 手写）** + 完整统计 +
  **事件日志**（条数显示、单主机「记录事件」开关、复制 / 导出 CSV / 清空，详见下文「事件日志」）。
- 底部状态栏：运行状态、主机数、活跃线程数、总发包/收包/丢包率、运行时长、最后刷新时间。
- 浅色 / 深色主题，选择持久化到 `localStorage`（默认浅色）。
- 行着色约定：**绿 = 正常，红 = 失败**。
- 状态文字配色：**「运行中」为绿色（`emerald-600`）**、**「已停止」为灰色（`slate-400`）**，
  在工具栏、底部状态栏、详情面板「线程」三处一致。

### 事件日志（可用性流水账）
详情面板底部的「最近日志」记录每台主机的**状态翻转**事件。它与上方折线图分工不同：
折线图看「**慢不慢**」，事件日志看「**什么时候断的、什么时候好的**」——半夜掉线、次日上午恢复，
折线图只能看出一个坑，具体时间点只有这里能回答。

**事件类型与三档等级**（等级在设置里切换）

| 等级 | 包含事件 | 适合场景 |
|---|---|---|
| 仅故障 | 故障、无法连通、解析失败、恢复 | 长期挂着盯梢，噪音最低 |
| **标准**（默认） | 上述 + 开始探测、已停止、首次连通 | 能分辨「这段空白是没在跑，还是没出事」 |
| 详细 | 上述 + 配置变更 | 排查「是不是谁把目标改坏了」 |

**关键行为**
- 「故障」指**先通、后不通**；「无法连通」指**从一开始就连不上**；两者分开记录。
- **去重**：同一台主机持续失败只记 **1 条**（边沿触发的状态翻转），不会每个探测周期重复刷屏。
- 「已停止」只在**用户主动停止**时记录；**程序退出不产生**该事件。
- 等级是**展示时过滤**：改等级后历史事件立即跟着显隐，后端始终记录全等级。
- **未读故障红点**：非当前选中主机发生故障时，其**主机列表行的备注名右侧**出现红点，点开该主机后消失。

**开关（两级，均为「生成时」生效）**
- 设置里的**总开关** `events_on`（默认开启）
- 详情面板日志标题右侧的**单主机开关**（`记录事件`）

关闭后不再产生新事件（省内存与磁盘），**事后重新打开不会回补历史**。

**持久化**（默认开启）
- 追加写入 `%APPDATA%\com.pingboard.desktop\pingboard-events.jsonl`（JSONL，一行一个事件，
  与配置文件同级），可在设置里用系统文件夹对话框改到任意目录。
- 文件超过 **8 MiB** 自动轮转为 `pingboard-events.1.jsonl`；启动时**归档与主文件都会读回**
  （各取尾部一段并按时间合并），再按主机归属回填（历史条目视为已读，不计入红点）。
- 「清空某主机」会**同时清理主文件与归档**里该主机的记录，不会出现「清空了又自己回来」。
- 目录不可写时**静默降级**：应用照常运行、事件仍在界面显示，只是不落盘。

**运行批次（会话）**
每次启动程序都会生成一个**会话标识**（即当次启动时刻），写进该次运行产生的每条事件。
- 界面上，**上一次运行**留下的日志时间显示为「月-日 时:分:秒」，本次运行的不带日期；
  列表里混有历史时，标题旁会出现「含历史」标记。
- 导出的 CSV 有独立的 **「会话（启动于）」** 列，在 Excel 里筛这一列即可按批次切分。
- 升级前写入的旧记录没有该字段，其会话列为空。

**导出**：日志区可「复制」为纯文本或「导出 CSV」。CSV 为 8 列
（`序号, 会话（启动于）, 时间(本地), 主机, 备注, 事件类型, 等级, 说明`），带 **UTF-8 BOM**，
Excel 直接打开中文不乱码。

### 添加主机
- 单个添加：主机名/IP + 备注名。
- 批量粘贴：每行 `主机` 或 `主机<空格/逗号/制表符>备注`；自动去重、跳过空行与 `#` 注释行。
- **IP 段展开**：`192.168.1.1-254` 或 `192.168.1.1-192.168.1.20`（单次上限 1024 个）。
- **从文件导入**：
  - 支持 `.txt` / `.csv`（逐行读取，兼容 UTF-8 / UTF-8 BOM / GBK 编码）与 `.xlsx` / `.xls` / `.xlsm` / `.ods`（仅读取第一个工作表）。
  - txt / csv：每行「主机 备注」，支持 `#` 注释与 IP 段；Excel：第 1 列主机、第 2 列备注，自动跳过表头行。
  - 解析逻辑与批量粘贴共用（自动去重、IP 段展开），导入前会显示目标预览。

### 设置项
| 设置 | 默认值 | 范围 |
|---|---|---|
| `interval_ms` 探测间隔 | 1000 | ≥100 |
| `timeout_ms` 超时时间 | 2000 | ≥100 |
| `payload_size` 负载大小 | 32 | 0..65500 |
| `ttl` | 128 | 1..255 |
| `max_threads` 最大线程数 | 256 | 1..1024 |
| `limit_max_threads` 启用线程数上限 | true | 开关 |
| `history_len` 趋势保留点数 | 60 | 10..600 |
| `beep_on_fail` 失败提示音 | false | —— |
| `auto_start` 启动时自动开始 | false | —— |
| `events_on` 启用事件日志 | true | 开关 |
| `events_level` 事件等级 | standard（标准） | fault / standard / detail |
| `events_persist` 事件保存到文件 | true | 开关 |
| `events_dir` 事件保存目录 | 空 = `%APPDATA%\com.pingboard.desktop\` | 任意可写目录 |
| `events_keep` 每主机保留条数 | 200 | 50 / 200 / 1000 |

> **并发与性能影响**：每台主机会占用 1 个系统线程 + 1 个 ICMP 句柄。开启上限时按「最大线程数」拦截，
> 可防止误加大量主机拖慢系统；关闭后不再限制（仅保留 4096 硬保护）——200 台以内影响很小，
> 500 台以上线程调度、内存与界面刷新开销会明显上升，1000 台以上建议保持开启并分批启动。
>
> 关闭「启用线程数上限」后，「最大线程数」输入框置灰但仍会被校验并持久化（仅运行时不拦截）。
> 旧版本（v1.0.0）配置文件不含 `limit_max_threads` 字段，加载时缺省为 `true`，不会丢失已有主机列表。

### 配置持久化
- 路径：`%APPDATA%\com.pingboard.desktop\pingboard-config.json`
- 结构：`{ version, settings, targets: [{ name, host, enabled, events_on }] }`
- 读取失败/文件缺失一律回退默认值，不 panic、不阻塞启动。
- 保存时机：目标增删改、设置变更、应用退出前。
- **新增字段一律带 serde 默认值**：旧版本配置缺字段时取默认值，**不会因单个字段缺失导致整份配置回落、
  丢失已保存的主机列表**；`events_level` 遇到无法识别的值降级为「标准」而非报错。
- 事件日志单独落盘在 `pingboard-events.jsonl`，与配置文件互不影响（见上文「事件日志」）。

---

## 开发与构建

### 环境要求
- Rust 1.77+（本项目在 rustc 1.98.1 / `x86_64-pc-windows-msvc` 下验证）
- Visual Studio 2022 C++ 生成工具 + Windows SDK
- Node.js 18+ 与 npm
- WebView2 Runtime（Windows 11 已内置）

### 安装依赖
```bash
npm install
```

### 开发模式（热更新）
```bash
npm run tauri dev
```
> 前端 devServer 固定端口 `1420`（`strictPort`），Vite 忽略 `src-tauri` 目录变更。

### 仅构建前端
```bash
npm run build
```

### 后端检查与单元测试
```bash
cd src-tauri
cargo check
cargo test
```

### 更换图标

图标的设计源是手写矢量 `docs/app-icon.svg`（深蓝底 + 翡翠绿雷达同心圆），
另有一版白底备选 `docs/app-icon-alt-white.svg`。改图标只需改 SVG，再重新渲染并生成全套尺寸：

```bash
# 1) SVG → 1024x1024 透明 PNG（借用 Edge 无头渲染，无需额外依赖）
msedge --headless=new --disable-gpu --hide-scrollbars \
       --default-background-color=00000000 --force-device-scale-factor=1 \
       --window-size=1024,1024 --screenshot=docs/app-icon.png "file:///$PWD/docs/app-icon.svg"

# 2) 生成全套平台图标（Windows ico 含 16/24/32/48/64/256 + icns + iOS/Android）
npx tauri icon docs/app-icon.png
```

> `--default-background-color=00000000` 是拿到**透明背景**的关键，漏掉会渲染成白底。
>
> 改完需手动把 Android 自适应图标背景色设回 `#12203A`
> （`src-tauri/icons/android/values/ic_launcher_background.xml`）——
> `npx tauri icon` 会把它重置为 Tauri 默认的白色。
>
> **定稿前务必看 16px / 24px 的真实效果**：细描边圆环在小尺寸下会被抗锯齿整圈吃掉，
> 只剩一个孤立的中心点。本图标因此改用 `fill-rule="evenodd"` 的**实心环带**而非细描边。
> 各尺寸放大对照见 `docs/app-icon-sizes.png`。

> `scripts/gen-icon.mjs` 与 `assets/app-icon.png` 是 v1.0.0 时期的占位图标生成脚本，已被上面的流程取代，保留仅为追溯。

### 打包 Windows 安装包（NSIS）
```bash
npx tauri build --bundles nsis
```
产物：
- 安装包：`src-tauri/target/release/bundle/nsis/PingBoard_1.1.4_x64-setup.exe`
- 可执行文件：`src-tauri/target/release/pingboard.exe`

安装方式与系统要求见上文「[系统要求](#系统要求)」与「[安装](#安装)」两节：
`installMode = currentUser`（**免 UAC**），三种 `webviewInstallMode` 的取舍如下。

切换 WebView2 打包方式（改 `src-tauri/tauri.conf.json` 的 `bundle.windows.webviewInstallMode` 后重新执行上面的命令）：

| `webviewInstallMode.type` | 产物体积 | 是否需要联网 | 说明 |
|---|---|---|---|
| `downloadBootstrapper`（默认） | ≈ 2.0 MB | 缺 WebView2 时需要 | 安装时才下载约 1.8 MB 引导程序 |
| `embedBootstrapper` | ≈ 3.7 MB | 缺 WebView2 时需要 | 引导程序内置，安装包 +≈1.8 MB |
| `offlineInstaller` | 217.9 MB | **不需要** | 内置 WebView2 离线安装程序（213 MB），**构建时需联网下载一次** |

```jsonc
// src-tauri/tauri.conf.json
"bundle": {
  "windows": {
    "nsis": {
      // 安装程序 / 卸载程序自身显示的图标；不设则用 NSIS 自带的通用图标
      "installerIcon": "icons/icon.ico",
      "uninstallerIcon": "icons/icon.ico"
    },
    "webviewInstallMode": { "type": "downloadBootstrapper" }  // 或 embedBootstrapper / offlineInstaller
  }
}
```

> `offlineInstaller` 首次构建会从微软下载
> `MicrosoftEdgeWebView2RuntimeInstallerX64.exe`（约 203 MB），耗时较长；缓存后再次构建会更快。

---

## 目录结构

```
pinginfo/
├─ README.md
├─ package.json / vite.config.ts / tsconfig.json / tsconfig.node.json
├─ tailwind.config.js / postcss.config.js / index.html
├─ docs/                         # 设计源与文档图
│  ├─ app-icon.svg               # 应用图标矢量源（在用）
│  ├─ app-icon.png               # 1024x1024 透明渲染图
│  ├─ app-icon-sizes.png         # 各尺寸放大对照（16/24/32/48/64）
│  ├─ app-icon-alt-white.svg/.png# 白底备选版
│  └─ screenshot.png             # 界面截图
├─ scripts/gen-icon.mjs          # v1.0.0 时期的占位图标脚本（已被 docs/app-icon.svg 流程取代）
├─ assets/app-icon.png           # 同上，历史遗留
├─ src/                          # 前端（React + TS）
│  ├─ main.tsx / App.tsx / styles.css / types.ts
│  ├─ lib/api.ts                 # invoke 封装
│  ├─ lib/format.ts              # 格式化工具
│  └─ components/                # Toolbar / TargetTable / StatusBar /
│                                # AddTargetsDialog / SettingsDialog /
│                                # DetailPanel / Sparkline / LatencyChart /
│                                # ConfirmDialog
└─ src-tauri/                    # 后端（Rust）
   ├─ Cargo.toml / build.rs / tauri.conf.json
   ├─ capabilities/default.json
   ├─ icons/                     # 全套平台图标（由 docs/app-icon.png 生成）
   └─ src/
      ├─ main.rs / lib.rs
      ├─ model.rs                # 数据结构（serde）
      ├─ state.rs                # 全局状态 + 调度器 + 快照发射任务
      ├─ config.rs               # 配置读写
      ├─ stats.rs                # 统计纯函数（含单元测试）
      ├─ pinger/{mod,icmp,fallback}.rs
      ├─ commands.rs             # Tauri 命令
      ├─ import.rs               # txt/csv/xlsx 文件导入（读文本 / 表格 + 编码探测）
      └─ export.rs               # CSV / TXT / HTML 导出
```

---

## 关键技术约定

- **不使用原始套接字**，保证普通用户权限可运行。
- 所有 `spawn` 子进程均设置 `CREATE_NO_WINDOW`，不闪黑框。
- 趋势图 / sparkline 使用原生 SVG 手写，**不引入任何图表库**。
- 仅引入 `tauri-plugin-dialog` 一个插件（导出选择保存路径所需）。
- CSV 导出带 UTF-8 BOM，避免 Excel 打开中文乱码。
- 前端只依赖后端推送的聚合快照，不处理逐次探测事件。

## 已知限制

- ICMP 主路径仅支持 IPv4；IPv6 目标自动走 `ping.exe -6` 降级路径。
- 降级路径（`ping.exe`）使用系统默认 32 字节负载，`payload_size` 设置对降级路径不生效。
- 导出报表中的「最后成功时间」为 UTC 时间。

---

## 更新日志

### v1.1.4 — 事件日志（可用性流水账）

- **修复：详情面板的「最近日志」一直是空的。** 根因是事件在**前端**用前后两次界面快照做差生成，
  判断规则只认「先通、后不通」与「先前不通、后通了」两种翻转；而后端真实的状态迁移是
  `空闲 → 解析中 → 失败`，于是「启动后就没连上」「域名解析失败」这两种最常见的情况永远不产生记录。
  改为**在后端的状态迁移点打点**，任何一次状态翻转都不会漏。
- **新增事件等级三档**（设置里切换）：`仅故障` / `标准`（默认）/ `详细`。
  等级只影响**展示过滤**，后端始终记录全等级，改等级后历史事件立即跟着显隐。
- **新增事件日志持久化**：追加写入 JSONL（`pingboard-events.jsonl`，与配置文件同级，可自选目录），
  超过 8 MiB 自动轮转；启动时**归档与主文件都会读回**并按主机归属回填。目录不可写时静默降级，不影响使用。
- **新增「运行批次（会话）」**：每次启动生成一个会话标识写入每条事件，用于在同一个文件里区分
  「哪一次运行」。界面上上一次运行的日志时间带「月-日」前缀并标注「含历史」；
  导出 CSV 多一列 **「会话（启动于）」**，Excel 里筛这一列即可按批次切分。
- **修复：轮转后界面上的历史会「消失」。** 旧数据被改名归档后启动时读不回来（数据仍在磁盘），
  现已改为归档与主文件合并读回。
- **修复：清空某主机时归档里的记录没被清掉。** 此前只重写主文件，与上一条修复叠加后会表现为
  「清空了又自己回来」；现已同时清理两个文件，全部清空时不留 0 字节残file。
- **新增两级开关**：设置里的**总开关** + 详情面板日志标题旁的**单主机开关**（`记录事件`）。
  关闭后不再产生新事件，**事后重新打开不会回补历史**。
- **新增未读故障红点**：非当前选中主机发生故障时，其**主机列表行的备注名右侧**出现红点，
  点开该主机后消失。
- **新增日志操作**：复制为纯文本 / 导出 CSV（8 列、UTF-8 BOM，Excel 直接打开不乱码）/ 清空。
- **新增保留条数设置**：每主机 50 / 200（默认）/ 1000 条。
- **修复：「解析失败」文案前缀重复**，此前显示为「解析失败：解析失败：不知道这样的主机。」。
- 事件类型共 8 种：故障、无法连通、解析失败、恢复、首次连通、开始探测、已停止、配置变更。
  其中「故障」= 先通后不通，「无法连通」= 从一开始就连不上；同一台主机持续失败只记 1 条（边沿触发去重）；
  「已停止」只在**用户主动停止**时记录，程序退出不产生该事件。

### v1.1.3 — 全新应用图标

- **更换应用图标**：由 Tauri 脚手架默认图标改为「深蓝底 + 翡翠绿雷达同心圆」，
  最外环与图标边缘相切，右上角一道柔和扫描扇区 + 环上一个目标亮点。
  全套 52 个尺寸（Windows `ico` 含 16/24/32/48/64/256、macOS `icns`、iOS / Android）全部重新生成。
- **安装程序与卸载程序也随之上新图标**（新增 `bundle.windows.nsis.installerIcon` / `uninstallerIcon`）——
  此前安装包显示的是一张 NSIS 自带的通用图标，与应用图标并不一致。
- 矢量设计源随仓库提供：`docs/app-icon.svg`（在用）、`docs/app-icon-alt-white.svg`（白底备选）、
  `docs/app-icon-sizes.png`（各尺寸放大对照）。

### v1.1.2 — 试用反馈修复

- **修复：设置项被周期性重置。** 设置对话框的初始化逻辑依赖了每 500 ms 刷新一次的 `settings` 对象，
  导致「启用线程数上限」等勾选在半秒后被冲掉，表现为「无法取消选择」「改了没反应」。
  改为只在对话框打开的那一瞬初始化草稿。
- **修复：停止缓慢。** 原实现逐个等待工作线程退出（总耗时 = 所有目标退出时间之和），
  且界面状态要等全部退出后才更新，表现为「要点好几下、等十几二十秒」。
  改为「置停止标志 → 立即更新界面 → 句柄交给后台线程回收」。
  实测 3 台规模停止耗时 205 / 282 / 368 ms，数百台规模也在一个刷新周期（500 ms）内完成。
  - 该改动曾引入一处新竞态（快速「停止 → 立即开始」时短暂显示已停止），
    已通过线程身份比对（`Arc::ptr_eq`）修复，72 个采样点复验全 0。
- **合并重复列**：「选择」列与「启用」列功能重叠，删除表格中的「启用」列（14 → **13 列**），
  启用/监控开关移至右侧详情面板标题栏，改为 `role="switch"` 开关。
- **状态文字配色**：「运行中」绿色（`emerald-600`）、「已停止」灰色（`slate-400`），
  工具栏 / 底部状态栏 / 详情面板「线程」三处一致。

### v1.1.1 — 细节修复

- 修复工具栏在选中主机后折行的问题，并新增重复目标与累加超限提示。
- 未单独发版，改动并入 v1.1.3。

### v1.1.0 — 文件导入 / 选择列 / 并发上限开关

- 新增**从文件导入主机**：支持 `.txt` / `.csv` / `.xlsx` / `.xls` / `.xlsm` / `.ods`，
  兼容 UTF-8 / UTF-8 BOM / GBK 编码，导入前显示目标预览。
- 新增**选择列**与「开始选中 / 停止选中 / 删除选中」批量操作，表头支持全选（仅作用于搜索过滤后的可见列表）。
- 「启用线程数上限」改为可选开关，关闭后不再按 `max_threads` 拦截（仅保留 4096 硬保护）。
- 首次同版发布三种 WebView2 打包变体。

### v1.0.0 — 首个版本

- 多主机并行 Ping 监视（Win32 ICMP API，普通权限可运行）、延迟趋势图、
  导出 CSV / TXT / HTML、浅色 / 深色主题。
