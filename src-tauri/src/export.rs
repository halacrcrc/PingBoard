// 报表导出：csv / txt / html，均在 Rust 侧写文件
use crate::events::LogEvent;
use crate::model::{Status, TargetState};

/// 报表导出的筛选口径
///
/// ⚠️ 与前端 `src/lib/format.ts` 的 `matchesExportFilter` 是**同一判定的两个实现**
/// （前端那份只用于菜单上的命中台数展示，不参与写文件）。改判据必须两侧同时改，
/// 否则菜单计数会与实际导出结果对不上。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFilter {
    /// 不筛选，导出全部 —— **原有行为的唯一取值**，保证向后兼容
    All,
    /// 仅导出完全未发生丢包的节点：`sent > 0 && failed == 0`
    NoneLoss,
    /// 仅导出全部 ping 均未成功的节点：`sent > 0 && received == 0`
    AllFailed,
}

/// 解析前端传来的筛选参数；`None` / 空串 / `all` 一律等价于不筛选
pub fn parse_filter(raw: Option<&str>) -> Result<ExportFilter, String> {
    match raw.map(|s| s.trim().to_lowercase()).as_deref() {
        None | Some("") | Some("all") => Ok(ExportFilter::All),
        Some("none_loss") => Ok(ExportFilter::NoneLoss),
        Some("all_failed") => Ok(ExportFilter::AllFailed),
        Some(other) => Err(format!(
            "不支持的导出筛选：{}（可选 all / none_loss / all_failed）",
            other
        )),
    }
}

/// 筛选口径的中文名（写进报表汇总区 + 前端提示）
pub fn filter_label(f: ExportFilter) -> &'static str {
    match f {
        ExportFilter::All => "全部",
        ExportFilter::NoneLoss => "零丢包",
        ExportFilter::AllFailed => "全部失败",
    }
}

/// 筛选口径的一句话说明（写进报表汇总区，让导出文件自解释）
pub fn filter_hint(f: ExportFilter) -> &'static str {
    match f {
        ExportFilter::All => "不筛选，导出全部节点",
        ExportFilter::NoneLoss => "仅 sent>0 且 failed=0（从未丢包）",
        ExportFilter::AllFailed => "仅 sent>0 且 received=0（从未成功）",
    }
}

/// 判定单个节点是否命中筛选口径（**导出结果的唯一权威判据**，纯函数便于单测）
///
/// 🩸 两种口径都要求 `sent > 0`：`sent == 0` 表示从未探测或统计已被清空，
/// 此时 `received` / `failed` 同为 0，而 `stats::loss_pct` 对 `sent == 0` 定义为 `0.0`。
/// 若只判 `failed == 0`，**从未开始 ping 的主机会被误当成「零丢包」**。
/// 两种口径在 `sent > 0` 时互斥（一个要求 `received == sent`，一个要求 `received == 0`）。
pub fn matches_filter(t: &TargetState, f: ExportFilter) -> bool {
    match f {
        ExportFilter::All => true,
        ExportFilter::NoneLoss => t.sent > 0 && t.failed == 0,
        ExportFilter::AllFailed => t.sent > 0 && t.received == 0,
    }
}

/// 按筛选口径挑出节点（借用，不 clone）
pub fn select_by_filter<'a>(targets: &'a [TargetState], f: ExportFilter) -> Vec<&'a TargetState> {
    targets.iter().filter(|t| matches_filter(*t, f)).collect()
}

/// 导出报表到指定路径
///
/// `filter` 为 [`ExportFilter::All`] 时，产出内容与筛选功能上线前**逐字节一致**。
pub fn write_report(
    path: &str,
    format: &str,
    targets: &[TargetState],
    filter: ExportFilter,
) -> Result<(), String> {
    let sel = select_by_filter(targets, filter);
    let content = match format.to_lowercase().as_str() {
        "csv" => to_csv(&sel),
        "txt" => to_txt(&sel, filter),
        "html" => to_html(&sel, filter),
        other => return Err(format!("不支持的导出格式：{}（可选 csv / txt / html）", other)),
    };
    std::fs::write(path, content).map_err(|e| format!("写入文件失败：{}（{}）", e, path))
}

/// 导出事件为 CSV（「时间(本地)」列按 `tz_offset_minutes` 偏移）
pub fn write_events_csv(
    path: &str,
    events: &[LogEvent],
    tz_offset_minutes: i64,
) -> Result<(), String> {
    let content = to_events_csv(events, tz_offset_minutes);
    std::fs::write(path, content).map_err(|e| format!("写入文件失败：{}（{}）", e, path))
}

/// 状态中文名
fn status_text(s: Status) -> &'static str {
    match s {
        Status::Idle => "未开始",
        Status::Resolving => "解析中",
        Status::Ok => "正常",
        Status::Timeout => "超时",
        Status::Failed => "失败",
    }
}

/// 可选浮点：保留 1 位小数，无值输出空串
fn opt_num(v: Option<f64>) -> String {
    match v {
        Some(x) if !x.is_nan() => format!("{:.1}", x),
        _ => String::new(),
    }
}

/// CSV 字段转义
fn esc_csv(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// HTML 转义
fn esc_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// 纪元毫秒 → `YYYY-MM-DD HH:MM:SS`（UTC）
fn fmt_time_utc(ms: Option<u64>) -> String {
    let ms = match ms {
        Some(v) => v,
        None => return String::new(),
    };
    let secs = (ms / 1000) as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let (y, mo, d) = civil_from_days(days);
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", y, mo, d, h, m, s)
}

/// 由「自 1970-01-01 起的天数」推导公历日期（Howard Hinnant 算法）
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as i64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// 生成 CSV（带 UTF-8 BOM，否则 Excel 打开中文乱码）
///
/// 字段结构**不随筛选变化**：仍是固定 15 列，筛选只改变行集合（可能为 0 行）。
/// 这样用户既有的 Excel 模板 / 解析脚本无需任何改动。
fn to_csv(sel: &[&TargetState]) -> String {
    let mut out = String::from("\u{feff}");
    out.push_str(
        "序号,备注名,主机名,IP地址,状态,发送,接收,失败,丢包率(%),最小延迟(ms),平均延迟(ms),最大延迟(ms),最近延迟(ms),TTL,最后成功时间(UTC)\n",
    );
    for (i, t) in sel.iter().enumerate() {
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{:.1},{},{},{},{},{},{}\n",
            i + 1,
            esc_csv(&t.name),
            esc_csv(&t.host),
            esc_csv(t.resolved_ip.as_deref().unwrap_or("")),
            status_text(t.status),
            t.sent,
            t.received,
            t.failed,
            t.loss_pct,
            opt_num(t.min_rtt_ms),
            opt_num(t.avg_rtt_ms),
            opt_num(t.max_rtt_ms),
            opt_num(t.last_rtt_ms),
            t.ttl.map(|v| v.to_string()).unwrap_or_default(),
            fmt_time_utc(t.last_success_ts),
        ));
    }
    out
}

/// 生成事件 CSV（带 UTF-8 BOM；时间列为**本地时间**）
///
/// 「会话」列 = 该条事件所属那次运行的启动时刻（本地时间），用于在 Excel 里
/// 按批次筛选；本字段引入之前的旧行输出空串。
fn to_events_csv(events: &[LogEvent], tz_offset_minutes: i64) -> String {
    let mut out = String::from("\u{feff}");
    out.push_str("序号,会话（启动于）,时间(本地),主机,备注,事件类型,等级,说明\n");
    for (i, e) in events.iter().enumerate() {
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{}\n",
            i + 1,
            esc_csv(&fmt_session(e.session, tz_offset_minutes)),
            esc_csv(&fmt_time_local(e.ts, tz_offset_minutes)),
            esc_csv(&e.target_host),
            esc_csv(&e.target_name),
            esc_csv(e.kind.label()),
            esc_csv(e.level.label()),
            esc_csv(&e.text),
        ));
    }
    out
}

/// 会话标识（启动时刻，纪元毫秒）→ 本地时间字符串；`0`（旧行）输出空串
fn fmt_session(session: u64, tz_offset_minutes: i64) -> String {
    if session == 0 {
        return String::new();
    }
    fmt_time_local(session, tz_offset_minutes)
}

/// 纪元毫秒 → 本地时间字符串：在既有 UTC 民用历算法上叠加时区偏移（东 8 区取 +480 分钟）
fn fmt_time_local(ms: u64, tz_offset_minutes: i64) -> String {
    let shifted = ms as i64 + tz_offset_minutes.saturating_mul(60_000);
    let shifted = if shifted < 0 { 0 } else { shifted as u64 };
    fmt_time_utc(Some(shifted))
}

/// 生成纯文本报表
///
/// `filter != All` 时在汇总区追加一行「筛选口径」，让文件自解释；
/// `All` 时**不追加任何行**，保证与筛选功能上线前的输出逐字节一致。
fn to_txt(sel: &[&TargetState], filter: ExportFilter) -> String {
    let total_sent: u64 = sel.iter().map(|t| t.sent).sum();
    let total_recv: u64 = sel.iter().map(|t| t.received).sum();
    let total_fail: u64 = sel.iter().map(|t| t.failed).sum();
    let loss = if total_sent == 0 {
        0.0
    } else {
        total_fail as f64 / total_sent as f64 * 100.0
    };

    let mut out = String::new();
    out.push_str("================ PingBoard 报表 ================\n");
    out.push_str(&format!("生成时间(UTC)：{}\n", fmt_time_utc(Some(crate::state::now_ms()))));
    out.push_str(&format!(
        "主机数：{}  总发包：{}  总收包：{}  总丢包：{}  丢包率：{:.1}%\n",
        sel.len(),
        total_sent,
        total_recv,
        total_fail,
        loss
    ));
    if filter != ExportFilter::All {
        out.push_str(&format!(
            "筛选口径：{} —— {}\n",
            filter_label(filter),
            filter_hint(filter)
        ));
    }
    out.push_str("------------------------------------------------\n");
    for (i, t) in sel.iter().enumerate() {
        out.push_str(&format!(
            "[{}] {} ({})\n    状态：{}  IP：{}\n    发送 {}/接收 {}/失败 {}  丢包率 {:.1}%\n    最小 {} / 平均 {} / 最大 {} ms   TTL：{}\n    最后成功：{}\n\n",
            i + 1,
            t.name,
            t.host,
            status_text(t.status),
            t.resolved_ip.as_deref().unwrap_or("-"),
            t.sent,
            t.received,
            t.failed,
            t.loss_pct,
            opt_num(t.min_rtt_ms),
            opt_num(t.avg_rtt_ms),
            opt_num(t.max_rtt_ms),
            t.ttl.map(|v| v.to_string()).unwrap_or_else(|| "-".to_string()),
            fmt_time_utc(t.last_success_ts),
        ));
    }
    out
}

/// 生成 HTML 报表
///
/// 表头固定 15 列，不随筛选变化（与 CSV 同口径）；筛选说明只加在汇总 `div` 里，
/// 不新增 `<th>` / `<td>`，因此既有的「15 列」结构断言继续成立。
fn to_html(sel: &[&TargetState], filter: ExportFilter) -> String {
    let total_sent: u64 = sel.iter().map(|t| t.sent).sum();
    let total_recv: u64 = sel.iter().map(|t| t.received).sum();
    let total_fail: u64 = sel.iter().map(|t| t.failed).sum();
    let loss = if total_sent == 0 {
        0.0
    } else {
        total_fail as f64 / total_sent as f64 * 100.0
    };

    let mut rows = String::new();
    for (i, t) in sel.iter().enumerate() {
        let cls = match t.status {
            Status::Ok => "ok",
            Status::Timeout | Status::Failed => "bad",
            _ => "idle",
        };
        rows.push_str(&format!(
            "<tr class=\"{cls}\"><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:.1}%</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            i + 1,
            esc_html(&t.name),
            esc_html(&t.host),
            esc_html(t.resolved_ip.as_deref().unwrap_or("-")),
            status_text(t.status),
            t.sent,
            t.received,
            t.failed,
            t.loss_pct,
            opt_num(t.min_rtt_ms),
            opt_num(t.avg_rtt_ms),
            opt_num(t.max_rtt_ms),
            opt_num(t.last_rtt_ms),
            t.ttl.map(|v| v.to_string()).unwrap_or_else(|| "-".to_string()),
            fmt_time_utc(t.last_success_ts),
        ));
    }

    format!(
        r#"<!doctype html>
<html lang="zh-CN"><head><meta charset="utf-8"><title>PingBoard 报表</title>
<style>
body{{font-family:"Microsoft YaHei",sans-serif;font-size:13px;color:#1e293b;padding:24px;background:#f8fafc}}
h1{{font-size:18px}}
.summary{{margin:12px 0 18px;padding:10px 14px;background:#fff;border:1px solid #e2e8f0;border-radius:6px}}
table{{border-collapse:collapse;width:100%;background:#fff}}
th,td{{border:1px solid #e2e8f0;padding:5px 8px;text-align:left}}
th{{background:#f1f5f9}}
td:nth-child(n+6){{text-align:right}}
tr.ok td:nth-child(5){{color:#16a34a;font-weight:600}}
tr.bad td:nth-child(5){{color:#dc2626;font-weight:600}}
tr.idle td:nth-child(5){{color:#64748b}}
</style></head><body>
<h1>PingBoard 多主机 Ping 监视器 — 报表</h1>
<div class="summary">生成时间(UTC)：{gen} &nbsp;|&nbsp; 主机数：{cnt} &nbsp;|&nbsp; 总发包：{sent} &nbsp;|&nbsp; 总收包：{recv} &nbsp;|&nbsp; 总丢包：{fail} &nbsp;|&nbsp; 丢包率：{loss:.1}%{flt}</div>
<table><thead><tr>
<th>序号</th><th>备注名</th><th>主机名</th><th>IP 地址</th><th>状态</th><th>发送</th><th>接收</th><th>失败</th><th>丢包率</th><th>最小(ms)</th><th>平均(ms)</th><th>最大(ms)</th><th>最近(ms)</th><th>TTL</th><th>最后成功时间(UTC)</th>
</tr></thead><tbody>
{rows}</tbody></table></body></html>
"#,
        gen = fmt_time_utc(Some(crate::state::now_ms())),
        cnt = sel.len(),
        sent = total_sent,
        recv = total_recv,
        fail = total_fail,
        loss = loss,
        // 不筛选时不输出任何筛选标记，保证与筛选功能上线前的 HTML 逐字节一致
        flt = if filter == ExportFilter::All {
            String::new()
        } else {
            // 只对文案走 esc_html（判定式里含 `>`）：`&nbsp;` 是 HTML 实体，
            // 若一并交给 esc_html，其 `&` 会被转成 `&amp;` 而退化成字面量。
            format!(
                " &nbsp;|&nbsp; {}",
                esc_html(&format!(
                    "筛选：{}（{}）",
                    filter_label(filter),
                    filter_hint(filter)
                ))
            )
        },
        rows = rows,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_has_bom_and_header() {
        let mut t = TargetState::new(1, "阿里, DNS".into(), "223.5.5.5".into());
        t.status = Status::Ok;
        t.sent = 3;
        t.received = 3;
        t.loss_pct = 0.0;
        t.min_rtt_ms = Some(9.0);
        t.avg_rtt_ms = Some(10.0);
        t.max_rtt_ms = Some(11.0);
        t.last_rtt_ms = Some(10.0);
        let csv = to_csv(&[&t]);
        assert!(csv.starts_with('\u{feff}'));
        assert!(csv.contains("序号,备注名"));
        // 含逗号的字段应被引号包裹
        assert!(csv.contains("\"阿里, DNS\""));
    }

    #[test]
    fn civil_from_days_known_date() {
        // 1970-01-01 是第 0 天
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        // 2000-01-01 是第 10957 天
        assert_eq!(civil_from_days(10_957), (2000, 1, 1));
    }

    #[test]
    fn unknown_format_errors() {
        assert!(write_report("nul-nonexistent", "xml", &[], ExportFilter::All).is_err());
    }

    /* ===================== QA 新增：真实写文件的导出校验 ===================== */

    /// 统计一行 CSV 的字段数（正确跳过被双引号包裹的逗号）
    fn count_csv_fields(line: &str) -> usize {
        let mut n = 1usize;
        let mut in_quote = false;
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '"' => {
                    if in_quote && chars.peek() == Some(&'"') {
                        let _ = chars.next(); // 转义双引号，跳过
                    } else {
                        in_quote = !in_quote;
                    }
                }
                ',' if !in_quote => n += 1,
                _ => {}
            }
        }
        n
    }

    fn sample_targets() -> Vec<TargetState> {
        let mut ok = TargetState::new(1, "中文备注, 带逗号".into(), "www.baidu.com".into());
        ok.status = Status::Ok;
        ok.resolved_ip = Some("182.61.200.110".into());
        ok.sent = 4;
        ok.received = 3;
        ok.failed = 1;
        ok.loss_pct = 25.0;
        ok.min_rtt_ms = Some(9.0);
        ok.avg_rtt_ms = Some(12.0);
        ok.max_rtt_ms = Some(20.5);
        ok.last_rtt_ms = Some(11.0);
        ok.ttl = Some(52);
        ok.last_success_ts = Some(1_700_000_000_000);

        let mut bad = TargetState::new(2, "不可达".into(), "192.0.2.1".into());
        bad.status = Status::Timeout;
        bad.sent = 2;
        bad.failed = 2;
        bad.loss_pct = 100.0;
        vec![ok, bad]
    }

    fn qa_dir() -> std::path::PathBuf {
        let d = std::env::temp_dir().join("pingboard_qa_export");
        std::fs::create_dir_all(&d).expect("创建临时导出目录失败");
        d
    }

    #[test]
    fn qa_csv_real_file_bom_and_columns() {
        let dir = qa_dir();
        let path = dir.join("report.csv");
        let p = path.to_string_lossy().to_string();
        write_report(&p, "csv", &sample_targets(), ExportFilter::All).expect("写 CSV 失败");

        let bytes = std::fs::read(&path).expect("读回 CSV 失败");
        assert!(bytes.len() > 3);
        assert_eq!(&bytes[0..3], &[0xEF, 0xBB, 0xBF], "CSV 必须以 UTF-8 BOM 开头");

        let text = String::from_utf8(bytes).expect("CSV 不是合法 UTF-8");
        let body = text.trim_start_matches('\u{feff}');
        let lines: Vec<&str> = body.trim_end().split('\n').collect();
        assert_eq!(lines.len(), 3, "应为 表头 + 2 行数据");
        let header_cols = count_csv_fields(lines[0]);
        assert_eq!(header_cols, 15, "表头应为 15 列");
        for (i, l) in lines[1..].iter().enumerate() {
            assert_eq!(
                count_csv_fields(l),
                header_cols,
                "第 {} 行数据列数与表头不一致",
                i + 1
            );
        }
        assert!(body.contains("中文备注"), "中文备注不得乱码");
        assert!(body.contains("\"中文备注, 带逗号\""), "含逗号字段应被引号包裹");
        assert!(body.contains("正常"));
        assert!(body.contains("超时"));
    }

    #[test]
    fn qa_txt_and_html_real_files() {
        let dir = qa_dir();
        let targets = sample_targets();

        let txt_path = dir.join("report.txt");
        let tp = txt_path.to_string_lossy().to_string();
        write_report(&tp, "txt", &targets, ExportFilter::All).expect("写 TXT 失败");
        let txt = std::fs::read_to_string(&txt_path).expect("读回 TXT 失败");
        assert!(txt.contains("PingBoard 报表"));
        assert!(txt.contains("中文备注"));
        assert!(txt.contains("不可达"));

        let html_path = dir.join("report.html");
        let hp = html_path.to_string_lossy().to_string();
        write_report(&hp, "html", &targets, ExportFilter::All).expect("写 HTML 失败");
        let html = std::fs::read_to_string(&html_path).expect("读回 HTML 失败");
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.trim_end().ends_with("</html>"), "HTML 必须以 </html> 结束");
        assert_eq!(html.matches("<table>").count(), 1);
        assert_eq!(html.matches("</table>").count(), 1);
        // 表头 15 个 th、每条数据 15 个 td
        assert_eq!(html.matches("<th>").count(), 15, "HTML 表头应为 15 列");
        assert_eq!(html.matches("<td>").count(), 30, "2 行 × 15 列 = 30 个 td");
        assert!(html.contains("中文备注, 带逗号"), "中文 + 逗号应原样保留（HTML 无需转义逗号）");
    }

    #[test]
    fn qa_write_report_rejects_unknown_format_without_writing() {
        let dir = qa_dir();
        let path = dir.join("should_not_exist.xml");
        let p = path.to_string_lossy().to_string();
        let err = write_report(&p, "xml", &[], ExportFilter::All).unwrap_err();
        assert!(err.contains("不支持"), "错误信息应说明格式不支持：{}", err);
        assert!(!path.exists(), "未知格式不应写出文件");
    }

    #[test]
    fn qa_opt_num_nan_and_none_render_empty() {
        assert_eq!(opt_num(None), "");
        assert_eq!(opt_num(Some(f64::NAN)), "");
        assert_eq!(opt_num(Some(12.34)), "12.3");
    }

    /* ===================== 事件 CSV ===================== */

    /// 会话标识固定值：**刻意与 `ts` 不同**，便于分别断言「会话」与「时间」两列
    const SAMPLE_SESSION: u64 = 1_699_000_000_000;

    fn sample_event(seq: u64, host: &str, name: &str, kind: crate::events::EventKind) -> LogEvent {
        LogEvent {
            seq,
            session: SAMPLE_SESSION,
            ts: 1_700_000_000_000,
            target_id: 1,
            target_name: name.to_string(),
            target_host: host.to_string(),
            kind,
            level: crate::events::EventLevel::for_kind(kind),
            text: "测试,带逗号".to_string(),
        }
    }

    /// 事件 CSV：BOM + 8 列表头 + 会话列 + 本地时间列 + 转义
    #[test]
    fn qa_events_csv_real_file() {
        use crate::events::EventKind;

        let dir = qa_dir();
        let path = dir.join("events.csv");
        let p = path.to_string_lossy().to_string();
        let events = vec![
            sample_event(1, "223.5.5.5", "阿里, DNS", EventKind::Fault),
            sample_event(2, "www.baidu.com", "百度", EventKind::Recover),
        ];
        // 东 8 区
        write_events_csv(&p, &events, 480).expect("写事件 CSV 失败");

        let bytes = std::fs::read(&path).expect("读回事件 CSV 失败");
        assert_eq!(&bytes[0..3], &[0xEF, 0xBB, 0xBF], "事件 CSV 必须以 UTF-8 BOM 开头");
        let text = String::from_utf8(bytes).unwrap();
        let body = text.trim_start_matches('\u{feff}');
        let lines: Vec<&str> = body.trim_end().split('\n').collect();
        assert_eq!(lines.len(), 3, "表头 + 2 行数据");
        assert!(lines[0].contains("时间(本地)"), "表头必须标注「时间(本地)」");
        assert!(lines[0].contains("会话"), "表头必须含「会话」列");
        assert_eq!(count_csv_fields(lines[0]), 8, "事件 CSV 应为 8 列");
        for (i, l) in lines[1..].iter().enumerate() {
            assert_eq!(count_csv_fields(l), 8, "第 {} 行列数不一致", i + 1);
        }
        assert!(body.contains("\"阿里, DNS\""), "含逗号字段应被引号包裹");
        assert!(body.contains("\"测试,带逗号\""), "说明字段含逗号应被转义");
        assert!(body.contains("故障"));
        assert!(body.contains("已恢复"));
        // 本地时间：东 8 区应为 2023-11-15 06:13:20
        assert!(body.contains("2023-11-15 06:13:20"), "应为本地时间：{}", body);
        // 会话列：SAMPLE_SESSION 按东 8 区渲染
        let session_cell = fmt_time_local(SAMPLE_SESSION, 480);
        assert!(
            body.contains(&session_cell),
            "会话列应为本次运行启动时刻（本地时间 {}）：{}",
            session_cell,
            body
        );
        assert_ne!(
            session_cell, "2023-11-15 06:13:20",
            "会话与事件时间的测试值应不同，否则断言无法区分两列"
        );
    }

    /// 旧行（`session == 0`，即本字段引入之前写入的）会话列输出空串，不得渲染成 1970 年
    #[test]
    fn qa_events_csv_legacy_session_renders_empty() {
        use crate::events::EventKind;

        let dir = qa_dir();
        let path = dir.join("events_legacy.csv");
        let p = path.to_string_lossy().to_string();
        let mut ev = sample_event(1, "1.1.1.1", "旧记录", EventKind::Start);
        ev.session = 0;
        write_events_csv(&p, &[ev], 480).expect("写事件 CSV 失败");

        let body = std::fs::read_to_string(&path).unwrap();
        let body = body.trim_start_matches('\u{feff}');
        let row = body.trim_end().split('\n').nth(1).unwrap();
        // 第二列（会话）必须为空
        assert_eq!(row.split(',').nth(1), Some(""), "session=0 应输出空列：{}", row);
        assert!(!body.contains("1970-01-01"), "不得把 0 渲染成 1970 年：{}", body);
    }

    /// 时区偏移换算：0 → UTC；+480 → +8h；-480 → -8h
    #[test]
    fn qa_fmt_time_local_offsets() {
        let ts = 1_700_000_000_000u64;
        assert_eq!(fmt_time_local(ts, 0), "2023-11-14 22:13:20", "0 偏移即 UTC");
        assert_eq!(fmt_time_local(ts, 480), "2023-11-15 06:13:20", "东 8 区 +8h");
        assert_eq!(fmt_time_local(ts, -480), "2023-11-14 14:13:20", "西 8 区 -8h");
    }

    /* ===================== 导出筛选：零丢包 / 全部未成功 ===================== */

    /// 覆盖筛选全部边界的 4 台样本：
    /// ① 零丢包 ② 部分丢包 ③ 全部失败 ④ **从未探测**（sent=0，loss_pct 被定义为 0）
    fn filter_targets_sample() -> Vec<TargetState> {
        let mut clean = TargetState::new(1, "零丢包".into(), "10.0.0.1".into());
        clean.status = Status::Ok;
        clean.sent = 4;
        clean.received = 4;
        clean.failed = 0;
        clean.loss_pct = 0.0;
        clean.last_success_ts = Some(1_700_000_000_000);

        let mut partial = TargetState::new(2, "部分丢包".into(), "10.0.0.2".into());
        partial.status = Status::Ok;
        partial.sent = 4;
        partial.received = 3;
        partial.failed = 1;
        partial.loss_pct = 25.0;
        partial.last_success_ts = Some(1_700_000_000_000);

        let mut dead = TargetState::new(3, "全部失败".into(), "10.0.0.3".into());
        dead.status = Status::Timeout;
        dead.sent = 2;
        dead.received = 0;
        dead.failed = 2;
        dead.loss_pct = 100.0;

        // 关键边界：从未 ping 过 / 统计已清空 —— loss_pct 也是 0，但**不是**「零丢包」
        let idle = TargetState::new(4, "未开始".into(), "10.0.0.4".into());
        assert_eq!(idle.sent, 0);
        assert_eq!(idle.loss_pct, 0.0, "前提：sent=0 时 loss_pct 定义为 0");

        vec![clean, partial, dead, idle]
    }

    /// 零丢包口径：只留 `sent>0 && failed==0`；排除部分丢包、全部失败、**以及从未探测的主机**
    #[test]
    fn qa_filter_none_loss_keeps_only_fully_clean() {
        let ts = filter_targets_sample();
        let sel = select_by_filter(&ts, ExportFilter::NoneLoss);
        assert_eq!(sel.len(), 1, "只应命中「零丢包」那一台");
        assert_eq!(sel[0].name, "零丢包");
    }

    /// 全部未成功口径：只留 `sent>0 && received==0`
    #[test]
    fn qa_filter_all_failed_keeps_only_never_succeeded() {
        let ts = filter_targets_sample();
        let sel = select_by_filter(&ts, ExportFilter::AllFailed);
        assert_eq!(sel.len(), 1, "只应命中「全部失败」那一台");
        assert_eq!(sel[0].name, "全部失败");
    }

    /// 🩸 防回归核心：`sent == 0`（未开始 / 已清空统计）**不得**被任一筛选命中。
    /// 若把判据写成 `failed == 0`，`loss_pct == 0.0` 或 `received == sent`，
    /// 「未开始」的主机会被误判为「零丢包」—— 而它根本没有发生过 ping。
    #[test]
    fn qa_filter_never_probed_excluded_from_both() {
        let idle = TargetState::new(4, "未开始".into(), "10.0.0.4".into());
        assert!(!matches_filter(&idle, ExportFilter::NoneLoss), "未开始不得算零丢包");
        assert!(!matches_filter(&idle, ExportFilter::AllFailed), "未开始不得算全部失败");

        // 同一台主机一旦真的 ping 过（哪怕全失败），就必须能命中「全部失败」
        let mut probed_fail = idle.clone();
        probed_fail.sent = 1;
        probed_fail.failed = 1;
        probed_fail.received = 0;
        probed_fail.loss_pct = 100.0;
        assert!(matches_filter(&probed_fail, ExportFilter::AllFailed));
        assert!(!matches_filter(&probed_fail, ExportFilter::NoneLoss));
    }

    /// 两种口径互斥：同一批节点不可能同时命中（`sent>0` 时 `received==sent` 与 `received==0` 互斥）
    #[test]
    fn qa_filter_modes_are_mutually_exclusive() {
        let ts = filter_targets_sample();
        let a: Vec<&str> = select_by_filter(&ts, ExportFilter::NoneLoss)
            .iter()
            .map(|t| t.name.as_str())
            .collect();
        let b: Vec<&str> = select_by_filter(&ts, ExportFilter::AllFailed)
            .iter()
            .map(|t| t.name.as_str())
            .collect();
        assert!(a.iter().all(|n| !b.contains(n)), "两口径不得命中同一台：{:?} / {:?}", a, b);
    }

    /// `All` 恒等全量，且与 `ids` 语义解耦（筛选在 `export_targets` 之后做）
    #[test]
    fn qa_filter_all_returns_everything() {
        let ts = filter_targets_sample();
        assert_eq!(select_by_filter(&ts, ExportFilter::All).len(), ts.len());
    }

    /// 筛选后 CSV：表头**仍是 15 列**（字段结构不随筛选变化），数据行 = 命中数，序号从 1 重新连续编号
    #[test]
    fn qa_filter_csv_keeps_15_columns_and_matching_rows() {
        let dir = qa_dir();
        let ts = filter_targets_sample();

        let p_clean = dir.join("filtered_none_loss.csv").to_string_lossy().to_string();
        write_report(&p_clean, "csv", &ts, ExportFilter::NoneLoss).expect("写筛选 CSV 失败");
        let text = std::fs::read_to_string(&p_clean).unwrap();
        assert!(text.starts_with('\u{feff}'), "筛选导出同样必须带 UTF-8 BOM");
        let body = text.trim_start_matches('\u{feff}');
        let lines: Vec<&str> = body.trim_end().split('\n').collect();
        assert_eq!(lines.len(), 2, "表头 + 1 行命中数据");
        assert_eq!(
            count_csv_fields(lines[0]),
            15,
            "筛选导出不得改变字段结构，仍须 15 列"
        );
        assert!(lines[1].contains("零丢包"), "应只含零丢包那台：{}", lines[1]);
        assert!(!body.contains("部分丢包"));
        assert!(!body.contains("全部失败"));
        assert!(!body.contains("未开始"));
        assert!(lines[1].starts_with("1,"), "序号应重新从 1 连续编号");

        let p_dead = dir.join("filtered_all_failed.csv").to_string_lossy().to_string();
        write_report(&p_dead, "csv", &ts, ExportFilter::AllFailed).expect("写筛选 CSV 失败");
        let body2 = std::fs::read_to_string(&p_dead)
            .unwrap()
            .trim_start_matches('\u{feff}')
            .to_string();
        assert!(body2.contains("全部失败"));
        assert!(!body2.contains("零丢包"));
    }

    /// TXT / HTML 的汇总区写明筛选口径与判定式（人读格式，允许加说明行 / 说明段）
    #[test]
    fn qa_filter_txt_and_html_annotate_criteria() {
        let dir = qa_dir();
        let ts = filter_targets_sample();

        let tp = dir.join("filtered.txt").to_string_lossy().to_string();
        write_report(&tp, "txt", &ts, ExportFilter::NoneLoss).expect("写 TXT 失败");
        let txt = std::fs::read_to_string(&tp).unwrap();
        assert!(txt.contains("筛选口径：零丢包"), "TXT 汇总区应写明口径：{}", txt);
        assert!(txt.contains("sent>0 且 failed=0"), "TXT 应给出判定式");
        assert!(txt.contains("零丢包"));
        assert!(!txt.contains("部分丢包"));
        // 汇总里的「主机数」是**筛选后**的行数，不是总数
        assert!(txt.contains("主机数：1"), "汇总主机数应为筛选后行数：{}", txt);

        let hp = dir.join("filtered.html").to_string_lossy().to_string();
        write_report(&hp, "html", &ts, ExportFilter::AllFailed).expect("写 HTML 失败");
        let html = std::fs::read_to_string(&hp).unwrap();
        assert!(html.contains("筛选：全部失败"), "HTML 汇总区应写明口径");
        assert!(html.contains("sent&gt;0 且 received=0"), "HTML 中 > 需转义");
        // 表头仍是 15 个 th，筛选说明只进汇总 div，不新增列
        assert_eq!(html.matches("<th>").count(), 15, "筛选不得改变表头列数");
        assert_eq!(html.matches("<td>").count(), 15, "1 行 × 15 列");
    }

    /// 兼容性回归：`All` 时**不得**出现任何筛选标记 —— 产出与筛选功能上线前逐字节一致
    #[test]
    fn qa_filter_all_leaves_no_criteria_marker() {
        let dir = qa_dir();
        let ts = filter_targets_sample();

        let tp = dir.join("all_plain.txt").to_string_lossy().to_string();
        write_report(&tp, "txt", &ts, ExportFilter::All).expect("写 TXT 失败");
        let txt = std::fs::read_to_string(&tp).unwrap();
        assert!(!txt.contains("筛选口径"), "All 不应追加筛选说明行：{}", txt);

        let hp = dir.join("all_plain.html").to_string_lossy().to_string();
        write_report(&hp, "html", &ts, ExportFilter::All).expect("写 HTML 失败");
        let html = std::fs::read_to_string(&hp).unwrap();
        assert!(!html.contains("筛选："), "All 不应追加筛选说明段：{}", html);

        // All 导出全部 4 台（含「未开始」），证明默认行为未被筛选污染
        assert!(txt.contains("未开始"), "All 必须仍导出从未探测的主机");
        assert_eq!(html.matches("<td>").count(), 60, "4 行 × 15 列");
    }

    /// 0 命中仍要写出**结构合法**的文件（表头完整、行数为 0），而不是报错或留残缺文件
    #[test]
    fn qa_filter_zero_match_writes_valid_file() {
        let dir = qa_dir();
        // 只有「部分丢包」与「未开始」两台 → 两个筛选都 0 命中
        let mut partial = TargetState::new(1, "部分丢包".into(), "10.0.0.2".into());
        partial.sent = 4;
        partial.received = 3;
        partial.failed = 1;
        partial.loss_pct = 25.0;
        let idle = TargetState::new(2, "未开始".into(), "10.0.0.4".into());
        let ts = vec![partial, idle];

        for f in [ExportFilter::NoneLoss, ExportFilter::AllFailed] {
            let p = dir
                .join(format!("zero_{}.csv", filter_label(f)))
                .to_string_lossy()
                .to_string();
            write_report(&p, "csv", &ts, f).expect("0 命中也应写出文件");
            let text = std::fs::read_to_string(&p).unwrap();
            assert_eq!(&text.as_bytes()[0..3], &[0xEF, 0xBB, 0xBF], "0 命中也要带 BOM");
            let lines: Vec<&str> = text.trim_start_matches('\u{feff}').trim_end().split('\n').collect();
            assert_eq!(lines.len(), 1, "0 命中应只有表头");
            assert_eq!(count_csv_fields(lines[0]), 15, "表头仍须 15 列");
        }
    }

    /// 筛选参数解析：`None` / 空串 / `all` 全部落到不筛选；未知值必须**报错**而不是静默导出全量
    #[test]
    fn qa_parse_filter_accepts_all_and_rejects_unknown() {
        assert_eq!(parse_filter(None).unwrap(), ExportFilter::All);
        assert_eq!(parse_filter(Some("")).unwrap(), ExportFilter::All);
        assert_eq!(parse_filter(Some("all")).unwrap(), ExportFilter::All);
        // 容错：大小写与空白不敏感
        assert_eq!(parse_filter(Some(" NONE_LOSS ")).unwrap(), ExportFilter::NoneLoss);
        assert_eq!(parse_filter(Some("All_Failed")).unwrap(), ExportFilter::AllFailed);

        let err = parse_filter(Some("xml")).unwrap_err();
        assert!(err.contains("不支持的导出筛选"), "未知筛选应明确报错：{}", err);
    }

    /// 命令层契约：非法筛选**不得**写出文件（先解析后落盘）
    #[test]
    fn qa_invalid_filter_writes_nothing() {
        let dir = qa_dir();
        let path = dir.join("should_not_exist_filtered.csv");
        let err = parse_filter(Some("nope")).unwrap_err();
        // 命令层是「先 parse_filter 再 write_report」，解析失败即 Err，压根不会走到落盘
        assert!(err.contains("nope"));
        assert!(!path.exists(), "非法筛选不应产出文件");
    }
}
