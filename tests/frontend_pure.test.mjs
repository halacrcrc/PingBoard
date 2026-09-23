// PingBoard 前端纯函数测试（无需浏览器 / WebView）
//
// 目的：用真实源码（src/components/AddTargetsDialog.tsx、src/lib/format.ts）验证
// 批量输入解析、IP 段展开与展示层格式化逻辑，而不是"只靠肉眼读代码"。
//
// 做法：用 esbuild 把真实 TSX/TS 转译并打包，把 React / Tauri 等运行时依赖打桩为空模块，
// 然后直接对导出的纯函数施加断言。
//
// 运行：node tests/frontend_pure.test.mjs
import { build } from "esbuild";
import { fileURLToPath, pathToFileURL } from "node:url";
import path from "node:path";
import fs from "node:fs";
import os from "node:os";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(__dirname, "..");

/** 把 react / @tauri-apps 相关依赖替换为空桩，便于在 Node 里直接加载组件模块 */
const stubPlugin = {
  name: "qa-stub",
  setup(b) {
    const stub = (filter, id) => b.onResolve({ filter }, () => ({ path: id, namespace: "qa-stub" }));
    stub(/^react\/jsx-runtime$/, "jsx-runtime");
    stub(/^react\/jsx-dev-runtime$/, "jsx-dev-runtime");
    stub(/^react$/, "react");
    stub(/^react-dom$/, "react-dom");
    stub(/^@tauri-apps\//, "tauri");
    stub(/lib\/api$/, "api");
    stub(/\.\.\/types$/, "types");

    b.onLoad({ filter: /.*/, namespace: "qa-stub" }, (args) => {
      let contents;
      if (args.path === "api") {
        contents = "export const addTargets = async () => [];\nexport const startPinging = async () => {};\n";
      } else if (args.path === "tauri") {
        // @tauri-apps/* 桩：SettingsDialog 需要 plugin-dialog 的具名导出 open
        contents = "export const open = async () => null;\nexport default {};\n";
      } else if (args.path === "jsx-runtime" || args.path === "jsx-dev-runtime") {
        contents =
          "export const jsx = () => null;\nexport const jsxs = () => null;\nexport const jsxDEV = () => null;\nexport const Fragment = {};\nexport default {};\n";
      } else {
        contents = "export default {};\n";
      }
      return { contents, loader: "js" };
    });
  },
};

async function loadPure(relEntry) {
  const entry = path.join(projectRoot, relEntry);
  const result = await build({
    entryPoints: [entry],
    bundle: true,
    write: false,
    format: "esm",
    platform: "node",
    target: "node20",
    jsx: "transform",
    logLevel: "silent",
    plugins: [stubPlugin],
  });
  const tmp = path.join(os.tmpdir(), `pingboard-fe-${path.basename(entry)}-${Date.now()}.mjs`);
  fs.writeFileSync(tmp, result.outputFiles[0].text, "utf8");
  try {
    return await import(pathToFileURL(tmp).href);
  } finally {
    fs.unlinkSync(tmp);
  }
}

/* --------------------------- 极简断言框架 --------------------------- */
let pass = 0;
const failures = [];
function check(name, fn) {
  try {
    fn();
    pass++;
  } catch (e) {
    failures.push({ name, message: e && e.message ? e.message : String(e) });
  }
}
function eq(actual, expected, msg) {
  const a = JSON.stringify(actual);
  const b = JSON.stringify(expected);
  if (a !== b) throw new Error(`${msg || "断言失败"}：期望 ${b}，实际 ${a}`);
}
function isNull(actual, msg) {
  if (actual !== null) throw new Error(`${msg || "断言失败"}：期望 null，实际 ${JSON.stringify(actual?.slice?.(0, 5) ?? actual)}`);
}

/* ============================ 开始测试 ============================ */
const dlg = await loadPure("src/components/AddTargetsDialog.tsx");
const fmt = await loadPure("src/lib/format.ts");
const settings = await loadPure("src/components/SettingsDialog.tsx");

const { expandIpRange, parseBatch } = dlg;

/* ---------- expandIpRange ---------- */
check("expandIpRange('192.168.1.1-254') 长度 254 且首尾正确", () => {
  const r = expandIpRange("192.168.1.1-254");
  eq(r.length, 254, "长度");
  eq(r[0], "192.168.1.1", "首元素");
  eq(r[253], "192.168.1.254", "尾元素");
});

check("expandIpRange('192.168.1.1-192.168.1.20') 完整区间", () => {
  const r = expandIpRange("192.168.1.1-192.168.1.20");
  eq(r.length, 20, "长度");
  eq(r[0], "192.168.1.1", "首元素");
  eq(r[19], "192.168.1.20", "尾元素");
});

check("expandIpRange('10.0.0.1-10.0.0.3') 三元素", () => {
  eq(expandIpRange("10.0.0.1-10.0.0.3"), ["10.0.0.1", "10.0.0.2", "10.0.0.3"]);
});

check("expandIpRange 反向区间 → null", () => {
  isNull(expandIpRange("192.168.1.5-1"), "start>end");
});

check("expandIpRange 末段越界(>255) → null", () => {
  isNull(expandIpRange("192.168.1.1-300"), "end>255");
});

check("expandIpRange 非 IP 段输入 → null", () => {
  isNull(expandIpRange("www.baidu.com"));
  isNull(expandIpRange("192.168.1.5")); // 无横线，不是区间
  isNull(expandIpRange(""));
});

check("expandIpRange 完整区间含非法八位组 → null", () => {
  isNull(expandIpRange("192.168.1.1-999.1.1.1"), "toInt 越界应拒绝");
});

check("expandIpRange 跨网段完整区间按 32 位递增（有效特性）", () => {
  const r = expandIpRange("192.168.1.1-192.168.2.5");
  eq(r.length, 261, "跨 /24 的完整区间元素数");
  eq(r[255], "192.168.2.0", "跨越边界后的地址");
});

// —— 八位组越界（300.x.x.x）应返回 null（已修复，此用例为回归保护）——
check("expandIpRange 前导八位组越界(300.x.x.x) → 应返回 null", () => {
  isNull(expandIpRange("300.1.1.1-5"), "前导八位组 300 非法，应拒绝");
});

/* ---------- parseBatch ---------- */
check("parseBatch 空输入 → []", () => {
  eq(parseBatch(""), []);
  eq(parseBatch("\n\n   \n\t\n"), []);
});

check("parseBatch 跳过 # 注释行", () => {
  const r = parseBatch("# 这是注释\n\n127.0.0.1\n   # 缩进注释");
  eq(r.length, 1, "仅 1 行有效");
  eq(r[0].host, "127.0.0.1");
  eq(r[0].name, "127.0.0.1", "无备注时备注名回落为主机名");
});

check("parseBatch 主机 + 备注（空格/制表符/分号分隔）", () => {
  eq(parseBatch("223.5.5.5 阿里 DNS")[0], { host: "223.5.5.5", name: "阿里 DNS" });
  eq(parseBatch("223.5.5.5\t阿里")[0], { host: "223.5.5.5", name: "阿里" });
  eq(parseBatch("223.5.5.5;阿里")[0], { host: "223.5.5.5", name: "阿里" });
});

check("parseBatch 去重（大小写不敏感）", () => {
  const r = parseBatch("WWW.BAIDU.COM\nwww.baidu.com\n127.0.0.1\n127.0.0.1 重复");
  eq(r.length, 2, "应去重为 2 条");
  eq(r.map((x) => x.host), ["WWW.BAIDU.COM", "127.0.0.1"]);
});

check("parseBatch IP 段展开并生成带序号的备注名", () => {
  const r = parseBatch("192.168.1.1-3 内网");
  eq(r.length, 3, "展开 3 个");
  eq(r.map((x) => x.host), ["192.168.1.1", "192.168.1.2", "192.168.1.3"]);
  eq(r.map((x) => x.name), ["内网-1", "内网-2", "内网-3"]);
});

check("parseBatch 单次上限保护（<=1024）", () => {
  const r = parseBatch("10.0.0.1-10.0.5.0"); // 完整区间 1281 个
  eq(r.length, 1024, "应被截断到 MAX_BATCH=1024");
});

check("parseBatch 混合：注释 + 普通 + 区间 一起解析", () => {
  const text = [
    "# 头部注释",
    "223.5.5.5 阿里 DNS",
    "114.114.114.114",
    "",
    "10.0.0.1-10.0.0.2 内网段",
    "# 尾部注释",
  ].join("\n");
  const r = parseBatch(text);
  eq(r.length, 4, "1(阿里) + 1(114) + 2(内网段) = 4");
  eq(r[0].name, "阿里 DNS");
  eq(r[1].host, "114.114.114.114");
  eq(r[3].host, "10.0.0.2");
});

/* ---------- format.ts ---------- */
check("fmtMs 空值/NaN → '-'，正常值保留 1 位", () => {
  eq(fmt.fmtMs(null), "-");
  eq(fmt.fmtMs(undefined), "-");
  eq(fmt.fmtMs(NaN), "-");
  eq(fmt.fmtMs(12.34), "12.3");
  eq(fmt.fmtMs(0), "0.0");
});

check("fmtPct 空值 → '0.0%'", () => {
  eq(fmt.fmtPct(null), "0.0%");
  eq(fmt.fmtPct(NaN), "0.0%");
  eq(fmt.fmtPct(25), "25.0%");
});

check("fmtDuration 分档（h/m/s）", () => {
  eq(fmt.fmtDuration(0), "0s");
  eq(fmt.fmtDuration(3000), "3s");
  eq(fmt.fmtDuration(123000), "2m3s");
  eq(fmt.fmtDuration(3723000), "1h2m3s");
});

check("rttColorClass 分档（<50 绿 / <150 黄 / >=150 橙）", () => {
  if (!fmt.rttColorClass(49).includes("emerald")) throw new Error("49ms 应为绿色档");
  if (!fmt.rttColorClass(50).includes("yellow")) throw new Error("50ms 应为黄色档");
  if (!fmt.rttColorClass(149).includes("yellow")) throw new Error("149ms 应为黄色档");
  if (!fmt.rttColorClass(150).includes("orange")) throw new Error("150ms 应为橙色档");
  if (!fmt.rttColorClass(null).includes("slate")) throw new Error("null 应为灰色档");
});

check("rttColorClass 色阶固定（浅色 700 档 / 占位 slate-600，达 WCAG AA）", () => {
  const cases = [
    [49, "text-emerald-700"],
    [50, "text-yellow-700"],
    [149, "text-yellow-700"],
    [150, "text-orange-700"],
    [null, "text-slate-600"],
    [undefined, "text-slate-600"],
  ];
  for (const [v, cls] of cases) {
    const got = fmt.rttColorClass(v);
    if (!got.startsWith(cls)) throw new Error(`${v} → 期望 ${cls}，实际 ${got}`);
  }
  // 占位色不得回退 slate-500：表格行底可能是失败行 red-50，其上 slate-500 仅 4.35:1
  if (fmt.rttColorClass(null).includes("text-slate-500")) {
    throw new Error("占位色不得用 slate-500（表格彩色行底上不达 AA）");
  }
});

check("statusLabel 全覆盖且不含 unknown 分支遗漏", () => {
  eq(fmt.statusLabel("ok"), "正常");
  eq(fmt.statusLabel("timeout"), "超时");
  eq(fmt.statusLabel("failed"), "失败");
  eq(fmt.statusLabel("resolving"), "解析中");
  eq(fmt.statusLabel("idle"), "未开始");
});

check("statusBadgeClass 各状态底色（idle 须为 slate-500：白字对比度 4.76:1）", () => {
  const idle = fmt.statusBadgeClass("idle");
  if (idle.includes("bg-slate-400"))
    throw new Error("idle 不得回退 bg-slate-400（白字仅 2.56:1，低于 WCAG AA 4.5:1）");
  if (!idle.includes("bg-slate-500")) throw new Error(`idle 应为 bg-slate-500，实得「${idle}」`);
  if (!idle.includes("text-white")) throw new Error("idle 应为白字");
  // 其余状态底色不得顺带改动（语义色，另议）
  const want = { ok: "bg-emerald-600", timeout: "bg-amber-600", failed: "bg-red-600", resolving: "bg-sky-600" };
  for (const [s, cls] of Object.entries(want)) {
    if (!fmt.statusBadgeClass(s).includes(cls)) throw new Error(`${s} 底色应为 ${cls}`);
  }
});

/* ==================== Round 2 回归：修复复验（QA 独立追加） ==================== */

check("R2 修复复验：前导/末段八位组越界 → null", () => {
  isNull(expandIpRange("300.1.1.1-5"), "前导八位组 300");
  isNull(expandIpRange("1.300.1.1-5"), "第二位 300");
  isNull(expandIpRange("1.1.300.1-5"), "第三位 300");
  isNull(expandIpRange("1.2.3.300-5"), "起始末段 300");
  isNull(expandIpRange("1.2.3.256-260"), "末段范围 256-260");
});

check("R2 修复复验：反向区间 → null", () => {
  isNull(expandIpRange("1.2.3.9-5"), "末段反向");
  isNull(expandIpRange("192.168.2.5-192.168.1.1"), "完整区间反向");
});

check("R2 修复复验：末段范围 192.168.1.1-254 → 254 项", () => {
  const r = expandIpRange("192.168.1.1-254");
  eq(r.length, 254, "长度");
  eq(r[0], "192.168.1.1", "首");
  eq(r[253], "192.168.1.254", "尾");
});

check("R2 修复复验：完整区间跨 /24 正确（192.168.1.250-192.168.2.5）", () => {
  const r = expandIpRange("192.168.1.250-192.168.2.5");
  eq(r.length, 12, "元素数");
  eq(r[0], "192.168.1.250", "首");
  eq(r[5], "192.168.1.255", "/24 末位");
  eq(r[6], "192.168.2.0", "跨段后首位");
  eq(r[11], "192.168.2.5", "尾");
});

check("R2 修复复验：完整区间形式 a.b.c.d-a.b.c.e 正确", () => {
  eq(expandIpRange("10.0.0.1-10.0.0.3"), ["10.0.0.1", "10.0.0.2", "10.0.0.3"]);
  eq(expandIpRange("192.168.1.1-192.168.1.20").length, 20);
});

check("R2 修复复验：超大区间恰好截断到 1024 项（不得 1025，修掉 off-by-one）", () => {
  const r = expandIpRange("10.0.0.1-10.255.255.254"); // 理论 16,777,214 项
  eq(r.length, 1024, "必须恰为 MAX_BATCH=1024");
  eq(r[0], "10.0.0.1", "首");
  eq(r[1023], "10.0.4.0", "第 1024 项");
  eq(r[1024], undefined, "不得出现第 1025 项");
});

check("R2 修复复验：末段形式上限 255 不被截断", () => {
  eq(expandIpRange("10.0.0.1-255").length, 255);
});

check("R2 修复复验：parseBatch 超大区间仍截断到 1024", () => {
  eq(parseBatch("10.0.0.1-10.255.255.254").length, 1024);
});

/* ============ Round 3 固化：defaultEventsFile（设置对话框默认事件文件路径） ============ */
// 依据：默认事件目录 = app_config_dir（与配置文件同目录），默认文件名 = pingboard-events.jsonl。
// 该纯函数从配置文件绝对路径推导默认事件文件路径，供「事件保存目录」在默认态显示具体绝对路径。
const { defaultEventsFile } = settings;
const EVENTS_FILE_NAME = "pingboard-events.jsonl";

check("defaultEventsFile 标准 Windows 绝对路径 → 同目录 + pingboard-events.jsonl", () => {
  eq(
    defaultEventsFile("C:\\Users\\x\\AppData\\Roaming\\com.pingboard.desktop\\pingboard-config.json"),
    "C:\\Users\\x\\AppData\\Roaming\\com.pingboard.desktop\\pingboard-events.jsonl"
  );
});

check("defaultEventsFile 正斜杠路径 → 保持正斜杠分隔符", () => {
  eq(defaultEventsFile("C:/a/b/pingboard-config.json"), "C:/a/b/pingboard-events.jsonl");
});

check("defaultEventsFile 无分隔符（仅文件名）→ 直接回落默认文件名", () => {
  eq(defaultEventsFile("pingboard-config.json"), "pingboard-events.jsonl");
});

check("defaultEventsFile 空串 → 空串（由调用方兜底文案）", () => {
  eq(defaultEventsFile(""), "");
});

check("defaultEventsFile 尾部带分隔符 → 结果不得出现双反斜杠", () => {
  const r = defaultEventsFile("C:\\dir\\");
  eq(r, "C:\\dir\\pingboard-events.jsonl");
  if (r.includes("\\\\")) throw new Error(`尾部分隔符场景不应产生双反斜杠，实际：${r}`);
});

check("defaultEventsFile UNC 路径 → 保留 UNC 前缀并同目录", () => {
  eq(
    defaultEventsFile("\\\\server\\share\\pingboard-config.json"),
    "\\\\server\\share\\pingboard-events.jsonl"
  );
});

check("defaultEventsFile 含中文/空格的目录名 → 目录原样保留", () => {
  eq(
    defaultEventsFile("C:\\用户\\我的 目录\\pingboard-config.json"),
    "C:\\用户\\我的 目录\\pingboard-events.jsonl"
  );
});

check("defaultEventsFile 属性式：目录部分与输入完全一致，文件名恒为 pingboard-events.jsonl", () => {
  const cases = [
    "C:\\Users\\x\\AppData\\Roaming\\com.pingboard.desktop\\pingboard-config.json",
    "C:/a/b/pingboard-config.json",
    "D:\\deep dir\\my folder\\pingboard-config.json",
    "\\\\server\\share\\pingboard-config.json",
    "C:\\用户\\我的 目录\\pingboard-config.json",
  ];
  for (const p of cases) {
    const out = defaultEventsFile(p);
    const i = Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/"));
    const sep = p[i];
    // ① 目录部分与输入完全一致
    eq(out.slice(0, i), p.slice(0, i), `目录部分应与输入一致：${p}`);
    // ② 分隔符与输入一致
    eq(out[i], sep, `分隔符应与输入一致：${p}`);
    // ③ 文件名恒为 pingboard-events.jsonl
    eq(out.slice(i + 1), EVENTS_FILE_NAME, `文件名应为 ${EVENTS_FILE_NAME}：${p}`);
  }
});

/* ------------------------------ 结果汇总 ------------------------------ */
console.log(`\n===== 前端纯函数测试：通过 ${pass} / 失败 ${failures.length} =====`);
for (const f of failures) {
  console.log(`FAIL  ${f.name}\n      ${f.message}`);
}
process.exit(failures.length === 0 ? 0 : 1);
