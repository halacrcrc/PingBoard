// 应用根组件：装配工具栏 / 主表格 / 详情面板 / 状态栏，订阅后端聚合快照事件
import React from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { save } from "@tauri-apps/plugin-dialog";
import type { LogEntry, PingSettings, Snapshot, Status, TargetEntry } from "./types";
import * as api from "./lib/api";
import Toolbar from "./components/Toolbar";
import TargetTable, { type SortKey } from "./components/TargetTable";
import StatusBar from "./components/StatusBar";
import DetailPanel from "./components/DetailPanel";
import AddTargetsDialog from "./components/AddTargetsDialog";
import SettingsDialog from "./components/SettingsDialog";

/** 默认设置（与 Rust 侧 PingSettings::default 保持一致） */
const DEFAULT_SETTINGS: PingSettings = {
  interval_ms: 1000,
  timeout_ms: 2000,
  payload_size: 32,
  ttl: 128,
  max_threads: 256,
  beep_on_fail: false,
  auto_start: false,
  history_len: 60,
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
};

/** 首次启动引导页展示的示例主机 */
const SAMPLE_TARGETS: TargetEntry[] = [
  { host: "223.5.5.5", name: "阿里 DNS" },
  { host: "114.114.114.114", name: "114 DNS" },
  { host: "www.baidu.com", name: "百度" },
];

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
  const [logs, setLogs] = React.useState<Record<number, LogEntry[]>>({});
  const [toast, setToast] = React.useState<string | null>(null);

  const prevStatusRef = React.useRef<Record<number, Status>>({});
  const audioCtxRef = React.useRef<AudioContext | null>(null);

  /* ------------------------- 事件订阅与初始化 ------------------------- */

  React.useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    listen<Snapshot>("ping-snapshot", (e) => setSnapshot(e.payload)).then((u) => {
      if (disposed) u();
      else unlisten = u;
    });
    api
      .getState()
      .then(setSnapshot)
      .catch((e) => showToast(`初始化失败：${String(e)}`));
    return () => {
      disposed = true;
      if (unlisten) unlisten();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /* ------------------------- 主题 ------------------------- */

  React.useEffect(() => {
    document.documentElement.classList.toggle("dark", theme === "dark");
    localStorage.setItem("pb-theme", theme);
  }, [theme]);

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

  /* ------------------------- 状态迁移 → 日志 + 提示音 ------------------------- */

  React.useEffect(() => {
    const prev = prevStatusRef.current;
    const next: Record<number, Status> = {};
    const additions: Record<number, LogEntry> = {};

    for (const t of snapshot.targets) {
      next[t.id] = t.status;
      const p = prev[t.id];
      if (p && p !== t.status) {
        const toFail = t.status === "timeout" || t.status === "failed";
        const fromFail = p === "timeout" || p === "failed";
        if (toFail && p === "ok") {
          additions[t.id] = {
            ts: Date.now(),
            kind: "fail",
            text:
              t.status === "timeout"
                ? `探测超时（连续 ${t.consecutive_fail} 次）`
                : `探测失败${t.last_error ? "：" + t.last_error : ""}`,
          };
        } else if (t.status === "ok" && fromFail) {
          additions[t.id] = {
            ts: Date.now(),
            kind: "recover",
            text: `已恢复（${t.last_rtt_ms === null ? "-" : t.last_rtt_ms.toFixed(1)} ms）`,
          };
        }
      }
    }
    prevStatusRef.current = next;

    const keys = Object.keys(additions);
    if (keys.length === 0) return;

    setLogs((prevLogs) => {
      const out: Record<number, LogEntry[]> = { ...prevLogs };
      for (const k of keys) {
        const id = Number(k);
        const arr = out[id] ? [...out[id]] : [];
        arr.unshift(additions[id]);
        out[id] = arr.slice(0, 50);
      }
      return out;
    });

    if (snapshot.settings.beep_on_fail && keys.some((k) => additions[Number(k)].kind === "fail")) {
      beep();
    }
  }, [snapshot, beep]);

  /* ------------------------- 提示条 ------------------------- */

  const showToast = React.useCallback((msg: string) => {
    setToast(msg);
    window.setTimeout(() => setToast(null), 4000);
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
  const handleResetAll = () => guard(() => api.resetStats(null));

  const handleToggleEnabled = (id: number, enabled: boolean) => {
    const t = snapshot.targets.find((x) => x.id === id);
    if (!t) return;
    guard(() => api.updateTarget(id, t.name, t.host, enabled));
  };

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
    });
  }, [selected, guard]);

  // 键盘 Delete 删除选中
  React.useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement | null)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;
      if (e.key === "Delete" && selected.size > 0) {
        e.preventDefault();
        deleteSelected();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [selected, deleteSelected]);

  const handleExport = async (format: "csv" | "txt" | "html", onlySelected: boolean) => {
    const ids = onlySelected ? [...selected] : null;
    try {
      const ext = format;
      const path = await save({
        defaultPath: `pingboard-report.${ext}`,
        filters: [{ name: `${format.toUpperCase()} 文件`, extensions: [ext] }],
      });
      if (!path) return;
      await api.exportReport(path, format, ids);
      showToast(`已导出：${path}`);
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

  const filtered = React.useMemo(() => {
    const kw = search.trim().toLowerCase();
    if (!kw) return snapshot.targets;
    return snapshot.targets.filter(
      (t) =>
        t.name.toLowerCase().includes(kw) ||
        t.host.toLowerCase().includes(kw) ||
        (t.resolved_ip ?? "").toLowerCase().includes(kw)
    );
  }, [snapshot.targets, search]);

  const primaryTarget = React.useMemo(
    () => snapshot.targets.find((t) => t.id === primaryId) ?? null,
    [snapshot.targets, primaryId]
  );

  const primaryLogs = primaryId !== null ? logs[primaryId] ?? [] : [];

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
        onOpenSettings={() => setShowSettings(true)}
        onExport={handleExport}
        selectionCount={selected.size}
        onDeleteSelected={deleteSelected}
      />

      <div className="flex-1 flex min-h-0">
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
                      {s.host} <span className="text-slate-400">· {s.name}</span>
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
              onToggleEnabled={handleToggleEnabled}
              historyLen={snapshot.settings.history_len}
            />
          )}
        </div>

        <div className="w-[420px] shrink-0 bg-white dark:bg-slate-900">
          <DetailPanel target={primaryTarget} logs={primaryLogs} />
        </div>
      </div>

      <StatusBar snapshot={snapshot} />

      <AddTargetsDialog
        open={showAdd}
        running={snapshot.running}
        onClose={() => setShowAdd(false)}
        onAdded={() => {
          /* 依赖快照事件刷新，无需本地处理 */
        }}
      />

      <SettingsDialog
        open={showSettings}
        settings={snapshot.settings}
        onClose={() => setShowSettings(false)}
        onSaved={() => showToast("设置已保存并应用")}
      />

      {toast && (
        <div className="fixed bottom-10 left-1/2 -translate-x-1/2 z-50 px-4 py-2 rounded shadow-lg bg-slate-800 text-white text-[12px] max-w-[80vw] truncate">
          {toast}
        </div>
      )}
    </div>
  );
};

export default App;
