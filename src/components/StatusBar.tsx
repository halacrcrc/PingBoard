// 底部状态栏：运行状态、主机数、总发包/收包/丢包率、活跃线程数、最后刷新时间与运行时长
import React from "react";
import type { Snapshot } from "../types";
import { fmtDuration, fmtPct, fmtTime } from "../lib/format";

export interface StatusBarProps {
  snapshot: Snapshot;
}

const StatusBar: React.FC<StatusBarProps> = ({ snapshot }) => {
  const { running, targets, active_threads, total_sent, total_received, total_failed, loss_pct, started_at, updated_at, settings } = snapshot;
  const now = Date.now();
  const uptime = running && started_at ? now - started_at : 0;

  const item = "flex items-center gap-1 whitespace-nowrap";
  const label = "text-slate-400 dark:text-slate-500";

  return (
    <div className="flex items-center gap-4 px-3 h-7 shrink-0 border-t bg-white dark:bg-slate-900 border-slate-200 dark:border-slate-700 text-[11px] text-slate-600 dark:text-slate-300 overflow-x-auto">
      <span className={item}>
        <span className={`inline-block w-2 h-2 rounded-full ${running ? "bg-emerald-500" : "bg-slate-400"}`} />
        {/* 状态文字与指示灯同色系：运行中绿色、已停止灰色 */}
        <span
          className={
            running ? "text-emerald-600 dark:text-emerald-400" : "text-slate-400 dark:text-slate-500"
          }
        >
          {running ? "运行中" : "已停止"}
        </span>
      </span>
      <span className={item}>
        <span className={label}>主机</span>
        <span className="font-mono">{targets.length}</span>
      </span>
      <span className={item}>
        <span className={label}>已启用</span>
        <span className="font-mono">{targets.filter((t) => t.enabled).length}</span>
      </span>
      <span className={item}>
        <span className={label}>活跃线程</span>
        <span className="font-mono">{active_threads}</span>
        <span className={label}>/{settings.max_threads}</span>
      </span>
      <span className={item}>
        <span className={label}>发包</span>
        <span className="font-mono text-sky-600 dark:text-sky-400">{total_sent}</span>
      </span>
      <span className={item}>
        <span className={label}>收包</span>
        <span className="font-mono text-emerald-600 dark:text-emerald-400">{total_received}</span>
      </span>
      <span className={item}>
        <span className={label}>丢包</span>
        <span className="font-mono text-red-600 dark:text-red-400">{total_failed}</span>
      </span>
      <span className={item}>
        <span className={label}>丢包率</span>
        <span className="font-mono">{fmtPct(loss_pct)}</span>
      </span>
      <span className={item}>
        <span className={label}>运行时长</span>
        <span className="font-mono">{fmtDuration(uptime)}</span>
      </span>
      <div className="flex-1" />
      <span className={item}>
        <span className={label}>最后刷新</span>
        <span className="font-mono">{fmtTime(updated_at)}</span>
      </span>
    </div>
  );
};

export default React.memo(StatusBar);
