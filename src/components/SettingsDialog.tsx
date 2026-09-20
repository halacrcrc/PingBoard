// 设置对话框：暴露全部 PingSettings，含输入校验，保存后立即生效
import React from "react";
import type { PingSettings } from "../types";
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
  { key: "max_threads", label: "最大线程数", hint: "1..1024", min: 1, max: 1024, step: 1, type: "number" },
  { key: "history_len", label: "趋势保留点数", hint: "10..600", min: 10, max: 600, step: 10, type: "number" },
  { key: "beep_on_fail", label: "失败时提示音", hint: "主机由成功转为失败时播放短提示音", type: "bool" },
  { key: "auto_start", label: "启动时自动开始", hint: "应用启动后自动开始 Ping 已启用目标", type: "bool" },
];

const SettingsDialog: React.FC<SettingsDialogProps> = ({ open, settings, onClose, onSaved }) => {
  const [draft, setDraft] = React.useState<PingSettings>(settings);
  const [error, setError] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState(false);
  const [configPath, setConfigPath] = React.useState<string>("");

  React.useEffect(() => {
    if (open) {
      setDraft(settings);
      setError(null);
      setBusy(false);
      api.getConfigPath().then(setConfigPath).catch(() => setConfigPath(""));
    }
  }, [open, settings]);

  if (!open) return null;

  const setNum = (key: keyof PingSettings, v: string) => {
    const n = v === "" ? 0 : Number(v);
    setDraft((d) => ({ ...d, [key]: Number.isNaN(n) ? 0 : n }));
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
