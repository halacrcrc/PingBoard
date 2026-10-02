# 报表导出筛选设计（零丢包 / 全部未成功）

> 版本：v1.1.9-dev · 2026-10-02 · 面向「改导出相关代码前必读」
> 落地范围：`src-tauri/src/export.rs`、`src-tauri/src/commands.rs`、`src/lib/format.ts`、
> `src/lib/api.ts`、`src/components/ExportDialog.tsx`、`src/components/Toolbar.tsx`、`src/App.tsx`

---

## 一、需求与结论

在**多 ping 监控已有统计结果**的前提下，导出报表时可选择只导出以下两类节点（两种**独立**、可分别执行）：

| 口径 | 含义 |
|---|---|
| **零丢包** | 该节点在本次统计周期内**一次都没有丢过包** |
| **全部未成功** | 该节点在本次统计周期内**所有 ping 一次都没有成功过** |

**关键前提澄清**：本项目**没有「一轮 ping 结束」的概念**。它是持续监控 —— 每个目标一个独立
工作线程（`state.rs:686` `worker_loop`），每轮 `probe` 一次并把结果累加进 `TargetState` 的
`sent / received / failed / loss_pct`，直到用户点「停止全部」或「清空统计」。
所以「多 ping 结束后的导出」在本实现中的确切语义是：

> **基于累计统计量（`sent` / `received` / `failed`）筛选**，
> 与现有报表导出口径完全一致 —— 现有 CSV 本来就导出这三列。

---

## 二、筛选逻辑的判定标准（唯一权威）

后端 `export::matches_filter`（`src-tauri/src/export.rs`）是**导出结果的唯一权威判据**：

```rust
pub fn matches_filter(t: &TargetState, f: ExportFilter) -> bool {
    match f {
        ExportFilter::All        => true,
        ExportFilter::NoneLoss   => t.sent > 0 && t.failed == 0,
        ExportFilter::AllFailed  => t.sent > 0 && t.received == 0,
    }
}
```

### 2.1 判定表

| 节点状态 | `sent` | `received` | `failed` | `loss_pct` | 零丢包 | 全部未成功 |
|---|---|---|---|---|---|---|
| 全部成功 | 4 | 4 | 0 | 0.0 | ✅ | ❌ |
| 部分丢包 | 4 | 3 | 1 | 25.0 | ❌ | ❌ |
| 全部失败 | 2 | 0 | 2 | 100.0 | ❌ | ✅ |
| **从未探测 / 统计已清空** | **0** | 0 | 0 | **0.0** | ❌ | ❌ |

### 2.2 🩸 三个必须记住的判定要点

1. **两种口径都必须加 `sent > 0`。**
   `sent == 0` 表示「从未 ping 过」或「统计已被清空」。此时 `received` 与 `failed` 同为 0，
   而 `stats::loss_pct`（`stats.rs:4`）**对 `sent == 0` 明确定义返回 `0.0`**。
   因此若把判据写成 `failed == 0` 或 `loss_pct == 0.0`，**从未开始的主机会被大量误判成「零丢包」** ——
   而它根本没有发生过任何一次 ping。这是本功能最容易踩的坑，已用变异测试锁定（见 §六）。

2. **两种口径在 `sent > 0` 时互斥。**
   零丢包要求 `received == sent > 0`，全部未成功要求 `received == 0`，二者不可能同时成立。
   所以不存在「同一台被两个筛选重复导出」的问题。

3. **判定只看统计量，不看 `status`、不看 `loss_pct` 字段本身。**
   `status` 表达的是**最近一次**探测的结果（可回退），`loss_pct` 是派生展示值；
   判定必须用累计的 `sent / received / failed` 三个原始计数。

### 2.3 与既有口径的关系

- 零丢包 ⇔ 前端「丢包率」列的 `loss_pct > 0 ? 红 : 常规`（`TargetTable.tsx:228`）中的「不着红」分支，
  且 `DetailPanel.tsx:136` 注释已写明「与丢包率同规则：0 次失败不着红」。**本实现与该口径一致。**
- 判定**不引入** `consecutive_fail`（连续失败次数）。理由：需求是「全部 ping 均未成功」
  （累计口径），而非「当前连续失败」（瞬时口径）。`consecutive_fail` 在一次成功后即清零（`stats.rs:50`），
  语义与需求不符。若将来要「当前连续失败 N 次」口径，应另立一档，不与本功能混用。

---

## 三、导出文件格式与字段结构

### 3.1 兼容性第一原则

> **筛选只改变导出的「行集合」，绝不改变「字段结构」。**

CSV 仍是固定 **15 列**、HTML 仍是 **15 个 `<th>`**、TXT 仍是每目标 4 行 + 汇总头。
用户既有的 Excel 模板、解析脚本、`qa` 断言**全部无需改动**。

### 3.2 CSV（`pingboard-report[-后缀].csv`）

- 编码 **UTF-8 with BOM**（`EF BB BF`，Excel 打开中文不乱码）
- 分隔符半角 `,`；转义规则：字段含 `,` `"` `\n` `\r` 时整体加双引号，内部 `"` → `""`（`esc_csv`）
- 换行符 `\n`
- **表头固定 15 列**（不随筛选变化）：

| # | 列名 | 来源字段 | 说明 |
|---|---|---|---|
| 1 | 序号 | — | **筛选后从 1 重新连续编号** |
| 2 | 备注名 | `name` | |
| 3 | 主机名 | `host` | |
| 4 | IP地址 | `resolved_ip` | 未解析出为空串 |
| 5 | 状态 | `status` | `未开始/解析中/正常/超时/失败` |
| 6 | 发送 | `sent` | |
| 7 | 接收 | `received` | |
| 8 | 失败 | `failed` | |
| 9 | 丢包率(%) | `loss_pct` | 1 位小数 |
| 10 | 最小延迟(ms) | `min_rtt_ms` | 无值空串 |
| 11 | 平均延迟(ms) | `avg_rtt_ms` | 无值空串 |
| 12 | 最大延迟(ms) | `max_rtt_ms` | 无值空串 |
| 13 | 最近延迟(ms) | `last_rtt_ms` | 无值空串 |
| 14 | TTL | `ttl` | 无值空串 |
| 15 | 最后成功时间(UTC) | `last_success_ts` | 无值空串 |

- **0 命中时**：仍写出带 BOM 的文件，内容为「表头 + 0 数据行」，结构合法（非残缺文件）。

### 3.3 TXT（`pingboard-report[-后缀].txt`）

无表头定宽文本。筛选导出时在**汇总区追加一行**：

```
主机数：1  总发包：4  总收包：4  总丢包：0  丢包率：0.0%
筛选口径：零丢包 —— 仅 sent>0 且 failed=0（从未丢包）
```

注意「主机数」是**筛选后**的行数，不是总数。`不过滤` 时**不追加**该行。

### 3.4 HTML（`pingboard-report[-后缀].html`）

单 `<table>`，**15 个 `<th>` 不变**。筛选说明只加进汇总 `<div class="summary">`：

```html
… 丢包率：0.0% &nbsp;|&nbsp; 筛选：零丢包（仅 sent&gt;0 且 failed=0（从未丢包））
```

- 判定式含 `>` ，**必须过 `esc_html`**；但 `&nbsp;` 分隔符在 `esc_html` **之外**拼接 ——
  否则 `&` 会被转成 `&amp;` 而退化成字面量 `&nbsp;`。
- 行 `class` 仍按 `status` 着色（`ok/bad/idle`），筛选不改变着色规则。

### 3.5 事件 CSV（`export_events`）—— **本次未改动**

详情面板「导出 CSV」走的是另一条命令 `export_events`（8 列：BOM + 序号/会话/时间(本地)/主机/备注/事件类型/等级/说明），
**不在本次筛选范围内**。若后续要给事件导出加筛选，需另立口径（事件是离散记录，没有「丢包率」概念）。

---

## 四、交互设计

### 4.1 为什么用对话框而不是下拉菜单

筛选口径 3 种 × 格式 3 种 × 范围 2 种 = 18 个组合，直接塞进工具栏「导出 ▾」下拉会撑到不可用。
`ExportDialog` 把三个维度**正交排布**，并提供**命中台数实时预览**（用户在点「导出」前就知道会导出几台）。

### 4.2 入口与原有流程的关系

工具栏「导出 ▾」下拉现有 **6 个菜单项一字未改**（导出全部 ×3 / 仅导出选中 ×3），
新增第 3 组：

```
按结果筛选
  零丢包 / 全部未成功…      ← 新增，点击打开 ExportDialog
```

**原有导出流程完全不受影响**：那 6 条路径的请求体、默认文件名（`pingboard-report.<ext>`）、
提示文案、文件内容全部保持不变。

### 4.3 命中台数预览的权威性说明

对话框里的「N 台」由**前端** `format.ts` 的 `matchesExportFilter` 计算，
它与 Rust `export::matches_filter` 是**同一判定的两个实现**：

- 前端那份**只用于预览计数，不参与写文件** → 即使两侧漂移，也**不会产生错误的文件内容**，
  最坏只是计数显示不准。
- 为防漂移，两侧测试用**逐台同构的同一组四台样本**
  （Rust `filter_targets_sample()` / 前端 `filterSample()`）断言相同的命中结果。
  **改判据必须两侧同时改。**

---

## 五、接口与兼容性

### 5.1 命令签名变更

```rust
// src-tauri/src/commands.rs
pub async fn export_report(
    state: State<'_, AppState>,
    path: String,
    format: String,
    ids: Option<Vec<u64>>,
    filter: Option<String>,   // ← 新增
) -> Result<(), String>
```

Tauri 命令总数仍为 **19**（未新增命令，只加参数），注册清单 `lib.rs` 不变。

### 5.2 兼容性保证（逐条）

| 项 | 保证 |
|---|---|
| `filter` 缺省 / `null` / `""` / `"all"` | 一律解析为 `ExportFilter::All`，**产出与本功能上线前逐字节一致** |
| 未知 `filter` 值 | **返回 Err 且不写文件**（先 `parse_filter` 再 `write_report`），不静默回退成全量 |
| 大小写 / 首尾空白 | `trim().to_lowercase()` 容错，`" NONE_LOSS "` 合法 |
| 原有 6 条菜单路径 | 请求体、默认文件名、toast 文案、文件内容全部不变 |
| 原有 `ids`（仅选中）语义 | 未变。`ids` 先过滤出集合，`filter` 再在其中筛选，**两者可叠加** |
| TXT / HTML 汇总区 | `All` 时不追加任何筛选说明行 / 段 |
| 事件导出 `export_events` | 完全未改动 |
| 配置格式 | **未新增任何配置字段**，不触碰 serde 配置契约（红线 R1） |

### 5.3 默认文件名

| 口径 | 默认文件名 |
|---|---|
| 不过滤 | `pingboard-report.<ext>`（**与改动前完全一致**） |
| 零丢包 | `pingboard-report-no-loss.<ext>` |
| 全部未成功 | `pingboard-report-all-failed.<ext>` |

### 5.4 红线遵守情况

- **R2 契约**：`Status` serde 行为未动；命令数未变，仅加可选参数。
- **B1（基线复审 🔴-2 同类）**：导出**仍然走无副作用的 `AppState::export_targets()`**，
  **没有**换成 `snapshot()`（后者会 `drain_pending()` 吃掉事件增量）。筛选是纯后端计算，不新增任何跨层调用。
- **R4**：`ExportDialog` 用自绘对话框，未引入 `window.confirm` / `alert`。
- **R5**：`ExportDialog` 的两个 `useEffect` 均写在 `if (!open) return null` **之前**。
- **R6**：`wasOpenRef` 模式初始化草稿，未把 `open` 放进 effect 依赖。
- **R7**：`ExportDialog` 用 `flex items-center justify-center` 居中，**不使用** `getBoundingClientRect()`
  数值定位 → 不受 `ui_scale` zoom 影响。
- **UI 铁律**：单行区域全部带 `whitespace-nowrap`；次要文字统一 `text-slate-500 dark:text-slate-400`；
  绿=正常（零丢包）/ 红=失败（全部未成功），未套股票涨红跌绿；**未隐藏或合并任何原有功能入口**。
- **B7（CSV 公式注入）**：属**既有未销**项，本次**未扩大也未修复** `esc_csv` 行为，保持原样。

---

## 六、测试与变异验证

### 6.1 Rust（`export.rs` `mod tests`，新增 **11** 条）

| # | 用例 | 锁住的行为 |
|---|---|---|
| 1 | `qa_filter_none_loss_keeps_only_fully_clean` | 零丢包只命中 `sent>0 && failed==0` |
| 2 | `qa_filter_all_failed_keeps_only_never_succeeded` | 全部未成功只命中 `sent>0 && received==0` |
| 3 | 🩸 `qa_filter_never_probed_excluded_from_both` | **核心防回归**：`sent==0` 两种口径都不得命中；并验证「真 ping 过但全失败」可命中 |
| 4 | `qa_filter_modes_are_mutually_exclusive` | 两种口径互斥，不重复导出同一台 |
| 5 | `qa_filter_all_returns_everything` | `All` 恒等全量（含未开始的那台） |
| 6 | `qa_filter_csv_keeps_15_columns_and_matching_rows` | 真实落盘：表头仍 15 列、行数=命中数、序号重编、异类主机不出现 |
| 7 | `qa_filter_txt_and_html_annotate_criteria` | TXT/HTML 汇总区含口径与判定式；HTML 中 `>` 正确转义 |
| 8 | `qa_filter_all_leaves_no_criteria_marker` | 兼容性回归：`All` 时**无任何**筛选标记，且仍导出全部 4 台 |
| 9 | `qa_filter_zero_match_writes_valid_file` | 0 命中仍写「BOM + 表头 + 0 行」的结构合法文件 |
| 10 | `qa_parse_filter_accepts_all_and_rejects_unknown` | `None`/`""`/`all`/大小写/空白容错；未知值报错 |
| 11 | `qa_invalid_filter_writes_nothing` | 非法筛选不产出文件（先解析后落盘） |

**实测结果**：`cargo test --lib` = **154 passed / 0 failed / 0 ignored**（143 基线 + 11）。

### 6.2 前端（`tests/frontend_pure.test.mjs`，81 → **90**）

新增 9 条，含与 Rust 同构四台样本的镜像断言、`sent==0` 边界、互斥性、
「判据不受 `status` / `loss_pct` 取值影响」属性测试、文案三档互异。**实测 90 passed / 0 failed**。

### 6.3 变异验证（按 `docs/code-review.md` §6「回滚到旧写法就失败」）

**已实测**：把前端判据从 `t.sent > 0 && t.failed === 0` 改成 `t.failed === 0` 后：

```
===== 前端纯函数测试：通过 88 / 失败 2 =====
FAIL  导出筛选：零丢包只命中全程无失败的那台
      断言失败：期望 ["零丢包"]，实际 ["零丢包","未开始"]
FAIL  导出筛选：sent=0（未开始/已清空统计）两种口径都不命中 🩸
      未开始不得算零丢包：期望 false，实际 true
```

**精确复现了 §2.2 要防的误判**（未开始被当成零丢包），证明测试真的锁住了行为。已还原，复跑 90/0。

### 6.4 Rust 侧编译与测试：已通过 ✅

> 记录一次环境受限：本轮实现时沙箱拒绝运行 `cargo.exe`
> （`The path cannot be traversed because it contains an untrusted mount point`，
> `cmd /c` 与绝对路径调用均失败；`node` 正常，故非权限通病），
> 因此当时 `export.rs` **未经编译器验证**。随后在正常环境补跑：

```
cd d:\pinginfo\src-tauri && cargo test --lib
```

```
test result: ok. 154 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.27s
```

`export.rs` 编译通过、11 条新用例全绿。同期 `npx tsc --noEmit` 0 error、`npm test` 90 passed / 0 failed。

---

## 七、待办 / 后续可扩展点

1. **人工 QA 筛选导出**（端到端实测两种口径），并按 `docs/code-review.md` §4.4 走发布前全量走查。
2. **B7：CSV 公式注入未防**（基线复审遗留）。筛选导出沿用同一 `esc_csv`，
   若修复应**一并覆盖**筛选路径。
3. **事件导出筛选**（`export_events`）尚未支持，事件无「丢包率」概念，需另立口径。
4. **「当前连续失败 N 次」口径**如需支持，应新增独立档位（见 §2.3 第 3 条），不要复用本功能。
5. 筛选与「清空统计」的交互：清空统计后 `sent` 归零，所有节点从两种筛选中**同时消失**，
   符合 §2.2 要点 1 的定义，但用户可能困惑，建议在 UI 上补一句说明或考虑禁用。
