// 详情面板：选中主机的延迟折线图、完整统计与最近日志
import React from "react";
import type { LogEvent, TargetState } from "../types";
import { eventColorClass, fmtDateTimeShort, fmtMs, fmtPct, fmtTime, statusLabel } from "../lib/format";
import LatencyChart from "./LatencyChart";

export interface DetailPanelProps {
  target: TargetState | null;
  /** 已按当前事件等级过滤后的日志（新→旧） */
  logs: LogEvent[];
  /** 本次运行的会话标识（= 进程启动时刻）；用于给「上一次运行」的日志加上日期前缀 */
  session: number;
  /** 全局事件总开关（关闭时单主机开关不产生事件） */
  globalEventsOn: boolean;
  /** 切换目标的监控（启用）状态：参与「开始全部」与开机自动启动 */
  onToggleEnabled: (id: number, enabled: boolean) => void;
  /** 切换单主机「记录事件」开关 */
  onToggleEvents: (id: number, eventsOn: boolean) => void;
  /** 复制当前可见日志到剪贴板 */
  onCopy: () => void;
  /** 导出当前主机的日志为 CSV */
  onExportCsv: () => void;
  /** 清空当前主机的日志（走 ConfirmDialog） */
  onClear: () => void;
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

/** 自绘 switch 基础样式（与「监控」开关一致） */
const switchClass = (on: boolean) =>
  `relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full transition-colors focus:outline-none focus:ring-2 focus:ring-sky-500 focus:ring-offset-1 dark:focus:ring-offset-slate-900 ${
    on ? "bg-emerald-500 dark:bg-emerald-600" : "bg-slate-300 dark:bg-slate-600"
  }`;
const switchKnobClass = (on: boolean) =>
  `inline-block h-4 w-4 transform rounded-full bg-white shadow transition-transform ${
    on ? "translate-x-4" : "translate-x-0.5"
  }`;

/** 日志区操作小按钮（沿用工具栏 chip 风格：禁止折行与压缩） */
const chip =
  "h-6 px-2 max-[1199px]:px-1 text-[11px] rounded border leading-none whitespace-nowrap shrink-0 border-slate-300 dark:border-slate-600 bg-white dark:bg-slate-800 hover:bg-slate-100 dark:hover:bg-slate-700 text-slate-700 dark:text-slate-200 disabled:opacity-40 disabled:cursor-not-allowed";
const chipDanger =
  "h-6 px-2 max-[1199px]:px-1 text-[11px] rounded border leading-none whitespace-nowrap shrink-0 border-transparent bg-red-600 hover:bg-red-700 text-white disabled:opacity-40 disabled:cursor-not-allowed";

const DetailPanel: React.FC<DetailPanelProps> = ({
  target,
  logs,
  session,
  globalEventsOn,
  onToggleEnabled,
  onToggleEvents,
  onCopy,
  onExportCsv,
  onClear,
}) => {
  if (!target) {
    return (
      <div className="h-full flex items-center justify-center text-slate-400 dark:text-slate-500 text-sm">
        选择左侧任意主机查看详情
      </div>
    );
  }

  const recording = globalEventsOn && target.events_on;
  /** 列表里是否混有「上一次运行」留下的日志（这些行的时间会带月-日前缀） */
  const hasHistory = logs.some((l) => l.session !== session);

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
          <span className="text-[11px] text-slate-500 dark:text-slate-400 whitespace-nowrap">
            {target.enabled ? "已监控" : "监控"}
          </span>
          <button
            type="button"
            role="switch"
            aria-checked={target.enabled}
            title="纳入监控：参与「开始全部」与开机自动启动"
            onClick={() => onToggleEnabled(target.id, !target.enabled)}
            className={switchClass(target.enabled)}
          >
            <span className={switchKnobClass(target.enabled)} />
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-auto px-3 py-2">
        {/* 折线图 */}
        <div className="mb-3">
          <div className="text-[11px] text-slate-500 dark:text-slate-400 mb-1 whitespace-nowrap">
            延迟趋势（最近 60 次）
          </div>
          <div className="rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-900 p-1">
            <LatencyChart data={target.history} avg={target.avg_rtt_ms} />
          </div>
        </div>

        {/* 统计 */}
        <div className="mb-3 text-[12px]">
          <div className="text-[11px] text-slate-500 dark:text-slate-400 mb-1 whitespace-nowrap">统计</div>
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
          {/* 标题行：条数 + 单主机「记录事件」开关
              （未读故障红点标在左侧主机列表的备注名旁，此处不重复） */}
          <div className="flex items-center justify-between gap-2 mb-1">
            <div className="flex items-center gap-1.5 min-w-0">
              <span className="text-[11px] text-slate-500 dark:text-slate-400 whitespace-nowrap">
                最近日志（{logs.length}）
              </span>
              {hasHistory && (
                <span
                  className="text-[10px] text-slate-400 dark:text-slate-500 whitespace-nowrap shrink-0"
                  title="包含上一次运行记录的事件，其时间显示为「月-日 时:分:秒」"
                >
                  含历史
                </span>
              )}
            </div>
            <div className="flex items-center gap-1.5 shrink-0">
              <span className="text-[11px] text-slate-500 dark:text-slate-400 whitespace-nowrap">
                记录事件
              </span>
              <button
                type="button"
                role="switch"
                aria-checked={target.events_on}
                title="为该主机记录事件（关闭后不再产生事件）"
                onClick={() => onToggleEvents(target.id, !target.events_on)}
                className={switchClass(target.events_on)}
              >
                <span className={switchKnobClass(target.events_on)} />
              </button>
            </div>
          </div>

          {/* 操作按钮行：复制 / 导出 CSV / 清空（三个独立入口，不合并不隐藏） */}
          <div className="flex items-center justify-end gap-1 mb-1">
            <button className={chip} onClick={onCopy} disabled={logs.length === 0}>
              复制
            </button>
            <button className={chip} onClick={onExportCsv} disabled={logs.length === 0}>
              导出 CSV
            </button>
            <button className={chipDanger} onClick={onClear} disabled={logs.length === 0}>
              清空
            </button>
          </div>

          <div className="rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-900 max-h-48 overflow-auto">
            {logs.length === 0 ? (
              <div className="px-2 py-3 text-center text-slate-400 text-[11px] whitespace-nowrap">
                {recording ? "暂无事件" : "已关闭事件记录"}
              </div>
            ) : (
              <ul className="divide-y divide-slate-100 dark:divide-slate-800 font-mono text-[11px]">
                {logs.map((l) => {
                  const current = l.session === session;
                  return (
                    <li key={`${l.session}-${l.seq}`} className="px-2 py-1 flex gap-2">
                      <span
                        className="text-slate-400 shrink-0"
                        title={
                          current
                            ? undefined
                            : l.session > 0
                              ? `上一次运行（${fmtDateTimeShort(l.session)} 启动）记录`
                              : "更早版本记录的事件"
                        }
                      >
                        {current ? fmtTime(l.ts) : fmtDateTimeShort(l.ts)}
                      </span>
                      <span className={`whitespace-nowrap ${eventColorClass(l.kind)}`}>{l.text}</span>
                    </li>
                  );
                })}
              </ul>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};

export default DetailPanel;
