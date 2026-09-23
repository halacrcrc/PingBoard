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
  /** 清空列表（删除全部目标及其统计） */
  onClearList: () => void;
  onOpenSettings: () => void;
  onExport: (format: "csv" | "txt" | "html", onlySelected: boolean) => void;
  selectionCount: number;
  onStartSelected: () => void;
  onStopSelected: () => void;
  onDeleteSelected: () => void;
}

// 关键：whitespace-nowrap 禁止按钮内部文字被压折行，shrink-0 保证按钮不被 flex 压扁。
// 这样窗口变窄时会先压缩可弹性收缩的搜索框，而不是挤压按钮文字。
// 窄视口（<1200px）用 max-[1199px]:px-2 进一步收窄内边距；宽视口(≥1200px)保持 px-3 原样。
const btnBase =
  "px-3 max-[1199px]:px-1.5 h-7 rounded border text-[13px] leading-none whitespace-nowrap shrink-0 transition-colors disabled:opacity-40 disabled:cursor-not-allowed";
const btnNeutral =
  `${btnBase} border-slate-300 dark:border-slate-600 bg-white dark:bg-slate-800 hover:bg-slate-100 dark:hover:bg-slate-700 text-slate-700 dark:text-slate-200`;
const btnPrimary =
  `${btnBase} border-emerald-600 bg-emerald-600 hover:bg-emerald-700 text-white border-transparent`;
const btnDanger =
  `${btnBase} border-red-600 bg-red-600 hover:bg-red-700 text-white border-transparent`;

// 选中操作组内的紧凑小按钮：比主按钮更小，但同样禁止折行与压缩。
// 窄视口进一步收窄到 px-1。
const btnChip =
  "h-6 px-2 max-[1199px]:px-1 text-[12px] rounded border leading-none whitespace-nowrap shrink-0 border-slate-300 dark:border-slate-600 bg-white dark:bg-slate-800 hover:bg-slate-100 dark:hover:bg-slate-700 text-slate-700 dark:text-slate-200";
const btnChipDanger =
  "h-6 px-2 max-[1199px]:px-1 text-[12px] rounded border leading-none whitespace-nowrap shrink-0 border-transparent bg-red-600 hover:bg-red-700 text-white";

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
  onClearList,
  onOpenSettings,
  onExport,
  selectionCount,
  onStartSelected,
  onStopSelected,
  onDeleteSelected,
}) => {
  const [exportOpen, setExportOpen] = React.useState(false);
  // 导出菜单采用 fixed 定位：工具栏根容器为「横向滚动兜底」加了 overflow，
  // absolute 菜单会被 overflow 裁剪，故用按钮实测坐标固定在视口上，保证菜单完整可见。
  const [exportPos, setExportPos] = React.useState<{ top: number; left: number }>({ top: 0, left: 0 });
  const exportBtnRef = React.useRef<HTMLButtonElement | null>(null);

  const toggleExport = () => {
    if (exportOpen) {
      setExportOpen(false);
      return;
    }
    const el = exportBtnRef.current;
    if (el) {
      const r = el.getBoundingClientRect();
      setExportPos({ top: r.bottom + 4, left: r.left });
    }
    setExportOpen(true);
  };

  // 滚动 / 改变窗口尺寸时关闭菜单，避免 fixed 坐标与按钮错位
  React.useEffect(() => {
    if (!exportOpen) return;
    const close = () => setExportOpen(false);
    window.addEventListener("scroll", close, true);
    window.addEventListener("resize", close);
    return () => {
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("resize", close);
    };
  }, [exportOpen]);

  const handleExport = (format: "csv" | "txt" | "html", onlySelected: boolean) => {
    setExportOpen(false);
    onExport(format, onlySelected);
  };

  return (
    <div className="pb-toolbar flex items-center gap-2 max-[1199px]:gap-[3px] px-3 h-12 shrink-0 border-b bg-white dark:bg-slate-900 border-slate-200 dark:border-slate-700 overflow-x-auto overflow-y-hidden">
      {/* 应用名 + 状态指示灯（shrink-0 + nowrap：状态文字不再被折行） */}
      <div className="flex items-center gap-2 max-[1199px]:gap-1 pr-2 max-[1199px]:pr-1 shrink-0 whitespace-nowrap">
        <span
          className={`inline-block w-2.5 h-2.5 rounded-full ${
            running ? "bg-emerald-500 animate-pulse" : "bg-slate-400"
          }`}
          title={running ? "正在运行" : "已停止"}
        />
        <span className="font-semibold text-[15px] text-slate-800 dark:text-slate-100">
          PingBoard
        </span>
        {/* 状态文字与左侧指示灯配色一致：运行中绿色、已停止灰色（绿=正常/活动） */}
        <span
          className={`text-xs whitespace-nowrap ${
            running ? "text-emerald-700 dark:text-emerald-400" : "text-slate-500 dark:text-slate-400"
          }`}
        >
          {running ? (
            `运行中 ${activeCount} 台`
          ) : (
            <>
              {/* 宽视口：完整文案；窄视口：精简为「N 台」（底部状态栏已展示运行状态，不丢信息） */}
              <span className="max-[1199px]:hidden">{`已停止 · 共 ${totalCount} 台`}</span>
              <span className="hidden max-[1199px]:inline">{`${totalCount} 台`}</span>
            </>
          )}
        </span>
      </div>

      <div className="w-px h-6 bg-slate-200 dark:bg-slate-700 shrink-0" />

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
      <button
        className={btnDanger}
        onClick={onClearList}
        disabled={disabled || totalCount === 0}
        title="删除全部目标（含其统计）"
      >
        清空列表
      </button>

      {/* 导出下拉（菜单 fixed 定位，避免被工具栏 overflow 裁剪）；窄视口隐藏箭头以省宽 */}
      <button
        ref={exportBtnRef}
        className={btnNeutral}
        onClick={toggleExport}
        disabled={disabled || totalCount === 0}
      >
        导出<span className="max-[1199px]:hidden"> ▾</span>
      </button>
      {exportOpen && (
        <>
          <div className="fixed inset-0 z-10" onClick={() => setExportOpen(false)} />
          <div
            className="fixed z-20 w-52 rounded border shadow-lg bg-white dark:bg-slate-800 border-slate-200 dark:border-slate-600 py-1"
            style={{ top: exportPos.top, left: exportPos.left }}
          >
            <div className="px-3 py-1 text-[11px] text-slate-500 dark:text-slate-400">导出全部</div>
            <button className="w-full text-left px-3 py-1.5 hover:bg-slate-100 dark:hover:bg-slate-700" onClick={() => handleExport("csv", false)}>CSV（Excel 兼容）</button>
            <button className="w-full text-left px-3 py-1.5 hover:bg-slate-100 dark:hover:bg-slate-700" onClick={() => handleExport("txt", false)}>纯文本 TXT</button>
            <button className="w-full text-left px-3 py-1.5 hover:bg-slate-100 dark:hover:bg-slate-700" onClick={() => handleExport("html", false)}>HTML 报表</button>
            <div className="px-3 py-1 mt-1 text-[11px] text-slate-500 dark:text-slate-400 border-t border-slate-200 dark:border-slate-600 pt-1">
              仅导出选中（{selectionCount} 台）
            </div>
            <button className="w-full text-left px-3 py-1.5 hover:bg-slate-100 dark:hover:bg-slate-700 disabled:opacity-40" disabled={selectionCount === 0} onClick={() => handleExport("csv", true)}>选中 → CSV</button>
            <button className="w-full text-left px-3 py-1.5 hover:bg-slate-100 dark:hover:bg-slate-700 disabled:opacity-40" disabled={selectionCount === 0} onClick={() => handleExport("txt", true)}>选中 → TXT</button>
            <button className="w-full text-left px-3 py-1.5 hover:bg-slate-100 dark:hover:bg-slate-700 disabled:opacity-40" disabled={selectionCount === 0} onClick={() => handleExport("html", true)}>选中 → HTML</button>
          </div>
        </>
      )}

      <button className={btnNeutral} onClick={onOpenSettings} disabled={disabled}>
        设置
      </button>

      {/* 选中操作区：一个紧凑成组容器（窄视口再压缩间距/内边距并省略「台」字） */}
      {selectionCount > 0 && (
        <div className="flex items-center gap-1 max-[1199px]:gap-0.5 shrink-0 rounded border border-slate-300 dark:border-slate-600 bg-slate-50 dark:bg-slate-800/60 px-1.5 max-[1199px]:px-[3px] h-7">
          <span className="text-[12px] text-slate-500 dark:text-slate-400 whitespace-nowrap">
            已选 <span className="font-medium tabular-nums">{selectionCount}</span>
            <span className="max-[1199px]:hidden"> 台</span>
          </span>
          <button
            className={btnChip}
            onClick={onStartSelected}
            title={`开始选中的 ${selectionCount} 台主机`}
          >
            开始
          </button>
          <button
            className={btnChip}
            onClick={onStopSelected}
            title={`停止选中的 ${selectionCount} 台主机`}
          >
            停止
          </button>
          <button
            className={btnChipDanger}
            onClick={onDeleteSelected}
            title={`删除选中的 ${selectionCount} 台主机`}
          >
            删除
          </button>
        </div>
      )}

      <div className="flex-1" />

      {/* 搜索框：可弹性收缩，窗口变窄时优先被压缩（窄视口收窄首选宽度，min-w 兜底防缩到不可用） */}
      <div className="relative w-56 max-[1199px]:w-48 shrink min-w-[72px]">
        <input
          value={search}
          onChange={(e) => onSearch(e.target.value)}
          placeholder="搜索备注名 / 主机 / IP"
          className="h-7 w-full pl-7 pr-2 rounded border bg-white dark:bg-slate-800 border-slate-300 dark:border-slate-600 text-slate-700 dark:text-slate-200 placeholder:text-slate-500 dark:placeholder:text-slate-400 focus:outline-none focus:ring-1 focus:ring-sky-500"
        />
        {/* 自绘 🔍：emoji 由彩色字形渲染，color 对其无效、无法控制对比度；
            flex 居中不依赖字体行高；pointer-events-none 避免遮挡输入框左缘点击 */}
        <span className="absolute inset-y-0 left-2 flex items-center pointer-events-none text-slate-500 dark:text-slate-400">
          <svg
            viewBox="0 0 16 16"
            className="w-3 h-3"
            fill="none"
            stroke="currentColor"
            strokeWidth={1.8}
            strokeLinecap="round"
            aria-hidden="true"
          >
            <circle cx="6.8" cy="6.8" r="4.3" />
            <path d="M10.2 10.2 13.6 13.6" />
          </svg>
        </span>
      </div>

      {/* 主题切换：窄视口退化为纯图标（保留 title 与无障碍），宽视口保持「🌙 深色 / ☀️ 浅色」 */}
      <button
        className={btnNeutral}
        onClick={onToggleTheme}
        title={theme === "light" ? "切换到深色" : "切换到浅色"}
      >
        <span className="max-[1199px]:hidden">{theme === "light" ? "🌙 深色" : "☀️ 浅色"}</span>
        <span className="hidden max-[1199px]:inline">{theme === "light" ? "🌙" : "☀️"}</span>
      </button>
    </div>
  );
};

export default React.memo(Toolbar);
