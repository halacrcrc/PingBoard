// 添加主机对话框：支持单个添加、批量粘贴（自动去重/跳过注释）与 IP 段展开、从文件导入
import React from "react";
import * as dialog from "@tauri-apps/plugin-dialog";
import type { ImportPayload, TargetEntry } from "../types";
import * as api from "../lib/api";
import ConfirmDialog from "./ConfirmDialog";

export interface AddTargetsDialogProps {
  open: boolean;
  onClose: () => void;
  onAdded: (ids: number[]) => void;
  /** 立即开始 ping（默认勾选） */
  running: boolean;
  /** 列表中已有目标的 host 列表（已小写化），用于跨批次重复检测 */
  existingHosts: string[];
}

/** 单次批量展开上限，防止误输入造成爆炸 */
const MAX_BATCH = 1024;

/** 文件选择对话框支持的扩展名 */
const FILE_EXTS = ["txt", "csv", "xlsx", "xls", "xlsm", "ods"];

const row = "flex items-center gap-3 py-1";
const label = "w-20 shrink-0 text-slate-600 dark:text-slate-300";
const input =
  "flex-1 h-8 px-2 rounded border bg-white dark:bg-slate-800 border-slate-300 dark:border-slate-600 text-slate-800 dark:text-slate-100 focus:outline-none focus:ring-1 focus:ring-sky-500";

/**
 * 展开 IP 段简写：
 *  - `192.168.1.1-254`           → 192.168.1.1 .. 192.168.1.254
 *  - `192.168.1.1-192.168.1.20`  → 起止全地址区间
 * 非 IP 段输入、任一八位组越界（>255）、反向区间均返回 null。
 * 单次展开最多 MAX_BATCH（1024）项。
 */
export function expandIpRange(spec: string): string[] | null {
  // 形式一：a.b.c.d-e（仅末段范围）
  const m1 = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})-(\d{1,3})$/.exec(spec);
  if (m1) {
    const [, a, b, c, d1, d2] = m1;
    // 五个八位组逐一校验：任一位越界（>255）即视为非法输入
    const nums = [a, b, c, d1, d2].map((x) => parseInt(x, 10));
    if (nums.some((n) => Number.isNaN(n) || n > 255)) return null;

    const start = nums[3];
    const end = nums[4];
    if (start > end) return null;

    const out: string[] = [];
    // 注意用 `<` 而非 `<=`：保证最多产出 MAX_BATCH 项（避免 off-by-one 多出 1 项）
    for (let i = start; i <= end && out.length < MAX_BATCH; i++) {
      out.push(`${a}.${b}.${c}.${i}`);
    }
    return out;
  }
  // 形式二：a.b.c.d-a.b.c.d（完整区间，按 32 位整数递增）
  const m2 = /^(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})-(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})$/.exec(spec);
  if (m2) {
    const toInt = (ip: string): number | null => {
      const p = ip.split(".").map((x) => parseInt(x, 10));
      if (p.length !== 4 || p.some((n) => Number.isNaN(n) || n < 0 || n > 255)) return null;
      return ((p[0] << 24) >>> 0) + (p[1] << 16) + (p[2] << 8) + p[3];
    };
    const s = toInt(m2[1]);
    const e = toInt(m2[2]);
    if (s === null || e === null || s > e) return null;
    const out: string[] = [];
    // 同上：使用 `<` 保证最多 MAX_BATCH 项
    for (let v = s; v <= e && out.length < MAX_BATCH; v++) {
      out.push(`${(v >>> 24) & 255}.${(v >>> 16) & 255}.${(v >>> 8) & 255}.${v & 255}`);
    }
    return out;
  }
  return null;
}

/**
 * 解析批量粘贴文本：
 *  - 每行 `主机` 或 `主机<空格/逗号/制表符>备注`
 *  - 跳过空行与以 # 开头的注释行
 *  - 自动去重（按主机名去重）
 *  - 支持 IP 段展开
 */
export function parseBatch(text: string): TargetEntry[] {
  const out: TargetEntry[] = [];
  const seen = new Set<string>();
  const push = (host: string, name: string) => {
    const key = host.toLowerCase();
    if (seen.has(key)) return;
    seen.add(key);
    out.push({ host, name: name || host });
  };

  for (const rawLine of text.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (!line || line.startsWith("#")) continue;
    // 以空白 / 逗号 / 制表符 分隔主机与备注
    const parts = line.split(/[\s,;\t]+/).filter(Boolean);
    if (parts.length === 0) continue;
    const spec = parts[0];
    const name = parts.slice(1).join(" ");
    const range = expandIpRange(spec);
    if (range && range.length > 0) {
      for (const ip of range) push(ip, name ? `${name}-${ip.split(".").pop()}` : ip);
    } else {
      push(spec, name);
    }
    if (out.length > MAX_BATCH) break;
  }
  return out.slice(0, MAX_BATCH);
}

/** 表头识别：第 1 列（主机）候选 */
const HEADER_HOST_RE = /^(主机|主机名|ip|ip地址|地址|host|hostname)$/;
/** 表头识别：第 2 列（备注）候选 */
const HEADER_NAME_RE = /^(备注|备注名|名称|名字|说明|描述|name|note|remark|desc|comment)$/;

/**
 * 把表格行规范化为「每行一个 `主机\t备注`」的批量文本，再交给 parseBatch 统一解析。
 *  - 仅识别第 1 行是否为表头（第 1 列命中主机关键字且第 2 列命中备注关键字则跳过）
 *  - 第 1 列 = 主机，第 2 列 = 备注（可为空）
 *  - 第 1 列为空的行跳过
 */
export function tableToBatchText(rows: string[][]): string {
  const lines: string[] = [];
  rows.forEach((r, idx) => {
    const host = (r[0] ?? "").trim();
    const name = (r[1] ?? "").trim();
    if (idx === 0 && HEADER_HOST_RE.test(host.toLowerCase()) && HEADER_NAME_RE.test(name.toLowerCase())) {
      return; // 跳过表头行
    }
    if (!host) return;
    lines.push(name ? `${host}\t${name}` : host);
  });
  return lines.join("\n");
}

const AddTargetsDialog: React.FC<AddTargetsDialogProps> = ({ open, onClose, onAdded, running, existingHosts }) => {
  const [tab, setTab] = React.useState<"single" | "batch" | "file">("single");
  const [host, setHost] = React.useState("");
  const [name, setName] = React.useState("");
  const [batch, setBatch] = React.useState("");
  const [startNow, setStartNow] = React.useState(true);
  const [error, setError] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState(false);
  // 文件导入状态
  const [filePath, setFilePath] = React.useState("");
  const [filePayload, setFilePayload] = React.useState<ImportPayload | null>(null);
  const [fileBusy, setFileBusy] = React.useState(false);
  // 跨批次重复确认状态：非空时弹出确认框（记录本次待提交条目与其重复 host）
  const [dupPrompt, setDupPrompt] = React.useState<{ entries: TargetEntry[]; dups: string[] } | null>(null);

  React.useEffect(() => {
    if (open) {
      setError(null);
      setBusy(false);
      setStartNow(true);
      setDupPrompt(null);
    }
  }, [open]);

  // 已有目标的 host 集合（去首尾空白 + 小写），用 Set 保证 O(1) 查找。
  // ⚠️ 必须在 `if (!open) return null` 之前无条件调用（React Hooks 规则：hook 不得位于条件 return 之后）
  const existingSet = React.useMemo(
    () => new Set(existingHosts.map((h) => h.trim().toLowerCase())),
    [existingHosts]
  );

  // 文件导入：文本直接解析；表格先规范化为批量文本再解析（复用同一套解析逻辑）
  // ⚠️ 必须在 `if (!open) return null` 之前无条件调用（React Hooks 规则：hook 不得位于条件 return 之后）
  const fileEntries = React.useMemo<TargetEntry[]>(() => {
    if (tab !== "file" || !filePayload) return [];
    const text = filePayload.kind === "text" ? filePayload.text : tableToBatchText(filePayload.rows);
    return parseBatch(text);
  }, [tab, filePayload]);

  // 批量页解析结果同样 memo 化：App 每秒多次重渲染，避免关闭态下重复全量解析。
  // ⚠️ 同样必须位于条件 return 之前。
  const parsed = React.useMemo<TargetEntry[]>(
    () => (tab === "batch" ? parseBatch(batch) : []),
    [tab, batch]
  );

  // 当前待添加列表（批量页 = parsed，文件页 = fileEntries，单条页 = []）
  const currentEntries = tab === "file" ? fileEntries : parsed;

  // 当前待添加列表中与「已有目标」重复的条目（比较规则：host 去首尾空白后转小写）
  const dupInfo = React.useMemo(() => {
    const dups: string[] = [];
    for (const e of currentEntries) {
      if (existingSet.has(e.host.trim().toLowerCase())) dups.push(e.host);
    }
    return { count: dups.length, hosts: dups };
  }, [currentEntries, existingSet]);

  if (!open) return null;

  const doAdd = async (entries: TargetEntry[]) => {
    if (entries.length === 0) {
      setError("请输入至少一个主机名或 IP 地址");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const ids = await api.addTargets(entries);
      if (startNow) {
        await api.startPinging(ids);
      }
      onAdded(ids);
      setHost("");
      setName("");
      setBatch("");
      setFilePath("");
      setFilePayload(null);
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const onSubmitSingle = async () => {
    const h = host.trim();
    if (!h) {
      setError("请输入主机名或 IP 地址");
      return;
    }
    const n = name.trim();
    await doAdd([{ host: h, name: n || h }]);
  };

  /** 批量 / 文件提交入口：存在重复时弹确认框让用户选择，否则直接提交 */
  const submitEntries = (entries: TargetEntry[], dups: string[]) => {
    if (entries.length === 0) {
      setError("请输入至少一个主机名或 IP 地址");
      return;
    }
    if (dups.length === 0) {
      void doAdd(entries);
      return;
    }
    setError(null);
    setDupPrompt({ entries, dups });
  };

  const onSubmitBatch = () => submitEntries(parsed, dupInfo.hosts);

  const onSubmitFile = () => submitEntries(fileEntries, dupInfo.hosts);

  /** 打开文件对话框 → 读取内容 → 解析（不在此处提交） */
  const pickFile = async () => {
    setError(null);
    try {
      const selected = await dialog.open({
        multiple: false,
        directory: false,
        title: "选择主机列表文件",
        filters: [{ name: "主机列表", extensions: FILE_EXTS }],
      });
      const path = Array.isArray(selected) ? selected[0] : selected;
      if (!path) return;
      setFilePath(path);
      setFileBusy(true);
      setFilePayload(null);
      const payload = await api.readImportFile(path);
      setFilePayload(payload);
    } catch (e) {
      setFilePayload(null);
      setError(`读取文件失败：${String(e)}`);
    } finally {
      setFileBusy(false);
    }
  };

  const tabClass = (active: boolean) =>
    `px-3 h-7 rounded-t border-b-2 ${
      active ? "border-sky-500 text-sky-600 dark:text-sky-400" : "border-transparent text-slate-500 dark:text-slate-400"
    }`;

  // 目标预览块（批量与文件页共用样式）；dups 为与列表已有目标重复的 host 列表
  const preview = (list: TargetEntry[], dups: string[]) => (
    <>
      <div className="mt-1 text-[12px] text-slate-500 dark:text-slate-400">
        解析到 <span className="font-mono text-sky-600 dark:text-sky-400">{list.length}</span> 个目标
        <span className="text-slate-500 dark:text-slate-400">（单次上限 {MAX_BATCH}，自动去重、跳过空行与 # 注释）</span>
      </div>
      {dups.length > 0 && (
        <div className="mt-1 text-[12px] text-amber-600 dark:text-amber-400">
          ⚠ 其中 {dups.length} 个与列表中已有目标重复：{dups.slice(0, 3).join("、")}
          {dups.length > 3 ? " 等" : ""}
        </div>
      )}
      {list.length > 0 && (
        <div className="mt-2 max-h-28 overflow-auto rounded border border-slate-200 dark:border-slate-700 p-1.5 font-mono text-[11px] text-slate-600 dark:text-slate-300">
          {list.slice(0, 200).map((p, i) => (
            <div key={i} className="truncate">
              {p.host}
              <span className="text-slate-500 dark:text-slate-400"> · {p.name}</span>
            </div>
          ))}
          {list.length > 200 && <div className="text-slate-500 dark:text-slate-400">… 其余 {list.length - 200} 个</div>}
        </div>
      )}
    </>
  );

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="w-[720px] max-h-[86vh] flex flex-col rounded-lg shadow-2xl bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700">
        <div className="px-4 h-11 flex items-center justify-between border-b border-slate-200 dark:border-slate-700">
          <div className="font-semibold text-slate-800 dark:text-slate-100">添加主机</div>
          <button className="text-slate-400 hover:text-slate-700 dark:hover:text-slate-200" onClick={onClose}>
            ✕
          </button>
        </div>

        <div className="px-4 py-3 flex-1 overflow-auto">
          {/* 标签切换 */}
          <div className="flex gap-1 mb-3">
            <button className={tabClass(tab === "single")} onClick={() => setTab("single")}>
              单个添加
            </button>
            <button className={tabClass(tab === "batch")} onClick={() => setTab("batch")}>
              批量粘贴 / IP 段
            </button>
            <button className={tabClass(tab === "file")} onClick={() => setTab("file")}>
              从文件导入
            </button>
          </div>

          {tab === "single" && (
            <div>
              <div className={row}>
                <span className={label}>主机名/IP</span>
                <input
                  className={input}
                  value={host}
                  autoFocus
                  placeholder="例如 223.5.5.5 或 www.baidu.com"
                  onChange={(e) => setHost(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && onSubmitSingle()}
                />
              </div>
              <div className={row}>
                <span className={label}>备注名</span>
                <input
                  className={input}
                  value={name}
                  placeholder="可选，留空则使用主机名"
                  onChange={(e) => setName(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && onSubmitSingle()}
                />
              </div>
            </div>
          )}

          {tab === "batch" && (
            <div>
              <textarea
                className="w-full h-56 px-2 py-1.5 rounded border bg-white dark:bg-slate-800 border-slate-300 dark:border-slate-600 text-slate-800 dark:text-slate-100 font-mono text-[12px] focus:outline-none focus:ring-1 focus:ring-sky-500 resize-none"
                value={batch}
                placeholder={"每行一个，支持「主机 备注」：\n223.5.5.5 阿里 DNS\n114.114.114.114 114DNS\nwww.baidu.com\n\n# 以 # 开头为注释，可跳过\n# 支持 IP 段展开：\n192.168.1.1-254 内网段\n10.0.0.1-10.0.0.20"}
                onChange={(e) => setBatch(e.target.value)}
              />
              {preview(parsed, dupInfo.hosts)}
            </div>
          )}

          {tab === "file" && (
            <div>
              <div className="flex items-center gap-3">
                <button
                  className="px-3 h-8 rounded border border-slate-300 dark:border-slate-600 bg-white dark:bg-slate-800 hover:bg-slate-100 dark:hover:bg-slate-700 text-slate-700 dark:text-slate-200"
                  onClick={pickFile}
                  disabled={fileBusy}
                >
                  选择文件…
                </button>
                <div className="flex-1 min-w-0 truncate font-mono text-[12px] text-slate-600 dark:text-slate-300" title={filePath}>
                  {filePath || <span className="text-slate-500 dark:text-slate-400">支持 .txt / .csv / .xlsx / .xls / .xlsm / .ods</span>}
                </div>
              </div>

              <div className="mt-2 text-[12px] leading-relaxed text-slate-500 dark:text-slate-400">
                <div>· txt / csv：每行「主机 备注」，支持 # 注释与 IP 段（如 192.168.1.1-254）</div>
                <div>· Excel：第 1 列主机、第 2 列备注，自动跳过表头行，仅读取第一个工作表</div>
              </div>

              {fileBusy && (
                <div className="mt-3 text-[12px] text-sky-600 dark:text-sky-400">正在读取并解析文件…</div>
              )}

              {!fileBusy && filePayload && preview(fileEntries, dupInfo.hosts)}
            </div>
          )}

          <label className="mt-3 flex items-center gap-2 text-slate-600 dark:text-slate-300">
            <input
              type="checkbox"
              className="accent-emerald-600"
              checked={startNow}
              onChange={(e) => setStartNow(e.target.checked)}
            />
            添加后立即开始 Ping
            {running && <span className="text-[11px] text-slate-500 dark:text-slate-400">（当前已在运行，新目标会自动启动）</span>}
          </label>

          {error && <div className="mt-2 text-red-600 dark:text-red-400">{error}</div>}
        </div>

        <div className="px-4 h-12 flex items-center justify-end gap-2 border-t border-slate-200 dark:border-slate-700">
          <button
            className="px-3 h-7 rounded border border-slate-300 dark:border-slate-600 bg-white dark:bg-slate-800 hover:bg-slate-100 dark:hover:bg-slate-700"
            onClick={onClose}
            disabled={busy}
          >
            取消
          </button>
          <button
            className="px-3 h-7 rounded bg-sky-600 hover:bg-sky-700 text-white disabled:opacity-50"
            onClick={tab === "single" ? onSubmitSingle : tab === "batch" ? onSubmitBatch : onSubmitFile}
            disabled={busy || (tab === "file" && fileBusy)}
          >
            {busy
              ? "添加中…"
              : tab === "single"
              ? "添加"
              : tab === "batch"
              ? `添加 ${parsed.length} 个`
              : `添加 ${fileEntries.length} 个`}
          </button>
        </div>
      </div>

      {/* 跨批次重复确认：让用户选择「跳过重复」或「仍然全部添加」（默认推荐跳过） */}
      <ConfirmDialog
        open={dupPrompt !== null}
        title="存在重复目标"
        danger={false}
        message={
          <>
            <b>{dupPrompt?.dups.length ?? 0}</b> 个目标已在列表中，重复条目会产生重复行。
            {dupPrompt && dupPrompt.dups.length > 0 && (
              <div className="mt-1 text-[12px] text-slate-500 dark:text-slate-400">
                例如：{dupPrompt.dups.slice(0, 3).join("、")}
                {dupPrompt.dups.length > 3 ? " 等" : ""}
              </div>
            )}
          </>
        }
        confirmText={
          dupPrompt && dupPrompt.entries.length - dupPrompt.dups.length === 0
            ? "无新目标可添加"
            : `跳过重复，添加 ${dupPrompt ? dupPrompt.entries.length - dupPrompt.dups.length : 0} 个`
        }
        confirmDisabled={!!dupPrompt && dupPrompt.entries.length - dupPrompt.dups.length === 0}
        secondaryText={dupPrompt ? `仍然全部添加 ${dupPrompt.entries.length} 个` : undefined}
        onSecondary={() => {
          if (!dupPrompt) return;
          const all = dupPrompt.entries;
          setDupPrompt(null);
          void doAdd(all);
        }}
        onConfirm={() => {
          if (!dupPrompt) return;
          // 只提交非重复条目；比较规则与检测一致（去首尾空白 + 小写）
          const unique = dupPrompt.entries.filter((e) => !existingSet.has(e.host.trim().toLowerCase()));
          setDupPrompt(null);
          void doAdd(unique);
        }}
        onCancel={() => setDupPrompt(null)}
      />
    </div>
  );
};

export default AddTargetsDialog;
