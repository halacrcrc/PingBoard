// 展示层格式化工具（纯函数）
import type { EventKind, Status, TargetState } from "../types";

/** 格式化延迟（毫秒），保留 1 位小数；null 显示为 "-" */
export function fmtMs(v: number | null | undefined): string {
  if (v === null || v === undefined || Number.isNaN(v)) return "-";
  return v.toFixed(1);
}

/** 格式化百分比，保留 1 位小数 */
export function fmtPct(v: number | null | undefined): string {
  if (v === null || v === undefined || Number.isNaN(v)) return "0.0%";
  return `${v.toFixed(1)}%`;
}

/** 将纪元毫秒格式化为 HH:MM:SS */
export function fmtTime(ts: number | null | undefined): string {
  if (!ts) return "-";
  const d = new Date(ts);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

/** 将纪元毫秒格式化为 MM-DD HH:MM:SS（跨会话事件用，便于看出是哪一批） */
export function fmtDateTimeShort(ts: number | null | undefined): string {
  if (!ts) return "-";
  const d = new Date(ts);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${fmtTime(ts)}`;
}

/** 将毫秒时长为 1h2m3s / 2m3s / 3s 形式 */
export function fmtDuration(ms: number | null | undefined): string {
  if (!ms || ms < 0) return "0s";
  const total = Math.floor(ms / 1000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  if (h > 0) return `${h}h${m}m${s}s`;
  if (m > 0) return `${m}m${s}s`;
  return `${s}s`;
}

/** 延迟分档颜色：<50 绿 / <150 黄 / >=150 橙
 *  浅色用 700 档：600 档四色全部低于 WCAG AA 4.5:1（绿 3.77 / 黄 2.94 / 橙 3.56，
 *  黄档最差），而 700 档在任意行底（白 / slate-50 / emerald-50 / red-50 / sky-50）均 ≥ 4.5:1。
 *  ⚠️ 四档必须同档位调整，只动一档会让色阶亮度失衡。深色 400 档已达标（7.89~11.66），不动。
 *  无数据（null）用 slate-600 而非 slate-500：本函数只服务于表格内，
 *  而表格行底会随状态变成彩色 -50 档（失败行 red-50 上 slate-500 仅 4.35:1）。 */
export function rttColorClass(v: number | null | undefined): string {
  if (v === null || v === undefined) return "text-slate-600 dark:text-slate-400";
  if (v < 50) return "text-emerald-700 dark:text-emerald-400";
  if (v < 150) return "text-yellow-700 dark:text-yellow-400";
  return "text-orange-700 dark:text-orange-400";
}

/** 状态中文标签 */
export function statusLabel(s: Status): string {
  switch (s) {
    case "ok":
      return "正常";
    case "timeout":
      return "超时";
    case "failed":
      return "失败";
    case "resolving":
      return "解析中";
    case "idle":
    default:
      return "未开始";
  }
}

/** 状态对应的行背景色（绿=正常，红=失败，灰=未开始） */
export function statusRowClass(s: Status): string {
  switch (s) {
    case "ok":
      return "bg-emerald-50 dark:bg-emerald-950/40 hover:bg-emerald-100 dark:hover:bg-emerald-900/40";
    case "timeout":
    case "failed":
      return "bg-red-50 dark:bg-red-950/40 hover:bg-red-100 dark:hover:bg-red-900/40";
    case "resolving":
      return "bg-sky-50 dark:bg-sky-950/40 hover:bg-sky-100 dark:hover:bg-sky-900/40";
    case "idle":
    default:
      return "bg-slate-50 dark:bg-slate-800/40 hover:bg-slate-100 dark:hover:bg-slate-700/40";
  }
}

/** 状态徽标样式 */
export function statusBadgeClass(s: Status): string {
  switch (s) {
    case "ok":
      return "bg-emerald-600 text-white";
    case "timeout":
      return "bg-amber-600 text-white";
    case "failed":
      return "bg-red-600 text-white";
    case "resolving":
      return "bg-sky-600 text-white";
    case "idle":
    default:
      // slate-500 起白字才达 WCAG AA（slate-400 仅 2.56:1，slate-500 → 4.76:1）
      return "bg-slate-500 text-white";
  }
}

/** 事件类型文字颜色：故障类红 / 恢复类绿 / 其余灰 */
export function eventColorClass(kind: EventKind): string {
  switch (kind) {
    case "fault":
    case "unreachable":
    case "dns_fail":
      return "text-red-600 dark:text-red-400";
    case "recover":
    case "first_ok":
      return "text-emerald-700 dark:text-emerald-400";
    default:
      return "text-slate-600 dark:text-slate-300";
  }
}

/** 事件类型中文标签（用于复制 / 展示） */
export function eventKindLabel(kind: EventKind): string {
  switch (kind) {
    case "fault":
      return "故障";
    case "unreachable":
      return "无法连通";
    case "dns_fail":
      return "解析失败";
    case "recover":
      return "已恢复";
    case "first_ok":
      return "首次连通";
    case "start":
      return "开始探测";
    case "stop":
      return "已停止";
    case "config_change":
      return "配置变更";
    default:
      return "事件";
  }
}

/** 是否属于「未读故障红点」计入的故障类型（不含 recover） */
export function isUnreadKind(kind: EventKind): boolean {
  return kind === "fault" || kind === "unreachable" || kind === "dns_fail";
}

/** 默认字体栈：必须与 src/styles.css 中 body 的 font-family 完全一致（守护测试校验） */
export const DEFAULT_FONT_STACK =
  '"Microsoft YaHei UI", "Microsoft YaHei", "Segoe UI", system-ui, -apple-system, sans-serif';

/**
 * 界面缩放百分比 → document.documentElement.style.zoom 的赋值字符串（纯函数）。
 * 100（或不合法值）返回空串 = 清除内联缩放、恢复默认；
 * 其余钳制到 50..=200 后换算为小数（与后端 stats::normalize_settings 的 50..=200 一致）。
 */
export function zoomStyle(uiScale: number): string {
  const n = Math.round(Number(uiScale));
  if (!Number.isFinite(n) || n === 100) return "";
  const clamped = Math.min(200, Math.max(50, n));
  return String(clamped / 100);
}

/**
 * 自定义字体族名 → document.body.style.fontFamily 的赋值字符串（纯函数）。
 * null / 空串 / 纯空白返回空串 = 恢复系统默认（styles.css 里的字体栈）；
 * 否则「"族名", 默认栈」——默认栈兜底，族名缺字时逐级降级。
 */
export function fontFamilyStyle(family: string | null | undefined): string {
  const f = (family ?? "").trim();
  if (!f) return "";
  return `"${f}", ${DEFAULT_FONT_STACK}`;
}

/**
 * 当前根元素缩放系数（zoom 100% / 未设置时为 1）。
 *
 * ⚠️ zoom 下 fixed 定位换算：`getBoundingClientRect()` 返回的是**视觉坐标**（已含缩放），
 * 而给 fixed 元素设置 `style.top/left` 时数值会被根元素 zoom **再放大一次**
 * → 用 rect 值定位会随缩放偏离（125% 时偏 25%）。
 * 修法：定位前先把视觉坐标除以本系数换算回 CSS 像素。
 */
export function currentZoomFactor(): number {
  const z = parseFloat(document.documentElement.style.zoom);
  return Number.isFinite(z) && z > 0 ? z : 1;
}

/* ===================== 报表导出筛选（与 Rust `export.rs` 对齐） ===================== */

/** 报表导出筛选口径（对应 Rust `export::ExportFilter`） */
export type ExportFilter = "all" | "none_loss" | "all_failed";

/**
 * 判定单个节点是否命中导出筛选口径。
 *
 * ⚠️ 与 Rust `export::matches_filter` 是**同一判定的两个实现**：
 * 本函数只用于对话框里的「命中台数」预览，**不参与写文件**；
 * 导出结果的唯一权威是后端。改判据必须两侧同时改
 * （两侧测试用同一组边界用例锁定：`sent>0` 零丢包 / 部分丢包 / 全失败 / `sent==0`）。
 *
 * 🩸 两种口径都要求 `sent > 0`：`sent === 0` 表示从未 ping 或统计已被清空，
 * 此时 `received` 与 `failed` 同为 0，且后端 `stats::loss_pct` 对 `sent === 0` 定义为 `0.0`。
 * 若只判 `failed === 0`，**从未开始的主机会被误当成「零丢包」**。
 * 在 `sent > 0` 的前提下两种口径互斥（一个要求 `received === sent`，一个要求 `received === 0`）。
 */
export function matchesExportFilter(t: TargetState, f: ExportFilter): boolean {
  switch (f) {
    case "all":
      return true;
    case "none_loss":
      return t.sent > 0 && t.failed === 0;
    case "all_failed":
      return t.sent > 0 && t.received === 0;
  }
}

/** 筛选口径中文名（与 Rust `export::filter_label` 保持一致） */
export function exportFilterLabel(f: ExportFilter): string {
  switch (f) {
    case "all":
      return "不过滤";
    case "none_loss":
      return "零丢包";
    case "all_failed":
      return "全部未成功";
  }
}

/** 筛选口径的一句话判定式说明（与 Rust `export::filter_hint` 语义一致） */
export function exportFilterHint(f: ExportFilter): string {
  switch (f) {
    case "all":
      return "导出全部节点，与原导出行为一致";
    case "none_loss":
      return "仅 sent>0 且 failed=0：全程一次都没丢包";
    case "all_failed":
      return "仅 sent>0 且 received=0：所有 ping 一次都没成功";
  }
}

/* ===================== 文件夹范围筛选（v1.1.10） ===================== */

/**
 * 判断某节点是否落在当前勾选的文件夹范围内。
 *
 * 口径：**一个都没勾 = 全部**（不是「什么都不显示」）。
 * `folder_id` 为 null 表示临时区，与后端 `list_folders` 里 `id === 0` 的
 * 「临时区」那一项对应，故这里用 `?? 0` 归一。
 *
 * 纯函数，便于单测锁住「不勾 = 全部」这条反直觉但已定稿的口径。
 */
export function inFolderScope(t: TargetState, scope: ReadonlySet<number>): boolean {
  if (scope.size === 0) return true;
  return scope.has(t.folder_id ?? 0);
}

/** 按文件夹范围过滤节点（侧边栏范围 ∩ 搜索关键词 的第一段） */
export function filterByFolderScope(
  targets: TargetState[],
  scope: ReadonlySet<number>
): TargetState[] {
  if (scope.size === 0) return targets;
  return targets.filter((t) => inFolderScope(t, scope));
}

/** 切换勾选某一项；返回新集合（不修改入参） */
export function toggleScope(scope: ReadonlySet<number>, id: number): Set<number> {
  const next = new Set(scope);
  if (!next.delete(id)) next.add(id);
  return next;
}

/** 文件夹 id 0 固定代表「临时区」（与后端 list_folders 的约定） */
export const TEMP_AREA_ID = 0;
/**
 * 「清空列表」要删除的主机 id —— **只含临时区**（`folder_id == null`）。
 *
 * 口径（已定稿，见 docs/folder-design.md 7.4）：清空列表**只清临时区**，
 * 文件夹里的 IP 不受影响。要删文件夹内的主机请用侧边栏的「删除文件夹」。
 *
 * 纯函数，便于单测锁住「文件夹里的 IP 不会被误删」这条安全性质。
 */
export function tempAreaIds(targets: ReadonlyArray<{ id: number; folder_id: number | null }>): number[] {
  return targets.filter((t) => t.folder_id === null).map((t) => t.id);
}

/** 临时区台数（用于「清空列表」确认框文案） */
export function tempAreaCount(targets: ReadonlyArray<{ folder_id: number | null }>): number {
  return targets.reduce((a, t) => (t.folder_id === null ? a + 1 : a), 0);
}