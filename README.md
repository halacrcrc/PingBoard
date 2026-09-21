# PingBoard — 多主机 Ping 监视器

一款面向 Windows 平台的多主机并行 Ping 监视工具。
使用 **Rust + Tauri 2 + React 18 + TypeScript + Tailwind CSS** 开发，界面语言为简体中文。

> **普通用户权限即可运行**：探测走 Win32 IP Helper 的 ICMP API（`IcmpSendEcho`），
> 不使用原始套接字（`SOCK_RAW`），因此**不需要管理员权限**，也不会触发 UAC。

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
- 中部主表格：选择 / 启用 / 备注名 / 主机名 / IP 地址 / 状态 / 延迟 / 平均 / 最小 / 最大 /
  丢包率 / 成功·失败 / 趋势 sparkline / 最后成功时间。支持点击表头排序、多选（`Ctrl` / `Shift` + 复选框）、
  **表头全选（仅作用于当前搜索过滤后的可见列表）**、`Delete` 键删除。
- 右侧详情面板：最近 60 次延迟**折线图（纯 SVG 手写）** + 完整统计 + 失败/恢复事件日志。
- 底部状态栏：运行状态、主机数、活跃线程数、总发包/收包/丢包率、运行时长、最后刷新时间。
- 浅色 / 深色主题，选择持久化到 `localStorage`（默认浅色）。
- 行着色约定：**绿 = 正常，红 = 失败**。

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

> **并发与性能影响**：每台主机会占用 1 个系统线程 + 1 个 ICMP 句柄。开启上限时按「最大线程数」拦截，
> 可防止误加大量主机拖慢系统；关闭后不再限制（仅保留 4096 硬保护）——200 台以内影响很小，
> 500 台以上线程调度、内存与界面刷新开销会明显上升，1000 台以上建议保持开启并分批启动。
>
> 关闭「启用线程数上限」后，「最大线程数」输入框置灰但仍会被校验并持久化（仅运行时不拦截）。
> 旧版本（v1.0.0）配置文件不含 `limit_max_threads` 字段，加载时缺省为 `true`，不会丢失已有主机列表。

### 配置持久化
- 路径：`%APPDATA%\com.pingboard.desktop\pingboard-config.json`
- 结构：`{ version, settings, targets: [{ name, host, enabled }] }`
- 读取失败/文件缺失一律回退默认值，不 panic、不阻塞启动。
- 保存时机：目标增删改、设置变更、应用退出前。

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

### 生成图标（可选）
```bash
node scripts/gen-icon.mjs assets/app-icon.png   # 生成 1024x1024 源图
npx tauri icon assets/app-icon.png              # 生成全套平台图标
```

### 打包 Windows 安装包（NSIS）
```bash
npx tauri build --bundles nsis
```
产物：
- 安装包：`src-tauri/target/release/bundle/nsis/PingBoard_1.1.0_x64-setup.exe`
- 可执行文件：`src-tauri/target/release/pingboard.exe`

安装方式：双击安装包，按向导完成即可（`installMode = currentUser`，**免 UAC**）。
若目标机器缺少 WebView2，安装包会自动下载引导程序（`downloadBootstrapper`）。

---

## 目录结构

```
pinginfo/
├─ README.md
├─ package.json / vite.config.ts / tsconfig.json / tsconfig.node.json
├─ tailwind.config.js / postcss.config.js / index.html
├─ scripts/gen-icon.mjs          # 纯 Node 图标生成脚本
├─ assets/app-icon.png           # 图标源图（生成物）
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
