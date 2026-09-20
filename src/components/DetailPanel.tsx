// 详情面板：选中主机的延迟折线图、完整统计与最近日志
import React from "react";
import type { LogEntry, TargetState } from "../types";
import { fmtMs, fmtPct, fmtTime, statusLabel } from "../lib/format";
import LatencyChart from "./LatencyChart";

export interface DetailPanelProps {
  target: TargetState | null;
  logs: LogEntry[];
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

const DetailPanel: React.FC<DetailPanelProps> = ({ target, logs }) => {
  if (!target) {
    return (
      <div className="h-full flex items-center justify-center text-slate-400 dark:text-slate-500 text-sm">
        选择左侧任意主机查看详情
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col overflow-hidden">
      {/* 标题 */}
      <div className="px-3 py-2 border-b border-slate-200 dark:border-slate-700 shrink-0">
        <div className="font-semibold text-slate-800 dark:text-slate-100 truncate">
          {target.name || target.host}
        </div>
        <div className="text-[11px] text-slate-500 dark:text-slate-400 font-mono truncate">
          {target.host}
          {target.resolved_ip ? ` → ${target.resolved_ip}` : ""}
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
            <Stat label="启用" value={target.enabled ? "是" : "否"} />
            <Stat label="线程" value={target.running ? "运行中" : "已停止"} />
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
