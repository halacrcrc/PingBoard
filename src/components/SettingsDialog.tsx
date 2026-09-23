// 设置对话框：分组卡片版式（双列卡片 + 说明右对齐），暴露全部 PingSettings，
// 含输入校验，保存后立即生效。
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

/** 纯数字字段定义 */
interface NumDef {
  key: keyof PingSettings;
  label: string;
  hint: string;
  min: number;
  max: number;
  step: number;
}

/** 关闭并发上限后仍保留的硬保护（与 Rust 侧 state::HARD_MAX_THREADS 一致） */
const HARD_MAX_THREADS = 4096;
/** 最大线程数取值范围（与后端 stats::normalize_settings 一致） */
const MAX_THREADS_MIN = 1;
const MAX_THREADS_MAX = 1024;
/** 默认事件文件名（与 Rust 侧 events::EVENTS_FILE_NAME 一致），默认目录 = app_config_dir */
const EVENTS_FILE_NAME = "pingboard-events.jsonl";
/** 显式固定滚动条宽度（px）：与标题/底部区的右内边距补位一一对应，杜绝“猜滚动条宽度” */
const SCROLLBAR_W = 12;

/** 探测参数（卡片内三列网格）
 *  hint 统一右对齐挂在标签行上，因此必须短（≤ 8 字），长解释放到 note。 */
const PROBE_FIELDS: NumDef[] = [
  { key: "interval_ms", label: "探测间隔 (ms)", hint: "≥100", min: 100, max: 600000, step: 100 },
  { key: "timeout_ms", label: "超时时间 (ms)", hint: "≥100", min: 100, max: 60000, step: 100 },
  { key: "payload_size", label: "负载大小 (字节)", hint: "0..65500", min: 0, max: 65500, step: 1 },
  { key: "ttl", label: "TTL", hint: "1..255", min: 1, max: 255, step: 1 },
  { key: "history_len", label: "趋势保留点数", hint: "10..600", min: 10, max: 600, step: 10 },
];

/** 事件等级三选一 */
const LEVEL_OPTIONS: { value: EventLevel; label: string }[] = [
  { value: "fault", label: "仅故障" },
  { value: "standard", label: "标准" },
  { value: "detail", label: "详细" },
];
/** 每主机保留条数三选一 */
const KEEP_OPTIONS: number[] = [50, 200, 1000];

/* ------------------------------- 样式常量 ------------------------------- */

/** 卡片：白底 + 细边框，浮在浅灰底上 */
const card = "rounded-lg bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800";
/** 数字输入框：先前的 h-7 px-2 与框内数字过挤，统一放松为 h-8 px-3 */
const inputCls =
  "w-full h-8 px-3 rounded-md border bg-slate-50 dark:bg-slate-800/60 border-slate-300 dark:border-slate-600 text-[13px] text-slate-800 dark:text-slate-100 focus:outline-none focus:ring-2 focus:ring-sky-500/40 focus:border-sky-500 disabled:opacity-50 disabled:cursor-not-allowed";
/** 卡片内小按钮（禁止折行 / 压缩） */
const smallBtn =
  "h-8 px-3 rounded-md border border-slate-300 dark:border-slate-600 bg-white dark:bg-slate-800 hover:bg-slate-100 dark:hover:bg-slate-700 text-[12px] text-slate-700 dark:text-slate-200 whitespace-nowrap shrink-0 disabled:opacity-40 disabled:cursor-not-allowed";
/** 复选框 / 单选 */
const checkCls = "accent-emerald-600 w-4 h-4 shrink-0";
const radioCls = "accent-sky-600 w-4 h-4 shrink-0";
/** 字段标签行（标签左、取值范围右对齐） */
const labelRow = "flex items-baseline justify-between gap-2 mb-1.5 min-w-0";
const labelCls = "text-[12px] text-slate-600 dark:text-slate-300 whitespace-nowrap";
const labelHint = "text-[11px] text-slate-400 dark:text-slate-500 whitespace-nowrap shrink-0";
/** 控件下方的补充说明（仅长解释使用），统一挂在控件下方、左边缘与控件对齐 */
const noteCls = "mt-1.5 text-[11px] leading-snug text-slate-400 dark:text-slate-500";

/**
 * 滚动区：显式固定 12px 滚动条宽度 + 常驻槽位（scrollbar-gutter: stable）。
 * 标题卡与底部区以 `pr-6`/`mr-6`(= pl-3 的 12px + 固定的 12px 滚动条) 补位，
 * 使三处卡片的右边缘严格对齐（不依赖浏览器默认滚动条宽度）。
 * 顶部区的 ✕ **不参与补位** —— 它贴弹窗右上角，与卡片右缘不对齐是有意为之。
 */
const scrollAreaCls =
  "flex-1 overflow-y-auto overflow-x-hidden pl-3 pr-3 py-3 [scrollbar-gutter:stable] " +
  "[&::-webkit-scrollbar]:w-3 [&::-webkit-scrollbar-thumb]:rounded-full " +
  "[&::-webkit-scrollbar-thumb]:bg-slate-300 dark:[&::-webkit-scrollbar-thumb]:bg-slate-600";

/* ------------------------------- 纯函数 ------------------------------- */

/**
 * 由配置文件绝对路径推出「同目录下默认事件文件」的绝对路径。
 * 默认事件目录 = app_config_dir（与配置文件同目录），默认文件 = pingboard-events.jsonl。
 * configPath 为空时返回空串，由调用方给出兜底文案。
 */
export function defaultEventsFile(configPath: string): string {
  if (!configPath) return "";
  const i = Math.max(configPath.lastIndexOf("\\"), configPath.lastIndexOf("/"));
  if (i < 0) return EVENTS_FILE_NAME;
  return configPath.slice(0, i) + configPath[i] + EVENTS_FILE_NAME;
}

/* ------------------------------- 小组件 ------------------------------- */

/** 分组卡片容器：左侧一道强调条 + 标题 + 可选副说明；span=2 时横跨双列 */
const Card: React.FC<{ title: string; sub?: string; span?: 1 | 2; children: React.ReactNode }> = ({
  title,
  sub,
  span,
  children,
}) => (
  <section
    className={`${card} px-4 py-2.5`}
    style={span === 2 ? { gridColumn: "span 2 / span 2" } : undefined}
  >
    <div className="flex items-center gap-2 mb-2">
      <span className="w-[3px] h-3.5 rounded-full bg-sky-500 shrink-0" />
      <h3 className="text-[13px] font-semibold text-slate-700 dark:text-slate-200 whitespace-nowrap">
        {title}
      </h3>
      {sub && (
        <span className="text-[11px] text-slate-400 dark:text-slate-500 truncate min-w-0">{sub}</span>
      )}
    </div>
    {children}
  </section>
);

/** 网格字段：标签行（标签左 / 取值范围右）+ 控件 + 可选补充说明 */
const Field: React.FC<{
  label?: string;
  hint?: string;
  note?: string;
  span?: number;
  children: React.ReactNode;
}> = ({ label, hint, note, span, children }) => (
  <div
    className="min-w-0"
    style={span ? { gridColumn: `span ${span} / span ${span}` } : undefined}
  >
    {(label || hint) && (
      <div className={labelRow}>
        {label && <span className={labelCls} title={label}>{label}</span>}
        {hint && <span className={labelHint}>{hint}</span>}
      </div>
    )}
    {children}
    {note && <div className={noteCls}>{note}</div>}
  </div>
);

/** 成组块：标签在上（左对齐、可带状态小标） / 控件在下。用于「事件日志」卡片，保证每行左边缘一致 */
const Block: React.FC<{ label: string; tag?: React.ReactNode; children: React.ReactNode }> = ({
  label,
  tag,
  children,
}) => (
  <div className="min-w-0">
    <div className="flex items-center gap-1.5 mb-1.5 min-w-0">
      <span className={labelCls} title={label}>
        {label}
      </span>
      {tag}
    </div>
    {children}
  </div>
);

/** 行内开关：复选框 + 状态文字（与输入框等高 h-8，保证同行视觉基线一致） */
const Toggle: React.FC<{
  checked: boolean;
  onChange: (v: boolean) => void;
  disabled?: boolean;
  onText?: string;
  offText?: string;
}> = ({ checked, onChange, disabled, onText = "已开启", offText = "已关闭" }) => (
  <label
    className={`flex items-center gap-2 h-8 select-none min-w-0 ${
      disabled ? "opacity-50 cursor-not-allowed" : "cursor-pointer"
    }`}
  >
    <input
      type="checkbox"
      className={checkCls}
      checked={checked}
      disabled={disabled}
      onChange={(e) => onChange(e.target.checked)}
    />
    <span className="text-[13px] text-slate-700 dark:text-slate-200 whitespace-nowrap truncate">
      {checked ? onText : offText}
    </span>
  </label>
);

/* ------------------------------- 主组件 ------------------------------- */

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

  /** 默认事件文件绝对路径（由配置文件路径推出；读不到时回落空串） */
  const defaultEventsPath = defaultEventsFile(configPath);
  const dirIsDefault = draft.events_dir === null || draft.events_dir === "";
  const dirValue = dirIsDefault ? defaultEventsPath : (draft.events_dir as string);
  /** 具体路径 / 兜底文案（绝不显示空白或半截路径） */
  const dirDisplay = dirValue || "（正在读取默认目录…）";

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
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4">
      <div className="w-[900px] min-w-0 max-h-[calc(100vh_-_2rem)] flex flex-col rounded-xl overflow-hidden bg-slate-100 dark:bg-slate-950 border border-slate-300 dark:border-slate-800 shadow-2xl">
        {/* 顶部区：✕ 贴弹窗右上角（不受滚动条补位约束）；标题卡用 mr-6 保持与滚动区卡片右对齐 */}
        <div className="shrink-0 pl-3 pt-0.5">
          <div className="flex items-center justify-end mb-2 pr-1.5">
            <button
              type="button"
              aria-label="关闭"
              title="关闭"
              className="w-7 h-7 shrink-0 rounded-md flex items-center justify-center text-slate-400 transition-colors hover:text-slate-700 hover:bg-slate-200 dark:text-slate-400 dark:hover:text-slate-200 dark:hover:bg-slate-800"
              onClick={onClose}
            >
              {/* 自绘 ✕：线条粗细由 strokeWidth 精确控制（字体符号无法保证粗细一致） */}
              <svg
                viewBox="0 0 16 16"
                className="w-3.5 h-3.5"
                fill="none"
                stroke="currentColor"
                strokeWidth={2.2}
                strokeLinecap="round"
                aria-hidden="true"
              >
                <path d="M4 4 12 12M12 4 4 12" />
              </svg>
            </button>
          </div>
          {/* 标题卡片（仍为整宽）：标题 + 副标题 + 配置文件路径 */}
          <div className={`${card} px-4 py-2.5 mr-6`}>
            <div className="text-[15px] font-semibold leading-6 text-slate-800 dark:text-slate-100">
              设置
            </div>
            <div className="text-[11px] text-slate-500 dark:text-slate-400 mt-0.5">
              探测参数、并发上限与事件日志；保存后立即生效
            </div>
            <div className="mt-1 flex items-baseline gap-1.5 min-w-0 text-[11px]">
              <span className="text-slate-400 dark:text-slate-500 shrink-0">配置文件</span>
              <span
                className="font-mono text-slate-500 dark:text-slate-400 truncate"
                title={configPath || "（读取中…）"}
              >
                {configPath || "（读取中…）"}
              </span>
            </div>
          </div>
        </div>

        {/* 卡片流（唯一滚动区）：双列卡片，宽卡片横跨两列 */}
        <div className={scrollAreaCls}>
          <div className="grid grid-cols-2 gap-2.5 items-start">
            <Card title="探测" span={2}>
              <div className="grid grid-cols-3 gap-x-4 gap-y-2.5">
                {PROBE_FIELDS.map((f) => (
                  <Field key={String(f.key)} label={f.label} hint={f.hint}>
                    <input
                      type="number"
                      className={inputCls}
                      value={String(draft[f.key] as number)}
                      min={f.min}
                      max={f.max}
                      step={f.step}
                      onChange={(e) => setNum(f.key, e.target.value)}
                    />
                  </Field>
                ))}
              </div>
            </Card>

            <Card title="界面与行为">
              <div className="grid grid-cols-2 gap-x-4 gap-y-3">
                <Field label="失败时提示音" hint="成功转失败时">
                  <Toggle
                    checked={draft.beep_on_fail}
                    onChange={(v) => setDraft((d) => ({ ...d, beep_on_fail: v }))}
                  />
                </Field>
                <Field label="启动时自动开始">
                  <Toggle
                    checked={draft.auto_start}
                    onChange={(v) => setDraft((d) => ({ ...d, auto_start: v }))}
                  />
                </Field>
              </div>
            </Card>

            <Card title="并发">
              <div className="grid grid-cols-2 gap-x-4 gap-y-3">
                <Field label="启用线程数上限" hint="关后不拦截">
                  <Toggle
                    checked={draft.limit_max_threads}
                    onChange={(v) => setDraft((d) => ({ ...d, limit_max_threads: v }))}
                    onText="已启用"
                    offText="未启用"
                  />
                </Field>
                <Field
                  label="最大线程数"
                  hint={draft.limit_max_threads ? `${MAX_THREADS_MIN}..${MAX_THREADS_MAX}` : "未启用上限，仅记录"}
                >
                  <input
                    type="number"
                    className={inputCls}
                    value={String(draft.max_threads)}
                    min={MAX_THREADS_MIN}
                    max={MAX_THREADS_MAX}
                    step={1}
                    disabled={!draft.limit_max_threads}
                    onChange={(e) => setNum("max_threads", e.target.value)}
                  />
                </Field>
              </div>
            </Card>

            {/* 并发说明（整宽）：从「并发」卡片移出，使两卡内容等高、下边缘对齐 */}
            <p
              className="px-1 text-[11px] leading-snug text-slate-400 dark:text-slate-500"
              style={{ gridColumn: "span 2 / span 2" }}
            >
              并发：每台主机占用 1 个系统线程 + 1 个 ICMP 句柄。500 台以上线程调度与内存开销会明显上升，
              建议保持上限开启并分批启动；关闭后仅保留 {HARD_MAX_THREADS} 硬保护。
            </p>

            <Card title="事件日志" sub="记录每台主机的故障与恢复" span={2}>
              {/* 成组块 2×2：标签在上 / 控件在下，每行左边缘一致，无右对齐空洞 */}
              <div className="grid grid-cols-2 gap-x-6 gap-y-3">
                <Block label="启用事件日志">
                  <Toggle
                    checked={draft.events_on}
                    onChange={(v) => setDraft((d) => ({ ...d, events_on: v }))}
                  />
                </Block>
                <Block label="保存事件到文件">
                  <Toggle
                    checked={draft.events_persist}
                    onChange={(v) => setDraft((d) => ({ ...d, events_persist: v }))}
                  />
                </Block>
                <Block label="每主机保留条数">
                  <div className="flex items-center gap-4 h-8">
                    {KEEP_OPTIONS.map((k) => (
                      <label key={k} className="flex items-center gap-1.5 cursor-pointer">
                        <input
                          type="radio"
                          name="events_keep"
                          className={radioCls}
                          checked={draft.events_keep === k}
                          onChange={() => setDraft((d) => ({ ...d, events_keep: k }))}
                        />
                        <span className="text-[13px] text-slate-700 dark:text-slate-200 whitespace-nowrap">
                          {k}
                        </span>
                      </label>
                    ))}
                  </div>
                </Block>
                <Block label="事件等级">
                  <div className="flex items-center gap-5 h-8">
                    {LEVEL_OPTIONS.map((o) => (
                      <label key={o.value} className="flex items-center gap-1.5 cursor-pointer">
                        <input
                          type="radio"
                          name="events_level"
                          className={radioCls}
                          checked={draft.events_level === o.value}
                          onChange={() => setDraft((d) => ({ ...d, events_level: o.value }))}
                        />
                        <span className="text-[13px] text-slate-700 dark:text-slate-200 whitespace-nowrap">
                          {o.label}
                        </span>
                      </label>
                    ))}
                  </div>
                </Block>
              </div>

              {/* 事件等级含义（整宽说明行） */}
              <p className={`${noteCls} mt-3`}>
                事件等级：仅故障 = 故障 / 无法连通 / 解析失败 / 恢复；标准另含首次连通 / 开始 / 停止；
                详细另含配置变更。
              </p>

              {/* 事件保存目录（整宽）：具体默认绝对路径 + 选择 / 恢复默认 */}
              <div className="mt-3.5">
                <Block
                  label="事件保存目录"
                  tag={
                    dirIsDefault ? (
                      <span className="text-[10px] leading-none px-1.5 py-0.5 rounded bg-slate-100 dark:bg-slate-800 text-slate-400 dark:text-slate-500 whitespace-nowrap shrink-0">
                        默认
                      </span>
                    ) : undefined
                  }
                >
                  <div className="flex items-center gap-2">
                    <div
                      className={`${inputCls} flex-1 min-w-0 flex items-center cursor-default select-all`}
                      title={dirDisplay}
                    >
                      <span className="truncate">{dirDisplay}</span>
                    </div>
                    <button type="button" className={smallBtn} onClick={chooseDir}>
                      选择…
                    </button>
                    <button
                      type="button"
                      className={smallBtn}
                      onClick={() => setDraft((d) => ({ ...d, events_dir: null }))}
                      disabled={dirIsDefault}
                    >
                      恢复默认
                    </button>
                  </div>
                  <div className={noteCls}>
                    历史文件在应用启动时读回，每主机只取最近「保留条数」条。
                  </div>
                </Block>
              </div>
            </Card>
          </div>
        </div>

        {/* 底部操作栏（卡片）：左侧承载错误提示，右侧两个按钮；pr-6 与滚动区补位对齐 */}
        <div className="shrink-0 pl-3 pr-6 pb-3">
          <div className={`${card} px-4 py-2.5 flex items-center justify-between gap-3`}>
            <div
              className="min-w-0 text-[12px] text-red-600 dark:text-red-400 truncate"
              title={error ?? ""}
            >
              {error ?? ""}
            </div>
            <div className="flex items-center gap-2 shrink-0">
              <button className={smallBtn} onClick={() => setDraft({ ...settings })} disabled={busy}>
                还原
              </button>
              <button
                className="h-8 px-4 rounded-md bg-sky-600 hover:bg-sky-700 text-white text-[12px] whitespace-nowrap shrink-0 disabled:opacity-50 disabled:cursor-not-allowed"
                onClick={save}
                disabled={busy}
              >
                {busy ? "保存中…" : "保存并应用"}
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};

export default SettingsDialog;
