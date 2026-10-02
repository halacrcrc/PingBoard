// 报表导出对话框：选择「导出范围 + 筛选口径 + 文件格式」，并实时预览命中台数
//
// 为什么用对话框而不是把筛选项塞进工具栏下拉：筛选口径有 3 种、格式有 3 种，
// 与「全部 / 仅选中」两个范围做笛卡尔积会产生 18 个菜单项，下拉会被撑到不可用。
// 对话框把三个维度正交排布，且能显示命中台数 —— 导出前就看得见「会导出几台」。
//
// 判定口径全部委托给 `format.ts` 的 `matchesExportFilter`（与 Rust `export::matches_filter`
// 是同一判定的两个实现，前端只做预览计数，写文件由后端权威决定）。
import React from "react";
import type { TargetState } from "../types";
import {
  exportFilterHint,
  exportFilterLabel,
  matchesExportFilter,
  type ExportFilter,
} from "../lib/format";

/** 报表格式：与 Rust `write_report` 支持的 csv / txt / html 一一对应 */
export type ExportFormat = "csv" | "txt" | "html";

export interface ExportDialogProps {
  open: boolean;
  onClose: () => void;
  /** 全量目标（命中台数预览用；导出范围为「仅选中」时会在其上再按 id 过滤） */
  targets: TargetState[];
  /** 已选中的目标 id */
  selected: Set<number>;
  /** 执行导出：弹保存框 + invoke 由 App 负责 */
  onExport: (format: ExportFormat, onlySelected: boolean, filter: ExportFilter) => void;
}

const FORMATS: { id: ExportFormat; label: string }[] = [
  { id: "csv", label: "CSV（Excel 兼容）" },
  { id: "txt", label: "纯文本 TXT" },
  { id: "html", label: "HTML 报表" },
];

/** 范围单选项。刻意用数组 + 单一 className 表达式驱动，不要拆成两个手写按钮：
 *  v1.1.9 曾把两个按钮的 className 都写成 onlySelected ? chipOn : chipOff，
 *  结果两项永远同色、用户看不出选的是哪个（aria-checked 仍正确，只有视觉错）。 */
export type ExportScope = "all" | "selected";

/** 范围项定义（计数在渲染时算，故只存标签与取值） */
const SCOPES: { id: ExportScope; label: string }[] = [
  { id: "all", label: "全部节点" },
  { id: "selected", label: "仅选中" },
];

/** 某范围项是否处于选中态（纯函数，供测试锁定「两项永不同色」） */
export function scopeSelected(scope: ExportScope, id: ExportScope): boolean {
  return scope === id;
}

const FILTERS: { id: ExportFilter; desc: string }[] = [
  { id: "none_loss", desc: "全程一次都没丢包" },
  { id: "all_failed", desc: "所有 ping 一次都没成功" },
  { id: "all", desc: "不筛选，导出全部" },
];

/** 口径对应的语义色：绿 = 正常、红 = 失败（项目配色铁律，不套股票涨红跌绿） */
const FILTER_TONE: Record<ExportFilter, string> = {
  none_loss: "text-emerald-700 dark:text-emerald-400",
  all_failed: "text-red-600 dark:text-red-400",
  all: "text-slate-600 dark:text-slate-300",
};

const chip =
  "px-2.5 h-7 rounded border text-[13px] leading-none whitespace-nowrap shrink-0 transition-colors";
const chipOff = `${chip} border-slate-300 dark:border-slate-600 bg-white dark:bg-slate-800 hover:bg-slate-100 dark:hover:bg-slate-700 text-slate-700 dark:text-slate-200`;
const chipOn = `${chip} border-sky-600 bg-sky-600 hover:bg-sky-700 text-white border-transparent`;

const ExportDialog: React.FC<ExportDialogProps> = ({
  open,
  onClose,
  targets,
  selected,
  onExport,
}) => {
  const [format, setFormat] = React.useState<ExportFormat>("csv");
  const [filter, setFilter] = React.useState<ExportFilter>("all");
  const [scope, setScope] = React.useState<ExportScope>("all");
  const onlySelected = scope === "selected";

  // 每次打开时把草稿复位到默认「全部 / 不过滤 / CSV」。
  // ⚠️ 用 wasOpenRef 而不是把 open 放进依赖：open 翻转时初始化一次即可，
  // 且 hooks 必须写在下面的 `if (!open) return null` 之前（R5）。
  const wasOpenRef = React.useRef(false);
  React.useEffect(() => {
    const justOpened = open && !wasOpenRef.current;
    wasOpenRef.current = open;
    if (!justOpened) return;
    setFormat("csv");
    setFilter("all");
    setScope("all");
  });

  // Esc 关闭
  React.useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  // 范围内的候选集：与后端 `export_targets(ids)` 的取数口径一致（仅选中 = 按 id 过滤）
  const scoped = React.useMemo(
    () => (onlySelected ? targets.filter((t) => selected.has(t.id)) : targets),
    [onlySelected, targets, selected]
  );
  // 各口径在范围内的命中台数
  const countBy = React.useMemo(() => {
    const m: Record<ExportFilter, number> = { all: 0, none_loss: 0, all_failed: 0 };
    for (const t of scoped) {
      for (const f of FILTERS) {
        if (matchesExportFilter(t, f.id)) m[f.id] += 1;
      }
    }
    return m;
  }, [scoped]);
  const matched = countBy[filter];

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/40">
      <div
        className="w-[560px] max-w-[92vw] max-h-[85vh] flex flex-col rounded-lg shadow-2xl bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700"
        role="dialog"
        aria-modal="true"
        aria-label="导出报表"
      >
        <div className="px-4 h-11 flex items-center justify-between border-b border-slate-200 dark:border-slate-700">
          <div className="font-semibold text-slate-800 dark:text-slate-100">导出报表</div>
          <button
            className="text-slate-500 dark:text-slate-400 hover:text-slate-700 dark:hover:text-slate-200 text-[13px] whitespace-nowrap"
            onClick={onClose}
          >
            关闭
          </button>
        </div>

        <div className="px-4 py-3 flex-1 overflow-auto text-[13px] text-slate-700 dark:text-slate-200">
          {/* 范围 */}
          <div className="flex items-center gap-2 mb-1">
            <span className="w-14 shrink-0 text-slate-600 dark:text-slate-300">范围</span>
            <div className="flex items-center gap-1.5" role="radiogroup" aria-label="导出范围">
              {SCOPES.map((s) => {
                const on = scopeSelected(scope, s.id);
                const cnt = s.id === "all" ? targets.length : selected.size;
                return (
                  <button
                    key={s.id}
                    className={on ? chipOn : chipOff}
                    role="radio"
                    aria-checked={on}
                    disabled={s.id === "selected" && selected.size === 0}
                    title={s.id === "selected" && selected.size === 0 ? "尚未选中任何主机" : undefined}
                    onClick={() => setScope(s.id)}
                  >
                    {s.label}（{cnt}）
                  </button>
                );
              })}
            </div>
          </div>

          {/* 筛选口径 */}
          <div className="flex items-start gap-2 mt-3">
            <span className="w-14 shrink-0 pt-1.5 text-slate-600 dark:text-slate-300">筛选</span>
            <div className="flex-1 flex flex-col gap-1" role="radiogroup" aria-label="筛选口径">
              {FILTERS.map((f) => (
                <button
                  key={f.id}
                  className={`w-full text-left px-2.5 py-1.5 rounded border whitespace-nowrap ${
                    filter === f.id
                      ? "border-sky-600 bg-sky-50 dark:bg-sky-900/30"
                      : "border-slate-200 dark:border-slate-700 hover:bg-slate-50 dark:hover:bg-slate-800"
                  }`}
                  role="radio"
                  aria-checked={filter === f.id}
                  onClick={() => setFilter(f.id)}
                >
                  <span className="flex items-center gap-2">
                    <span
                      className={`w-2.5 h-2.5 rounded-full shrink-0 border ${
                        filter === f.id
                          ? "border-sky-600 bg-sky-600"
                          : "border-slate-400 dark:border-slate-500"
                      }`}
                      aria-hidden="true"
                    />
                    <span className={`font-medium ${FILTER_TONE[f.id]}`}>
                      {exportFilterLabel(f.id)}
                    </span>
                    <span className="text-slate-500 dark:text-slate-400">{f.desc}</span>
                    <span className="ml-auto shrink-0 tabular-nums text-slate-500 dark:text-slate-400">
                      {countBy[f.id]} 台
                    </span>
                  </span>
                  <span className="block pl-[18px] text-[12px] text-slate-500 dark:text-slate-400 whitespace-nowrap">
                    {exportFilterHint(f.id)}
                  </span>
                </button>
              ))}
            </div>
          </div>

          {/* 格式 */}
          <div className="flex items-center gap-2 mt-3">
            <span className="w-14 shrink-0 text-slate-600 dark:text-slate-300">格式</span>
            <div className="flex items-center gap-1.5" role="radiogroup" aria-label="文件格式">
              {FORMATS.map((f) => (
                <button
                  key={f.id}
                  className={format === f.id ? chipOn : chipOff}
                  role="radio"
                  aria-checked={format === f.id}
                  onClick={() => setFormat(f.id)}
                >
                  {f.label}
                </button>
              ))}
            </div>
          </div>

          <div className="mt-3 px-2.5 py-2 rounded bg-slate-50 dark:bg-slate-800/60 text-[12px] text-slate-500 dark:text-slate-400">
            三种格式的字段结构完全一致（15 列 / 15 个表头），筛选只改变导出的行集合，
            不改变字段结构；「序号」在筛选后从 1 重新连续编号。
          </div>
        </div>

        <div className="px-4 h-12 flex items-center justify-between gap-2 border-t border-slate-200 dark:border-slate-700">
          <span
            className={`text-[12px] whitespace-nowrap ${
              matched === 0
                ? "text-red-600 dark:text-red-400"
                : "text-slate-500 dark:text-slate-400"
            }`}
          >
            {matched === 0
              ? "当前条件下没有可导出的节点"
              : `将导出 ${matched} 台节点（CSV 固定 15 列）`}
          </span>
          <div className="flex items-center gap-2 shrink-0">
            <button className={chipOff} onClick={onClose}>
              取消
            </button>
            <button
              className="px-3 h-7 rounded bg-sky-600 hover:bg-sky-700 text-white text-[13px] leading-none whitespace-nowrap disabled:opacity-40 disabled:cursor-not-allowed"
              disabled={matched === 0}
              title={matched === 0 ? "当前条件下没有可导出的节点" : undefined}
              onClick={() => onExport(format, onlySelected, filter)}
            >
              导出
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};

export default ExportDialog;
