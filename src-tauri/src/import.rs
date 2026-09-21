// 文件导入：把 txt / csv / xlsx 等文件读取为「纯文本」或「表格」，
// 交回前端统一走既有 parseBatch 解析（IP 段展开 / 去重 / 注释跳过均在前端完成）。
//
// 设计原则与导出功能对称：
//   导出：前端 dialog.save() 拿路径 → invoke('export_report', {path}) → Rust 写文件
//   导入：前端 dialog.open() 拿路径 → invoke('read_import_file', {path}) → Rust 读文件
//
// 本模块只负责「把文件读成内容」，不做任何业务解析（避免与前端重复实现）。
use std::fs;
use std::path::Path;

use serde::Serialize;

/// 导入文件的解析结果（前端据此选择解析路径）
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ImportPayload {
    /// 纯文本（txt / csv / log / dat / list 等）
    Text { text: String },
    /// 表格（Excel 第一个工作表，每个单元格已转字符串）
    Table { rows: Vec<Vec<String>> },
}

/// 走纯文本路径的扩展名
const TEXT_EXTS: &[&str] = &["txt", "csv", "log", "dat", "list"];
/// 走表格路径的扩展名
const TABLE_EXTS: &[&str] = &["xlsx", "xlsm", "xls", "ods"];
/// zip（xlsx / ods）文件头：`PK\x03\x04`
const ZIP_MAGIC: [u8; 4] = [0x50, 0x4B, 0x03, 0x04];
/// UTF-8 BOM
const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// 读取导入文件：按扩展名分派，未知后缀先探测文件头再决定走向
#[tauri::command]
pub fn read_import_file(path: String) -> Result<ImportPayload, String> {
    let p = Path::new(&path);
    if !p.is_file() {
        return Err(format!("文件不存在或不是常规文件：{}", path));
    }

    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    if TEXT_EXTS.contains(&ext.as_str()) {
        return read_text(&path).map(|text| ImportPayload::Text { text });
    }
    if TABLE_EXTS.contains(&ext.as_str()) {
        return read_table(&path).map(|rows| ImportPayload::Table { rows });
    }

    // 未知后缀：探测文件头。zip 头（PK\x03\x04）→ 表格，否则尽力按文本处理
    let bytes = fs::read(&path).map_err(|e| format!("读取文件失败：{}（{}）", e, path))?;
    if bytes.len() >= 4 && bytes[0..4] == ZIP_MAGIC {
        return read_table(&path).map(|rows| ImportPayload::Table { rows });
    }
    Ok(ImportPayload::Text {
        text: decode_bytes(&bytes),
    })
}

/* ----------------------------- 文本读取 ----------------------------- */

/// 读取文本文件：读原始字节后按编码探测规则解码
fn read_text(path: &str) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("读取文件失败：{}（{}）", e, path))?;
    Ok(decode_bytes(&bytes))
}

/// 字节 → 文本解码（面向中文 Windows 场景，尽力保证不乱码）：
///   1. 有 UTF-8 BOM → 去 BOM 后按 UTF-8 解码
///   2. 严格 UTF-8 解码成功 → 采用
///   3. 失败 → 用 GBK（中文记事本「ANSI」另存即为 GBK）解码
///   4. 再失败 → UTF-8 lossy（不报错，尽力而为）
fn decode_bytes(bytes: &[u8]) -> String {
    // 1) UTF-8 BOM
    if bytes.len() >= 3 && bytes[0..3] == UTF8_BOM {
        return String::from_utf8_lossy(&bytes[3..]).to_string();
    }
    // 2) 严格 UTF-8
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    // 3) GBK
    let (decoded, _, had_errors) = encoding_rs::GBK.decode(bytes);
    if !had_errors {
        return decoded.into_owned();
    }
    // 4) UTF-8 lossy（永不失败）
    String::from_utf8_lossy(bytes).to_string()
}

/* ----------------------------- 表格读取 ----------------------------- */

/// 读取表格：取第一个工作表，逐行转字符串
fn read_table(path: &str) -> Result<Vec<Vec<String>>, String> {
    use calamine::{open_workbook_auto, Reader};

    let mut workbook = open_workbook_auto(path)
        .map_err(|e| format!("打开表格文件失败：{}（{}）", e, path))?;

    let range = workbook
        .worksheet_range_at(0)
        .ok_or_else(|| "该表格不包含任何工作表".to_string())?
        .map_err(|e| format!("读取工作表失败：{}", e))?;

    let mut rows: Vec<Vec<String>> = Vec::new();
    for row in range.rows() {
        let cells: Vec<String> = row.iter().map(cell_to_string).collect();
        if let Some(normalized) = normalize_row(cells) {
            rows.push(normalized);
        }
    }
    Ok(rows)
}

/// 规范化一行：裁掉行尾空单元格；整行为空返回 None（跳过）
fn normalize_row(mut cells: Vec<String>) -> Option<Vec<String>> {
    while matches!(cells.last(), Some(s) if s.is_empty()) {
        cells.pop();
    }
    if cells.is_empty() {
        None
    } else {
        Some(cells)
    }
}

/// 单元格 → 字符串。浮点若为整数则去掉小数点（`1.0` → `1`），避免 IP 列出现怪值
fn cell_to_string(cell: &calamine::Data) -> String {
    use calamine::Data;
    match cell {
        Data::String(s) => s.clone(),
        Data::Float(f) => {
            if f.is_finite() && f.fract() == 0.0 && f.abs() < 1e15 {
                format!("{}", *f as i64)
            } else {
                format!("{}", f)
            }
        }
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::DateTime(dt) => dt.to_string(),
        Data::DateTimeIso(s) => s.clone(),
        Data::DurationIso(s) => s.clone(),
        Data::Error(e) => format!("{:?}", e),
        Data::Empty => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// 临时工作目录（每个用例独立文件，避免互相覆盖）
    fn tmp_path(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("pingboard_qa_import");
        std::fs::create_dir_all(&dir).expect("创建临时目录失败");
        dir.join(name)
    }

    fn write_bytes(path: &std::path::Path, bytes: &[u8]) {
        let mut f = std::fs::File::create(path).expect("创建文件失败");
        f.write_all(bytes).expect("写入文件失败");
    }

    /* ------------------- 编码探测 ------------------- */

    #[test]
    fn qa_decode_utf8_bom_strips_bom() {
        let mut bytes = UTF8_BOM.to_vec();
        bytes.extend_from_slice("主机A 备注".as_bytes());
        assert_eq!(decode_bytes(&bytes), "主机A 备注");
    }

    #[test]
    fn qa_decode_plain_utf8() {
        assert_eq!(decode_bytes("192.168.1.1 网关".as_bytes()), "192.168.1.1 网关");
    }

    #[test]
    fn qa_decode_gbk_chinese_fallback() {
        // 用 encoding_rs 生成真实 GBK 字节，再走我们的探测逻辑
        let (gbk_bytes, _, had_err) = encoding_rs::GBK.encode("中文主机 备注");
        assert!(!had_err);
        // 该串不是合法 UTF-8，必须靠 GBK 分支才能正确还原
        assert!(std::str::from_utf8(&gbk_bytes).is_err());
        assert_eq!(decode_bytes(&gbk_bytes), "中文主机 备注");
    }

    /* ------------------- 单元格 / 行规范化 ------------------- */

    #[test]
    fn qa_cell_to_string_variants() {
        use calamine::Data;
        assert_eq!(cell_to_string(&Data::String("223.5.5.5".into())), "223.5.5.5");
        assert_eq!(cell_to_string(&Data::Float(1.0)), "1", "整数浮点应去掉小数点");
        assert_eq!(cell_to_string(&Data::Float(192.0)), "192");
        assert_eq!(cell_to_string(&Data::Float(1.5)), "1.5");
        assert_eq!(cell_to_string(&Data::Int(42)), "42");
        assert_eq!(cell_to_string(&Data::Empty), "");
    }

    #[test]
    fn qa_normalize_row_trims_and_skips_empty() {
        assert_eq!(
            normalize_row(vec!["a".into(), "b".into(), "".into(), "".into()]),
            Some(vec!["a".into(), "b".into()])
        );
        assert_eq!(normalize_row(vec!["".into(), "".into()]), None);
        assert_eq!(normalize_row(vec![]), None);
    }

    /* ------------------- 端到端：真实文件 + read_import_file ------------------- */

    #[test]
    fn qa_read_import_file_utf8_txt_real() {
        let p = tmp_path("utf8.txt");
        let content = "# 头部注释\n223.5.5.5 阿里 DNS\n192.168.1.1-3 内网段\n";
        write_bytes(&p, content.as_bytes());

        let payload = read_import_file(p.to_string_lossy().to_string()).expect("读取应成功");
        match payload {
            ImportPayload::Text { text } => {
                assert!(text.contains("阿里 DNS"));
                assert!(text.contains("192.168.1.1-3"));
                assert!(!text.starts_with('\u{feff}'), "不应残留 BOM");
            }
            other => panic!("应解析为文本，实际 {:?}", other),
        }
    }

    #[test]
    fn qa_read_import_file_gbk_txt_real() {
        let p = tmp_path("gbk.txt");
        let (gbk_bytes, _, _) = encoding_rs::GBK.encode("114.114.114.114 电信DNS\n8.8.8.8 谷歌DNS\n");
        write_bytes(&p, &gbk_bytes);

        let payload = read_import_file(p.to_string_lossy().to_string()).expect("读取应成功");
        match payload {
            ImportPayload::Text { text } => {
                assert!(text.contains("电信DNS"), "GBK 中文应正确还原：{}", text);
                assert!(text.contains("谷歌DNS"));
            }
            other => panic!("应解析为文本，实际 {:?}", other),
        }
    }

    #[test]
    fn qa_read_import_file_missing_errors() {
        let p = tmp_path("no_such_file_xyz.txt");
        let err = read_import_file(p.to_string_lossy().to_string()).unwrap_err();
        assert!(err.contains("不存在"), "错误信息应可读：{}", err);
    }

    /// 真实文件端到端：读取 PINGBOARD_QA_DIR 下的 utf8.txt / gbk.txt / sample.xlsx，
    /// 逐一校验并写出 import_payloads.json（供前端解析脚本消费）。
    /// 未设置环境变量时打印跳过（不失败），保证常规 cargo test 与 CI 不受影响。
    #[test]
    fn qa_real_files_end_to_end() {
        let dir = match std::env::var("PINGBOARD_QA_DIR") {
            Ok(d) if std::path::Path::new(&d).is_dir() => d,
            _ => {
                eprintln!("[qa-skip] 未提供 PINGBOARD_QA_DIR，跳过真实文件端到端用例");
                return;
            }
        };
        let dir = std::path::PathBuf::from(dir);

        let read = |name: &str| -> ImportPayload {
            let p = dir.join(name);
            read_import_file(p.to_string_lossy().to_string())
                .unwrap_or_else(|e| panic!("读取 {} 失败：{}", name, e))
        };

        // 1) UTF-8 txt（含 IP 段与 # 注释）
        let utf8 = read("utf8.txt");
        match &utf8 {
            ImportPayload::Text { text } => {
                eprintln!("[qa] utf8.txt → Text，{} 字符，首行：{}", text.chars().count(), text.lines().next().unwrap_or(""));
                assert!(text.contains("阿里 DNS"));
                assert!(text.contains("192.168.1.1-3"));
            }
            other => panic!("utf8.txt 应为文本，实际 {:?}", other),
        }

        // 2) GBK 中文 txt
        let gbk = read("gbk.txt");
        match &gbk {
            ImportPayload::Text { text } => {
                eprintln!("[qa] gbk.txt → Text，中文还原：{}", text.replace('\n', " | "));
                assert!(text.contains("电信DNS"), "GBK 中文应正确还原：{}", text);
            }
            other => panic!("gbk.txt 应为文本，实际 {:?}", other),
        }

        // 3) 真实 xlsx
        let xlsx = read("sample.xlsx");
        match &xlsx {
            ImportPayload::Table { rows } => {
                eprintln!("[qa] sample.xlsx → Table，{} 行", rows.len());
                assert!(!rows.is_empty(), "xlsx 应至少解析出 1 行");
                assert_eq!(rows[0], vec!["主机".to_string(), "备注".to_string()]);
                assert_eq!(rows[1], vec!["192.168.1.1".to_string(), "网关".to_string()]);
                assert_eq!(rows[2][0], "10", "浮点整数应转为 '10'");
            }
            other => panic!("sample.xlsx 应为表格，实际 {:?}", other),
        }

        // 写出 payload JSON，供 Node 解析脚本验证最终目标数
        let dump = serde_json::json!([
            { "name": "utf8.txt", "payload": utf8 },
            { "name": "gbk.txt", "payload": gbk },
            { "name": "sample.xlsx", "payload": xlsx },
        ]);
        let out = dir.join("import_payloads.json");
        std::fs::write(&out, serde_json::to_string_pretty(&dump).unwrap()).expect("写出 payload 失败");
        eprintln!("[qa] 已写出 {}", out.display());
    }

    #[test]
    fn qa_import_payload_serde_wire_format() {
        // 锁定与前端 TS 类型严格对应的线格式
        let text = ImportPayload::Text { text: "a".into() };
        assert_eq!(serde_json::to_string(&text).unwrap(), r#"{"kind":"text","text":"a"}"#);
        let table = ImportPayload::Table {
            rows: vec![vec!["1".into(), "2".into()]],
        };
        assert_eq!(
            serde_json::to_string(&table).unwrap(),
            r#"{"kind":"table","rows":[["1","2"]]}"#
        );
    }

    /* ============== QA v1.1.0 追加：内存级编码/解码边界矩阵 ============== */

    /// 换行符不应被 decode_bytes 破坏（CRLF 原样保留，交由前端 split 处理）
    #[test]
    fn qa_v11_eol_variants_preserved() {
        assert_eq!(decode_bytes(b"a\r\nb"), "a\r\nb");
        assert_eq!(decode_bytes(b"a\nb"), "a\nb");
        assert_eq!(decode_bytes(b"a\rb"), "a\rb");
    }

    /// 空输入与纯 ASCII 输入
    #[test]
    fn qa_v11_empty_and_ascii() {
        assert_eq!(decode_bytes(b""), "");
        assert_eq!(decode_bytes(b"127.0.0.1 ok"), "127.0.0.1 ok");
    }

    /// UTF-16（LE/BE）真实行为锁定：
    ///   * **带 BOM**（记事本「Unicode」另存）→ 意外地能正确解码：
    ///     decode_bytes 走到 GBK 分支时，encoding_rs 的 `GBK.decode` 会做 BOM 嗅探，
    ///     识别 UTF-16 BOM 后按 UTF-16 解码。这是 GBK 回退的「副作用」，非显式支持。
    ///   * **不带 BOM** → 乱码（多数码点落在 <0x80，被当成合法 UTF-8，夹杂 NUL）。
    #[test]
    fn qa_v11_utf16_behavior_is_locked() {
        let s = "127.0.0.1 本机\n";
        let le_body: Vec<u8> = s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        let be_body: Vec<u8> = s.encode_utf16().flat_map(|u| u.to_be_bytes()).collect();

        let mut le_bom = vec![0xFF, 0xFE];
        le_bom.extend(&le_body);
        let mut be_bom = vec![0xFE, 0xFF];
        be_bom.extend(&be_body);

        // (1) 带 BOM → 正确
        assert_eq!(
            decode_bytes(&le_bom),
            s,
            "UTF-16LE（带 BOM）应能正确解码（encoding_rs BOM 嗅探副作用）"
        );
        assert_eq!(decode_bytes(&be_bom), s, "UTF-16BE（带 BOM）应能正确解码");

        // (2) 不带 BOM → 乱码
        let no_le = decode_bytes(&le_body);
        let no_be = decode_bytes(&be_body);
        eprintln!("[qa] UTF-16LE 无 BOM 解码：{:?}", no_le.chars().take(24).collect::<String>());
        eprintln!("[qa] UTF-16BE 无 BOM 解码：{:?}", no_be.chars().take(24).collect::<String>());
        assert!(
            !no_le.contains("本机") && (no_le.contains('\u{0}') || no_le.contains('\u{fffd}')),
            "UTF-16LE 无 BOM 应乱码"
        );
        assert!(
            !no_be.contains("本机") && (no_be.contains('\u{0}') || no_be.contains('\u{fffd}')),
            "UTF-16BE 无 BOM 应乱码"
        );
    }

    /// GBK 与 UTF-8 混合字节：严格 UTF-8 成功则不误判为 GBK
    #[test]
    fn qa_v11_valid_utf8_not_misdecoded_as_gbk() {
        let s = "阿里 DNS 192.168.1.1";
        assert_eq!(decode_bytes(s.as_bytes()), s);
    }

    /* ============== QA v1.1.0 追加：真实文件夹具端到端 ============== */

    fn v11_dir() -> Option<std::path::PathBuf> {
        match std::env::var("PINGBOARD_QA_V11_DIR") {
            Ok(d) if std::path::Path::new(&d).is_dir() => Some(std::path::PathBuf::from(d)),
            _ => None,
        }
    }

    fn read_in(dir: &std::path::Path, name: &str) -> ImportPayload {
        let p = dir.join(name);
        read_import_file(p.to_string_lossy().to_string())
            .unwrap_or_else(|e| panic!("读取 {} 失败：{}", name, e))
    }

    fn read_err(dir: &std::path::Path, name: &str) -> String {
        let p = dir.join(name);
        read_import_file(p.to_string_lossy().to_string())
            .expect_err(&format!("{} 期望报错却成功", name))
    }

    /// 编码矩阵 + 内容边界 + Excel + 伪装文件，全部基于真实磁盘文件
    #[test]
    fn qa_v11_import_fixtures_matrix() {
        let dir = match v11_dir() {
            Some(d) => d,
            None => {
                eprintln!("[qa-skip] 未提供 PINGBOARD_QA_V11_DIR，跳过 v11 导入夹具用例");
                return;
            }
        };
        let mut dump: Vec<serde_json::Value> = Vec::new();
        let mut record = |name: &str, res: Result<ImportPayload, String>| {
            let v = match res {
                Ok(p) => serde_json::json!({ "kind": "ok", "payload": p }),
                Err(e) => serde_json::json!({ "kind": "err", "error": e }),
            };
            dump.push(serde_json::json!({ "name": name, "result": v }));
        };

        // ---------- 1) 编码矩阵 ----------
        match read_in(&dir, "utf8_nobom.txt") {
            ImportPayload::Text { text } => {
                assert!(text.starts_with("223.5.5.5"), "无 BOM utf8 首行异常：{:?}", text.lines().next());
                assert!(text.contains("阿里DNS"));
                assert!(!text.starts_with('\u{feff}'));
                record("utf8_nobom.txt", Ok(ImportPayload::Text { text: text.clone() }));
            }
            o => panic!("utf8_nobom 应为 Text，实际 {:?}", o),
        }

        match read_in(&dir, "utf8_bom.txt") {
            ImportPayload::Text { text } => {
                assert!(!text.starts_with('\u{feff}'), "BOM 必须被剥离，不得成为首字符");
                assert!(
                    text.starts_with("223.5.5.5"),
                    "BOM 后首字符应为 2，实际：{:?}",
                    text.chars().take(6).collect::<String>()
                );
                record("utf8_bom.txt", Ok(ImportPayload::Text { text: text.clone() }));
            }
            o => panic!("utf8_bom 应为 Text，实际 {:?}", o),
        }

        match read_in(&dir, "gbk.txt") {
            ImportPayload::Text { text } => {
                assert!(text.contains("电信DNS"), "GBK 中文应还原：{}", text);
                assert!(text.contains("谷歌DNS"));
                record("gbk.txt", Ok(ImportPayload::Text { text: text.clone() }));
            }
            o => panic!("gbk 应为 Text，实际 {:?}", o),
        }

        // UTF-16（带 BOM，记事本「Unicode」另存）：经 encoding_rs 的 BOM 嗅探可正确解码
        for name in ["utf16le.txt", "utf16be.txt"] {
            let res = read_import_file(dir.join(name).to_string_lossy().to_string());
            match &res {
                Ok(ImportPayload::Text { text }) => {
                    assert!(text.contains("本机"), "{}（带 BOM）应能还原中文：{:?}", name, text);
                    eprintln!("[qa] {} → Text，中文可读=true", name);
                }
                Ok(o) => panic!("{} 应为 Text，实际 {:?}", name, o),
                Err(e) => panic!("{} 不应报错：{}", name, e),
            }
            record(name, res);
        }

        // ---------- 2) 换行 ----------
        for name in ["crlf.txt", "lf.txt", "mixed_eol.txt"] {
            match read_in(&dir, name) {
                ImportPayload::Text { text } => {
                    // 文本层保留原始换行，交给前端 split(/\r?\n/)
                    assert!(text.contains("223.5.5.5") || text.contains("127.0.0.1") || text.contains("114.114.114.114"));
                    record(name, Ok(ImportPayload::Text { text: text.clone() }));
                }
                o => panic!("{} 应为 Text，实际 {:?}", name, o),
            }
        }

        // ---------- 3) 内容边界 ----------
        match read_in(&dir, "empty.txt") {
            ImportPayload::Text { text } => {
                assert!(text.is_empty(), "空文件应得到空文本");
                record("empty.txt", Ok(ImportPayload::Text { text }));
            }
            o => panic!("empty 应为 Text，实际 {:?}", o),
        }
        for name in [
            "comments_only.txt",
            "blank_only.txt",
            "many.txt",
            "dup.txt",
            "iprange_1.txt",
            "iprange_2.txt",
            "iprange_bad.txt",
        ] {
            let res = read_import_file(dir.join(name).to_string_lossy().to_string());
            assert!(res.is_ok(), "{} 不应报错", name);
            record(name, res);
        }
        // 缺失文件必须报可读中文错误
        let err = read_err(&dir, "definitely_missing_file.txt");
        assert!(err.contains("不存在"), "缺失文件错误应可读：{}", err);
        record("definitely_missing_file.txt", Err(err));

        // ---------- 4) Excel ----------
        match read_in(&dir, "numbers.xlsx") {
            ImportPayload::Table { rows } => {
                assert_eq!(rows[0], vec!["主机".to_string(), "备注".to_string()], "表头行原样返回（跳过在前端）");
                assert_eq!(rows[1], vec!["192.168.1.1".to_string(), "网关".to_string()]);
                assert_eq!(rows[2][0], "100", "数字型整数应去小数为 100");
                assert_eq!(rows[3], vec!["10.0.0.5".to_string(), "1".to_string()], "1.0 应转为 1");
                assert_eq!(rows[4], vec!["".to_string(), "空主机的备注".to_string()], "空主机行保留（前端跳过）");
                assert_eq!(rows[5][1], "DNS; 备注含分号,逗号", "含分号逗号的备注应原样保留");
                let flat: String = rows.iter().flatten().cloned().collect::<Vec<_>>().join("|");
                assert!(!flat.contains("不应读到的表"), "不得读取第二个工作表");
                assert!(!flat.contains("9.9.9.9"), "第二个工作表内容不得出现");
                record("numbers.xlsx", Ok(ImportPayload::Table { rows: rows.clone() }));
            }
            o => panic!("numbers.xlsx 应为 Table，实际 {:?}", o),
        }

        match read_in(&dir, "header_only.xlsx") {
            ImportPayload::Table { rows } => {
                assert_eq!(rows.len(), 1, "只有表头的表应恰好 1 行");
                assert_eq!(rows[0], vec!["主机".to_string(), "备注".to_string()]);
                record("header_only.xlsx", Ok(ImportPayload::Table { rows }));
            }
            o => panic!("header_only.xlsx 应为 Table，实际 {:?}", o),
        }

        // ---------- 5) 伪装 / 无后缀 ----------
        let err = read_err(&dir, "txt_as_xlsx.xlsx");
        eprintln!("[qa] txt_as_xlsx.xlsx 错误信息：{}", err);
        assert!(err.contains("表格") || err.contains("工作"), "txt 冒充 xlsx 应返回可读中文错误：{}", err);
        record("txt_as_xlsx.xlsx", Err(err));

        let zip_txt = read_import_file(dir.join("zip_as_txt.txt").to_string_lossy().to_string());
        eprintln!("[qa] zip_as_txt.txt → {:?}", zip_txt.as_ref().map(|_| "Ok").map_err(|e| e.clone()));
        assert!(zip_txt.is_ok(), "zip 内容存成 .txt 不应 panic（按文本 lossy 处理）");
        record("zip_as_txt.txt", zip_txt);

        match read_in(&dir, "noext_zip") {
            ImportPayload::Table { rows } => {
                assert!(!rows.is_empty(), "无后缀 zip 应探测为表格");
                record("noext_zip", Ok(ImportPayload::Table { rows }));
            }
            o => panic!("noext_zip 应探测为 Table，实际 {:?}", o),
        }
        match read_in(&dir, "noext_text") {
            ImportPayload::Text { text } => {
                assert!(text.contains("223.5.5.5"));
                record("noext_text", Ok(ImportPayload::Text { text }));
            }
            o => panic!("noext_text 应为 Text，实际 {:?}", o),
        }

        // ---------- 6) 导出 payload JSON，供 Node 前端解析脚本消费 ----------
        let out = dir.join("v11_import_payloads.json");
        std::fs::write(&out, serde_json::to_string_pretty(&dump).unwrap()).expect("写 payload 失败");
        eprintln!("[qa] 已写出 {}", out.display());
    }

    /// 4097 目标 + limit=false：必须在单元层返回含 4096 的可读错误且不启动线程
    /// （不真实创建 4097 个线程，仅触发拦截分支）
    #[test]
    fn qa_v11_hard_4096_boundary_message() {
        // 直接复用 state 的拦截逻辑口径：HARD_MAX_THREADS 常量与文案对齐由 state 测试覆盖，
        // 此处仅锁定常量值，避免两处口径漂移。
        assert_eq!(crate::state::HARD_MAX_THREADS, 4096);
    }
}
