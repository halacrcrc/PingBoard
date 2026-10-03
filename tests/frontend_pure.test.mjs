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

/**
 * 剥掉块注释与行注释，只留可执行代码。
 * 必须剥：源码里多处注释会提到 `doAdd([entry])`、`window.confirm` 这类字面量
 * （例如 decideAdd 的说明注释里就写着「曾直接调 `doAdd([entry])`」），
 * 直接扫原文会把说明文字误判成真实调用。
 */
const stripComments = (src) => src.replace(/\/\*[\s\S]*?\*\//g, "").replace(/(^|[^:])\/\/[^\n]*/g, "$1");

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
    const r = fn();
    // 拒绝 thenable：`check` 是同步调用 + 同步 try/catch，若有人误写 `async () => {...}`，
    // 返回的 Promise 永远不会同步 throw → 用例静默 pass++。这种"写了但不会失败"的用例
    // 比没写测试更危险（虚假安全感），因此在这里显式拦截并报错。
    if (r && typeof r.then === "function") {
      throw new Error("check() 不支持异步用例：请改为同步断言，或新增 asyncCheck() 并 await");
    }
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
const edlg = await loadPure("src/components/ExportDialog.tsx");

const { expandIpRange, parseBatch, findDuplicateHosts, decideAdd, tableToBatchText } = dlg;

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

/* ============ Round 5 固化：跨批次重复检测（单个添加曾绕过，E4 回归保护） ============ */
// 依据：三条提交路径（单个 / 批量 / 文件）必须共用同一套重复检测；
// 比较规则 = host 去首尾空白 + 转小写，重复时返回 host 原文供弹窗展示。
//
// ⚠️ 断言对象从 findDuplicateHosts 换成 decideAdd：原来的用例名说「单条与批量口径一致」，
// 断言的却只是 findDuplicateHosts 本身 —— 把 onSubmitSingle 回滚成 `void doAdd([entry])`
// 它依然全绿（纯函数没坏，坏的是接线）。真正的行为锁由下面的 decideAdd 决策用例
// + Round 7 的静态接线守卫共同承担。

check("findDuplicateHosts 命中已有目标（大小写不敏感 + 去首尾空白）", () => {
  eq(
    findDuplicateHosts(
      [{ host: "WWW.Baidu.COM", name: "x" }, { host: " 223.5.5.5 ", name: "y" }],
      ["www.baidu.com", "223.5.5.5"]
    ),
    ["WWW.Baidu.COM", " 223.5.5.5 "],
    "重复项应返回 host 原文"
  );
});

check("findDuplicateHosts 无重复 → 空数组；空输入 → 空数组", () => {
  eq(findDuplicateHosts([{ host: "9.9.9.9", name: "x" }], ["8.8.8.8"]), []);
  eq(findDuplicateHosts([], ["8.8.8.8"]), []);
  eq(findDuplicateHosts([{ host: "8.8.8.8", name: "x" }], []), []);
});

/* ---------- decideAdd：添加决策（空 / 确认 / 直接添加） ---------- */
check("decideAdd 三条路径同口径：单条输入与批量解析结果决策一致", () => {
  // 单个添加路径：[{host: 输入框原文}]；批量路径：parseBatch 输出 —— 两者过同一决策必须等价
  const existing = ["192.0.2.1"];
  const single = decideAdd([{ host: "192.0.2.1", name: "192.0.2.1" }], existing);
  const batch = decideAdd(parseBatch("192.0.2.1"), existing);
  eq(single.action, "confirm", "单个添加必须走确认（曾直接 doAdd 绕过检测）");
  eq(batch.action, "confirm", "批量路径同样走确认");
  eq(single.dups.length, 1, "单个添加必须检出 1 个重复");
  eq(batch.dups.length, 1);
  eq(single.dups[0].toLowerCase(), batch.dups[0].toLowerCase(), "命中 host 一致");
});

check("decideAdd 空输入 → reject", () => {
  eq(decideAdd([], ["a"]).action, "reject", "空条目必须被拒绝");
});

check("decideAdd 命中已有目标 → confirm 且带回重复 host 原文", () => {
  const d = decideAdd([{ host: "192.0.2.1", name: "x" }], ["192.0.2.1"]);
  eq(d.action, "confirm");
  eq(d.dups, ["192.0.2.1"], "dups 应为 host 原文");
  eq(d.entries.length, 1, "confirm 分支必须原样带回 entries");
});

check("decideAdd 无重复 → add（直接提交，不弹确认）", () => {
  const d = decideAdd([{ host: "9.9.9.9", name: "x" }], ["192.0.2.1"]);
  eq(d.action, "add");
  eq(d.entries, [{ host: "9.9.9.9", name: "x" }], "add 分支必须原样带回 entries");
});

check("decideAdd 混合批次：部分重复仍走 confirm，dups 只含重复项", () => {
  const d = decideAdd(
    [
      { host: "10.0.0.1", name: "新" },
      { host: "192.0.2.1", name: "旧" },
    ],
    ["192.0.2.1"]
  );
  eq(d.action, "confirm");
  eq(d.dups, ["192.0.2.1"], "dups 只应含重复项，不含新增项");
  eq(d.entries.length, 2, "entries 应保留全部条目，由用户在弹窗里选择");
});

check("decideAdd 属性：action 只可能是 reject / add / confirm 三种", () => {
  const allowed = ["reject", "add", "confirm"];
  const samples = [
    [[], ["a"]],
    [[{ host: "1.1.1.1", name: "a" }], ["2.2.2.2"]],
    [[{ host: "1.1.1.1", name: "a" }], ["1.1.1.1"]],
  ];
  for (const [entries, existing] of samples) {
    const a = decideAdd(entries, existing).action;
    if (!allowed.includes(a)) throw new Error(`出现未知 action：${a}`);
  }
});

/* ---------- Round 5b：添加提交路径接线守卫（静态源码断言） ---------- */
// 为什么需要这一组：decideAdd 是纯函数测试，天生测不出「某个调用点忘了调它」。
// 单个添加路径曾直接 `void doAdd([entry])` 绕过跨批次重复检测，而当时所有用例全绿
// —— 纯函数没坏，坏的是接线。因此这里读源码文本，断言提交路径都接到统一入口。
//
// ── 能力边界（务必如实理解，不要高估）──────────────────────────────
// 本组守卫按「函数名 + 字面调用」匹配文本，它防的是**最可能的那一类回归**：
// 把 `submitEntries(x)` 直接改回 `doAdd(x)`（即本批次刚根治的历史缺陷）。
//
// 已知**不在**其覆盖范围内（QA 构造绕过变体实测确认）：
//   ① 别名调用     `const g = doAdd; g([entry])`   —— 文本里没有 `doAdd(` 字面量
//   ② 间接调用     `submitEntries.apply(null,[entry])`
//   ③ 将来新增的第 4 个提交入口（旧的「按名字列举」式守卫列不到它）
//
// ①② 是文本法的天花板，纯静态断言无法可靠识别；根治手段是用真实 React 桩渲染组件、
// 在 api 层放间谍、断言「重复目标绝不能到达 addTargets」的运行时测试（已列入待办）。
// ③ 由下面那条**倒置守卫（白名单式）**补上：它不靠列举函数名，而是复查
// 「全文每一处 doAdd( 是否都落在白名单函数体内」，因此新增入口必然报警。
//
// ⚠️ 另一处有意的取舍：花括号配对 + 标识符名 → 对重命名敏感。
// 重命名 onSubmitSingle / doAdd 时必须同步更新此处；那是一次显式的人工确认，
// 好过悄悄绕过跨批次重复检测。

/**
 * 从源码文本中截出 `const NAME = ...` 声明的文本。
 * 优先做花括号配对；若是无花括号的表达式体（`() => submitEntries(parsed);`），
 * 退化为截到该声明的 `;`。
 */
function extractDecl(src, name) {
  const start = src.indexOf(`const ${name}`);
  if (start < 0) return null;
  const brace = src.indexOf("{", start);
  const semi = src.indexOf(";", start);
  if (brace < 0 || (semi >= 0 && semi < brace)) {
    return semi >= 0 ? src.slice(start, semi + 1) : null;
  }
  let depth = 0;
  for (let i = brace; i < src.length; i++) {
    const c = src[i];
    if (c === "{") depth++;
    else if (c === "}") {
      depth--;
      if (depth === 0) return src.slice(start, i + 1);
    }
  }
  return null;
}

/**
 * 定位一个函数在源码中的覆盖区间 `{ start, end }`。
 * 同时支持两种写法：`const NAME = ...` 与 JSX 属性 `name={() => {...}`
 * （ConfirmDialog 的 onSecondary / onConfirm 属于后者）。
 */
function regionOf(src, name) {
  const constIdx = src.indexOf(`const ${name}`);
  if (constIdx >= 0) {
    const d = extractDecl(src, name);
    if (d) return { start: constIdx, end: constIdx + d.length };
  }
  const jsxIdx = src.indexOf(`${name}={() => {`);
  if (jsxIdx >= 0) {
    const brace = src.indexOf("{", jsxIdx + name.length);
    let depth = 0;
    for (let i = brace; i < src.length; i++) {
      if (src[i] === "{") depth++;
      else if (src[i] === "}") {
        depth--;
        if (depth === 0) return { start: jsxIdx, end: i + 1 };
      }
    }
  }
  return null;
}

const addDlgSrc = fs.readFileSync(path.join(projectRoot, "src", "components", "AddTargetsDialog.tsx"), "utf8");
const addDlgCode = stripComments(addDlgSrc);

check("接线守卫：onSubmitSingle 走 submitEntries，不得直接 doAdd", () => {
  const body = extractDecl(addDlgCode, "onSubmitSingle");
  if (body === null) throw new Error("未找到 onSubmitSingle 声明（被重命名/删除？）");
  if (!body.includes("submitEntries(")) {
    throw new Error("onSubmitSingle 未调用 submitEntries —— 接线断了，跨批次重复检测会被绕过");
  }
  if (body.includes("doAdd(")) {
    throw new Error("onSubmitSingle 直接调用了 doAdd —— 绕过 decideAdd，重复检测失效（E4 回归）");
  }
});

check("接线守卫：onSubmitBatch / onSubmitFile 同样走 submitEntries", () => {
  for (const name of ["onSubmitBatch", "onSubmitFile"]) {
    const body = extractDecl(addDlgCode, name);
    if (body === null) throw new Error(`未找到 ${name} 声明（被重命名/删除？）`);
    if (!body.includes("submitEntries(")) throw new Error(`${name} 未调用 submitEntries —— 接线断了`);
    if (body.includes("doAdd(")) throw new Error(`${name} 直接调用了 doAdd —— 绕过 decideAdd`);
  }
});

check("接线守卫：三条路径都接上（submitEntries( 至少 3 处调用点）", () => {
  const callSites = addDlgCode.match(/submitEntries\s*\(/g) || [];
  // 1 = onSubmitSingle、1 = onSubmitBatch、1 = onSubmitFile；定义处写作 `submitEntries = (` 不计入
  if (callSites.length < 3) {
    throw new Error(`submitEntries( 仅有 ${callSites.length} 处调用点，应 ≥ 3（单个 / 批量 / 文件三条路径）`);
  }
});

check("接线守卫：submitEntries 内部必须调 decideAdd（决策收敛到单一入口）", () => {
  const body = extractDecl(addDlgCode, "submitEntries");
  if (body === null) throw new Error("未找到 submitEntries 声明（被重命名/删除？）");
  if (!body.includes("decideAdd(")) throw new Error("submitEntries 未调用 decideAdd —— 决策逻辑分散了");
});

/* ---------- Round 5c：倒置守卫（白名单式）—— 补上「新增第 4 个入口」的缺口 ---------- */
// 思路反转：上面几条是「黑名单列举」（去三个已知函数里查 doAdd），所以列不到
// **将来新增**的第 4 个提交入口 —— QA 的 V4 变体（新入口直接 doAdd）实测 79/0 全绿漏报，
// 而那复现的正是本批次刚根治的同一个 bug。
//
// 改为白名单：复查「全文每一处 doAdd( 是否都落在允许的函数体内」。
// 新增入口必然不在白名单里 → 必然报警。白名单三项的语义已逐行复核：
//   ① submitEntries —— decideAdd 判定为 add（即无重复）时才提交
//   ② onSecondary  —— 确认弹窗「仍然全部添加 N 个」，用户已显式决策
//   ③ onConfirm    —— 确认弹窗「跳过重复，添加 N 个」，用户已显式决策且已过滤重复项
const DOADD_ALLOWED = ["submitEntries", "onSecondary", "onConfirm"];

check("倒置守卫：doAdd( 的每一处出现都必须落在白名单函数体内", () => {
  const code = addDlgCode;

  // ① 收集 doAdd( 的全部出现位置
  const hits = [];
  const re = /doAdd\s*\(/g;
  let m;
  while ((m = re.exec(code)) !== null) hits.push(m.index);
  if (hits.length === 0) throw new Error("未找到任何 doAdd( 调用点 —— 提交链路可能被整体删除");

  // ② 解析白名单函数覆盖的区间
  const regions = DOADD_ALLOWED.map((n) => {
    const r = regionOf(code, n);
    if (r === null) throw new Error(`未找到白名单函数 ${n}（被重命名？）`);
    return { name: n, start: r.start, end: r.end };
  });

  // ③ 每一处 doAdd( 都必须落在某个白名单区间内
  const strays = [];
  for (const pos of hits) {
    const owner = regions.find((d) => pos >= d.start && pos < d.end);
    if (!owner) {
      const near = code.slice(Math.max(0, pos - 90), pos + 30).replace(/\s*\n\s*/g, " ⏎ ");
      strays.push(`· …${near}…`);
    }
  }
  if (strays.length > 0) {
    throw new Error(
      `doAdd( 出现在白名单之外（新增入口直连 doAdd，绕过 decideAdd）：\n      ${strays.join("\n      ")}`
    );
  }

  // ④ 反向：白名单三项必须真的各含至少一处 doAdd(，否则等于把某条合法提交链路删了
  for (const d of regions) {
    const n = hits.filter((pos) => pos >= d.start && pos < d.end).length;
    if (n === 0) throw new Error(`白名单函数 ${d.name} 内没有任何 doAdd( —— 合法提交链路可能被删除`);
  }
});

check("倒置守卫：三条提交路径必须真的调用 submitEntries（与倒置守卫互为反向）", () => {
  for (const n of ["onSubmitSingle", "onSubmitBatch", "onSubmitFile"]) {
    const body = extractDecl(addDlgCode, n);
    if (body === null) throw new Error(`未找到 ${n}`);
    if (!/submitEntries\s*\(/.test(body)) throw new Error(`${n} 未调用 submitEntries(`);
  }
});

check("接线守卫：不得出现 window.confirm / window.alert（确认交互一律自绘 ConfirmDialog）", () => {
  // 用模块级 stripComments：ConfirmDialog.tsx 的文件头注释里写着「替代 window.confirm」，
  // 直接扫原文会把这行说明误判成违规调用。
  for (const rel of [
    "src/components/AddTargetsDialog.tsx",
    "src/components/ConfirmDialog.tsx",
    "src/components/SettingsDialog.tsx",
    "src/App.tsx",
  ]) {
    const src = stripComments(fs.readFileSync(path.join(projectRoot, ...rel.split("/")), "utf8"));
    for (const banned of ["window.confirm", "window.alert"]) {
      if (src.includes(banned)) throw new Error(`${rel} 出现被禁用的 ${banned}，请改用 ConfirmDialog`);
    }
  }
});

/* ---------- Round 8 补零覆盖：红线 R12 前端守护点（statusRowClass / eventColorClass / isUnreadKind） ---------- */
// 这三个函数此前零测试覆盖，而它们正是红线 R12「绿=正常 / 红=失败」在前端的唯一守护点：
// 改错了没有任何测试会拦你，而后果是「失败行变绿」这类静默失真。

check("statusRowClass 红线 R12：ok=绿 / failed·timeout=红 / resolving=蓝 / idle=灰", () => {
  const want = {
    ok: "bg-emerald-50",
    timeout: "bg-red-50",
    failed: "bg-red-50",
    resolving: "bg-sky-50",
    idle: "bg-slate-50",
  };
  for (const [s, cls] of Object.entries(want)) {
    const got = fmt.statusRowClass(s);
    if (!got.includes(cls)) throw new Error(`statusRowClass(${s}) 应含 ${cls}，实际「${got}」`);
  }
});

check("statusRowClass 红线 R12：ok 绝不可红，failed/timeout 绝不可绿（防静默失真）", () => {
  if (fmt.statusRowClass("ok").includes("bg-red-")) throw new Error("ok 行不得用红底（失败行变绿 / 正常行变红）");
  for (const s of ["failed", "timeout"]) {
    if (fmt.statusRowClass(s).includes("bg-emerald-")) throw new Error(`${s} 行不得用绿底（失败行变绿）`);
  }
  if (fmt.statusRowClass("idle").includes("bg-emerald-")) throw new Error("idle 行不得用绿底");
  if (fmt.statusRowClass("resolving").includes("bg-red-")) throw new Error("resolving 行不得用红底");
});

check("statusRowClass 未知状态回落 idle（不得返回空串，否则行底丢失）", () => {
  const fallback = fmt.statusRowClass("unknown-status");
  if (!fallback.includes("bg-slate-50")) throw new Error(`未知状态应回落 idle 灰底，实际「${fallback}」`);
  if (fallback.trim() === "") throw new Error("未知状态不得返回空串");
});

check("eventColorClass 故障类红 / 恢复类绿 / 其余灰", () => {
  for (const k of ["fault", "unreachable", "dns_fail"]) {
    const got = fmt.eventColorClass(k);
    if (!got.includes("text-red-600")) throw new Error(`eventColorClass(${k}) 应为红色档，实际「${got}」`);
  }
  for (const k of ["recover", "first_ok"]) {
    const got = fmt.eventColorClass(k);
    if (!got.includes("text-emerald-700")) throw new Error(`eventColorClass(${k}) 应为绿色档，实际「${got}」`);
  }
  for (const k of ["start", "stop", "config_change"]) {
    const got = fmt.eventColorClass(k);
    if (!got.includes("text-slate-600")) throw new Error(`eventColorClass(${k}) 应为灰色档，实际「${got}」`);
  }
});

check("isUnreadKind 未读红点口径：故障类计入，recover 不计入", () => {
  for (const k of ["fault", "unreachable", "dns_fail"]) {
    if (fmt.isUnreadKind(k) !== true) throw new Error(`${k} 应计入未读红点`);
  }
  for (const k of ["recover", "first_ok", "start", "stop", "config_change"]) {
    if (fmt.isUnreadKind(k) !== false) throw new Error(`${k} 不应计入未读红点（recover 计入会导致恢复后红点不消）`);
  }
});

check("isUnreadKind 返回值恒为布尔（不返回真值/假值）", () => {
  for (const k of ["fault", "recover", "start", "stop", "config_change", "dns_fail", "unreachable", "first_ok"]) {
    const r = fmt.isUnreadKind(k);
    if (typeof r !== "boolean") throw new Error(`isUnreadKind(${k}) 应返回 boolean，实际 ${typeof r}`);
  }
});

/* ---------- Round 8 补零覆盖：tableToBatchText（Excel/CSV 表头识别） ---------- */
check("tableToBatchText 识别「主机/备注」表头并跳过", () => {
  eq(tableToBatchText([["主机", "备注"], ["1.1.1.1", "阿里"], ["2.2.2.2", "腾讯"]]), "1.1.1.1\t阿里\n2.2.2.2\t腾讯");
});

check("tableToBatchText 表头识别大小写不敏感且兼容英文候选", () => {
  eq(tableToBatchText([["IP", "Name"], ["1.1.1.1", "a"]]), "1.1.1.1\ta", "IP/Name 应识别为表头");
  eq(tableToBatchText([["Host", "备注"], ["1.1.1.1", "a"]]), "1.1.1.1\ta", "Host/备注 应识别为表头");
});

check("tableToBatchText 仅第 1 列命中不算表头（须两列同时命中才跳过）", () => {
  eq(tableToBatchText([["主机", "8.8.8.8"], ["1.1.1.1", "a"]]), "主机\t8.8.8.8\n1.1.1.1\ta");
});

check("tableToBatchText 备注列为空 → 只输出主机名", () => {
  eq(tableToBatchText([["1.1.1.1", ""], ["2.2.2.2"]]), "1.1.1.1\n2.2.2.2");
});

check("tableToBatchText 主机列为空的行被跳过；全空 → 空串", () => {
  eq(tableToBatchText([["1.1.1.1", "a"], ["", "孤行"], ["   ", "空白行"], ["2.2.2.2", "b"]]), "1.1.1.1\ta\n2.2.2.2\tb");
  eq(tableToBatchText([]), "");
  eq(tableToBatchText([["", ""]]), "");
});

check("tableToBatchText 结果可被 parseBatch 无损回读（Excel → 批量文本 → 条目）", () => {
  const text = tableToBatchText([["主机名", "备注名"], ["1.1.1.1", "阿里 DNS"], ["www.baidu.com", ""]]);
  const entries = parseBatch(text);
  eq(entries.length, 2, "应解析出 2 个目标");
  eq(entries[0], { host: "1.1.1.1", name: "阿里 DNS" });
  eq(entries[1], { host: "www.baidu.com", name: "www.baidu.com" }, "空备注应回落为主机名");
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

/* ============ Round 4 固化：界面缩放 / 字体（E3） ============ */
// 依据：zoom 赋值串 = 百分比 / 100，100 或不合法值清空内联样式；
// 字体 = 「"族名", 默认栈」，默认栈必须与 src/styles.css body 的 font-family 一致。
const { zoomStyle, fontFamilyStyle, DEFAULT_FONT_STACK } = fmt;

check("zoomStyle 100 → 空串（清除内联缩放，恢复默认）", () => {
  eq(zoomStyle(100), "");
});

check("zoomStyle 预设档位 → 正确小数字符串", () => {
  eq(zoomStyle(90), "0.9");
  eq(zoomStyle(110), "1.1");
  eq(zoomStyle(125), "1.25");
  eq(zoomStyle(50), "0.5");
  eq(zoomStyle(200), "2");
});

check("zoomStyle 越界值钳制到 50..=200（与后端 normalize_settings 口径一致）", () => {
  eq(zoomStyle(10), "0.5", "低于 50 钳到 50");
  eq(zoomStyle(300), "2", "高于 200 钳到 200");
  eq(zoomStyle(0), "0.5");
  eq(zoomStyle(-125), "0.5");
});

check("zoomStyle 不合法输入 → 空串（不产生 NaN/Infinity 赋值）", () => {
  eq(zoomStyle(NaN), "");
  eq(zoomStyle(undefined), "");
});

check("fontFamilyStyle 空值/空白 → 空串（恢复 styles.css 默认字体栈）", () => {
  eq(fontFamilyStyle(null), "");
  eq(fontFamilyStyle(undefined), "");
  eq(fontFamilyStyle(""), "");
  eq(fontFamilyStyle("   "), "");
});

check("fontFamilyStyle 自定义族名 → 引号族名 + 默认栈兜底", () => {
  eq(
    fontFamilyStyle("Consolas"),
    '"Consolas", "Microsoft YaHei UI", "Microsoft YaHei", "Segoe UI", system-ui, -apple-system, sans-serif'
  );
  // 带首尾空白的族名先 trim
  eq(
    fontFamilyStyle("  宋体  "),
    '"宋体", "Microsoft YaHei UI", "Microsoft YaHei", "Segoe UI", system-ui, -apple-system, sans-serif'
  );
});

check("DEFAULT_FONT_STACK 与 src/styles.css body 字体栈逐字符一致（忽略空白）", () => {
  const css = fs.readFileSync(path.join(projectRoot, "src", "styles.css"), "utf8");
  const m = css.match(/body\s*\{([^}]*)\}/);
  if (!m) throw new Error("styles.css 中未找到 body 规则");
  const stackInCss = m[1].replace(/\s+/g, "");
  const stackConst = DEFAULT_FONT_STACK.replace(/\s+/g, "");
  if (!stackInCss.includes(stackConst)) {
    throw new Error(`字体栈不同步：styles.css 实际「${stackInCss}」 vs 常量「${stackConst}」`);
  }
  if (!stackInCss.includes("font-family:")) {
    throw new Error("body 规则中未找到 font-family 声明");
  }
});

/* ============ Round 6 固化：Delete 键弹层守卫（对话框打开时穿透删除，E5） ============ */
// 依据（原缺陷）：App 的 window keydown 只排除 INPUT/TEXTAREA，对话框打开时按 Delete 会
// 穿透到 deleteSelected() → api.removeTargets 删除并立即落盘，无确认、无撤销。
// 守卫契约：
//   ① 任意弹层打开（dialogOpen=true）→ 一律忽略，与焦点落在哪个元素无关；
//   ② 弹层关闭时，仅 INPUT / TEXTAREA 内打字才忽略；其余元素（含 body / null）Delete 生效。
// dialogOpen 由 App.tsx 取 document.querySelector('[role="dialog"]') !== null，
// 因此「常驻元素带 role=dialog」会让 ① 永久成立 → Delete 键彻底失效（本守卫的最大误伤风险）。
const kb = await loadPure("src/lib/keyboard.ts");
const { shouldIgnoreDeleteKey } = kb;

check("shouldIgnoreDeleteKey：弹层打开 + 焦点在按钮 → 忽略（本次修复核心不变量）", () => {
  eq(shouldIgnoreDeleteKey("BUTTON", true), true, "确认框 / 添加 / 设置弹层打开时 Delete 必须失效");
});

check("shouldIgnoreDeleteKey：弹层关闭 + 焦点在按钮 → 不忽略（正常删除仍可用）", () => {
  eq(shouldIgnoreDeleteKey("BUTTON", false), false, "弹层未打开时按钮上按 Delete 应生效");
});

check("shouldIgnoreDeleteKey：弹层关闭 + INPUT / TEXTAREA → 忽略（打字不得触发删除）", () => {
  eq(shouldIgnoreDeleteKey("INPUT", false), true);
  eq(shouldIgnoreDeleteKey("TEXTAREA", false), true);
});

check("shouldIgnoreDeleteKey：弹层关闭 + 焦点在 body（targetTag 为 null）→ 不忽略", () => {
  eq(shouldIgnoreDeleteKey(null, false), false, "焦点在 body 上时 Delete 应生效");
});

check("shouldIgnoreDeleteKey 属性：dialogOpen=true 时对任意元素一律忽略", () => {
  for (const tag of ["BUTTON", "INPUT", "TEXTAREA", "SELECT", "DIV", "TABLE", "A", "SPAN"]) {
    eq(shouldIgnoreDeleteKey(tag, true), true, `弹层打开时 ${tag} 上的 Delete 必须被忽略`);
  }
  eq(shouldIgnoreDeleteKey(null, true), true, "弹层打开且无焦点元素时也必须忽略");
});

check("shouldIgnoreDeleteKey 属性：dialogOpen=false 时仅 INPUT / TEXTAREA 忽略", () => {
  for (const tag of ["BUTTON", "SELECT", "DIV", "TABLE", "BODY", "A", "SPAN", "LABEL"]) {
    eq(shouldIgnoreDeleteKey(tag, false), false, `弹层关闭时 ${tag} 上的 Delete 应生效`);
  }
  eq(shouldIgnoreDeleteKey("INPUT", false), true);
  eq(shouldIgnoreDeleteKey("TEXTAREA", false), true);
  eq(shouldIgnoreDeleteKey(null, false), false);
});

check("shouldIgnoreDeleteKey：弹层打开优先级高于输入框判定（INPUT / TEXTAREA + open → 忽略）", () => {
  eq(shouldIgnoreDeleteKey("INPUT", true), true);
  eq(shouldIgnoreDeleteKey("TEXTAREA", true), true);
});

check("shouldIgnoreDeleteKey 返回值恒为布尔（不返回 undefined / 真值）", () => {
  for (const tag of [null, "INPUT", "TEXTAREA", "BUTTON"]) {
    for (const open of [true, false]) {
      const r = shouldIgnoreDeleteKey(tag, open);
      if (typeof r !== "boolean") throw new Error(`(${tag}, ${open}) 应返回 boolean，实际 ${typeof r}`);
    }
  }
});

/* ---------- 导出对话框「范围」单选：两项永不同色 ---------- */

const { scopeSelected } = edlg;

check("导出范围：任一状态下恰好一项选中（v1.1.9 两按钮同色的回归防护）", () => {
  // 真实缺陷：v1.1.9 两个按钮的 className 都写成 onlySelected ? chipOn : chipOff，
  // 表现为「点全部节点 -> 两项都灰」「点仅选中 -> 两项都蓝」，用户看不出选的是哪个。
  for (const scope of ["all", "selected"]) {
    const on = ["all", "selected"].filter((id) => scopeSelected(scope, id));
    eq(on, [scope], "scope=" + scope + " 时应恰好只有 " + scope + " 选中");
  }
});

check("导出范围：scopeSelected 恒返回布尔且对自身为 true", () => {
  for (const scope of ["all", "selected"]) {
    eq(scopeSelected(scope, scope), true, scope + " 对自身应为 true");
    for (const id of ["all", "selected"]) {
      const r = scopeSelected(scope, id);
      if (typeof r !== "boolean") throw new Error("(" + scope + "," + id + ") 应返回 boolean");
    }
  }
});

/* ---------- 报表导出筛选（镜像 Rust export::matches_filter） ---------- */

const { matchesExportFilter, exportFilterLabel, exportFilterHint } = fmt;

/** 构造一个最小 TargetState（只需统计字段参与判定） */
const mkTarget = (o = {}) => ({
  id: 1, name: "n", host: "h", enabled: true, resolved_ip: null, status: "idle",
  sent: 0, received: 0, failed: 0,
  last_rtt_ms: null, min_rtt_ms: null, max_rtt_ms: null, avg_rtt_ms: null,
  sum_rtt_ms: 0, ttl: null, consecutive_fail: 0, loss_pct: 0,
  last_success_ts: null, history: [], running: false, last_error: null, events_on: false,
  ...o,
});

/**
 * 与 Rust `export::tests::filter_targets_sample()` **逐台同构**的四台样本。
 * 两侧用同一组数据判定，才能保证「菜单显示的命中台数」与「实际导出的行」一致。
 */
const filterSample = () => [
  mkTarget({ id: 1, name: "零丢包",   status: "ok",     sent: 4, received: 4, failed: 0, loss_pct: 0,   last_success_ts: 1 }),
  mkTarget({ id: 2, name: "部分丢包", status: "ok",     sent: 4, received: 3, failed: 1, loss_pct: 25 }),
  mkTarget({ id: 3, name: "全部失败", status: "timeout", sent: 2, received: 0, failed: 2, loss_pct: 100 }),
  mkTarget({ id: 4, name: "未开始",   status: "idle",   sent: 0, received: 0, failed: 0, loss_pct: 0 }),
];

const names = (ts, f) => ts.filter((t) => matchesExportFilter(t, f)).map((t) => t.name);

check("导出筛选：零丢包只命中全程无失败的那台", () => {
  eq(names(filterSample(), "none_loss"), ["零丢包"]);
});

check("导出筛选：全部未成功只命中 received=0 的那台", () => {
  eq(names(filterSample(), "all_failed"), ["全部失败"]);
});

check("导出筛选：部分丢包（25%）两种口径都不命中", () => {
  const partial = filterSample()[1];
  eq(matchesExportFilter(partial, "none_loss"), false, "有失败即非零丢包");
  eq(matchesExportFilter(partial, "all_failed"), false, "收到过包即非全部未成功");
});

check("导出筛选：sent=0（未开始/已清空统计）两种口径都不命中 🩸", () => {
  // 前提：后端 stats::loss_pct 对 sent==0 定义为 0.0，
  // 若判据写成 failed===0 或 loss_pct===0，未开始的主机会被误判成「零丢包」。
  const idle = filterSample()[3];
  eq(idle.sent, 0, "前提：sent 必须为 0");
  eq(idle.loss_pct, 0, "前提：sent=0 时 loss_pct 也是 0");
  eq(idle.failed, 0, "前提：failed 也是 0");
  eq(matchesExportFilter(idle, "none_loss"), false, "未开始不得算零丢包");
  eq(matchesExportFilter(idle, "all_failed"), false, "未开始不得算全部未成功");
});

check("导出筛选：sent=0 的主机一旦真的 ping 过（全失败）即可命中「全部未成功」", () => {
  const probed = mkTarget({ sent: 1, received: 0, failed: 1, loss_pct: 100, status: "timeout" });
  eq(matchesExportFilter(probed, "all_failed"), true);
  eq(matchesExportFilter(probed, "none_loss"), false);
});

check("导出筛选：sent>0 时两种口径互斥（不得命中同一台）", () => {
  const ts = filterSample();
  const a = names(ts, "none_loss");
  const b = names(ts, "all_failed");
  for (const n of a) {
    if (b.includes(n)) throw new Error(`「${n}」同时命中两种口径，判据互斥性被破坏`);
  }
});

check("导出筛选：all 恒为 true（不筛选 = 原导出行为）", () => {
  eq(names(filterSample(), "all").length, 4, "不过滤时必须导出全部 4 台，含未开始的那台");
});

check("导出筛选：判据只依赖统计字段，不受 status / loss_pct 取值影响", () => {
  // 同一组统计量，status 与 loss_pct 无论怎么写都不该改变判定结果
  const base = { sent: 5, received: 5, failed: 0 };
  for (const status of ["ok", "timeout", "failed", "idle", "resolving"]) {
    for (const loss_pct of [0, 99.9]) {
      const t = mkTarget({ ...base, status, loss_pct });
      eq(matchesExportFilter(t, "none_loss"), true, `status=${status} loss=${loss_pct} 应仍判为零丢包`);
      eq(matchesExportFilter(t, "all_failed"), false, `status=${status} loss=${loss_pct} 不应判为全部未成功`);
    }
  }
});

check("导出筛选：文案 label / hint 三档互不相同且非空", () => {
  const labels = ["all", "none_loss", "all_failed"].map(exportFilterLabel);
  const hints = ["all", "none_loss", "all_failed"].map(exportFilterHint);
  for (const s of [...labels, ...hints]) {
    if (typeof s !== "string" || s.length === 0) throw new Error("文案不得为空");
  }
  eq(new Set(labels).size, 3, "三档 label 必须互不相同");
  eq(new Set(hints).size, 3, "三档 hint 必须互不相同");
});

/* ---------- 文件夹范围筛选（v1.1.10 批次 D） ---------- */

const { inFolderScope, filterByFolderScope, toggleScope, TEMP_AREA_ID } = fmt;

/** 最小 TargetState：只需 folder_id 参与判定 */
const mkT = (id, folderId) => ({
  id, name: "n" + id, host: "h" + id, enabled: true, resolved_ip: null,
  status: "idle", sent: 0, received: 0, failed: 0,
  last_rtt_ms: null, min_rtt_ms: null, max_rtt_ms: null, avg_rtt_ms: null,
  sum_rtt_ms: 0, ttl: null, consecutive_fail: 0, loss_pct: 0,
  last_success_ts: null, history: [], running: false, last_error: null,
  events_on: false, folder_id: folderId,
});

const TS = [mkT(1, null), mkT(2, null), mkT(3, 1), mkT(4, 2)];

check("文件夹范围：一个都不勾 = 全部（反直觉但已定稿的口径）", () => {
  eq(TS.filter((t) => inFolderScope(t, new Set())).length, 4, "空集合必须命中全部");
  eq(filterByFolderScope(TS, new Set()), TS, "空集合过滤应原样返回");
});

check("文件夹范围：folder_id 为 null 归入 id 0（临时区）", () => {
  eq(TEMP_AREA_ID, 0, "临时区哨兵必须是 0");
  const scope = new Set([TEMP_AREA_ID]);
  eq(TS.filter((t) => inFolderScope(t, scope)).map((t) => t.id), [1, 2], "临时区两台");
});

check("文件夹范围：勾选某文件夹只返回该文件夹内的主机", () => {
  const scope = new Set([1]);
  eq(TS.filter((t) => inFolderScope(t, scope)).map((t) => t.id), [3], "机房1 只有 1 台");
});

check("文件夹范围：多选取并集（临时区 + 机房1 = 3 台）", () => {
  const scope = new Set([TEMP_AREA_ID, 1]);
  eq(TS.filter((t) => inFolderScope(t, scope)).map((t) => t.id), [1, 2, 3], "并集应为 1、2、3");
});

check("文件夹范围：勾选不存在的文件夹 = 无命中（而非退回全部）", () => {
  const scope = new Set([999]);
  eq(TS.filter((t) => inFolderScope(t, scope)).length, 0, "未知文件夹应为空");
});

check("toggleScope：切换勾选且不改入参", () => {
  const a = new Set([1]);
  const b = toggleScope(a, 2);
  eq(Array.from(a).sort(), [1], "入参不得被修改");
  eq(Array.from(b).sort(), [1, 2], "应新增 2");
  eq(Array.from(toggleScope(b, 1)).sort(), [2], "再次点击应取消勾选");
  eq(b.size, 2, "返回的是新集合，原集合不变");
});

check("文件夹范围：filterByFolderScope 不改原数组", () => {
  eq(filterByFolderScope(TS, new Set([1])).length, 1);
  eq(TS.length, 4, "原数组长度不应变");
});
/* ---------- 清空列表只清临时区（v1.1.10 批次 E） ---------- */

const { tempAreaIds, tempAreaCount } = fmt;

check("清空列表：只返回临时区（folder_id === null）的主机 🩸", () => {
  // 前提：TS[0]/TS[1] 是临时区（folder_id=null），TS[2]/TS[3] 在文件夹里
  eq(TS.map((t) => t.folder_id), [null, null, 1, 2], "前提：样本归属应如上");
  eq(tempAreaIds(TS), [1, 2], "只应返回临时区那两台");
  eq(tempAreaCount(TS), 2);
});

check("清空列表：文件夹内的 IP 一个都不能被删掉", () => {
  const inFolder = TS.filter((t) => t.folder_id !== null).map((t) => t.id);
  const toDelete = tempAreaIds(TS);
  for (const id of inFolder) {
    if (toDelete.includes(id)) {
      throw new Error("主机 " + id + " 在文件夹里却被列入删除清单，会误删用户整理好的 IP");
    }
  }
});

check("清空列表：全是文件夹主机时删除清单为空（而不是退回全删）", () => {
  const onlyFolder = TS.filter((t) => t.folder_id !== null);
  eq(tempAreaIds(onlyFolder), [], "无临时区主机时不得返回任何 id");
  eq(tempAreaCount(onlyFolder), 0);
});

check("清空列表：空列表返回空数组", () => {
  eq(tempAreaIds([]), []);
  eq(tempAreaCount([]), 0);
});

check("清空列表：folder_id 为 0 视为文件夹而非临时区", () => {
  // 侧边栏里「临时区」用 id 0 代表，但目标上的 folder_id 为 null 才算临时区。
  // 两者不得混淆：folder_id === 0 的目标仍属于某个真实文件夹。
  const t0 = { ...mkT(9, 0) };
  eq(tempAreaIds([t0]), [], "folder_id=0 不等于临时区，不得被清空列表删掉");
  eq(tempAreaCount([t0]), 0);
});
/* ---------- C2 导出范围跟随侧边栏（v1.1.10 批次 F） ---------- */

const { exportScopeIds } = fmt;

/** 4 台：id1/2 在文件夹 1，id3 在文件夹 2，id4 在临时区（folder_id=null） */
const scopeSample = () => [
  { id: 1, folder_id: 1 }, { id: 2, folder_id: 1 },
  { id: 3, folder_id: 2 }, { id: 4, folder_id: null },
];

check("C2 导出范围：侧边栏未勾选任何文件夹时返回 null（= 全部）🩸", () => {
  // null 在后端语义是「导出全部」。若这里返回全部 id 数组，v1.1.9 的
  // 「未勾选 -> 导出全部」路径就会产生额外差异，破坏兼容性。
  eq(exportScopeIds(scopeSample(), new Set()), null, "范围为空必须返回 null");
});

check("C2 导出范围：勾选某文件夹后只导出该文件夹的主机", () => {
  eq(exportScopeIds(scopeSample(), new Set([1])), [1, 2]);
  eq(exportScopeIds(scopeSample(), new Set([2])), [3]);
});

check("C2 导出范围：多选文件夹时取并集", () => {
  eq(exportScopeIds(scopeSample(), new Set([1, 2])), [1, 2, 3]);
});

check("C2 导出范围：勾选「临时区」(id 0) 导出 folder_id 为 null 的主机", () => {
  eq(exportScopeIds(scopeSample(), new Set([0])), [4]);
});

check("C2 导出范围：结果顺序与传入顺序一致（导出序号才稳定）", () => {
  const shuffled = [{ id: 9, folder_id: 1 }, { id: 7, folder_id: 1 }];
  eq(exportScopeIds(shuffled, new Set([1])), [9, 7], "不得自行排序或反序");
});

check("C2 导出范围：勾选的文件夹为空（无主机）时返回空数组而非 null", () => {
  // 空数组 = 明确导出 0 台；null = 全部。两者语义不同，不得混淆。
  eq(exportScopeIds(scopeSample(), new Set([99])), []);
});

check("C2 导出范围：空目标列表 + 空范围 = null（仍是全部，不报错）", () => {
  eq(exportScopeIds([], new Set()), null);
});

check("C2 导出范围：结果不含范围外的任何主机 🩸", () => {
  const s = new Set([1]);
  const got = exportScopeIds(scopeSample(), s) ?? [];
  for (const t of scopeSample()) {
    if ((t.folder_id ?? 0) !== 1) {
      if (got.includes(t.id)) {
        throw new Error("主机 " + t.id + " 不在所选文件夹内却被列入导出清单");
      }
    }
  }
});
/* ---------- 侧边栏勾选语义（v1.1.10 bug 回归） ---------- */



check("侧边栏：scope 为空 = 一个都不勾 = 显示全部节点", () => {
  const ts = [{ id: 1, folder_id: 1 }, { id: 2, folder_id: null }];
  eq(ts.filter((t) => inFolderScope(t, new Set())).length, 2, "空 scope 必须放行全部");
});

check("侧边栏：勾选一个文件夹时，勾选框只应如实反映 scope 🩸", () => {
  // 回归：曾用 `allChecked || scope.has(id)`，导致 scope 为空时把「全部」
  // 画成「全勾」。于是「回到全部」与「初始状态」长得一模一样 ——
  // 用户点两次文件夹，看到临时区和文件夹同时被勾选，却没意识到自己已回到全部。
  const scope = new Set([1]);
  const on = (id) => scope.has(id);          // 修复后的语义
  eq(on(1), true, "所选文件夹应勾选");
  eq(on(0), false, "临时区不应被顺带勾上");
  eq(on(2), false, "其他文件夹不应被顺带勾上");
});

check("侧边栏：取消勾选 = 清空 scope，不得用逐个 toggle 🩸", () => {
  // 回归：曾用 folders.forEach((f) => onToggle(f.folder.id))。
  // scope={文件夹1} 时，forEach 会先 toggle 临时区（把它勾上）再 toggle 文件夹1，
  // 结果得到 {临时区} —— 「取消勾选」后反而只剩临时区被选中。
  const scope = new Set([1]);
  const buggy = new Set(scope);
  for (const id of [0, 1, 2]) {
    if (!buggy.delete(id)) buggy.add(id);
  }
  // scope={文件夹1} 时逐个 toggle：文件夹1 被移除，但**没勾的** 0 与 2 被加入
  eq([...buggy].sort(), [0, 2], "旧实现把没勾的文件夹也勾上了（这正是 bug）");
  eq(new Set().size, 0, "正确实现是清空 -> 空集合");
});

check("侧边栏：反复点击同一文件夹回到「未筛选」，且不残留其他 id", () => {
  let scope = new Set();
  scope = fmt.toggleScope(scope, 1);
  eq([...scope], [1], "点一次 -> 只勾它");
  scope = fmt.toggleScope(scope, 1);
  eq([...scope].length, 0, "再点一次 -> 回到未筛选，不得残留任何 id");
});

check("侧边栏：新建文件夹后勾选它，临时区不得被连带勾上", () => {
  // 新建后 folders = [临时区(0), 新文件夹(1)]，scope 仍为空（=未筛选）。
  // 用户点新文件夹期望「只看它」，实际必须只有它被勾。
  let scope = new Set();
  scope = fmt.toggleScope(scope, 1);
  const checked = [0, 1].filter((id) => scope.has(id));
  eq(checked, [1], "只能勾中新文件夹");
});
/* ------------------------------ 结果汇总 ------------------------------ */
console.log(`\n===== 前端纯函数测试：通过 ${pass} / 失败 ${failures.length} =====`);
for (const f of failures) {
  console.log(`FAIL  ${f.name}\n      ${f.message}`);
}
process.exit(failures.length === 0 ? 0 : 1);
