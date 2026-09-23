// 主表格：多列展示，支持排序、多选（Ctrl/Shift + 复选框）、表头全选、键盘 Delete 删除
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
  /** 勾选/取消勾选单行（与行选中状态共用 selected 集合） */
  onToggleSelect: (id: number) => void;
  /** 表头全选/全不选（仅作用当前传入的 targets） */
  onToggleSelectAll: (selectAll: boolean) => void;
  historyLen: number;
  /** 有新故障事件未查看的目标 id —— 在备注名右侧显示红点，点开该主机后消失 */
  unreadIds: Set<number>;
}

interface Col {
  key: SortKey | null;
  label: string;
  className?: string;
  align?: "left" | "right" | "center";
}

const COLS: Col[] = [
  // 选择列：无排序键，表头位置渲染全选复选框（收窄以给数据列留出空间）
  { key: null, label: "", align: "center", className: "w-8" },
  { key: "name", label: "备注名", align: "left", className: "min-w-[100px]" },
  { key: "host", label: "主机名", align: "left", className: "min-w-[124px]" },
  { key: "resolved_ip", label: "IP 地址", align: "left", className: "min-w-[112px]" },
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
  onToggleSelect,
  onToggleSelectAll,
  historyLen,
  unreadIds,
}) => {
  const headCheckRef = React.useRef<HTMLInputElement | null>(null);

  const sorted = React.useMemo(() => {
    const arr = [...targets];
    arr.sort((a, b) => {
      const c = compare(a, b, sortKey);
      return sortDir === "asc" ? c : -c;
    });
    return arr;
  }, [targets, sortKey, sortDir]);

  // 全选状态只针对当前传入的 targets（即搜索过滤后可见的列表）
  const allSelected = targets.length > 0 && selected.size === targets.length;
  const indeterminate = selected.size > 0 && selected.size < targets.length;

  React.useEffect(() => {
    if (headCheckRef.current) {
      headCheckRef.current.indeterminate = indeterminate;
    }
  }, [indeterminate]);

  return (
    <div className="h-full overflow-auto">
      {/* min-w 让列宽由内容决定：窗口过窄时横向滚动而非挤压列宽/折行表头 */}
      <table className="w-full min-w-[912px] border-collapse text-[12px]">
        <thead className="sticky top-0 z-10">
          <tr className="bg-slate-100 dark:bg-slate-800 border-b border-slate-300 dark:border-slate-600">
            {COLS.map((c, i) => (
              <th
                key={i}
                onClick={() => c.key && onSort(c.key)}
                className={`px-2 py-1.5 font-medium text-slate-600 dark:text-slate-300 whitespace-nowrap ${
                  c.key ? "cursor-pointer select-none hover:bg-slate-200 dark:hover:bg-slate-700" : ""
                } ${c.align === "right" ? "text-right" : c.align === "center" ? "text-center" : "text-left"} ${
                  c.className ?? ""
                }`}
                title={c.key ? "点击排序" : undefined}
              >
                {i === 0 ? (
                  <input
                    ref={headCheckRef}
                    type="checkbox"
                    checked={allSelected}
                    onChange={(e) => onToggleSelectAll(e.target.checked)}
                    className="align-middle cursor-pointer accent-sky-600"
                    title="全选 / 全不选"
                  />
                ) : (
                  <>
                    {c.label}
                    {/* 表头底为 slate-100/slate-800：sky-500 在浅色底仅 2.53:1，改 sky-700 → 5.42:1 */}
                    {c.key === sortKey && (
                      <span className="ml-1 text-sky-700 dark:text-sky-400">{sortDir === "asc" ? "▲" : "▼"}</span>
                    )}
                  </>
                )}
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
                    checked={isSel}
                    onClick={(e) => e.stopPropagation()}
                    onChange={() => onToggleSelect(t.id)}
                    className="align-middle cursor-pointer accent-sky-600"
                    title="选择此行"
                  />
                </td>
                <td className="px-2 py-1 font-medium" title={t.name}>
                  <div className="flex items-center gap-2 min-w-0 max-w-[220px]">
                    <span className="truncate min-w-0">
                      {t.name || <span className="text-slate-500 dark:text-slate-400">（未命名）</span>}
                    </span>
                    {unreadIds.has(t.id) && (
                      <span
                        role="img"
                        aria-label="有新故障事件未查看"
                        title="有新故障事件未查看"
                        className="w-2 h-2 rounded-full bg-red-600 dark:bg-red-500 shrink-0"
                      />
                    )}
                  </div>
                </td>
                <td className="px-2 py-1 font-mono truncate max-w-[240px]" title={t.host}>
                  {t.host}
                </td>
                <td className="px-2 py-1 font-mono text-slate-600 dark:text-slate-300">
                  {t.resolved_ip ?? <span className="text-slate-500 dark:text-slate-400">-</span>}
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
                  <span className="text-slate-500 dark:text-slate-400">/</span>
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
              <td colSpan={COLS.length} className="px-3 py-10 text-center text-slate-500 dark:text-slate-400">
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
