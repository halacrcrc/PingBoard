// 报表导出：csv / txt / html，均在 Rust 侧写文件
use crate::events::LogEvent;
use crate::model::{Status, TargetState};

/// 导出报表到指定路径
pub fn write_report(path: &str, format: &str, targets: &[TargetState]) -> Result<(), String> {
    let content = match format.to_lowercase().as_str() {
        "csv" => to_csv(targets),
        "txt" => to_txt(targets),
        "html" => to_html(targets),
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
fn to_csv(targets: &[TargetState]) -> String {
    let mut out = String::from("\u{feff}");
    out.push_str(
        "序号,备注名,主机名,IP地址,状态,发送,接收,失败,丢包率(%),最小延迟(ms),平均延迟(ms),最大延迟(ms),最近延迟(ms),TTL,最后成功时间(UTC)\n",
    );
    for (i, t) in targets.iter().enumerate() {
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
fn to_events_csv(events: &[LogEvent], tz_offset_minutes: i64) -> String {
    let mut out = String::from("\u{feff}");
    out.push_str("序号,时间(本地),主机,备注,事件类型,等级,说明\n");
    for (i, e) in events.iter().enumerate() {
        out.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            i + 1,
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

/// 纪元毫秒 → 本地时间字符串：在既有 UTC 民用历算法上叠加时区偏移（东 8 区取 +480 分钟）
fn fmt_time_local(ms: u64, tz_offset_minutes: i64) -> String {
    let shifted = ms as i64 + tz_offset_minutes.saturating_mul(60_000);
    let shifted = if shifted < 0 { 0 } else { shifted as u64 };
    fmt_time_utc(Some(shifted))
}

/// 生成纯文本报表
fn to_txt(targets: &[TargetState]) -> String {
    let total_sent: u64 = targets.iter().map(|t| t.sent).sum();
    let total_recv: u64 = targets.iter().map(|t| t.received).sum();
    let total_fail: u64 = targets.iter().map(|t| t.failed).sum();
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
        targets.len(),
        total_sent,
        total_recv,
        total_fail,
        loss
    ));
    out.push_str("------------------------------------------------\n");
    for (i, t) in targets.iter().enumerate() {
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
fn to_html(targets: &[TargetState]) -> String {
    let total_sent: u64 = targets.iter().map(|t| t.sent).sum();
    let total_recv: u64 = targets.iter().map(|t| t.received).sum();
    let total_fail: u64 = targets.iter().map(|t| t.failed).sum();
    let loss = if total_sent == 0 {
        0.0
    } else {
        total_fail as f64 / total_sent as f64 * 100.0
    };

    let mut rows = String::new();
    for (i, t) in targets.iter().enumerate() {
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
<div class="summary">生成时间(UTC)：{gen} &nbsp;|&nbsp; 主机数：{cnt} &nbsp;|&nbsp; 总发包：{sent} &nbsp;|&nbsp; 总收包：{recv} &nbsp;|&nbsp; 总丢包：{fail} &nbsp;|&nbsp; 丢包率：{loss:.1}%</div>
<table><thead><tr>
<th>序号</th><th>备注名</th><th>主机名</th><th>IP 地址</th><th>状态</th><th>发送</th><th>接收</th><th>失败</th><th>丢包率</th><th>最小(ms)</th><th>平均(ms)</th><th>最大(ms)</th><th>最近(ms)</th><th>TTL</th><th>最后成功时间(UTC)</th>
</tr></thead><tbody>
{rows}</tbody></table></body></html>
"#,
        gen = fmt_time_utc(Some(crate::state::now_ms())),
        cnt = targets.len(),
        sent = total_sent,
        recv = total_recv,
        fail = total_fail,
        loss = loss,
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
        let csv = to_csv(&[t]);
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
        assert!(write_report("nul-nonexistent", "xml", &[]).is_err());
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
        write_report(&p, "csv", &sample_targets()).expect("写 CSV 失败");

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
        write_report(&tp, "txt", &targets).expect("写 TXT 失败");
        let txt = std::fs::read_to_string(&txt_path).expect("读回 TXT 失败");
        assert!(txt.contains("PingBoard 报表"));
        assert!(txt.contains("中文备注"));
        assert!(txt.contains("不可达"));

        let html_path = dir.join("report.html");
        let hp = html_path.to_string_lossy().to_string();
        write_report(&hp, "html", &targets).expect("写 HTML 失败");
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
        let err = write_report(&p, "xml", &[]).unwrap_err();
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

    fn sample_event(seq: u64, host: &str, name: &str, kind: crate::events::EventKind) -> LogEvent {
        LogEvent {
            seq,
            ts: 1_700_000_000_000,
            target_id: 1,
            target_name: name.to_string(),
            target_host: host.to_string(),
            kind,
            level: crate::events::EventLevel::for_kind(kind),
            text: "测试,带逗号".to_string(),
        }
    }

    /// 事件 CSV：BOM + 7 列表头 + 本地时间列 + 转义
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
        assert_eq!(count_csv_fields(lines[0]), 7, "事件 CSV 应为 7 列");
        for (i, l) in lines[1..].iter().enumerate() {
            assert_eq!(count_csv_fields(l), 7, "第 {} 行列数不一致", i + 1);
        }
        assert!(body.contains("\"阿里, DNS\""), "含逗号字段应被引号包裹");
        assert!(body.contains("\"测试,带逗号\""), "说明字段含逗号应被转义");
        assert!(body.contains("故障"));
        assert!(body.contains("已恢复"));
        // 本地时间：东 8 区应为 2023-11-15 06:13:20
        assert!(body.contains("2023-11-15 06:13:20"), "应为本地时间：{}", body);
    }

    /// 时区偏移换算：0 → UTC；+480 → +8h；-480 → -8h
    #[test]
    fn qa_fmt_time_local_offsets() {
        let ts = 1_700_000_000_000u64;
        assert_eq!(fmt_time_local(ts, 0), "2023-11-14 22:13:20", "0 偏移即 UTC");
        assert_eq!(fmt_time_local(ts, 480), "2023-11-15 06:13:20", "东 8 区 +8h");
        assert_eq!(fmt_time_local(ts, -480), "2023-11-14 14:13:20", "西 8 区 -8h");
    }
}
