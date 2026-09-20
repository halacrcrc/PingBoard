// 展示层格式化工具（纯函数）
import type { Status } from "../types";

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

/** 延迟分档颜色：<50 绿 / <150 黄 / >=150 橙 */
export function rttColorClass(v: number | null | undefined): string {
  if (v === null || v === undefined) return "text-slate-400 dark:text-slate-500";
  if (v < 50) return "text-emerald-600 dark:text-emerald-400";
  if (v < 150) return "text-yellow-600 dark:text-yellow-400";
  return "text-orange-600 dark:text-orange-400";
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
      return "bg-slate-400 text-white";
  }
}
