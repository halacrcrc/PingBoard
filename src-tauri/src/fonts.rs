// 系统字体枚举：读注册表 HKLM / HKCU 的 Fonts 键，返回去重排序后的字体族名列表。
//
// 设计要点：
//   * 纯注册表读取，无子进程（不涉及 CREATE_NO_WINDOW 约定）。
//   * 过滤口径按注册表**值数据**的文件扩展名（.ttf / .ttc / .otf），
//     排除 .fon 点阵字体等非 TrueType/OpenType 项。
//   * 族名清洗为纯函数 `font_value_to_family`（可单测）：
//     值名形如 "Arial Bold (TrueType)" → 剥括号注记 → 剥样式词 → "Arial"。
//   * 「A & B」复合族名（如 "Microsoft YaHei & Microsoft YaHei UI"）拆分为两个族名。
//   * 枚举失败（注册表不可读）返回空列表，由前端回退为「系统默认」单项，绝不 panic。
use std::collections::HashSet;

use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
use winreg::RegKey;

/// 字体注册表子键（HKLM / HKCU 同名）
const FONTS_SUBKEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts";

/// 允许的字体文件扩展名（小写，含点）
const FONT_FILE_EXTS: [&str; 3] = [".ttf", ".ttc", ".otf"];

/// 需要从族名尾部剥除的样式词（小写比较）。
/// 注意集合刻意保守：不收 "ui" / "text" / "math" 等可能属于族名本身的词
/// （如 "Segoe UI"、"Sitka Text"、"Cambria Math"）。
const STYLE_WORDS: [&str; 17] = [
    "regular",
    "bold",
    "italic",
    "oblique",
    "light",
    "extralight",
    "thin",
    "black",
    "medium",
    "semibold",
    "semilight",
    "extrabold",
    "heavy",
    "book",
    "roman",
    "condensed",
    "narrow",
];

/// 把注册表值名清洗为字体族名；不构成合法族名时返回 None。
///
/// 例：
///   "Arial (TrueType)"                     → "Arial"
///   "Arial Bold Italic (TrueType)"         → "Arial"
///   "Microsoft YaHei & Microsoft YaHei UI (TrueType)" → 拆两段，本函数只处理单段
///   "宋体 (TrueType)"                       → "宋体"
///   "(TrueType)"                            → None（剥完为空）
pub fn font_value_to_family(value_name: &str) -> Option<String> {
    let mut s = value_name.trim().to_string();
    if s.is_empty() {
        return None;
    }
    // 1) 剥尾部括号注记：" (TrueType)" / " (OpenType)" 等各种历史写法。
    //    标准形态带前导空格（" (TrueType)"）；退化形态无空格（"(TrueType)"）也一并剥净。
    if s.ends_with(')') {
        if let Some(i) = s.rfind(" (") {
            s.truncate(i);
        } else if let Some(i) = s.rfind('(') {
            s.truncate(i);
        }
    }
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    // 2) 剥尾部样式词（反复剥，直到不再变化）："Bold Italic" 两段都要剥掉
    let mut parts: Vec<&str> = s.split_whitespace().collect();
    while parts.len() > 1 {
        let last = parts[parts.len() - 1].to_lowercase();
        if STYLE_WORDS.contains(&last.as_str()) {
            parts.pop();
        } else {
            break;
        }
    }
    let family = parts.join(" ").trim().to_string();
    if family.is_empty() {
        None
    } else {
        Some(family)
    }
}

/// 值数据的文件名是否为 TrueType / OpenType 字体文件
fn is_font_file(file_name: &str) -> bool {
    let lower = file_name.to_lowercase();
    FONT_FILE_EXTS.iter().any(|ext| lower.ends_with(ext))
}

/// 枚举系统已安装字体族名：去重（不区分大小写）、按名称不区分大小写排序。
/// 注册表不可读时返回空列表（调用方降级为仅「系统默认」）。
pub fn list_system_fonts() -> Vec<String> {
    let mut families: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for hive in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
        let Ok(key) = RegKey::predef(hive).open_subkey_with_flags(FONTS_SUBKEY, KEY_READ) else {
            continue;
        };
        for entry in key.enum_values().flatten() {
            let value_name = entry.0;
            // 按值数据扩展名过滤（.fon 点阵字体等被排除）；读失败跳过该项
            let Ok(data) = key.get_value::<String, _>(&value_name) else {
                continue;
            };
            if !is_font_file(&data) {
                continue;
            }
            // 「A & B」复合族名拆分；单段族名清洗后非空才收录
            for part in value_name.split('&') {
                if let Some(family) = font_value_to_family(part) {
                    let fold = family.to_lowercase();
                    if seen.insert(fold) {
                        families.push(family);
                    }
                }
            }
        }
    }
    families.sort_by_key(|f| f.to_lowercase());
    families
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qa_family_strips_paren_note_and_styles() {
        assert_eq!(font_value_to_family("Arial (TrueType)").as_deref(), Some("Arial"));
        assert_eq!(
            font_value_to_family("Arial Bold (TrueType)").as_deref(),
            Some("Arial")
        );
        assert_eq!(
            font_value_to_family("Arial Bold Italic (TrueType)").as_deref(),
            Some("Arial")
        );
        assert_eq!(
            font_value_to_family("Segoe UI Black (TrueType)").as_deref(),
            Some("Segoe UI")
        );
        assert_eq!(
            font_value_to_family("Microsoft YaHei Light (TrueType)").as_deref(),
            Some("Microsoft YaHei")
        );
    }

    #[test]
    fn qa_family_keeps_cjk_and_family_words() {
        // 中文族名原样保留
        assert_eq!(font_value_to_family("宋体 (TrueType)").as_deref(), Some("宋体"));
        assert_eq!(
            font_value_to_family("微软雅黑 (TrueType)").as_deref(),
            Some("微软雅黑")
        );
        // "UI" / "Math" 不在样式词集合内，必须保留（防误剥）
        assert_eq!(font_value_to_family("Segoe UI (TrueType)").as_deref(), Some("Segoe UI"));
        assert_eq!(
            font_value_to_family("Cambria Math (TrueType)").as_deref(),
            Some("Cambria Math")
        );
    }

    #[test]
    fn qa_family_none_cases() {
        assert!(font_value_to_family("").is_none(), "空串应返回 None");
        assert!(font_value_to_family("   ").is_none(), "纯空白应返回 None");
        assert!(font_value_to_family("(TrueType)").is_none(), "剥完为空应返回 None");
        // 单段样式词不应被剥成空：族名至少保留一段
        assert_eq!(font_value_to_family("Bold (TrueType)").as_deref(), Some("Bold"));
    }

    #[test]
    fn qa_font_file_filter() {
        assert!(is_font_file("arial.ttf"));
        assert!(is_font_file("MSYH.TTC"));
        assert!(is_font_file("SourceHanSansSC-Regular.otf"));
        assert!(!is_font_file("svgafix.fon"), "点阵字体必须被排除");
        assert!(!is_font_file(""));
    }
}
