// 设置对话框：暴露全部 PingSettings，含输入校验，保存后立即生效
import React from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type { EventLevel, PingSettings } from "../types";
import * as api from "../lib/api";

export interface SettingsDialogProps {
  open: boolean;
  settings: PingSettings;
  onClose: () => void;
  onSaved: () => void;
}

interface FieldDef {
  key: keyof PingSettings;
  label: string;
  hint?: string;
  min?: number;
  max?: number;
  step?: number;
  type: "number" | "bool";
}

const FIELDS: FieldDef[] = [
  { key: "interval_ms", label: "探测间隔 (ms)", hint: "≥100", min: 100, max: 600000, step: 100, type: "number" },
  { key: "timeout_ms", label: "超时时间 (ms)", hint: "≥100", min: 100, max: 60000, step: 100, type: "number" },
  { key: "payload_size", label: "负载大小 (字节)", hint: "0..65500", min: 0, max: 65500, step: 1, type: "number" },
  { key: "ttl", label: "TTL", hint: "1..255", min: 1, max: 255, step: 1, type: "number" },
  { key: "history_len", label: "趋势保留点数", hint: "10..600", min: 10, max: 600, step: 10, type: "number" },
  { key: "beep_on_fail", label: "失败时提示音", hint: "主机由成功转为失败时播放短提示音", type: "bool" },
  { key: "auto_start", label: "启动时自动开始", hint: "应用启动后自动开始 Ping 已启用目标", type: "bool" },
];

/** 关闭并发上限后仍保留的硬保护（与 Rust 侧 state::HARD_MAX_THREADS 一致） */
const HARD_MAX_THREADS = 4096;
/** 最大线程数取值范围（与后端 stats::normalize_settings 一致） */
const MAX_THREADS_MIN = 1;
const MAX_THREADS_MAX = 1024;

/** 事件等级三选一（含各档包含的事件说明） */
const LEVEL_OPTIONS: { value: EventLevel; label: string; desc: string }[] = [
  { value: "fault", label: "仅故障", desc: "故障 / 无法连通 / 解析失败 / 恢复" },
  { value: "standard", label: "标准", desc: "故障 + 首次连通 / 开始 / 停止（默认）" },
  { value: "detail", label: "详细", desc: "标准 + 配置变更" },
];
/** 每主机保留条数三选一 */
const KEEP_OPTIONS: number[] = [50, 200, 1000];
/** 设置内的小按钮样式（禁止折行 / 压缩） */
const smallBtn =
  "h-7 px-2 rounded border border-slate-300 dark:border-slate-600 bg-white dark:bg-slate-800 hover:bg-slate-100 dark:hover:bg-slate-700 text-slate-700 dark:text-slate-200 whitespace-nowrap shrink-0 disabled:opacity-40 disabled:cursor-not-allowed";

const SettingsDialog: React.FC<SettingsDialogProps> = ({ open, settings, onClose, onSaved }) => {
  const [draft, setDraft] = React.useState<PingSettings>(settings);
  const [error, setError] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState(false);
  const [configPath, setConfigPath] = React.useState<string>("");

  // 仅在对话框「刚刚打开」时把外部设置拷进本地草稿。
  // ⚠️ 不能把 settings 放进依赖数组：App 每 500ms 推送一次快照，settings 每次都是新对象，
  // 会让草稿被周期性重置（曾表现为「启用线程数上限」无法取消勾选）。
  const wasOpenRef = React.useRef(false);
  React.useEffect(() => {
    const justOpened = open && !wasOpenRef.current;
    wasOpenRef.current = open;
    if (!justOpened) return;
    setDraft(settings);
    setError(null);
    setBusy(false);
    api.getConfigPath().then(setConfigPath).catch(() => setConfigPath(""));
    // 故意不写依赖数组：每次渲染后执行，但仅在 justOpened 时做事。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  });

  if (!open) return null;

  const setNum = (key: keyof PingSettings, v: string) => {
    const n = v === "" ? 0 : Number(v);
    setDraft((d) => ({ ...d, [key]: Number.isNaN(n) ? 0 : n }));
  };

  /** 选择事件保存目录（系统文件夹选择对话框） */
  const chooseDir = async () => {
    try {
      const picked = await openDialog({ directory: true, multiple: false, title: "选择事件保存目录" });
      if (typeof picked === "string" && picked) {
        setDraft((d) => ({ ...d, events_dir: picked }));
      }
    } catch {
      /* 取消或权限异常：忽略，保持原值 */
    }
  };

  const save = async () => {
    // 前端校验（后端也会再校验一次）
    const checks: [keyof PingSettings, number, number, string][] = [
      ["interval_ms", 100, 600000, "探测间隔不得小于 100ms"],
      ["timeout_ms", 100, 60000, "超时时间不得小于 100ms"],
      ["payload_size", 0, 65500, "负载大小需在 0..65500 之间"],
      ["ttl", 1, 255, "TTL 需在 1..255 之间"],
      ["max_threads", 1, 1024, "线程数需在 1..1024 之间"],
      ["history_len", 10, 600, "趋势保留点数需在 10..600 之间"],
    ];
    for (const [key, min, max, msg] of checks) {
      const val = draft[key] as number;
      if (val < min || val > max) {
        setError(msg);
        return;
      }
    }
    // 事件保留条数须为 50 / 200 / 1000
    if (!KEEP_OPTIONS.includes(draft.events_keep)) {
      setError("每主机保留条数须为 50 / 200 / 1000");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await api.updateSettings(draft);
      onSaved();
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="w-[560px] max-h-[86vh] flex flex-col rounded-lg shadow-2xl bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700">
        <div className="px-4 h-11 flex items-center justify-between border-b border-slate-200 dark:border-slate-700">
          <div className="font-semibold text-slate-800 dark:text-slate-100">设置</div>
          <button className="text-slate-400 hover:text-slate-700 dark:hover:text-slate-200" onClick={onClose}>
            ✕
          </button>
        </div>

        <div className="px-4 py-3 flex-1 overflow-auto">
          <table className="w-full">
            <tbody>
              {FIELDS.map((f) => (
                <tr key={String(f.key)} className="align-middle">
                  <td className="py-1.5 pr-3 w-40 text-slate-600 dark:text-slate-300">{f.label}</td>
                  <td className="py-1.5">
                    {f.type === "number" ? (
                      <input
                        type="number"
                        className="w-32 h-7 px-2 rounded border bg-white dark:bg-slate-800 border-slate-300 dark:border-slate-600 text-slate-800 dark:text-slate-100 focus:outline-none focus:ring-1 focus:ring-sky-500"
                        value={String(draft[f.key] as number)}
                        min={f.min}
                        max={f.max}
                        step={f.step}
                        onChange={(e) => setNum(f.key, e.target.value)}
                      />
                    ) : (
                      <input
                        type="checkbox"
                        className="accent-emerald-600 w-4 h-4"
                        checked={Boolean(draft[f.key])}
                        onChange={(e) => setDraft((d) => ({ ...d, [f.key]: e.target.checked }))}
                      />
                    )}
                    {f.hint && <span className="ml-2 text-[11px] text-slate-400">{f.hint}</span>}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>

          {/* 并发线程数：开关 + 数字组合行 + 性能影响说明 */}
          <div className="mt-3 pt-3 border-t border-slate-200 dark:border-slate-700">
            <table className="w-full">
              <tbody>
                <tr className="align-middle">
                  <td className="py-1.5 pr-3 w-40 text-slate-600 dark:text-slate-300">启用线程数上限</td>
                  <td className="py-1.5">
                    <input
                      type="checkbox"
                      className="accent-emerald-600 w-4 h-4"
                      checked={draft.limit_max_threads}
                      onChange={(e) => setDraft((d) => ({ ...d, limit_max_threads: e.target.checked }))}
                    />
                    <span className="ml-2 text-[11px] text-slate-400">
                      关闭后不再按「最大线程数」拦截，仅保留 {HARD_MAX_THREADS} 硬保护
                    </span>
                  </td>
                </tr>
                <tr className="align-middle">
                  <td className="py-1.5 pr-3 w-40 text-slate-600 dark:text-slate-300">最大线程数</td>
                  <td className="py-1.5">
                    <input
                      type="number"
                      className="w-32 h-7 px-2 rounded border bg-white dark:bg-slate-800 border-slate-300 dark:border-slate-600 text-slate-800 dark:text-slate-100 focus:outline-none focus:ring-1 focus:ring-sky-500 disabled:opacity-50 disabled:cursor-not-allowed"
                      value={String(draft.max_threads)}
                      min={MAX_THREADS_MIN}
                      max={MAX_THREADS_MAX}
                      step={1}
                      disabled={!draft.limit_max_threads}
                      onChange={(e) => setNum("max_threads", e.target.value)}
                    />
                    <span className="ml-2 text-[11px] text-slate-400">
                      {MAX_THREADS_MIN}..{MAX_THREADS_MAX}
                      {!draft.limit_max_threads && "（当前未启用，仅记录）"}
                    </span>
                  </td>
                </tr>
              </tbody>
            </table>
            <p className="mt-1 text-[11px] leading-relaxed text-slate-400 dark:text-slate-500">
              每台主机会占用 1 个系统线程 + 1 个 ICMP 句柄。开启上限时按「最大线程数」拦截，可防止误加大量主机拖慢系统；
              关闭后不再限制（仅保留 {HARD_MAX_THREADS} 硬保护）——200 台以内影响很小，500 台以上线程调度、内存与界面刷新开销会明显上升，
              1000 台以上建议保持开启并分批启动。
            </p>
          </div>

          {/* 事件日志分组 */}
          <div className="mt-3 pt-3 border-t border-slate-200 dark:border-slate-700">
            <div className="mb-2 font-medium text-slate-700 dark:text-slate-200">事件日志</div>
            <table className="w-full">
              <tbody>
                <tr className="align-middle">
                  <td className="py-1.5 pr-3 w-40 text-slate-600 dark:text-slate-300">启用事件日志</td>
                  <td className="py-1.5">
                    <input
                      type="checkbox"
                      className="accent-emerald-600 w-4 h-4"
                      checked={draft.events_on}
                      onChange={(e) => setDraft((d) => ({ ...d, events_on: e.target.checked }))}
                    />
                    <span className="ml-2 text-[11px] text-slate-400 whitespace-nowrap">
                      总开关，关闭后不再产生任何事件
                    </span>
                  </td>
                </tr>
                <tr className="align-top">
                  <td className="py-1.5 pr-3 w-40 text-slate-600 dark:text-slate-300">事件等级</td>
                  <td className="py-1.5">
                    <div className="flex flex-col gap-1">
                      {LEVEL_OPTIONS.map((o) => (
                        <label key={o.value} className="flex items-start gap-1.5 cursor-pointer">
                          <input
                            type="radio"
                            name="events_level"
                            className="accent-sky-600 w-4 h-4 mt-0.5"
                            checked={draft.events_level === o.value}
                            onChange={() => setDraft((d) => ({ ...d, events_level: o.value }))}
                          />
                          <span className="text-slate-700 dark:text-slate-200 whitespace-nowrap">{o.label}</span>
                          <span className="text-[11px] text-slate-400">{o.desc}</span>
                        </label>
                      ))}
                    </div>
                  </td>
                </tr>
                <tr className="align-middle">
                  <td className="py-1.5 pr-3 w-40 text-slate-600 dark:text-slate-300">保存事件到文件</td>
                  <td className="py-1.5">
                    <input
                      type="checkbox"
                      className="accent-emerald-600 w-4 h-4"
                      checked={draft.events_persist}
                      onChange={(e) => setDraft((d) => ({ ...d, events_persist: e.target.checked }))}
                    />
                    <span className="ml-2 text-[11px] text-slate-400 whitespace-nowrap">
                      默认保存到配置目录下的 pingboard-events.jsonl
                    </span>
                  </td>
                </tr>
                <tr className="align-middle">
                  <td className="py-1.5 pr-3 w-40 text-slate-600 dark:text-slate-300">事件保存目录</td>
                  <td className="py-1.5">
                    <div className="flex items-center gap-2">
                      <div
                        className="flex-1 min-w-0 h-7 px-2 rounded border border-slate-300 dark:border-slate-600 bg-slate-50 dark:bg-slate-800 text-slate-600 dark:text-slate-300 leading-7 truncate text-[12px]"
                        title={draft.events_dir ?? "（默认：配置目录）"}
                      >
                        {draft.events_dir ?? "（默认：配置目录）"}
                      </div>
                      <button type="button" className={smallBtn} onClick={chooseDir}>
                        选择…
                      </button>
                      <button
                        type="button"
                        className={smallBtn}
                        onClick={() => setDraft((d) => ({ ...d, events_dir: null }))}
                        disabled={draft.events_dir === null}
                      >
                        恢复默认
                      </button>
                    </div>
                  </td>
                </tr>
                <tr className="align-middle">
                  <td className="py-1.5 pr-3 w-40 text-slate-600 dark:text-slate-300">每主机保留条数</td>
                  <td className="py-1.5">
                    <div className="flex items-center gap-3">
                      {KEEP_OPTIONS.map((k) => (
                        <label key={k} className="flex items-center gap-1 cursor-pointer">
                          <input
                            type="radio"
                            name="events_keep"
                            className="accent-sky-600 w-4 h-4"
                            checked={draft.events_keep === k}
                            onChange={() => setDraft((d) => ({ ...d, events_keep: k }))}
                          />
                          <span className="text-slate-700 dark:text-slate-200">{k}</span>
                        </label>
                      ))}
                      <span className="text-[11px] text-slate-400 whitespace-nowrap">条/主机</span>
                    </div>
                  </td>
                </tr>
              </tbody>
            </table>
            <p className="mt-1 text-[11px] leading-relaxed text-slate-400 dark:text-slate-500">
              事件在探测状态迁移时产生，按等级过滤展示；「仅故障」最简洁，「详细」额外记录配置变更。
              历史文件在应用启动时读回（每主机只取最近「保留条数」条）。
            </p>
          </div>

          {error && <div className="mt-2 text-red-600 dark:text-red-400">{error}</div>}

          <div className="mt-4 pt-2 border-t border-slate-200 dark:border-slate-700 text-[11px] text-slate-400 break-all">
            配置文件：{configPath || "（读取中…）"}
          </div>
        </div>

        <div className="px-4 h-12 flex items-center justify-end gap-2 border-t border-slate-200 dark:border-slate-700">
          <button
            className="px-3 h-7 rounded border border-slate-300 dark:border-slate-600 bg-white dark:bg-slate-800 hover:bg-slate-100 dark:hover:bg-slate-700"
            onClick={() => setDraft({ ...settings })}
            disabled={busy}
          >
            还原
          </button>
          <button
            className="px-3 h-7 rounded bg-sky-600 hover:bg-sky-700 text-white disabled:opacity-50"
            onClick={save}
            disabled={busy}
          >
            {busy ? "保存中…" : "保存并应用"}
          </button>
        </div>
      </div>
    </div>
  );
};

export default SettingsDialog;
