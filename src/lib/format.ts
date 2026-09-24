// 展示层格式化工具（纯函数）
import type { EventKind, Status } from "../types";

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
