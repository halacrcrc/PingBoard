// 详情面板：选中主机的延迟折线图、完整统计与最近日志
import React from "react";
import type { LogEntry, TargetState } from "../types";
import { fmtMs, fmtPct, fmtTime, statusLabel } from "../lib/format";
import LatencyChart from "./LatencyChart";

export interface DetailPanelProps {
  target: TargetState | null;
  logs: LogEntry[];
  /** 切换目标的监控（启用）状态：参与「开始全部」与开机自动启动 */
  onToggleEnabled: (id: number, enabled: boolean) => void;
}

const Stat: React.FC<{ label: string; value: React.ReactNode; accent?: string }> = ({
  label,
  value,
  accent,
}) => (
  <div className="flex items-center justify-between py-0.5">
    <span className="text-slate-400 dark:text-slate-500">{label}</span>
    <span className={`font-mono ${accent ?? "text-slate-700 dark:text-slate-200"}`}>{value}</span>
  </div>
);

const DetailPanel: React.FC<DetailPanelProps> = ({ target, logs, onToggleEnabled }) => {
  if (!target) {
    return (
      <div className="h-full flex items-center justify-center text-slate-400 dark:text-slate-500 text-sm">
        选择左侧任意主机查看详情
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col overflow-hidden">
      {/* 标题 + 监控开关（开关放右侧，与表格里的普通复选框明显区分） */}
      <div className="px-3 py-2 border-b border-slate-200 dark:border-slate-700 shrink-0 flex items-start justify-between gap-2">
        <div className="min-w-0">
          <div className="font-semibold text-slate-800 dark:text-slate-100 truncate">
            {target.name || target.host}
          </div>
          <div className="text-[11px] text-slate-500 dark:text-slate-400 font-mono truncate">
            {target.host}
            {target.resolved_ip ? ` → ${target.resolved_ip}` : ""}
          </div>
        </div>
        {/* 自绘 switch：绿色=已纳入监控，灰色=未监控 */}
        <div className="flex items-center gap-1.5 shrink-0 pt-0.5">
          <span className="text-[11px] text-slate-500 dark:text-slate-400">
            {target.enabled ? "已监控" : "监控"}
          </span>
          <button
            type="button"
            role="switch"
            aria-checked={target.enabled}
            title="纳入监控：参与「开始全部」与开机自动启动"
            onClick={() => onToggleEnabled(target.id, !target.enabled)}
            className={`relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full transition-colors focus:outline-none focus:ring-2 focus:ring-sky-500 focus:ring-offset-1 dark:focus:ring-offset-slate-900 ${
              target.enabled ? "bg-emerald-500 dark:bg-emerald-600" : "bg-slate-300 dark:bg-slate-600"
            }`}
          >
            <span
              className={`inline-block h-4 w-4 transform rounded-full bg-white shadow transition-transform ${
                target.enabled ? "translate-x-4" : "translate-x-0.5"
              }`}
            />
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-auto px-3 py-2">
        {/* 折线图 */}
        <div className="mb-3">
          <div className="text-[11px] text-slate-500 dark:text-slate-400 mb-1">延迟趋势（最近 60 次）</div>
          <div className="rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-900 p-1">
            <LatencyChart data={target.history} avg={target.avg_rtt_ms} />
          </div>
        </div>

        {/* 统计 */}
        <div className="mb-3 text-[12px]">
          <div className="text-[11px] text-slate-500 dark:text-slate-400 mb-1">统计</div>
          <div className="rounded border border-slate-200 dark:border-slate-700 p-2 grid grid-cols-2 gap-x-4">
            <Stat label="状态" value={statusLabel(target.status)} />
            <Stat label="TTL" value={target.ttl ?? "-"} />
            <Stat label="最近延迟" value={fmtMs(target.last_rtt_ms)} accent="text-emerald-600 dark:text-emerald-400" />
            <Stat label="平均延迟" value={fmtMs(target.avg_rtt_ms)} />
            <Stat label="最小延迟" value={fmtMs(target.min_rtt_ms)} />
            <Stat label="最大延迟" value={fmtMs(target.max_rtt_ms)} />
            <Stat label="发送" value={target.sent} />
            <Stat label="接收" value={target.received} />
            <Stat label="失败" value={target.failed} accent="text-red-600 dark:text-red-400" />
            <Stat label="丢包率" value={fmtPct(target.loss_pct)} accent={target.loss_pct > 0 ? "text-red-600 dark:text-red-400" : undefined} />
            <Stat label="连续失败" value={target.consecutive_fail} />
            <Stat label="最后成功" value={fmtTime(target.last_success_ts)} />
            {/* 与工具栏/状态栏统一：运行中绿色、已停止灰色 */}
            <Stat
              label="线程"
              value={target.running ? "运行中" : "已停止"}
              accent={
                target.running
                  ? "text-emerald-600 dark:text-emerald-400"
                  : "text-slate-400 dark:text-slate-500"
              }
            />
            {target.last_error && (
              <div className="col-span-2 pt-1 text-red-600 dark:text-red-400 break-all">
                错误：{target.last_error}
              </div>
            )}
          </div>
        </div>

        {/* 日志 */}
        <div className="text-[12px]">
          <div className="text-[11px] text-slate-500 dark:text-slate-400 mb-1">
            最近日志（失败 / 恢复事件）
          </div>
          <div className="rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-900 max-h-48 overflow-auto">
            {logs.length === 0 ? (
              <div className="px-2 py-3 text-center text-slate-400 text-[11px]">暂无事件</div>
            ) : (
              <ul className="divide-y divide-slate-100 dark:divide-slate-800 font-mono text-[11px]">
                {logs.map((l, i) => (
                  <li key={i} className="px-2 py-1 flex gap-2">
                    <span className="text-slate-400">{fmtTime(l.ts)}</span>
                    <span
                      className={
                        l.kind === "fail"
                          ? "text-red-600 dark:text-red-400"
                          : l.kind === "recover"
                          ? "text-emerald-600 dark:text-emerald-400"
                          : "text-slate-600 dark:text-slate-300"
                      }
                    >
                      {l.text}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};

export default DetailPanel;
