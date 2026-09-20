// 主表格：对标 PingInfoView 的列布局，支持排序、多选（Ctrl/Shift）、键盘 Delete 删除
import React from "react";
import type { Status, TargetState } from "../types";
import {
  fmtMs,
  fmtPct,
  fmtTime,
  rttColorClass,
  statusBadgeClass,
  statusLabel,
  statusRowClass,
} from "../lib/format";
import Sparkline from "./Sparkline";

export type SortKey =
  | "enabled"
  | "name"
  | "host"
  | "resolved_ip"
  | "status"
  | "last_rtt_ms"
  | "avg_rtt_ms"
  | "min_rtt_ms"
  | "max_rtt_ms"
  | "loss_pct"
  | "received"
  | "last_success_ts";

export interface TargetTableProps {
  targets: TargetState[];
  selected: Set<number>;
  primaryId: number | null;
  sortKey: SortKey;
  sortDir: "asc" | "desc";
  onSort: (key: SortKey) => void;
  onRowClick: (id: number, e: React.MouseEvent) => void;
  onToggleEnabled: (id: number, enabled: boolean) => void;
  historyLen: number;
}

interface Col {
  key: SortKey | null;
  label: string;
  className?: string;
  align?: "left" | "right" | "center";
}

const COLS: Col[] = [
  { key: "enabled", label: "启用", align: "center", className: "w-12" },
  { key: "name", label: "备注名", align: "left", className: "min-w-[120px]" },
  { key: "host", label: "主机名", align: "left", className: "min-w-[150px]" },
  { key: "resolved_ip", label: "IP 地址", align: "left", className: "min-w-[130px]" },
  { key: "status", label: "状态", align: "center", className: "w-20" },
  { key: "last_rtt_ms", label: "延迟(ms)", align: "right", className: "w-20" },
  { key: "avg_rtt_ms", label: "平均", align: "right", className: "w-20" },
  { key: "min_rtt_ms", label: "最小", align: "right", className: "w-20" },
  { key: "max_rtt_ms", label: "最大", align: "right", className: "w-20" },
  { key: "loss_pct", label: "丢包率", align: "right", className: "w-20" },
  { key: "received", label: "成功/失败", align: "right", className: "w-24" },
  { key: null, label: "趋势", align: "center", className: "w-28" },
  { key: "last_success_ts", label: "最后成功时间", align: "right", className: "w-28" },
];

const STATUS_ORDER: Record<Status, number> = {
  ok: 0,
  resolving: 1,
  idle: 2,
  timeout: 3,
  failed: 4,
};

/** 比较函数：null 视为最小，字符串使用本地化比较 */
function compare(a: TargetState, b: TargetState, key: SortKey): number {
  const av = a[key] as number | string | boolean | null;
  const bv = b[key] as number | string | boolean | null;
  if (av === null || av === undefined) {
    if (bv === null || bv === undefined) return 0;
    return -1;
  }
  if (bv === null || bv === undefined) return 1;
  if (typeof av === "boolean" && typeof bv === "boolean") {
    return (av ? 1 : 0) - (bv ? 1 : 0);
  }
  if (typeof av === "number" && typeof bv === "number") {
    return av - bv;
  }
  if (key === "status") {
    return STATUS_ORDER[a.status] - STATUS_ORDER[b.status];
  }
  return String(av).localeCompare(String(bv), "zh-Hans-CN");
}

const TargetTable: React.FC<TargetTableProps> = ({
  targets,
  selected,
  primaryId,
  sortKey,
  sortDir,
  onSort,
  onRowClick,
  onToggleEnabled,
  historyLen,
}) => {
  const sorted = React.useMemo(() => {
    const arr = [...targets];
    arr.sort((a, b) => {
      const c = compare(a, b, sortKey);
      return sortDir === "asc" ? c : -c;
    });
    return arr;
  }, [targets, sortKey, sortDir]);

  return (
    <div className="h-full overflow-auto">
      <table className="w-full border-collapse text-[12px]">
        <thead className="sticky top-0 z-10">
          <tr className="bg-slate-100 dark:bg-slate-800 border-b border-slate-300 dark:border-slate-600">
            {COLS.map((c) => (
              <th
                key={c.label}
                onClick={() => c.key && onSort(c.key)}
                className={`px-2 py-1.5 font-medium text-slate-600 dark:text-slate-300 ${
                  c.key ? "cursor-pointer select-none hover:bg-slate-200 dark:hover:bg-slate-700" : ""
                } ${c.align === "right" ? "text-right" : c.align === "center" ? "text-center" : "text-left"} ${
                  c.className ?? ""
                }`}
                title={c.key ? "点击排序" : undefined}
              >
                {c.label}
                {c.key === sortKey && <span className="ml-1 text-sky-500">{sortDir === "asc" ? "▲" : "▼"}</span>}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {sorted.map((t) => {
            const isSel = selected.has(t.id);
            const isPrimary = primaryId === t.id;
            return (
              <tr
                key={t.id}
                onClick={(e) => onRowClick(t.id, e)}
                className={`pb-table-row border-b border-slate-200 dark:border-slate-700 cursor-pointer ${statusRowClass(
                  t.status
                )} ${isSel ? "outline outline-1 outline-sky-500 -outline-offset-1" : ""} ${
                  isPrimary ? "ring-1 ring-sky-400 ring-inset" : ""
                }`}
              >
                <td className="px-2 py-1 text-center">
                  <input
                    type="checkbox"
                    checked={t.enabled}
                    onClick={(e) => e.stopPropagation()}
                    onChange={(e) => onToggleEnabled(t.id, e.target.checked)}
                    className="align-middle cursor-pointer accent-emerald-600"
                  />
                </td>
                <td className="px-2 py-1 font-medium truncate max-w-[220px]" title={t.name}>
                  {t.name || <span className="text-slate-400">（未命名）</span>}
                </td>
                <td className="px-2 py-1 font-mono truncate max-w-[240px]" title={t.host}>
                  {t.host}
                </td>
                <td className="px-2 py-1 font-mono text-slate-600 dark:text-slate-300">
                  {t.resolved_ip ?? <span className="text-slate-400">-</span>}
                </td>
                <td className="px-2 py-1 text-center">
                  <span className={`inline-block px-1.5 py-0.5 rounded text-[11px] ${statusBadgeClass(t.status)}`}>
                    {statusLabel(t.status)}
                  </span>
                </td>
                <td className={`px-2 py-1 text-right font-mono font-semibold ${rttColorClass(t.last_rtt_ms)}`}>
                  {fmtMs(t.last_rtt_ms)}
                </td>
                <td className="px-2 py-1 text-right font-mono text-slate-600 dark:text-slate-300">{fmtMs(t.avg_rtt_ms)}</td>
                <td className="px-2 py-1 text-right font-mono text-slate-600 dark:text-slate-300">{fmtMs(t.min_rtt_ms)}</td>
                <td className="px-2 py-1 text-right font-mono text-slate-600 dark:text-slate-300">{fmtMs(t.max_rtt_ms)}</td>
                <td className={`px-2 py-1 text-right font-mono ${t.loss_pct > 0 ? "text-red-600 dark:text-red-400" : "text-slate-600 dark:text-slate-300"}`}>
                  {fmtPct(t.loss_pct)}
                </td>
                <td className="px-2 py-1 text-right font-mono">
                  <span className="text-emerald-600 dark:text-emerald-400">{t.received}</span>
                  <span className="text-slate-400">/</span>
                  <span className="text-red-600 dark:text-red-400">{t.failed}</span>
                </td>
                <td className="px-2 py-1">
                  <div className="flex justify-center">
                    <Sparkline data={t.history} slots={historyLen} />
                  </div>
                </td>
                <td className="px-2 py-1 text-right font-mono text-slate-500 dark:text-slate-400">
                  {fmtTime(t.last_success_ts)}
                </td>
              </tr>
            );
          })}
          {sorted.length === 0 && (
            <tr>
              <td colSpan={COLS.length} className="px-3 py-10 text-center text-slate-400">
                没有匹配的目标
              </td>
            </tr>
          )}
        </tbody>
      </table>
    </div>
  );
};

export default React.memo(TargetTable);
