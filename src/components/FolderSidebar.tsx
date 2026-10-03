// 文件夹侧边栏（v1.1.10）：多选筛选 + 命名 / 颜色 / 删除
//
// 口径（已定稿，见 docs/folder-design.md 7.1）：
//   * 一个都不勾 = 全部节点（不是「什么都不显示」）
//   * 「临时区」是 folder_id 为 null 的那一组，列表里固定用 id 0 代表
//   * 只做筛选，**不隐藏任何既有功能入口**；主表 13 列布局与排序完全不动
import React from "react";
import type { FolderEntry } from "../types";
import { TEMP_AREA_ID, toggleScope } from "../lib/format";
import ConfirmDialog from "./ConfirmDialog";

export interface FolderSidebarProps {
  folders: FolderEntry[];
  /** 当前勾选的文件夹 id 集合；空 = 全部 */
  scope: ReadonlySet<number>;
  onToggle: (id: number) => void;
  /** 清空勾选 = 回到「显示全部节点」。**不是**逐个 toggle —— toggle 会把
   *  当前没勾的文件夹也勾上（scope={A} 时清空会得到 {临时区} 而非 {}）。*/
  onClearAll: () => void;
  onCreate: (name: string, color: string) => void;
  onRename: (id: number, name: string, color: string) => void;
  onDelete: (id: number) => void;
  disabled: boolean;
}

/** 预设色板：与 Rust `FOLDER_COLORS` 一致（不可自由取色，保证 WCAG AA） */
const COLORS: { key: string; dot: string; label: string }[] = [
  { key: "slate", dot: "bg-slate-500", label: "灰" },
  { key: "sky", dot: "bg-sky-600", label: "蓝" },
  { key: "emerald", dot: "bg-emerald-600", label: "绿" },
  { key: "amber", dot: "bg-amber-600", label: "黄" },
  { key: "red", dot: "bg-red-600", label: "红" },
  { key: "violet", dot: "bg-violet-600", label: "紫" },
  { key: "pink", dot: "bg-pink-600", label: "粉" },
  { key: "teal", dot: "bg-teal-600", label: "青" },
];

const dotClass = (key: string): string =>
  (COLORS.find((c) => c.key === key) ?? COLORS[0]).dot;

const btn = "px-2 h-7 rounded border text-[12px] leading-none whitespace-nowrap shrink-0";
const btnNeutral = `${btn} border-slate-300 dark:border-slate-600 bg-white dark:bg-slate-800 hover:bg-slate-100 dark:hover:bg-slate-700 text-slate-700 dark:text-slate-200`;
const btnPrimary = `${btn} border-sky-600 bg-sky-600 hover:bg-sky-700 text-white border-transparent`;

/** 新建 / 重命名对话框的表单状态 */
interface EditState {
  /** null = 新建；数字 = 重命名该文件夹 */
  id: number | null;
  name: string;
  color: string;
}

const FolderSidebar: React.FC<FolderSidebarProps> = ({
  folders,
  scope,
  onToggle,
  onClearAll,
  onCreate,
  onRename,
  onDelete,
  disabled,
}) => {
  const [edit, setEdit] = React.useState<EditState | null>(null);
  const [askDelete, setAskDelete] = React.useState<FolderEntry | null>(null);

  // 每次打开编辑框时初始化草稿（wasOpenRef 模式，避免依赖对象，见 R6）
  const wasOpenRef = React.useRef(false);
  React.useEffect(() => {
    const justOpened = edit !== null && !wasOpenRef.current;
    wasOpenRef.current = edit !== null;
    if (!justOpened || edit === null) return;
    if (edit.id === null) setEdit({ ...edit, name: "", color: "sky" });
  });

  const total = folders.reduce((a, f) => a + f.count, 0);
  const allChecked = scope.size === 0;
  return (
    <div className="w-48 max-[1199px]:w-36 shrink-0 flex flex-col border-r border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-900">
      <div className="px-2.5 h-9 flex items-center justify-between border-b border-slate-200 dark:border-slate-700 shrink-0">
        <span className="text-[12px] text-slate-500 dark:text-slate-400 whitespace-nowrap">范围</span>
        <button
          className="text-[11px] text-slate-500 dark:text-slate-400 hover:text-sky-600 dark:hover:text-sky-400 whitespace-nowrap disabled:opacity-40"
          disabled={allChecked || disabled}
          title="清空勾选 = 显示全部节点"
          onClick={onClearAll}
        >
          {allChecked ? "全部（未筛选）" : "取消勾选"}
        </button>
      </div>

      <div className="flex-1 overflow-y-auto py-1">
        {folders.map((f) => {
          // 🩸 勾选框**如实反映 scope**，不再让 allChecked 把全部都画成勾选。
          // 设计的语义是「一个都不勾 = 显示全部节点」；此前却把「全部」画成全勾，
          // 于是「回到全部」和「初始状态」长得一模一样 —— 用户点了两次文件夹，
          // 看到临时区和文件夹同时被勾选，却完全没意识到自己已经回到了「全部」。
          const on = scope.has(f.folder.id);
          const isTemp = f.folder.id === TEMP_AREA_ID;
          return (
            <div
              key={f.folder.id}
              className={`mx-1 px-1.5 py-1 rounded flex items-center gap-1.5 cursor-pointer hover:bg-slate-100 dark:hover:bg-slate-800 ${on ? "" : "opacity-60"}`}
              onClick={() => onToggle(f.folder.id)}
              title={on ? "点击取消勾选" : "点击勾选（筛选到此文件夹）"}
            >
              <input
                type="checkbox"
                className="w-3.5 h-3.5 shrink-0 accent-sky-600"
                checked={on}
                readOnly
                tabIndex={-1}
                aria-label={`筛选 ${f.folder.name}`}
              />
              <span className={`w-2.5 h-2.5 rounded-full shrink-0 ${dotClass(f.folder.color)}`} aria-hidden="true" />
              <span className="flex-1 text-[12.5px] truncate text-slate-700 dark:text-slate-200">
                {f.folder.name || "(未命名)"}
              </span>
              <span className="text-[11px] tabular-nums text-slate-500 dark:text-slate-400 shrink-0">{f.count}</span>
              {!isTemp && (
                <button
                  className="w-4 h-4 shrink-0 text-[11px] leading-none text-slate-400 hover:text-slate-700 dark:hover:text-slate-200"
                  title="重命名 / 改颜色"
                  onClick={(e) => {
                    e.stopPropagation();
                    setEdit({ id: f.folder.id, name: f.folder.name, color: f.folder.color });
                  }}
                >
                  ⋯
                </button>
              )}
            </div>
          );
        })}
        {folders.length <= 1 && (
          <div className="px-2.5 py-2 text-[11.5px] leading-relaxed text-slate-500 dark:text-slate-400">
            还没有文件夹。点下方「新建文件夹」把常管的 IP 归类，临时添加的留在临时区。
          </div>
        )}
      </div>

      <div className="px-1.5 py-1.5 border-t border-slate-200 dark:border-slate-700 shrink-0">
        <button
          className={`${btnNeutral} w-full`}
          disabled={disabled}
          onClick={() => setEdit({ id: null, name: "", color: "sky" })}
        >
          + 新建文件夹
        </button>
        <div className="mt-1 text-[11px] text-slate-500 dark:text-slate-400 whitespace-nowrap">
          共 {total} 台 · 不勾=全部
        </div>
      </div>
      {/* 新建 / 重命名 */}
      {edit !== null && (
        <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/40">
          <div
            className="w-[380px] max-w-[92vw] rounded-lg shadow-2xl bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700"
            role="dialog"
            aria-modal="true"
            aria-label={edit.id === null ? "新建文件夹" : "重命名文件夹"}
          >
            <div className="px-4 h-11 flex items-center border-b border-slate-200 dark:border-slate-700">
              <div className="font-semibold text-slate-800 dark:text-slate-100 text-[13px]">
                {edit.id === null ? "新建文件夹" : "重命名 / 改颜色"}
              </div>
            </div>
            <div className="px-4 py-3">
              <label className="block text-[12px] text-slate-500 dark:text-slate-400 mb-1">名称</label>
              <input
                autoFocus
                value={edit.name}
                onChange={(e) => setEdit({ ...edit, name: e.target.value })}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && edit.name.trim()) {
                    if (edit.id === null) onCreate(edit.name.trim(), edit.color);
                    else onRename(edit.id, edit.name.trim(), edit.color);
                    setEdit(null);
                  }
                }}
                className="w-full h-8 px-2 rounded border bg-white dark:bg-slate-800 border-slate-300 dark:border-slate-600 text-slate-800 dark:text-slate-100 text-[13px] focus:outline-none focus:ring-1 focus:ring-sky-500"
                placeholder="如：机房A / 客户名 / 项目名"
              />
              <div className="mt-3 text-[12px] text-slate-500 dark:text-slate-400 mb-1.5">颜色</div>
              <div className="flex items-center gap-1.5 flex-wrap">
                {COLORS.map((c) => (
                  <button
                    key={c.key}
                    className={`w-6 h-6 rounded-full ${c.dot} ${edit.color === c.key ? "ring-2 ring-sky-600 ring-offset-1 dark:ring-offset-slate-900" : ""}`}
                    title={c.label}
                    aria-label={`颜色 ${c.label}`}
                    onClick={() => setEdit({ ...edit, color: c.key })}
                  />
                ))}
              </div>
              <div className="mt-2 text-[11px] text-slate-500 dark:text-slate-400">
                颜色为预设（保证浅色/深色主题下都达到 WCAG AA），不支持自由取色。
              </div>
            </div>
            <div className="px-4 h-12 flex items-center justify-end gap-2 border-t border-slate-200 dark:border-slate-700">
              <button className={btnNeutral} onClick={() => setEdit(null)}>取消</button>
              <button
                className={btnPrimary}
                disabled={!edit.name.trim()}
                onClick={() => {
                  if (edit.id === null) onCreate(edit.name.trim(), edit.color);
                  else onRename(edit.id, edit.name.trim(), edit.color);
                  setEdit(null);
                }}
              >
                确定
              </button>
            </div>
          </div>
        </div>
      )}

      {/* 删除：二次确认，文案明确说明「主机不会被删除」 */}
      <ConfirmDialog
        open={askDelete !== null}
        title="删除文件夹"
        danger
        confirmText="删除文件夹"
        message={
          askDelete === null ? "" : (
            <>
              将删除文件夹「{askDelete.folder.name || "(未命名)"}」。
              <br />
              <b>其中的 {askDelete.count} 台主机不会被删除</b>，会移到「临时区」。
            </>
          )
        }
        onConfirm={() => {
          if (askDelete !== null) onDelete(askDelete.folder.id);
          setAskDelete(null);
        }}
        onCancel={() => setAskDelete(null)}
      />
    </div>
  );
};

export default React.memo(FolderSidebar);