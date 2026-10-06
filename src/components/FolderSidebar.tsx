// 文件夹侧边栏（v1.1.10）：多选筛选 + 命名 / 颜色 / 删除
//
// 口径（已定稿，见 docs/folder-design.md 7.1）：
//   * 一个都不勾 = 全部节点（不是「什么都不显示」）
//   * 「临时区」是 folder_id 为 null 的那一组，列表里固定用 id 0 代表
//   * 只做筛选，**不隐藏任何既有功能入口**；主表 13 列布局与排序完全不动
import React from "react";
import type { FolderEntry } from "../types";
import { sidebarCount, TEMP_AREA_ID, toggleScope } from "../lib/format";
import ConfirmDialog from "./ConfirmDialog";

export interface FolderSidebarProps {
  folders: FolderEntry[];
  /** 各文件夹实时台数（key 0 = 临时区），由 `folderCounts(snapshot.targets)` 得出。
   *  🩸 `folders[].count` 已在 App.tsq 的 `liveFolders` 里被统一覆盖为实时值
   *  （源头治理，避免每个消费点各自踩「后端 count 滞后」的坑）；此 prop 是
   *  第二道保险，且 `sidebarCount` 的语义是「counts 一旦提供就完全信任」。 */
  counts?: Map<number, number>;
  /** 当前勾选的文件夹 id 集合；空 = 全部 */
  scope: ReadonlySet<number>;
  onToggle: (id: number) => void;
  /** 清空勾选 = 回到「显示全部节点」。**不是**逐个 toggle —— toggle 会把
   *  当前没勾的文件夹也勾上（scope={A} 时清空会得到 {临时区} 而非 {}）。*/
  onClearAll: () => void;
  /** 勾选全部（临时区 + 所有文件夹）—— 新语义下「不勾=不显示」，
   *  用户需要一个快捷方式一次性回到「看全部」 */
  onSelectAll: () => void;
  /** 从主表拖拽 IP 放下：`folderId = null` 表示移回临时区 */
  onDropTargets: (ids: number[], folderId: number | null) => void;
  /** 拖拽过程中鼠标扫过某文件夹（`null` = 离开，用于清除高亮） */
  onDragEnterFolder?: (id: number | null) => void;
  /** 当前高亮的文件夹（拖拽落点预览） */
  hoverId?: number | null;
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
  counts,
  scope,
  onToggle,
  onClearAll,
  onSelectAll,
  onDropTargets,
  onDragEnterFolder,
  hoverId,
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

  // 降级数据：listFolders 自带的 count（后端算的，可能滞后但绝不为 0）。
  // ⚠️ 必须先建 Map 再查 —— countOf 在 map 内逐项调用，对 folders 做 find 会退化成 O(n²)。
  const fallbackCounts = React.useMemo(() => {
    const m = new Map<number, number>();
    for (const f of folders) m.set(f.folder.id, f.count);
    return m;
  }, [folders]);

  // 实时台数。实现见 format.ts 的 sidebarCount（抽成纯函数才能被单测覆盖 ——
  // 之前它作为组件内局部函数，测试文件里的复刻副本测不到真实代码）。
  const countOf = (id: number) => sidebarCount(counts, fallbackCounts, id);
  const total = folders.reduce((a, f) => a + countOf(f.folder.id), 0);
  // 语义变更（用户定稿）：不再有「不勾 = 全部」。
  // 空集 = 什么都不显示；allChecked 改为判断「是否已全部勾上」（供「全选」按钮禁用）
  const allChecked = scope.size === 0;
  const allSelected = folders.length > 0 && scope.size >= folders.length;
  return (
    // 宽度由外层容器控制（v1.2.1 起可拖动）；此处用 w-full 撑满容器
    // ⚠️ 刻意**不加 border-r**：外侧紧挨着可拖动的分隔条，两条线并排会让人
    // 误以为那条边框才是把手（而它不可拖），反馈为「拖动没实现」。
    <div className="w-full flex flex-col bg-white dark:bg-slate-900">
      <div className="px-2.5 h-9 flex items-center justify-between border-b border-slate-200 dark:border-slate-700 shrink-0">
        <span className="text-[12px] text-slate-500 dark:text-slate-400 whitespace-nowrap">范围</span>
        <div className="flex items-center gap-1.5 shrink-0">
          {/* 语义变更：不再有「不勾=全部」。空集 = 什么都不显示，
              故这里提供「全选」把范围一次性铺满，方便用户快速回到看全部 */}
          <button
            className="text-[11px] text-slate-500 dark:text-slate-400 hover:text-sky-600 dark:hover:text-sky-400 whitespace-nowrap disabled:opacity-40"
            disabled={disabled || folders.length === 0 || allSelected}
            title="勾选全部文件夹与临时区"
            onClick={onSelectAll}
          >
            全选
          </button>
          <button
            className="text-[11px] text-slate-500 dark:text-slate-400 hover:text-sky-600 dark:hover:text-sky-400 whitespace-nowrap disabled:opacity-40"
            disabled={disabled || scope.size === 0}
            title="取消全部勾选（主表将不显示任何主机）"
            onClick={onClearAll}
          >
            清空
          </button>
        </div>
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
              // 拖拽落点：自实现拖拽靠「鼠标扫过」判定，不依赖 HTML5 DnD
              onMouseEnter={() => onDragEnterFolder?.(f.folder.id)}
              onMouseLeave={() => onDragEnterFolder?.(null)}
              className={`mx-1 px-1.5 py-1 rounded flex items-center gap-1.5 cursor-pointer hover:bg-slate-100 dark:hover:bg-slate-800 ${on ? "" : "opacity-60"} ${
                hoverId === f.folder.id ? "ring-2 ring-sky-500 bg-sky-50 dark:bg-sky-950/40" : ""
              }`}
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
              <span className="text-[11px] tabular-nums text-slate-500 dark:text-slate-400 shrink-0">{countOf(f.folder.id)}</span>
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
          {scope.size === 0 ? "未选范围 · 勾选后显示" : `已选 ${total} 台`}
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
              {/* 🩸 删除入口。此前删除逻辑（askDelete + ConfirmDialog + onDelete）
                  全部实现完毕却**没有任何触发点** —— `⋯` 只调 setEdit(重命名)，
                  用户完全无法删除文件夹。放在这个对话框里是最小改动：
                  不新增浮层结构，也就不涉及 R7 的 zoom 定位适配。 */}
              {edit.id !== null && (
                <button
                  className="mr-auto px-2 h-7 rounded border text-[12px] leading-none border-red-300 dark:border-red-800 bg-white dark:bg-slate-800 text-red-600 dark:text-red-400 hover:bg-red-50 dark:hover:bg-red-950/40"
                  title="删除这个文件夹（其中的主机会移到临时区，不会被删除）"
                  onClick={() => {
                    const target = folders.find((x) => x.folder.id === edit.id);
                    setEdit(null);
                    // 找不到对应项（列表已刷新）时无事可做，勿弹空确认框
                    if (target) setAskDelete(target);
                  }}
                >
                  删除文件夹
                </button>
              )}
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
              <b>其中的 {countOf(askDelete.folder.id)} 台主机不会被删除</b>，会移到「临时区」。
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