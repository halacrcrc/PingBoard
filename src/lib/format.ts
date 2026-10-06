// 展示层格式化工具（纯函数）
import type { EventKind, Status, TargetState } from "../types";

/** 格式化延迟（毫秒），保留 1 位小数；null 显示为 "-" */
export function fmtMs(v: number | null | undefined): string {
  if (v === null || v === undefined || Number.isNaN(v)) return "-";
  return v.toFixed(1);
}

/** 格式化百分比，保留 1 位小数 */
export function fmtPct(v: number | null | undefined): string {
  if (v === null || v === undefined || Number.isNaN(v)) return "0.0%";
  return `${v.toFixed(1)}%`;
}

/** 将纪元毫秒格式化为 HH:MM:SS */
export function fmtTime(ts: number | null | undefined): string {
  if (!ts) return "-";
  const d = new Date(ts);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

/** 将纪元毫秒格式化为 MM-DD HH:MM:SS（跨会话事件用，便于看出是哪一批） */
export function fmtDateTimeShort(ts: number | null | undefined): string {
  if (!ts) return "-";
  const d = new Date(ts);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${fmtTime(ts)}`;
}

/** 将毫秒时长为 1h2m3s / 2m3s / 3s 形式 */
export function fmtDuration(ms: number | null | undefined): string {
  if (!ms || ms < 0) return "0s";
  const total = Math.floor(ms / 1000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  if (h > 0) return `${h}h${m}m${s}s`;
  if (m > 0) return `${m}m${s}s`;
  return `${s}s`;
}

/** 延迟分档颜色：<50 绿 / <150 黄 / >=150 橙
 *  浅色用 700 档：600 档四色全部低于 WCAG AA 4.5:1（绿 3.77 / 黄 2.94 / 橙 3.56，
 *  黄档最差），而 700 档在任意行底（白 / slate-50 / emerald-50 / red-50 / sky-50）均 ≥ 4.5:1。
 *  ⚠️ 四档必须同档位调整，只动一档会让色阶亮度失衡。深色 400 档已达标（7.89~11.66），不动。
 *  无数据（null）用 slate-600 而非 slate-500：本函数只服务于表格内，
 *  而表格行底会随状态变成彩色 -50 档（失败行 red-50 上 slate-500 仅 4.35:1）。 */
export function rttColorClass(v: number | null | undefined): string {
  if (v === null || v === undefined) return "text-slate-600 dark:text-slate-400";
  if (v < 50) return "text-emerald-700 dark:text-emerald-400";
  if (v < 150) return "text-yellow-700 dark:text-yellow-400";
  return "text-orange-700 dark:text-orange-400";
}

/** 状态中文标签 */
export function statusLabel(s: Status): string {
  switch (s) {
    case "ok":
      return "正常";
    case "timeout":
      return "超时";
    case "failed":
      return "失败";
    case "resolving":
      return "解析中";
    case "idle":
    default:
      return "未开始";
  }
}

/** 状态对应的行背景色（绿=正常，红=失败，灰=未开始） */
export function statusRowClass(s: Status): string {
  switch (s) {
    case "ok":
      return "bg-emerald-50 dark:bg-emerald-950/40 hover:bg-emerald-100 dark:hover:bg-emerald-900/40";
    case "timeout":
    case "failed":
      return "bg-red-50 dark:bg-red-950/40 hover:bg-red-100 dark:hover:bg-red-900/40";
    case "resolving":
      return "bg-sky-50 dark:bg-sky-950/40 hover:bg-sky-100 dark:hover:bg-sky-900/40";
    case "idle":
    default:
      return "bg-slate-50 dark:bg-slate-800/40 hover:bg-slate-100 dark:hover:bg-slate-700/40";
  }
}

/** 状态徽标样式 */
export function statusBadgeClass(s: Status): string {
  switch (s) {
    case "ok":
      return "bg-emerald-600 text-white";
    case "timeout":
      return "bg-amber-600 text-white";
    case "failed":
      return "bg-red-600 text-white";
    case "resolving":
      return "bg-sky-600 text-white";
    case "idle":
    default:
      // slate-500 起白字才达 WCAG AA（slate-400 仅 2.56:1，slate-500 → 4.76:1）
      return "bg-slate-500 text-white";
  }
}

/** 事件类型文字颜色：故障类红 / 恢复类绿 / 其余灰 */
export function eventColorClass(kind: EventKind): string {
  switch (kind) {
    case "fault":
    case "unreachable":
    case "dns_fail":
      return "text-red-600 dark:text-red-400";
    case "recover":
    case "first_ok":
      return "text-emerald-700 dark:text-emerald-400";
    default:
      return "text-slate-600 dark:text-slate-300";
  }
}

/** 事件类型中文标签（用于复制 / 展示） */
export function eventKindLabel(kind: EventKind): string {
  switch (kind) {
    case "fault":
      return "故障";
    case "unreachable":
      return "无法连通";
    case "dns_fail":
      return "解析失败";
    case "recover":
      return "已恢复";
    case "first_ok":
      return "首次连通";
    case "start":
      return "开始探测";
    case "stop":
      return "已停止";
    case "config_change":
      return "配置变更";
    default:
      return "事件";
  }
}

/** 是否属于「未读故障红点」计入的故障类型（不含 recover） */
export function isUnreadKind(kind: EventKind): boolean {
  return kind === "fault" || kind === "unreachable" || kind === "dns_fail";
}

/** 默认字体栈：必须与 src/styles.css 中 body 的 font-family 完全一致（守护测试校验） */
export const DEFAULT_FONT_STACK =
  '"Microsoft YaHei UI", "Microsoft YaHei", "Segoe UI", system-ui, -apple-system, sans-serif';

/**
 * 界面缩放百分比 → document.documentElement.style.zoom 的赋值字符串（纯函数）。
 * 100（或不合法值）返回空串 = 清除内联缩放、恢复默认；
 * 其余钳制到 50..=200 后换算为小数（与后端 stats::normalize_settings 的 50..=200 一致）。
 */
export function zoomStyle(uiScale: number): string {
  const n = Math.round(Number(uiScale));
  if (!Number.isFinite(n) || n === 100) return "";
  const clamped = Math.min(200, Math.max(50, n));
  return String(clamped / 100);
}

/**
 * 自定义字体族名 → document.body.style.fontFamily 的赋值字符串（纯函数）。
 * null / 空串 / 纯空白返回空串 = 恢复系统默认（styles.css 里的字体栈）；
 * 否则「"族名", 默认栈」——默认栈兜底，族名缺字时逐级降级。
 */
export function fontFamilyStyle(family: string | null | undefined): string {
  const f = (family ?? "").trim();
  if (!f) return "";
  return `"${f}", ${DEFAULT_FONT_STACK}`;
}

/**
 * 当前根元素缩放系数（zoom 100% / 未设置时为 1）。
 *
 * ⚠️ zoom 下 fixed 定位换算：`getBoundingClientRect()` 返回的是**视觉坐标**（已含缩放），
 * 而给 fixed 元素设置 `style.top/left` 时数值会被根元素 zoom **再放大一次**
 * → 用 rect 值定位会随缩放偏离（125% 时偏 25%）。
 * 修法：定位前先把视觉坐标除以本系数换算回 CSS 像素。
 */
export function currentZoomFactor(): number {
  const z = parseFloat(document.documentElement.style.zoom);
  return Number.isFinite(z) && z > 0 ? z : 1;
}

/* ===================== 报表导出筛选（与 Rust `export.rs` 对齐） ===================== */

/** 报表导出筛选口径（对应 Rust `export::ExportFilter`） */
export type ExportFilter = "all" | "none_loss" | "all_failed";

/**
 * 判定单个节点是否命中导出筛选口径。
 *
 * ⚠️ 与 Rust `export::matches_filter` 是**同一判定的两个实现**：
 * 本函数只用于对话框里的「命中台数」预览，**不参与写文件**；
 * 导出结果的唯一权威是后端。改判据必须两侧同时改
 * （两侧测试用同一组边界用例锁定：`sent>0` 零丢包 / 部分丢包 / 全失败 / `sent==0`）。
 *
 * 🩸 两种口径都要求 `sent > 0`：`sent === 0` 表示从未 ping 或统计已被清空，
 * 此时 `received` 与 `failed` 同为 0，且后端 `stats::loss_pct` 对 `sent === 0` 定义为 `0.0`。
 * 若只判 `failed === 0`，**从未开始的主机会被误当成「零丢包」**。
 * 在 `sent > 0` 的前提下两种口径互斥（一个要求 `received === sent`，一个要求 `received === 0`）。
 */
export function matchesExportFilter(t: TargetState, f: ExportFilter): boolean {
  switch (f) {
    case "all":
      return true;
    case "none_loss":
      return t.sent > 0 && t.failed === 0;
    case "all_failed":
      return t.sent > 0 && t.received === 0;
  }
}

/** 筛选口径中文名（与 Rust `export::filter_label` 保持一致） */
export function exportFilterLabel(f: ExportFilter): string {
  switch (f) {
    case "all":
      return "不过滤";
    case "none_loss":
      return "零丢包";
    case "all_failed":
      return "全部未成功";
  }
}

/** 筛选口径的一句话判定式说明（与 Rust `export::filter_hint` 语义一致） */
export function exportFilterHint(f: ExportFilter): string {
  switch (f) {
    case "all":
      return "导出全部节点，与原导出行为一致";
    case "none_loss":
      return "仅 sent>0 且 failed=0：全程一次都没丢包";
    case "all_failed":
      return "仅 sent>0 且 received=0：所有 ping 一次都没成功";
  }
}

/* ===================== 文件夹范围筛选（v1.1.10） ===================== */

/**
 * 判断某节点是否落在当前勾选的文件夹范围内。
 *
 * 口径：**一个都没勾 = 全部**（不是「什么都不显示」）。
 * `folder_id` 为 null 表示临时区，与后端 `list_folders` 里 `id === 0` 的
 * 「临时区」那一项对应，故这里用 `?? 0` 归一。
 *
 * 纯函数，便于单测锁住「不勾 = 全部」这条反直觉但已定稿的口径。
 */
export function inFolderScope(t: TargetState, scope: ReadonlySet<number>): boolean {
  // 🩸 语义变更（用户定稿 2026-10-06）：**空集 = 不显示任何主机**。
  // 原为 `if (scope.size === 0) return true`（一个都不勾 = 全部），
  // 与「勾选状态」视觉矛盾 —— 全都不勾却显示全部。现改为严格一致：
  // 勾了什么就显示什么，一个都不勾就什么都不显示（初始即空白）。
  if (scope.size === 0) return false;
  return scope.has(t.folder_id ?? 0);
}

/** 按文件夹范围过滤节点（侧边栏范围 ∩ 搜索关键词 的第一段） */
export function filterByFolderScope(
  targets: TargetState[],
  scope: ReadonlySet<number>
): TargetState[] {
  // 空集 = 空范围（不再是全部），见 inFolderScope 的说明
  return targets.filter((t) => inFolderScope(t, scope));
}

/** 切换勾选某一项；返回新集合（不修改入参） */
export function toggleScope(scope: ReadonlySet<number>, id: number): Set<number> {
  const next = new Set(scope);
  if (!next.delete(id)) next.add(id);
  return next;
}

/** 文件夹 id 0 固定代表「临时区」（与后端 list_folders 的约定） */
export const TEMP_AREA_ID = 0;
/**
 * 「清空列表」要删除的主机 id —— **只含临时区**（`folder_id == null`）。
 *
 * 口径（已定稿，见 docs/folder-design.md 7.4）：清空列表**只清临时区**，
 * 文件夹里的 IP 不受影响。要删文件夹内的主机请用侧边栏的「删除文件夹」。
 *
 * 纯函数，便于单测锁住「文件夹里的 IP 不会被误删」这条安全性质。
 */
export function tempAreaIds(targets: ReadonlyArray<{ id: number; folder_id: number | null }>): number[] {
  return targets.filter((t) => t.folder_id === null).map((t) => t.id);
}

/** 临时区台数（用于「清空列表」确认框文案） */
export function tempAreaCount(targets: ReadonlyArray<{ folder_id: number | null }>): number {
  return targets.reduce((a, t) => (t.folder_id === null ? a + 1 : a), 0);
}

/**
 * 按侧边栏范围算出导出要用的目标 id 列表（C2，已定稿见 docs/folder-design.md 7.3）。
 *
 * 🩸 **范围为空时返回 `null` 而不是 `[]`/`[全部 id]`** —— `null` 在后端语义是
 * 「导出全部」。这样侧边栏没勾选任何文件夹时，导出请求与 v1.1.9 **逐字节一致**
 * （含默认文件名），不因新增功能产生任何输出差异。
 *
 * 只跟随**侧边栏范围**，不跟随搜索关键词：搜索是瞬时视图过滤，
 * 而文件夹范围是结构性的归属。「我正在看机房A，导出机房A的全部」才符合直觉。
 */
export function exportScopeIds(
  targets: ReadonlyArray<{ id: number; folder_id: number | null }>,
  scope: ReadonlySet<number>
): number[] {
  // 🩸 空集 = 空范围（导出 0 台），不再是「全部」。
  // 与侧边栏「都不勾 = 都不显示」同口径（用户定稿 2026-10-06）。
  // ⚠️ 不再返回 null：后端 `export_targets(Some([]))` 会正确产出 0 行，
  // 而 null 的语义是「导出全部」，两者绝不可混。
  const s = scope as ReadonlySet<number>;
  return targets.filter((t) => s.has(t.folder_id ?? 0)).map((t) => t.id);
}
/**
 * 按文件夹实时统计台数（key 0 = 临时区）。
 *
 * 🩸 为什么不用 `listFolders` 返回的 `count`：那份 count 只有在**显式调用
 * `refreshFolders()`** 时才更新，而刷新点极易漏 —— 漏一个就会出现
 * 「清空列表后侧边栏台数不变」这类脏数据（v1.1.10 实际踩过）。
 *
 * 快照 `targets` 本就带 `folder_id`，且随 ping 轮询自动刷新，
 * 因此台数**从 targets 实时算**可让任何目标增删改都自动同步，
 * 不依赖「记得在每个操作点调 refreshFolders」。
 *
 * ⚠️ 失效的 folder_id（配置被手改等）归入临时区，与后端 `folder_counts` 同口径。
 */
export function folderCounts(
  targets: ReadonlyArray<{ folder_id: number | null }>,
  validFolderIds?: ReadonlySet<number>
): Map<number, number> {
  const m = new Map<number, number>();
  const bump = (k: number) => m.set(k, (m.get(k) ?? 0) + 1);
  // 🩸 `validFolderIds` 为空集时**不做孤儿回落**。
  //
  // 理由：空集有两种含义 ——「确实一个文件夹都没有」与「列表还没加载」。
  // 前者无害（此时所有主机 folder_id 本来就是 null，全在临时区）；
  // 后者致命：把「还没加载」当成「都不合法」，会让**所有**文件夹内主机
  // 被算进临时区，台数全错。宁可少回落（那一行暂时不显示）也不可错回落。
  //
  // 真正的孤儿回落只在拿到非空合法集合后才启用。
  const loaded = validFolderIds !== undefined && validFolderIds.size > 0;
  for (const t of targets) {
    const raw = t.folder_id;
    if (raw === null || raw === undefined) {
      bump(0);
    } else if (loaded && !validFolderIds!.has(raw)) {
      // 失效的 folder_id（配置被手改等）归入临时区，与后端
      // `folder_counts` + `list_folders` 同口径。前端无法自行区分
      // 「孤儿 id」与「真实但恰好同号的 id」，故需调用方传入合法 id 集合。
      bump(0);
    } else {
      bump(raw);
    }
  }
  return m;
}
/**
 * 侧边栏某个文件夹该显示多少台。
 *
 * 🩸 `counts` 一旦提供就**完全信任**它 —— Map 里没有该 key 即代表「0 台」，
 * 必须显示 0。
 *
 * 此前写成 `counts?.get(id) ?? fallback.get(id) ?? 0` 是错的：空文件夹的 key
 * 不在 Map 里，`counts.get(id)` 返回 `undefined`，`??` 会**穿透**到 fallback，
 * 把 `listFolders` 的旧值显示出来 —— 现象是「删掉文件夹内最后一台 IP 后
 * 计数仍停在 1」（fallback 只在增删文件夹时刷新）。`??` 把「真实为 0」与
 * 「未提供 counts」混为一谈。
 *
 * @param counts 实时台数（来自快照）；`undefined` = 未提供，走降级
 * @param fallback `listFolders` 自带的 count（可能滞后，但绝不为 0）
 */
export function sidebarCount(
  counts: ReadonlyMap<number, number> | undefined,
  fallback: ReadonlyMap<number, number>,
  id: number
): number {
  if (counts !== undefined) return counts.get(id) ?? 0;
  return fallback.get(id) ?? 0;
}
/**
 * 把 `folders[].count`（后端 `list_folders` 算的，**只在增删文件夹时刷新**）
 * 统一覆盖为实时台数。
 *
 * 🩸 为什么必须在**数据源头**做：`FolderEntry.count` 是个滞后的派生值，
 * 任何消费点直接读 `f.count` 都会显示旧值。此前只在侧边栏绕开它
 * （改用 `sidebarCount`），结果添加主机的「归入」下拉仍渲染 `f.count` ——
 * 侧边栏全 0、下拉却显示「临时区（3）」。逐个给消费方传 props 治标，
 * 源头覆盖才能让**新增**的消费点不再踩同一个坑。
 *
 * 降级判据是 **`folders.length === 0`** 而非 `counts.size === 0` ——
 * 后端 `list_folders` 恒会插入一个 id=0 的「临时区」项，故 `folders` 为空
 * 只可能意味着「列表尚未加载」。而 `counts` 为空 Map 有两种含义：
 * 未加载，或**用户把主机全删光了**（`targets` 为空）。若用 `counts.size`
 * 判降级，后一种情况会走降级分支把所有文件夹恢复成后端旧值 ——
 * 现象正是「侧边栏显示 0、下拉却显示『临时区（3）』」。
 */
export function withLiveCounts<
  F extends { folder: { id: number }; count: number }
>(folders: ReadonlyArray<F>, counts: ReadonlyMap<number, number>): F[] {
  if (folders.length === 0) return [];
  return folders.map((f) => ({ ...f, count: counts.get(f.folder.id) ?? 0 }));
}
/* ===================== 主表复选框选中集合 ===================== */

/**
 * 切换某行的选中状态（**返回新集合**，不修改入参）。
 *
 * 供 `setSelected(prev => toggleSelection(prev, id))` 的函数式更新使用。
 *
 * 🩸 不得写成 `new Set(selected)` 直接读组件当前渲染的 selected：
 * 那是**当前渲染的快照**，一旦与其它 setSelected 交错（快照每 500ms 推送、
 * 表头全选、搜索过滤都会引发重渲染），新值就会基于过期状态计算，
 * 表现为「勾选一个就把之前的选中弄丢」。
 */
export function toggleSelection(
  selected: ReadonlySet<number>,
  id: number
): Set<number> {
  const next = new Set(selected);
  if (!next.delete(id)) next.add(id);
  return next;
}

/**
 * 「当前可见的行」里有多少条已被选中。
 *
 * 🩸 不能用 `selected.size === targets.length`：selected 是**全局**集合
 * （含筛选范围外的主机），targets 是**筛选后**的列表。筛选状态下两者
 * 不可比 —— 会让表头复选框的「全选 / 半选」状态显示错误。
 */
export function visibleSelectedCount(
  targets: ReadonlyArray<{ id: number }>,
  selected: ReadonlySet<number>
): number {
  let n = 0;
  for (const t of targets) if (selected.has(t.id)) n++;
  return n;
}

/**
 * 表头「全选」：**并入**可见行，而不是替换整个集合。
 *
 * 🩸 替换（`new Set(ids)`）会清掉筛选范围外已选中的主机 ——
 * 用户在「全部」下选了 3 台，筛到某个只有 1 台的文件夹后点表头全选，
 * 之前那 2 台就被静默取消了。
 *
 * @param onNewlySelected 新增选中时的回调（用于把主选切到第一个新增项）
 */
export function mergeSelection(
  selected: ReadonlySet<number>,
  ids: ReadonlyArray<number>,
  onNewlySelected: (firstNewId: number) => void
): Set<number> {
  const next = new Set(selected);
  let firstNew: number | null = null;
  for (const id of ids) {
    if (!next.has(id)) {
      next.add(id);
      if (firstNew === null) firstNew = id;
    }
  }
  if (firstNew !== null) onNewlySelected(firstNew);
  return next;
}

/**
 * 表头「全不选」：只清可见的，保留筛选范围外的已选。
 * 与 `mergeSelection` 对称 —— 否则「全不选」也会静默清掉范围外的选中。
 */
export function removeSelection(
  selected: ReadonlySet<number>,
  ids: ReadonlyArray<number>
): Set<number> {
  const drop = new Set(ids);
  return new Set([...selected].filter((id) => !drop.has(id)));
}
/**
 * Shift 连续选择：算出「从锚点到目标行」之间（含两端）的 id。
 *
 * 🩸 `orderedIds` **必须是当前可见且排序后的行 id 顺序**。此前用的是
 * `snapshot.targets`（全局原始顺序），于是：
 *   - 按某列排序后，区间按**原始**顺序取，视觉上根本不连续；
 *   - 搜索/侧边栏筛选后，区间会跨越不可见的主机，把它们也悄悄选上。
 *
 * @param orderedIds 当前可见行的 id，按屏幕上的顺序
 * @param anchor     锚点（上一次非 Shift 点击的行）；`null` 表示无锚点
 * @param target     本次 Shift 点击的行
 * @returns 区间内所有 id（含两端）；无锚点或端点不可见时退化为 `[target]`
 */
export function rangeIds(
  orderedIds: ReadonlyArray<number>,
  anchor: number | null,
  target: number
): number[] {
  const ti = orderedIds.indexOf(target);
  if (ti < 0) return [target];
  const ai = anchor === null ? -1 : orderedIds.indexOf(anchor);
  if (ai < 0) return [target]; // 锚点已不可见（筛选变了）-> 退化为单选
  const lo = Math.min(ai, ti);
  const hi = Math.max(ai, ti);
  return orderedIds.slice(lo, hi + 1);
}
/* ===================== 拖拽 IP 到文件夹 ===================== */

/**
 * 拖拽一行时，实际要移动的主机 id 列表。
 *
 * 规则（与文件管理器一致）：
 * - 拖的行**已在选中集合里** → 移动**整个选中集合**（拖一个带走一批）
 * - 否则 → 只移动这一行
 *
 * 这样用户先 Shift/Ctrl 选好一批，再拖其中任意一行即可整批移动。
 */
export function dragMoveIds(
  draggedId: number,
  selected: ReadonlySet<number>
): number[] {
  if (selected.has(draggedId)) return [...selected];
  return [draggedId];
}

/**
 * 拖拽提示文案（显示在拖影与放置反馈里）。
 * 纯函数便于断言「1 台 / N 台」的措辞一致。
 */
export function dragHint(n: number, folderName: string): string {
  const name = folderName || "(未命名)";
  return n > 1 ? `将 ${n} 台主机移入「${name}」` : `将这台主机移入「${name}」`;
}

/* ===================== 三区域宽度拖动 ===================== */

/** 各区域宽度极限（px）。**默认值保持既有观感不变**，只在用户拖动后才生效。 */
export const PANE_LIMITS = {
  /** 侧边栏：文件夹名 + 台数至少要放得下 */
  sidebar: { min: 120, max: 480, initial: 192 },
  /** 详情区：趋势图与事件日志至少要有可读宽度 */
  detail: { min: 280, max: 720, initial: 420 },
  /** 主表：13 列，至少要看得到几列（被动挤压，不直接拖动它的宽度） */
  table: { min: 320 },
} as const;

/**
 * 把拖动产生的宽度钳制到合法区间。
 *
 * 🩸 必须同时受两侧约束：只夹自己的 min/max 是不够的 —— 侧边栏拉太宽会
 * 把主表挤到看不见。故接受一个「可用空间」，保证主表始终 >= 其最小宽度。
 *
 * @param raw      用户拖出来的原始宽度（CSS px）
 * @param limits   该区域的 min / max
 * @param avail    容器可用宽度（用于保证对侧区域不被压垮）
 * @param otherMin 对侧区域（主表）的最小宽度
 */
export function clampPaneWidth(
  raw: number,
  limits: { min: number; max: number },
  avail: number,
  otherMin: number
): number {
  const capped = Math.min(limits.max, Math.max(limits.min, raw));
  // 给主表留出 otherMin 后，本区域最多还能占多少
  const roomForMe = Math.max(limits.min, avail - otherMin);
  return Math.min(capped, roomForMe);
}
/**
 * 读取持久化的区域宽度；越界或解析失败时回落到默认值。
 *
 * ⚠️ 存 localStorage 而**不是** Rust 配置：这属于纯界面偏好，
 * 放进 config.json 会改动配置契约（R1 红线要求新增字段必须带 serde 默认值，
 * 且会让「配置是唯一记录主机列表的地方」这一定位变模糊）。
 */
export function loadPaneWidth(
  key: string,
  fallback: number,
  limits: { min: number; max: number }
): number {
  try {
    const raw = localStorage.getItem(key);
    if (raw === null) return fallback;
    const n = Number(raw);
    if (!Number.isFinite(n)) return fallback;
    return Math.min(limits.max, Math.max(limits.min, n));
  } catch {
    return fallback; // localStorage 被禁用时静默回落
  }
}

/** 持久化区域宽度（失败静默忽略，不影响使用） */
export function savePaneWidth(key: string, width: number): void {
  try {
    localStorage.setItem(key, String(Math.round(width)));
  } catch {
    /* 忽略 */
  }
}