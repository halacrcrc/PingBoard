// 顶部工具栏：应用名、操作按钮、搜索框、主题切换与状态指示灯
import React from "react";

export interface ToolbarProps {
  running: boolean;
  activeCount: number;
  totalCount: number;
  search: string;
  onSearch: (v: string) => void;
  theme: "light" | "dark";
  onToggleTheme: () => void;
  disabled: boolean;
  onAdd: () => void;
  onStartAll: () => void;
  onStopAll: () => void;
  onResetAll: () => void;
  onOpenSettings: () => void;
  onExport: (format: "csv" | "txt" | "html", onlySelected: boolean) => void;
  selectionCount: number;
  onDeleteSelected: () => void;
}

const btnBase =
  "px-3 h-7 rounded border text-[13px] leading-none transition-colors disabled:opacity-40 disabled:cursor-not-allowed";
const btnNeutral =
  `${btnBase} border-slate-300 dark:border-slate-600 bg-white dark:bg-slate-800 hover:bg-slate-100 dark:hover:bg-slate-700 text-slate-700 dark:text-slate-200`;
const btnPrimary =
  `${btnBase} border-emerald-600 bg-emerald-600 hover:bg-emerald-700 text-white border-transparent`;
const btnDanger =
  `${btnBase} border-red-600 bg-red-600 hover:bg-red-700 text-white border-transparent`;

const Toolbar: React.FC<ToolbarProps> = ({
  running,
  activeCount,
  totalCount,
  search,
  onSearch,
  theme,
  onToggleTheme,
  disabled,
  onAdd,
  onStartAll,
  onStopAll,
  onResetAll,
  onOpenSettings,
  onExport,
  selectionCount,
  onDeleteSelected,
}) => {
  const [exportOpen, setExportOpen] = React.useState(false);

  const handleExport = (format: "csv" | "txt" | "html", onlySelected: boolean) => {
    setExportOpen(false);
    onExport(format, onlySelected);
  };

  return (
    <div className="flex items-center gap-2 px-3 h-12 shrink-0 border-b bg-white dark:bg-slate-900 border-slate-200 dark:border-slate-700">
      {/* 应用名 + 状态指示灯 */}
      <div className="flex items-center gap-2 pr-2">
        <span
          className={`inline-block w-2.5 h-2.5 rounded-full ${
            running ? "bg-emerald-500 animate-pulse" : "bg-slate-400"
          }`}
          title={running ? "正在运行" : "已停止"}
        />
        <span className="font-semibold text-[15px] text-slate-800 dark:text-slate-100">
          PingBoard
        </span>
        <span className="text-slate-400 dark:text-slate-500 text-xs">
          {running ? `运行中 ${activeCount} 台` : `已停止 · 共 ${totalCount} 台`}
        </span>
      </div>

      <div className="w-px h-6 bg-slate-200 dark:bg-slate-700" />

      <button className={btnPrimary} onClick={onAdd} disabled={disabled}>
        添加主机
      </button>
      <button className={btnNeutral} onClick={onStartAll} disabled={disabled || totalCount === 0}>
        开始全部
      </button>
      <button className={btnNeutral} onClick={onStopAll} disabled={disabled || !running}>
        停止全部
      </button>
      <button
        className={btnNeutral}
        onClick={onResetAll}
        disabled={disabled || totalCount === 0}
        title="清空全部统计（不影响主机列表）"
      >
        清空统计
      </button>

      {/* 导出下拉 */}
      <div className="relative">
        <button className={btnNeutral} onClick={() => setExportOpen((v) => !v)} disabled={disabled || totalCount === 0}>
          导出 ▾
        </button>
        {exportOpen && (
          <>
            <div className="fixed inset-0 z-10" onClick={() => setExportOpen(false)} />
            <div className="absolute z-20 mt-1 w-52 rounded border shadow-lg bg-white dark:bg-slate-800 border-slate-200 dark:border-slate-600 py-1">
              <div className="px-3 py-1 text-[11px] text-slate-400 dark:text-slate-500">导出全部</div>
              <button className="w-full text-left px-3 py-1.5 hover:bg-slate-100 dark:hover:bg-slate-700" onClick={() => handleExport("csv", false)}>CSV（Excel 兼容）</button>
              <button className="w-full text-left px-3 py-1.5 hover:bg-slate-100 dark:hover:bg-slate-700" onClick={() => handleExport("txt", false)}>纯文本 TXT</button>
              <button className="w-full text-left px-3 py-1.5 hover:bg-slate-100 dark:hover:bg-slate-700" onClick={() => handleExport("html", false)}>HTML 报表</button>
              <div className="px-3 py-1 mt-1 text-[11px] text-slate-400 dark:text-slate-500 border-t border-slate-200 dark:border-slate-600 pt-1">
                仅导出选中（{selectionCount} 台）
              </div>
              <button className="w-full text-left px-3 py-1.5 hover:bg-slate-100 dark:hover:bg-slate-700 disabled:opacity-40" disabled={selectionCount === 0} onClick={() => handleExport("csv", true)}>选中 → CSV</button>
              <button className="w-full text-left px-3 py-1.5 hover:bg-slate-100 dark:hover:bg-slate-700 disabled:opacity-40" disabled={selectionCount === 0} onClick={() => handleExport("txt", true)}>选中 → TXT</button>
              <button className="w-full text-left px-3 py-1.5 hover:bg-slate-100 dark:hover:bg-slate-700 disabled:opacity-40" disabled={selectionCount === 0} onClick={() => handleExport("html", true)}>选中 → HTML</button>
            </div>
          </>
        )}
      </div>

      <button className={btnNeutral} onClick={onOpenSettings} disabled={disabled}>
        设置
      </button>

      {selectionCount > 0 && (
        <button className={btnDanger} onClick={onDeleteSelected}>
          删除选中（{selectionCount}）
        </button>
      )}

      <div className="flex-1" />

      {/* 搜索框 */}
      <div className="relative">
        <input
          value={search}
          onChange={(e) => onSearch(e.target.value)}
          placeholder="搜索备注名 / 主机 / IP"
          className="h-7 w-56 pl-7 pr-2 rounded border bg-white dark:bg-slate-800 border-slate-300 dark:border-slate-600 text-slate-700 dark:text-slate-200 placeholder:text-slate-400 focus:outline-none focus:ring-1 focus:ring-sky-500"
        />
        <span className="absolute left-2 top-1 text-slate-400 text-xs">🔍</span>
      </div>

      <button
        className={btnNeutral}
        onClick={onToggleTheme}
        title={theme === "light" ? "切换到深色" : "切换到浅色"}
      >
        {theme === "light" ? "🌙 深色" : "☀️ 浅色"}
      </button>
    </div>
  );
};

export default React.memo(Toolbar);
