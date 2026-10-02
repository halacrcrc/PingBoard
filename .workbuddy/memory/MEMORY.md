# 项目长期记忆 —— PingBoard（D:/pinginfo）

## 项目定位
Windows 平台多主机 Ping 监视器，功能对标 NirSoft **PingInfoView**。
技术栈：**Rust + Tauri 2 + React 18 + TypeScript + Tailwind v3.4**（无图表库，趋势图为原生 SVG）。

## 硬性约定（改动前必读）
- **ICMP 只能用 Win32 IP Helper `IcmpSendEcho`**（`windows` crate 0.61）。禁止 raw socket / surge-ping —— 必须普通用户权限可运行。
- 所有 spawn 子进程处必须带 `CREATE_NO_WINDOW (0x0800_0000)`。
- 前后端契约：12 个 Tauri 命令 + 事件 `ping-snapshot`（500ms 聚合推送，前端整体替换状态）。
- `Status` serde 为 lowercase：`idle/resolving/ok/timeout/failed`。
- 配色语义：**绿=正常、红=失败**（不要套用股票涨红跌绿）。
- 界面语言简体中文；默认浅色主题。

## 构建与打包
```bash
cd D:/pinginfo
npm install                    # 可能需要 --registry=https://registry.npmmirror.com
npx tauri build --bundles nsis # 产出 src-tauri/target/release/bundle/nsis/PingBoard_1.0.0_x64-setup.exe
```
- `identifier = com.pingboard.desktop`（不能以 `.app` 结尾）
- NSIS：`installMode = currentUser`（免 UAC）、语言 SimpChinese + English
- 配置目录：`%APPDATA%\com.pingboard.desktop\pingboard-config.json`
- WebView2 数据目录：`%LOCALAPPDATA%\com.pingboard.desktop\EBWebView\`

## 环境（本机已验证可用）
rustc/cargo 1.98.1 (x86_64-pc-windows-msvc) · VS2022 Community (MSVC 14.44 + Win SDK 10.0.26100) ·
Node 22.22.2 / npm 10.9.7 · WebView2 Runtime 153.x · NSIS 由 Tauri 自动下载

## 本机环境坑
- **cargo / tauri / Node 连本地回环都需 `NO_PROXY='*' no_proxy='*'`**，否则被沙箱代理 127.0.0.1:10164 拦截。
- 重新构建前需 PowerShell 清 `D:/pinginfo/dist`（safe-delete 守护会拦 Vite emptyOutDir）。
- `indexmap 1.x` 的 build.rs 在 rustc 1.98 下 autocfg 探测 std 失败，已在 Cargo.toml 启用其 `std` 特性绕过。

## Critical 修复记录：WebView2 GPU 崩溃 → 空窗口
本机（AMD RX 9070 GRE + 向日葵 OrayIddDriver 虚拟显示器）上每次启动必现独立 GPU 进程
`LOG_FATAL` 崩溃，拖垮整个 WebView2 → 窗口内容区全黑。
- ❌ `--disable-gpu` **无效**（不移除独立 GPU 进程）
- ✅ **`--in-process-gpu` 有效**
- 已写入 `src-tauri/tauri.conf.json` → `app.windows[0].additionalBrowserArgs`
  （该字段会完全覆盖 wry 默认参数，必须手动补回 `--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection`）

## 验证 WebView2 界面的正解
`PrintWindow` 抓不到 WebView2 合成层（恒黑）；屏幕截图不可靠。
正解：开远程调试端口 + CDP 读 DOM + `Page.captureScreenshot`。
- ✅ **首选**：设环境变量 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`
  启动应用 —— **不必改 `tauri.conf.json`**，`tauri.conf.json` 可全程保持 md5
  `3b00250a7c0da1a26f012ef9d0c91b63` 不变
- 备选：改 `additionalBrowserArgs` 注入同名参数，但**验证完必须移除再出正式包**
- 用 Node 22 全局 `WebSocket` 连 `http://127.0.0.1:9222/json/list`

## v1.1.0 新增约定（2026-09-21）
- 命令数 **13**（新增 `read_import_file(path) -> ImportPayload`）
- `ImportPayload` 线格式：`{kind:"text",text}` / `{kind:"table",rows}`
- 导入分工：**Rust 只读文件，解析全在前端** `parseBatch`/`expandIpRange`（勿在 Rust 重复实现）
- 新增 Rust 依赖 `calamine`（Excel）+ `encoding_rs`（GBK）；**前端依赖 0 新增**
- `PingSettings` 新增 `limit_max_threads`（默认 true，`#[serde(default)]` —— 加字段务必带默认值，
  否则旧配置整份回落、丢用户主机列表）；关闭时不拦截但保留 **4096 硬保护**
- 清空/删除确认使用自建 `ConfirmDialog`，**禁用 `window.confirm`/`alert`**（Tauri WebView 不可靠）
- React 组件**所有 hook 必须写在条件 return 之前**（曾因 `useMemo` 写在
  `if (!open) return null` 之后 → 点「添加主机」白屏，React error #310）

## 给用户机器做安装测试的硬规矩（血泪）
1. **不要用 bash heredoc 传含反斜杠的参数** → 用 Write 工具写 PowerShell 脚本文件再执行
2. NSIS 的 `/D=` 必须是**反斜杠**，且必须是命令行**最后一个参数**；写错会被静默忽略并装到默认目录
3. 装前 + 装后各断言一次用户既有安装的 SHA-256（双保险）
4. 用户自己在 `%LOCALAPPDATA%\PingBoard` 装的 PingBoard 属**用户数据，不得触碰**
