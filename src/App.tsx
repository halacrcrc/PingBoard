// 应用根组件：装配工具栏 / 主表格 / 详情面板 / 状态栏，订阅后端聚合快照事件
import React from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { save } from "@tauri-apps/plugin-dialog";
import type { EventLevel, FolderEntry, LogEvent, PingSettings, Snapshot, TargetEntry } from "./types";
import * as api from "./lib/api";
import {
  exportFilterLabel,
  fmtTime,
  fontFamilyStyle,
  isUnreadKind,
  zoomStyle,
  type ExportFilter,
} from "./lib/format";
import { exportScopeIds, filterByFolderScope, folderCounts, tempAreaCount, tempAreaIds, toggleScope } from "./lib/format";
import FolderSidebar from "./components/FolderSidebar";
import { shouldIgnoreDeleteKey } from "./lib/keyboard";
import Toolbar from "./components/Toolbar";
import TargetTable, { type SortKey } from "./components/TargetTable";
import StatusBar from "./components/StatusBar";
import DetailPanel from "./components/DetailPanel";
import AddTargetsDialog from "./components/AddTargetsDialog";
import SettingsDialog from "./components/SettingsDialog";
import ConfirmDialog from "./components/ConfirmDialog";
import ExportDialog, { type ExportFormat } from "./components/ExportDialog";

/** 默认设置（与 Rust 侧 PingSettings::default 保持一致） */
const DEFAULT_SETTINGS: PingSettings = {
  interval_ms: 1000,
  timeout_ms: 2000,
  payload_size: 32,
  ttl: 128,
  max_threads: 256,
  limit_max_threads: true,
  beep_on_fail: false,
  auto_start: false,
  history_len: 60,
  events_on: true,
  events_level: "standard",
  events_persist: true,
  events_dir: null,
  events_keep: 200,
  ui_scale: 100,
  ui_font_family: null,
};

const EMPTY_SNAPSHOT: Snapshot = {
  targets: [],
  settings: DEFAULT_SETTINGS,
  running: false,
  active_threads: 0,
  total_sent: 0,
  total_received: 0,
  total_failed: 0,
  loss_pct: 0,
  started_at: null,
  updated_at: 0,
  session: 0,
  events: [],
};

/** 事件等级排序（与 Rust EventLevel::rank 一致）：值越小越严重 */
const LEVEL_RANK: Record<EventLevel, number> = { fault: 0, standard: 1, detail: 2 };

/**
 * 复制文本到剪贴板。WebView2 下 `navigator.clipboard` 可能抛 `NotAllowedError`，
 * 逐级降级：Clipboard API → 隐藏 textarea + `execCommand('copy')`；仍失败则抛出。
 */
async function copyToClipboard(text: string): Promise<void> {
  try {
    if (navigator.clipboard && navigator.clipboard.writeText) {
      await navigator.clipboard.writeText(text);
      return;
    }
    throw new Error("clipboard unavailable");
  } catch {
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.style.position = "fixed";
    ta.style.opacity = "0";
    ta.style.left = "-9999px";
    document.body.appendChild(ta);
    try {
      ta.focus();
      ta.select();
      const ok = document.execCommand("copy");
      if (!ok) throw new Error("execCommand copy failed");
    } finally {
      document.body.removeChild(ta);
    }
  }
}

/** 首次启动引导页展示的示例主机 */
const SAMPLE_TARGETS: TargetEntry[] = [
  { host: "223.5.5.5", name: "阿里 DNS" },
  { host: "114.114.114.114", name: "114 DNS" },
  { host: "www.baidu.com", name: "百度" },
];

/** 关闭并发上限后仍保留的硬保护（与 Rust state::HARD_MAX_THREADS 及 SettingsDialog 保持一致） */
const HARD_MAX_THREADS = 4096;

const App: React.FC = () => {
  const [snapshot, setSnapshot] = React.useState<Snapshot>(EMPTY_SNAPSHOT);
  const [selected, setSelected] = React.useState<Set<number>>(new Set());
  const [primaryId, setPrimaryId] = React.useState<number | null>(null);
  const [search, setSearch] = React.useState("");
  const [theme, setTheme] = React.useState<"light" | "dark">(() => {
    const saved = localStorage.getItem("pb-theme");
    return saved === "dark" ? "dark" : "light";
  });
  const [showAdd, setShowAdd] = React.useState(false);
  const [showSettings, setShowSettings] = React.useState(false);
  const [sortKey, setSortKey] = React.useState<SortKey>("name");
  const [sortDir, setSortDir] = React.useState<"asc" | "desc">("asc");
  const [logs, setLogs] = React.useState<Record<number, LogEvent[]>>({});
  const [unread, setUnread] = React.useState<Record<number, number>>({});
  const [toast, setToast] = React.useState<string | null>(null);
  const [confirmClear, setConfirmClear] = React.useState(false);
  const [confirmClearLogs, setConfirmClearLogs] = React.useState(false);
  const [confirmDelete, setConfirmDelete] = React.useState(false);
  const [showExport, setShowExport] = React.useState(false);
  // v1.1.10 文件夹：勾选的文件夹 id；**空集 = 全部**（口径见 docs/folder-design.md 7.1）
  const [folderScope, setFolderScope] = React.useState<Set<number>>(new Set());
  const [folders, setFolders] = React.useState<FolderEntry[]>([]);

  const audioCtxRef = React.useRef<AudioContext | null>(null);
  /** 保存 ping-snapshot 的取消订阅函数，卸载时判空 + 容错调用 */
  const unlistenRef = React.useRef<(() => void) | null>(null);
  /** 已处理的最大事件 seq（严格去重） */
  const lastSeqRef = React.useRef(0);
  /** 已从后端回填过历史的主机 id */
  const loadedIdsRef = React.useRef<Set<number>>(new Set());
  /** 供订阅回调读取当前主选主机（避免闭包过期） */
  const primaryIdRef = React.useRef<number | null>(null);

  /* ------------------------- 提示音 ------------------------- */

  const beep = React.useCallback(() => {
    try {
      const Ctx = window.AudioContext || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
      if (!Ctx) return;
      if (!audioCtxRef.current) audioCtxRef.current = new Ctx();
      const ctx = audioCtxRef.current;
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.type = "sine";
      osc.frequency.value = 880;
      gain.gain.value = 0.06;
      osc.connect(gain);
      gain.connect(ctx.destination);
      osc.start();
      osc.stop(ctx.currentTime + 0.12);
    } catch {
      /* 音频不可用时忽略 */
    }
  }, []);

  /* ------------------------- 事件增量消费 ------------------------- */

  // 快照到达：消费增量事件（seq 严格去重）→ 更新 logs / unread → 整体替换快照。
  // 依赖仅 beep（稳定），保证订阅只挂一次。
  const handleSnapshot = React.useCallback(
    (snap: Snapshot) => {
      const incoming = snap.events ?? [];
      if (incoming.length > 0) {
        const last = lastSeqRef.current;
        const fresh = incoming.filter((ev) => ev.seq > last);
        if (fresh.length > 0) {
          lastSeqRef.current = fresh.reduce((m, ev) => Math.max(m, ev.seq), last);
          const cap = snap.settings.events_keep || 200;
          setLogs((prev) => {
            const out: Record<number, LogEvent[]> = { ...prev };
            for (const ev of fresh) {
              const arr = out[ev.target_id] ? [...out[ev.target_id]] : [];
              arr.unshift(ev);
              out[ev.target_id] = arr.slice(0, cap);
            }
            return out;
          });
          const primary = primaryIdRef.current;
          setUnread((prev) => {
            let changed = false;
            const out = { ...prev };
            for (const ev of fresh) {
              // 仅非选中主机的「故障级」计入未读（recover 不计）
              if (ev.target_id !== primary && isUnreadKind(ev.kind)) {
                out[ev.target_id] = (out[ev.target_id] ?? 0) + 1;
                changed = true;
              }
            }
            return changed ? out : prev;
          });
          if (snap.settings.beep_on_fail && fresh.some((ev) => isUnreadKind(ev.kind))) {
            beep();
          }
        }
      }
      setSnapshot(snap);
    },
    [beep]
  );

  /* ------------------------- 事件订阅与初始化 ------------------------- */

  React.useEffect(() => {
    let disposed = false;
    listen<Snapshot>("ping-snapshot", (e) => handleSnapshot(e.payload))
      .then((u) => {
        if (disposed) {
          // 订阅在清理之后才就绪：立即取消，并容错（资源可能已释放）
          try {
            u();
          } catch {
            /* 忽略 */
          }
        } else {
          unlistenRef.current = u;
        }
      })
      .catch(() => {
        /* 订阅失败时静默忽略，不影响 get_state 兜底 */
      });
    api
      .getState()
      .then(handleSnapshot)
      .catch((e) => showToast(`初始化失败：${String(e)}`));
    // 启动期一次性提示（配置损坏 / 已备份原文件 / 抢救结果）：
    // 后端读走即清空，所以这里不需要做任何去重；没提示时后端返回 null。
    // 文案含完整的备份文件路径，20 秒才够用户看清并记下（默认 4 秒会被 truncate 吃掉路径）。
    api
      .takeStartupNotice()
      .then((n) => {
        if (n) showToast(n, 20_000);
      })
      .catch((e) => {
        // 不能静默吞掉：命令没注册 / 改签名时前端会走到这里，
        // 静默忽略会让整条提示链路无声失效，而用户毫不知情。
        console.error("[pingboard] 启动提示获取失败：", e);
      });
    return () => {
      disposed = true;
      const u = unlistenRef.current;
      unlistenRef.current = null;
      // 仅在确实是函数时调用；卸载阶段底层资源可能已释放，用 try/catch 兜底
      if (typeof u === "function") {
        try {
          u();
        } catch {
          /* 忽略卸载期清理异常 */
        }
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [handleSnapshot]);

  /* ------------------------- 主题 ------------------------- */

  React.useEffect(() => {
    document.documentElement.classList.toggle("dark", theme === "dark");
    localStorage.setItem("pb-theme", theme);
  }, [theme]);

  /* ------------------------- 界面缩放 / 字体 ------------------------- */

  // ⚠️ 依赖只写原始值（ui_scale / ui_font_family），绝不依赖 settings 对象引用：
  // App 每 500ms 收到快照都会重建 settings 对象，依赖对象引用会导致 effect 每 500ms 重跑。
  const uiScale = snapshot.settings.ui_scale;
  const uiFontFamily = snapshot.settings.ui_font_family;

  React.useEffect(() => {
    document.documentElement.style.zoom = zoomStyle(uiScale);
  }, [uiScale]);

  React.useEffect(() => {
    document.body.style.fontFamily = fontFamilyStyle(uiFontFamily);
  }, [uiFontFamily]);

  /* ------------------------- 窗口标题 ------------------------- */

  React.useEffect(() => {
    const title = snapshot.running
      ? `PingBoard — 运行中 (${snapshot.active_threads} 台)`
      : "PingBoard — 多主机 Ping 监视器";
    document.title = title;
    getCurrentWindow()
      .setTitle(title)
      .catch(() => {
        /* 无权限时静默忽略 */
      });
  }, [snapshot.running, snapshot.active_threads]);

  /* ------------------------- 主选主机同步 / 历史回填 / 未读清零 ------------------------- */

  // 同步主选 id 到 ref（供订阅回调读取，避免闭包过期）
  React.useEffect(() => {
    primaryIdRef.current = primaryId;
  }, [primaryId]);

  // 选中主机：首访懒回填历史（新→旧）；每次切换都清零该主机未读
  React.useEffect(() => {
    if (primaryId === null) return;
    if (!loadedIdsRef.current.has(primaryId)) {
      loadedIdsRef.current.add(primaryId);
      const keep = snapshot.settings.events_keep || 200;
      api
        .listEvents(primaryId, keep)
        .then((hist) => {
          setLogs((prev) => ({ ...prev, [primaryId]: hist }));
        })
        .catch(() => {
          /* 回填失败：保留实时增量，静默忽略 */
        });
    }
    setUnread((prev) => {
      if (!prev[primaryId]) return prev;
      const out = { ...prev };
      delete out[primaryId];
      return out;
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [primaryId]);

  /* ------------------------- 提示条 ------------------------- */

  // 时长可传参：启动期提示含备份文件完整路径，需要更长的停留时间（默认仍为 4 秒）
  const showToast = React.useCallback((msg: string, ms = 4000) => {
    setToast(msg);
    window.setTimeout(() => setToast(null), ms);
  }, []);

  /* ------------------------- 操作封装 ------------------------- */

  const guard = React.useCallback(
    async (fn: () => Promise<unknown>) => {
      try {
        await fn();
      } catch (e) {
        showToast(String(e));
      }
    },
    [showToast]
  );

  const handleStartAll = () => guard(() => api.startPinging(null));
  const handleStopAll = () => guard(() => api.stopPinging(null));
  /** 清空统计：成功后同步清掉全部未读红点（语义 = 已知悉全部故障；运行中的新故障照常重新计红点） */
  const handleResetAll = () =>
    guard(() => api.resetStats(null)).then(() => {
      setUnread({});
    });

  const handleToggleEnabled = (id: number, enabled: boolean) => {
    const t = snapshot.targets.find((x) => x.id === id);
    if (!t) return;
    guard(() => api.updateTarget(id, t.name, t.host, enabled));
  };

  /** 切换单主机「记录事件」开关（走独立命令，避免全量更新副作用） */
  const handleToggleEvents = (id: number, eventsOn: boolean) => {
    guard(() => api.setTargetEvents(id, eventsOn));
  };

  /** 复制当前可见日志（带降级链） */
  const handleCopyLogs = () => {
    const text = visiblePrimaryLogs.map((l) => `${fmtTime(l.ts)}\t${l.text}`).join("\n");
    if (!text) return;
    copyToClipboard(text)
      .then(() => showToast("已复制日志"))
      .catch(() => showToast("复制失败，请手动选择"));
  };

  /** 导出当前选中的主机日志为 CSV（本地时间列） */
  const handleExportCsv = async () => {
    const t = primaryTarget;
    if (!t) return;
    try {
      const safeName = (t.name || t.host).replace(/[\\/:*?"<>|]/g, "_");
      const path = await save({
        defaultPath: `pingboard-events-${safeName}.csv`,
        filters: [{ name: "CSV 文件", extensions: ["csv"] }],
      });
      if (!path) return;
      const tz = -new Date().getTimezoneOffset();
      await api.exportEvents(path, t.id, tz);
      showToast(`已导出：${path}`);
    } catch (e) {
      showToast(`导出失败：${String(e)}`);
    }
  };

  /** 打开「清空事件日志」确认框 */
  const handleClearLogs = () => {
    if (primaryId !== null) setConfirmClearLogs(true);
  };

  /** 确认清空当前主机日志（内存 + 文件） */
  const doClearLogs = () => {
    setConfirmClearLogs(false);
    const id = primaryId;
    if (id === null) return;
    guard(() => api.clearEvents(id)).then(() => {
      setLogs((prev) => ({ ...prev, [id]: [] }));
      setUnread((prev) => {
        if (!prev[id]) return prev;
        const out = { ...prev };
        delete out[id];
        return out;
      });
    });
  };

  /** 拉取文件夹列表（侧边栏渲染用；后台每次改文件夹后都要刷新） */
  const refreshFolders = React.useCallback(() => {
    api
      .listFolders()
      .then(setFolders)
      .catch((e) => showToast("读取文件夹失败：" + String(e)));
  }, [showToast]);

  const handleSort = (key: SortKey) => {
    if (key === sortKey) {
      setSortDir((d) => (d === "asc" ? "desc" : "asc"));
    } else {
      setSortKey(key);
      setSortDir("asc");
    }
  };

  const handleRowClick = (id: number, e: React.MouseEvent) => {
    setPrimaryId(id);
    setSelected((prevSel) => {
      const nextSel = new Set(prevSel);
      if (e.shiftKey && primaryId !== null) {
        const ids = snapshot.targets.map((t) => t.id);
        const a = ids.indexOf(primaryId);
        const b = ids.indexOf(id);
        if (a >= 0 && b >= 0) {
          const lo = Math.min(a, b);
          const hi = Math.max(a, b);
          for (let i = lo; i <= hi; i++) nextSel.add(ids[i]);
        }
        return nextSel;
      }
      if (e.ctrlKey || e.metaKey) {
        if (nextSel.has(id)) nextSel.delete(id);
        else nextSel.add(id);
        return nextSel;
      }
      return new Set([id]);
    });
  };

  const deleteSelected = React.useCallback(() => {
    if (selected.size === 0) return;
    const ids = [...selected];
    guard(() => api.removeTargets(ids)).then(() => {
      setSelected(new Set());
      setPrimaryId((p) => (p !== null && ids.includes(p) ? null : p));
      setLogs((prevLogs) => {
        const out = { ...prevLogs };
        for (const id of ids) delete out[id];
        return out;
      });
      setUnread((prevUnread) => {
        const out = { ...prevUnread };
        for (const id of ids) delete out[id];
        return out;
      });
      for (const id of ids) loadedIdsRef.current.delete(id);
    });
  }, [selected, guard]);

  /** 删除选中前先请求确认（键盘 Delete 与工具栏按钮共用同一入口） */
  const requestDeleteSelected = React.useCallback(() => {
    if (selected.size === 0) return;
    setConfirmDelete(true);
  }, [selected]);

  // 键盘 Delete 删除选中：
  // ① 任意对话框打开时完全不响应（含确认框自身），避免焦点落在按钮/下拉上时误删；
  // ② 输入框内打字不响应；
  // ③ 命中后走确认框，与「清空列表」口径一致（删除后立即落盘，无撤销）。
  React.useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const dialogOpen = document.querySelector('[role="dialog"]') !== null;
      const targetTag = (e.target as HTMLElement | null)?.tagName ?? null;
      if (shouldIgnoreDeleteKey(targetTag, dialogOpen)) return;
      if (e.key === "Delete" && selected.size > 0) {
        e.preventDefault();
        requestDeleteSelected();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [selected, requestDeleteSelected]);

  /**
   * 导出报表。
   *
   * `filter` 缺省为 `"all"`，此时请求体与筛选功能上线前完全一致
   * （默认文件名也仍是 `pingboard-report.<ext>`），保证原有导出流程不受影响。
   */
  const handleExport = async (
    format: ExportFormat,
    onlySelected: boolean,
    filter: ExportFilter = "all"
  ) => {
    // C2：导出范围跟随侧边栏。优先级「仅选中」> 侧边栏范围 > 全部(null)。
    // exportScopeIds 在范围为空时返回 null，故未勾选文件夹时与 v1.1.9 逐字节一致。
    const ids = onlySelected
      ? [...selected]
      : exportScopeIds(snapshot.targets, folderScope);
    try {
      const ext = format;
      // 仅在带筛选时给默认文件名加后缀，避免与既有报表同名混淆
      const suffix =
        filter === "none_loss" ? "-no-loss" : filter === "all_failed" ? "-all-failed" : "";
      const path = await save({
        defaultPath: `pingboard-report${suffix}.${ext}`,
        filters: [{ name: `${format.toUpperCase()} 文件`, extensions: [ext] }],
      });
      if (!path) return;
      setShowExport(false);
      await api.exportReport(path, format, ids, filter);
      showToast(
        filter === "all" ? `已导出：${path}` : `已导出（${exportFilterLabel(filter)}）：${path}`
      );
    } catch (e) {
      showToast(`导出失败：${String(e)}`);
    }
  };

  const addSamples = (startNow: boolean) =>
    guard(async () => {
      const ids = await api.addTargets(SAMPLE_TARGETS);
      if (startNow) await api.startPinging(ids);
    });

  /* ------------------------- 派生数据 ------------------------- */

  // v1.1.10：过滤链 = 侧边栏范围 ∩ 搜索关键词（顺序固定，见 folder-design.md C5）
  // 真实存在的文件夹 id 集合（**排除临时区哨兵 0** —— 后端 list_folders 会
  // 补一个 id=0 的「临时区」项，那不是真文件夹）。用于让 folderCounts
  // 把失效 folder_id 归入临时区，与后端 folder_counts 同口径。
  const realFolderIds = React.useMemo(
    () => new Set(folders.filter((f) => f.folder.id !== 0).map((f) => f.folder.id)),
    [folders]
  );

  // ⚠️ 必须 memo：ping-snapshot 每 500ms 重建 snapshot 引用，
  // 内联调用会让 FolderSidebar 每 500ms 拿一个新 Map 而无谓重渲染。
  const folderCountsMap = React.useMemo(
    () => folderCounts(snapshot.targets, realFolderIds),
    [snapshot.targets, realFolderIds]
  );

  const scopedTargets = React.useMemo(
    () => filterByFolderScope(snapshot.targets, folderScope),
    [snapshot.targets, folderScope]
  );
  const filtered = React.useMemo(() => {
    const kw = search.trim().toLowerCase();
    if (!kw) return scopedTargets;
    return scopedTargets.filter(
      (t) =>
        t.name.toLowerCase().includes(kw) ||
        t.host.toLowerCase().includes(kw) ||
        (t.resolved_ip ?? "").toLowerCase().includes(kw)
    );
  }, [scopedTargets, search]);

  const primaryTarget = React.useMemo(
    () => snapshot.targets.find((t) => t.id === primaryId) ?? null,
    [snapshot.targets, primaryId]
  );

  const primaryLogs = primaryId !== null ? logs[primaryId] ?? [] : [];

  // 按当前事件等级过滤（等级->展示时过滤，改等级历史事件立即显隐）
  const visiblePrimaryLogs = React.useMemo(() => {
    const rank = LEVEL_RANK[snapshot.settings.events_level] ?? 1;
    return primaryLogs.filter((ev) => (LEVEL_RANK[ev.level] ?? 0) <= rank);
  }, [primaryLogs, snapshot.settings.events_level]);

  // 未读故障：红点标在主机列表的备注名右侧（点开该主机即清零，见选中主机的 effect）。
  // 必须用 useMemo 保持引用稳定，否则 TargetTable 的 React.memo 会被每帧击穿。
  const unreadIds = React.useMemo(
    () => new Set(Object.entries(unread).filter(([, n]) => n > 0).map(([k]) => Number(k))),
    [unread]
  );

  // 已有目标 host 列表（小写化），用于「添加主机」对话框的跨批次重复检测
  const existingHosts = React.useMemo(
    () => snapshot.targets.map((t) => t.host.toLowerCase()),
    [snapshot.targets]
  );

  /* ------------------------- 复选框选择 / 选中项启停 / 清空列表 ------------------------- */

  // 切换单行勾选（与行选中共用 selected 集合）；勾选时同步主选中项
  const handleToggleSelect = (id: number) => {
    const next = new Set(selected);
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
      setPrimaryId(id);
    }
    setSelected(next);
  };

  // 表头全选 / 全不选：仅作用当前过滤后可见的列表
  const handleToggleSelectAll = (selectAll: boolean) => {
    if (selectAll) {
      const ids = filtered.map((t) => t.id);
      setSelected(new Set(ids));
      if (ids.length > 0) setPrimaryId(ids[0]);
    } else {
      setSelected(new Set());
    }
  };

  const handleStartSelected = () => guard(() => api.startPinging([...selected]));
  const handleStopSelected = () => guard(() => api.stopPinging([...selected]));

  // 文件夹内（非临时区）的主机台数 —— 供「清空列表」确认框说明影响范围
  const folderedCount = snapshot.targets.length - tempAreaCount(snapshot.targets);

  const handleClearList = () => {
    if (tempAreaCount(snapshot.targets) > 0) setConfirmClear(true);
  };

  const doClearList = () => {
    setConfirmClear(false);
    // 🩸 只删临时区（folder_id === null）。文件夹里的 IP **不受影响**，
    // 这是已定稿口径（docs/folder-design.md 7.4）—— 要删文件夹内的主机
    // 请用侧边栏的「删除文件夹」，避免一次误操作丢掉长期整理好的清单。
    const ids = tempAreaIds(snapshot.targets);
    if (ids.length === 0) return;
    // remove_targets 内部会先停止对应工作线程，再删除目标
    guard(() => api.removeTargets(ids)).then(() => {
      setSelected(new Set());
      setPrimaryId(null);
      setLogs({});
      setUnread({});
      loadedIdsRef.current.clear();
    });
  };

  /* ------------------------- 渲染 ------------------------- */

  return (
    <div className="h-full flex flex-col bg-slate-50 dark:bg-slate-900">
      <Toolbar
        running={snapshot.running}
        activeCount={snapshot.active_threads}
        totalCount={snapshot.targets.length}
        search={search}
        onSearch={setSearch}
        theme={theme}
        onToggleTheme={() => setTheme((t) => (t === "light" ? "dark" : "light"))}
        disabled={false}
        onAdd={() => setShowAdd(true)}
        onStartAll={handleStartAll}
        onStopAll={handleStopAll}
        onResetAll={handleResetAll}
        onClearList={handleClearList}
        onOpenSettings={() => setShowSettings(true)}
        onExport={handleExport}
        onOpenExportDialog={() => setShowExport(true)}
        selectionCount={selected.size}
        onStartSelected={handleStartSelected}
        onStopSelected={handleStopSelected}
        onDeleteSelected={requestDeleteSelected}
      />


      <div className="flex-1 flex min-h-0">
      <FolderSidebar
        folders={folders}
        counts={folderCountsMap}
        scope={folderScope}
        disabled={false}
        onToggle={(id) => setFolderScope((s) => toggleScope(s, id))}
        onClearAll={() => setFolderScope(new Set())}
        onCreate={(name, color) => {
          api
            .createFolder(name, color)
            .then(refreshFolders)
            .catch((e) => showToast("新建失败：" + String(e)));
        }}
        onRename={(id, name, color) => {
          api
            .updateFolder(id, name, color)
            .then(refreshFolders)
            .catch((e) => showToast("保存失败：" + String(e)));
        }}
        onDelete={(id) => {
          api
            .deleteFolder(id, null)
            .then((n: number) => {
              showToast(
                n > 0 ? "已删除文件夹，" + n + " 台主机已移回临时区" : "已删除文件夹"
              );
              setFolderScope(new Set());
            })
            .then(refreshFolders)
            .catch((e) => showToast("删除失败：" + String(e)));
        }}
      />
        <div className="flex-1 min-w-0 border-r border-slate-200 dark:border-slate-700">
          {snapshot.targets.length === 0 ? (
            <div className="h-full flex items-center justify-center px-6">
              <div className="max-w-[560px] text-center">
                <div className="text-4xl mb-3">📡</div>
                <h1 className="text-lg font-semibold text-slate-800 dark:text-slate-100 mb-2">
                  欢迎使用 PingBoard
                </h1>
                <p className="text-slate-500 dark:text-slate-400 mb-5 leading-relaxed">
                  多主机 Ping 监视器。点击「添加主机」录入目标，支持批量粘贴与 IP 段展开；
                  普通用户权限即可运行，无需管理员。
                </p>
                <div className="flex items-center justify-center gap-3 mb-6">
                  <button
                    className="px-4 h-8 rounded bg-sky-600 hover:bg-sky-700 text-white"
                    onClick={() => setShowAdd(true)}
                  >
                    添加主机
                  </button>
                  <button
                    className="px-4 h-8 rounded border border-emerald-600 text-emerald-700 dark:text-emerald-400 hover:bg-emerald-50 dark:hover:bg-emerald-950"
                    onClick={() => addSamples(true)}
                  >
                    添加示例并开始
                  </button>
                </div>
                <div className="text-left inline-block rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-4 py-3 text-[12px] text-slate-600 dark:text-slate-300">
                  <div className="font-medium mb-1">示例主机</div>
                  {SAMPLE_TARGETS.map((s) => (
                    <div key={s.host} className="font-mono">
                      {s.host} <span className="text-slate-500 dark:text-slate-400">· {s.name}</span>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          ) : (
            <TargetTable
              targets={filtered}
              selected={selected}
              primaryId={primaryId}
              sortKey={sortKey}
              sortDir={sortDir}
              onSort={handleSort}
              onRowClick={handleRowClick}
              onToggleSelect={handleToggleSelect}
              onToggleSelectAll={handleToggleSelectAll}
              historyLen={snapshot.settings.history_len}
              unreadIds={unreadIds}
            />
          )}
        </div>

        <div className="w-[420px] shrink-0 bg-white dark:bg-slate-900">
          <DetailPanel
            target={primaryTarget}
            logs={visiblePrimaryLogs}
            session={snapshot.session}
            globalEventsOn={snapshot.settings.events_on}
            onToggleEnabled={handleToggleEnabled}
            onToggleEvents={handleToggleEvents}
            onCopy={handleCopyLogs}
            onExportCsv={handleExportCsv}
            onClear={handleClearLogs}
          />
        </div>
      </div>

      <StatusBar snapshot={snapshot} />

      {/* 导出报表（可选范围 + 筛选口径 + 格式）。默认路径不变，故仍从工具栏「导出 ▾」进入 */}
      <ExportDialog
        open={showExport}
        targets={scopedTargets}
        selected={selected}
        onClose={() => setShowExport(false)}
        onExport={handleExport}
      />

      <AddTargetsDialog
        open={showAdd}
        running={snapshot.running}
        existingHosts={existingHosts}
        folders={folders}
        onClose={() => setShowAdd(false)}
        onAdded={(ids) => {
          // 新增主机可能进了文件夹，侧边栏台数需重取
          refreshFolders();
          // 用「添加前快照长度 + 本次新增数」估算添加后总数（刻意不等快照刷新后再判断）。
          // 限制实际发生在启动 Ping 时：> max_threads（且启用上限）会被拦截，> 4096 为硬保护。
          const nextTotal = snapshot.targets.length + ids.length;
          if (nextTotal > HARD_MAX_THREADS) {
            showToast(`共 ${nextTotal} 台，已超过 4096 台硬上限，超出部分无法启动`);
          } else if (snapshot.settings.limit_max_threads && nextTotal > snapshot.settings.max_threads) {
            showToast(
              `共 ${nextTotal} 台，已超过并发上限 ${snapshot.settings.max_threads} 台：开始前请在「设置」中调高上限或关闭上限开关`
            );
          }
        }}
      />

      <SettingsDialog
        open={showSettings}
        settings={snapshot.settings}
        onClose={() => setShowSettings(false)}
        onSaved={() => showToast("设置已保存并应用")}
      />

      <ConfirmDialog
        open={confirmClear}
        title="清空列表"
        danger
        confirmText="清空列表"
        message={
          <>
            确定要删除临时区的 <b>{tempAreaCount(snapshot.targets)}</b> 个目标吗？此操作会同时清除它们的
            主机条目与统计数据，且无法撤销。
            {folderedCount > 0 && (
              <>
                <br />
                <b>文件夹内的 {folderedCount} 个 IP 不受影响</b>，仍会保留。
              </>
            )}
          </>
        }
        onConfirm={doClearList}
        onCancel={() => setConfirmClear(false)}
      />

      <ConfirmDialog
        open={confirmClearLogs}
        title="清空事件日志"
        danger
        confirmText="清空日志"
        message={
          <>
            确定要清空主机{" "}
            <b>{primaryTarget ? primaryTarget.name || primaryTarget.host : ""}</b>{" "}
            的事件日志吗？此操作会同时清除内存与已保存文件中的该主机事件，且无法撤销。
          </>
        }
        onConfirm={doClearLogs}
        onCancel={() => setConfirmClearLogs(false)}
      />

      <ConfirmDialog
        open={confirmDelete}
        title="删除选中主机"
        danger
        confirmText="删除"
        message={
          <>
            确定要删除选中的 <b>{selected.size}</b> 个目标吗？统计数据一并清除，且无法撤销。
          </>
        }
        onConfirm={() => {
          setConfirmDelete(false);
          deleteSelected();
        }}
        onCancel={() => setConfirmDelete(false)}
      />

      {/* 不截断、允许换行：提示里含备份文件完整路径，被 ellipsis 吃掉的恰好是手工恢复的唯一凭据 */}
      {toast && (
        <div className="fixed bottom-10 left-1/2 -translate-x-1/2 z-50 px-4 py-2 rounded shadow-lg bg-slate-800 text-white text-[12px] max-w-[80vw] whitespace-pre-wrap break-words">
          {toast}
        </div>
      )}
    </div>
  );
};

export default App;
