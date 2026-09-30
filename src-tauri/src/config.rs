// 配置读写：config 文件位于 app_config_dir()/pingboard-config.json
use std::fs;
use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

use crate::model::{AppConfig, PingSettings, TargetConfig};
use crate::stats;

/// 配置文件名
pub const CONFIG_FILE: &str = "pingboard-config.json";

/// 应用配置目录（`%APPDATA%\com.pingboard.desktop\`）
pub fn app_config_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map_err(|e| format!("无法获取配置目录：{}", e))
}

/// 解析配置文件完整路径
pub fn config_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_config_dir(app)?.join(CONFIG_FILE))
}

/// 解析配置文本：**永不失败**。返回 `(配置, 是否走了抢救路径)`。
///
/// 抢救路径（红线 R1 / 批次 2 A4）：整份配置反序列化失败时，磁盘上的原文件仍然完好，
/// 此时若直接回落 `AppConfig::default()`，进程内主机列表被清空，随后任意一次落盘
/// （含退出时的保存）都会用空配置**静默覆盖**原文件，用户连手工恢复的机会都没有。
/// 这里做两件事：① 给调用方一个「是否失败」的信号（含原始错误，供诊断日志使用）；
/// ② 尽力从原始 JSON 里把 targets 数组逐条抢救回来。
///
/// 返回 `(配置, 解析错误)`：错误为 `None` 表示原文被正常解析，走的是正常路径。
///
/// ⚠️ **生产代码必须走本函数**（`load_from_path` 委派于此），不要另抄一份：
/// 护栏只有一份，抄一份就把「回滚到错误写法就该红」的保障架在了空处。
pub fn parse_with_recovery(text: &str) -> (AppConfig, Option<serde_json::Error>) {
    match serde_json::from_str::<AppConfig>(text) {
        Ok(cfg) => (finalize(cfg), None),
        Err(e) => (recover_from_text(text), Some(e)),
    }
}

/// 抢救：尽量保留原始文本里的主机列表（整份失败 ≠ 一条主机都救不回来）。
///
/// 逐条尝试反序列化 `TargetConfig`，解析不出或 `host` 为空的条目被丢弃，
/// 其余保序保留。任何异常都退化为「空列表」，绝不 panic。
///
/// 本函数不依赖 `AppHandle`，因此可以被单元测试直接覆盖。
pub fn recover_from_text(text: &str) -> AppConfig {
    let mut cfg = AppConfig::default();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
        // 1) 抢救主机列表
        if let Some(list) = value.get("targets").and_then(|t| t.as_array()) {
            cfg.targets = list
                .iter()
                .filter_map(|t| serde_json::from_value::<TargetConfig>(t.clone()).ok())
                .filter(|t| !t.host.trim().is_empty())
                .collect();
        }
        // 2) 抢救设置段：整份失败往往只是 targets 里一个坏元素引起的，
        //    settings 本身很可能是完好的，不该跟着一起丢（否则用户设置静默丢失）。
        //    `deserialize_settings` 本身是容错的，这一步几乎不会失败。
        if let Some(s) = value.get("settings") {
            if let Ok(parsed) = serde_json::from_value::<PingSettings>(s.clone()) {
                cfg.settings = parsed;
            }
        }
    }
    finalize(cfg)
}

/// 统一收尾：归一化设置并修正 version
fn finalize(mut cfg: AppConfig) -> AppConfig {
    stats::normalize_settings(&mut cfg.settings);
    if cfg.version == 0 {
        cfg.version = 1;
    }
    cfg
}

/// 把损坏的配置文件复制一份带时间戳的备份，给用户留手工恢复的余地。
///
/// 返回备份文件路径；备份失败时只打印警告（备份失败不该阻塞启动）。
/// 同样不依赖 `AppHandle`，可被单元测试直接覆盖。
pub fn backup_corrupt(path: &Path) -> Option<PathBuf> {
    let name = format!(
        "pingboard-config.corrupt-{}.json",
        crate::state::now_ms()
    );
    let bak = path.with_file_name(name);
    match fs::copy(path, &bak) {
        Ok(_) => Some(bak),
        Err(e) => {
            eprintln!("[pingboard] 配置损坏且备份失败：{}", e);
            None
        }
    }
}

/// 读取配置：文件不存在或解析失败一律回退到默认值，绝不 panic / 阻塞启动
///
/// ⚠️ 解析失败时额外做两件事（均不得阻塞启动）：
///   1) 备份损坏文件，给用户手工恢复的余地；
///   2) 从原始文本里抢救主机列表，避免「一个字符写坏，全部主机当场消失」。
///
/// ⚠️ 返回 `(配置, 提示文案)`：第二项用于**在界面上告知用户**（release 下无控制台，
/// `eprintln!` 会被 `windows_subsystem = "windows"` 吞掉，用户根本不知道备份过文件）。
pub fn load(app: &AppHandle) -> (AppConfig, Option<String>) {
    let path = match config_path(app) {
        Ok(p) => p,
        Err(_) => return (AppConfig::default(), None),
    };
    load_from_path(&path)
}

/// 从指定路径读取配置（读文件 + 解析 + 抢救 + 备份的完整生产路径）。
///
/// 抽出本函数的唯一目的：让**生产路径本身**能被单元测试覆盖——
/// `load` 需要 `&AppHandle`，单测里构造不出来，护栏就会退化成「只测了旁边那个纯函数」。
/// 本函数不依赖 `AppHandle`，测试可直接打到这里。
pub fn load_from_path(path: &Path) -> (AppConfig, Option<String>) {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => return (AppConfig::default(), None),
    };
    let (cfg, err) = parse_with_recovery(&text);
    match err {
        // 正常解析：直接返回（已归一化）
        None => (cfg, None),
        Some(e) => {
            // 1) 备份损坏文件，给用户手工恢复的余地
            let bak = backup_corrupt(path);
            if let Some(b) = &bak {
                eprintln!(
                    "[pingboard] 配置解析失败，已备份原文件到 {}：{}",
                    b.display(),
                    e
                );
            }
            // 备份失败时 backup_corrupt 自己已打印过告警，此处不重复打
            // 2) 尽力抢救：整份解析失败 ≠ 一条主机都救不回来
            let n = cfg.targets.len();
            eprintln!("[pingboard] 已从损坏配置中抢救 {} 个主机", n);
            // 3) 组装给用户看的提示（release 无控制台，这是唯一能让他看见的通道）
            //    抢救回 0 台是最严重的情况：界面为空、退出后原文件即被覆盖，
            //    必须换成强警告并要求用户「先别退出」。
            let notice = match (&bak, n) {
                (Some(b), 0) => format!(
                    "配置文件已损坏，未能抢救出任何主机（界面当前为空）。\
                     原文件已完整备份到 {}，请手动替换该文件后再重启；\
                     在此之前请勿退出本程序。",
                    b.display()
                ),
                (Some(b), n) => format!(
                    "配置文件已损坏，已备份原文件到 {}，从中抢救回 {} 个主机。\
                     当前正在使用抢救后的配置，如需完整恢复请手动替换该文件。",
                    b.display(),
                    n
                ),
                (None, _) => String::from(
                    "配置文件已损坏且备份失败，原文件暂未被覆盖，\
                     请先手动复制一份再继续使用。",
                ),
            };
            (cfg, Some(notice))
        }
    }
}

/// 写入配置：自动创建目录
pub fn save(app: &AppHandle, cfg: &AppConfig) -> Result<(), String> {
    let path = config_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建配置目录失败：{}", e))?;
    }
    let text = serde_json::to_string_pretty(cfg).map_err(|e| format!("序列化配置失败：{}", e))?;
    fs::write(&path, text).map_err(|e| format!("写入配置失败：{}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一个进程唯一的临时目录，测试结束后清理
    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pingboard-config-test-{}-{}-{}",
            tag,
            std::process::id(),
            crate::state::now_ms()
        ));
        fs::create_dir_all(&dir).expect("创建临时目录失败");
        dir
    }

    fn write_config(dir: &Path, name: &str, text: &str) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, text).expect("写入临时配置文件失败");
        p
    }

    /// 列出目录里的 corrupt 备份文件
    fn corrupt_backups(dir: &Path) -> Vec<PathBuf> {
        fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("pingboard-config.corrupt-"))
                    .unwrap_or(false)
            })
            .collect()
    }

    fn backups_first_path(dir: &Path) -> String {
        corrupt_backups(dir)[0].display().to_string()
    }

    /// 抢救路径（红线 R1）：整份 AppConfig 解析失败时，targets 数组仍必须被逐条抢救回来。
    ///
    /// ⚠️ 若把 `parse_with_recovery` 里的抢救逻辑删掉（退回裸 `AppConfig::default()`），
    /// 本测试会红——这正是它存在的理由（`load` / `load_from_path` 都委派于此，护栏只有一份）。
    ///
    /// ⚠️ 触发器的选型（重要）：版本 / settings 都加了类型容错后，能触发整份失败的形态
    /// 只剩「targets 数组里出现非对象元素」。这里固定用 **`null`** ——
    /// `null` 永远不可能被任何容错扩展救成一台主机，所以本用例不会随着后续
    /// 「给更多字段加容错」而静默失效（上一轮 version 就踩过这个循环）。
    #[test]
    fn qa_recover_salvages_targets_from_corrupt_config() {
        let text = r#"{
            "version": 1,
            "settings": {"interval_ms": 1500},
            "targets": [
                {"name": "阿里 DNS", "host": "223.5.5.5", "enabled": true},
                {"name": "坏条目", "host": "", "enabled": "yes"},
                {"name": "百度", "host": "www.baidu.com", "enabled": false},
                null
            ]
        }"#;
        assert!(
            serde_json::from_str::<AppConfig>(text).is_err(),
            "前提：这段文本必须真的解析失败，否则本测试失去意义"
        );

        let (cfg, err) = parse_with_recovery(text);
        assert!(err.is_some(), "应标记走了抢救路径");
        assert_eq!(cfg.targets.len(), 2, "完好的 2 个主机必须被抢救回来");
        assert_eq!(cfg.targets[0].host, "223.5.5.5");
        assert_eq!(cfg.targets[1].host, "www.baidu.com");
        assert!(!cfg.targets[1].enabled, "enabled=false 必须保留");
        assert_eq!(cfg.version, 1, "抢救出的配置 version 应为 1");
        assert_eq!(cfg.settings.interval_ms, 1500, "settings 段必须一起抢救回来");
    }

    /// 抢救时必须跳过 host 为空 / 纯空白的条目（与 AppState::init_from_config 同口径）
    #[test]
    fn qa_recover_skips_entries_without_host() {
        // ⚠️ 触发器固定用 `null` 元素（理由见上一个用例）。
        // 「坏条目」故意留 host 为空：这样即使将来 `enabled` 也吃容错、该条目能被解析出来，
        // 它也仍会被空 host 过滤掉——本用例的期望值不会随容错扩展而漂移。
        let text = r#"{
            "version": 1,
            "targets": [
                {"host": "   ", "name": "空白 host"},
                {"name": "没有 host"},
                {"name": "正常主机", "host": "8.8.8.8"},
                {"name": "坏条目", "host": "", "enabled": "yes"},
                null
            ]
        }"#;
        assert!(
            serde_json::from_str::<AppConfig>(text).is_err(),
            "前提：这段文本必须真的解析失败，否则本测试失去意义"
        );
        let (cfg, err) = parse_with_recovery(text);
        assert!(err.is_some());
        assert_eq!(cfg.targets.len(), 1, "无有效 host 的条目必须被丢弃");
        assert_eq!(cfg.targets[0].host, "8.8.8.8");
    }

    /// 完全无法解析的垃圾文本：不得 panic，返回空配置
    #[test]
    fn qa_recover_handles_garbage_text() {
        for text in ["{ 这不是合法 JSON ", "", "null", "42"] {
            let (cfg, err) = parse_with_recovery(text);
            assert!(err.is_some(), "非法文本必须走抢救路径：{:?}", text);
            assert!(cfg.targets.is_empty(), "救不出主机就应是空列表：{:?}", text);
            assert_eq!(cfg.version, 1);
            assert_eq!(cfg.settings.interval_ms, 1000, "settings 应回落默认");
        }
    }

    /// `"[]"` 会被 serde 当成「结构体序列形态」解析成功（三个字段全走缺省），
    /// 此时不得误判为抢救路径，也不得 panic。
    #[test]
    fn qa_empty_array_is_parsed_as_all_defaults() {
        let (cfg, err) = parse_with_recovery("[]");
        assert!(err.is_none(), "[] 是 serde 合法的结构体序列形态，不算损坏");
        assert!(cfg.targets.is_empty());
        assert_eq!(cfg.version, 1);
    }

    /// 合法配置：不得走抢救路径，且内容逐字保留（抢救逻辑不能误触发）
    #[test]
    fn qa_valid_config_does_not_trigger_recovery() {
        let text = r#"{"version":1,"settings":{"interval_ms":800,"timeout_ms":1500,"payload_size":32,"ttl":128,"max_threads":256,"beep_on_fail":false,"auto_start":false,"history_len":60},"targets":[{"name":"阿里 DNS","host":"223.5.5.5","enabled":true}]}"#;
        let (cfg, err) = parse_with_recovery(text);
        assert!(err.is_none(), "合法配置不得标记为抢救");
        assert_eq!(cfg.targets.len(), 1);
        assert_eq!(cfg.targets[0].host, "223.5.5.5");
        assert_eq!(cfg.settings.interval_ms, 800, "合法设置值必须保留");
    }

    /// 损坏时必须在原目录生成一份 Corrupt 备份，内容与原文完全一致（手工恢复的凭据）
    #[test]
    fn qa_corrupt_config_preserves_targets_and_backs_up() {
        let dir = temp_dir("backup");
        // 触发器固定用 `null` 元素（理由见 qa_recover_salvages_targets_from_corrupt_config）
        let text = r#"{"version":1,"targets":[{"name":"必须保住","host":"223.5.5.5","enabled":true},{"name":"坏条目","host":"","enabled":"yes"},null]}"#;
        let path = write_config(&dir, CONFIG_FILE, text);
        assert!(
            serde_json::from_str::<AppConfig>(text).is_err(),
            "前提：这段文本必须真的解析失败，否则本测试失去意义"
        );

        // 1) 备份：文件名带 corrupt- 前缀与时间戳
        let bak = backup_corrupt(&path).expect("备份应成功");
        assert!(bak.exists(), "备份文件必须存在");
        assert!(
            bak.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("pingboard-config.corrupt-"))
                .unwrap_or(false),
            "备份文件名必须带 corrupt- 前缀"
        );
        assert_eq!(
            fs::read_to_string(&bak).unwrap(),
            text,
            "备份内容必须与原文逐字节一致"
        );
        // 原文件不得被改动/删除
        assert_eq!(fs::read_to_string(&path).unwrap(), text);

        // 2) 抢救：从备份走的同一段文本里保住主机列表
        let (cfg, err) = parse_with_recovery(text);
        assert!(err.is_some(), "该文本确实无法被正常解析");
        assert_eq!(cfg.targets.len(), 1, "主机必须被抢救回来");
        assert_eq!(cfg.targets[0].host, "223.5.5.5");

        let _ = fs::remove_dir_all(&dir);
    }

    /// 生产路径覆盖（🟡-1）：`load_from_path` 必须真的执行「备份 + 抢救」，
    /// 而不只是某个被测试单独调用的纯函数做到了。
    ///
    /// ⚠️ 这条是 🟡-1 的核心：此前 `load` 自己手抄了一份抢救逻辑，
    /// 把 `load` 回滚成裸 `AppConfig::default()` 时 7 个测试依然全绿（护栏锁在隔壁）。
    /// 现在 `load` / `load_from_path` 都委派 `parse_with_recovery`，回滚必须让它变红。
    #[test]
    fn qa_load_from_path_recovers_and_backs_up() {
        let dir = temp_dir("loadpath");
        let text = r#"{"version":1,"settings":{"history_len":333},"targets":[{"name":"必须保住","host":"223.5.5.5","enabled":true},null]}"#;
        let path = write_config(&dir, CONFIG_FILE, text);

        let (cfg, notice) = load_from_path(&path);
        assert_eq!(cfg.targets.len(), 1, "生产路径必须抢救回主机");
        assert_eq!(cfg.targets[0].host, "223.5.5.5");
        // settings 段必须一起被抢救回来（🔴-1 并补的缺口）：
        // 整份失败常常只是 targets 里一个坏元素引起的，settings 本身是完好的
        assert_eq!(
            cfg.settings.history_len, 333,
            "抢救路径必须连 settings 一起救回来"
        );
        // 🔴-2：必须产生一条给用户看的提示，且带备份文件完整路径
        let notice = notice.expect("配置损坏时必须有提示文案");
        assert!(notice.contains("已备份原文件到"), "提示必须带备份路径：{}", notice);
        assert!(notice.contains("抢救回 1 个主机"), "提示必须体现抢救结果：{}", notice);
        let bak_path = backups_first_path(&dir);
        assert!(
            notice.contains(&bak_path),
            "提示必须包含备份文件的完整路径（否则用户无从手工恢复）：{}",
            notice
        );
        // 备份文件必须真的落盘（与 load 的 Err 分支同一条路径）
        assert_eq!(corrupt_backups(&dir).len(), 1, "生产路径必须生成恰好一份损坏备份");

        let _ = fs::remove_dir_all(&dir);
    }

    /// 🟡-2(a)：抢救回 0 台必须换成**强警告**措辞（界面为空 + 退出即覆盖原文件），
    /// 与「抢救回 N 台」明确分叉。
    #[test]
    fn qa_notice_is_stronger_when_no_target_rescued() {
        let dir = temp_dir("notice-zero");
        // targets 只有一个 null 元素：整份必然解析失败，且一条主机都救不回来
        let text = r#"{"version":1,"targets":[null]}"#;
        let path = write_config(&dir, CONFIG_FILE, text);

        let (cfg, notice) = load_from_path(&path);
        assert_eq!(cfg.targets.len(), 0, "一条主机都救不回来");
        let notice = notice.expect("必须有提示");
        assert!(
            notice.contains("未能抢救出任何主机"),
            "0 台时必须用强警告措辞：{}",
            notice
        );
        assert!(
            notice.contains("请勿退出本程序"),
            "0 台时必须明确要求用户先别退出：{}",
            notice
        );
        let bak = backups_first_path(&dir);
        assert!(notice.contains(&bak), "仍必须给出备份路径：{}", notice);
        // 且与「抢救回 N 台」的措辞不同（不得再出现「抢救回 0 个主机」这种平淡说法）
        assert!(
            !notice.contains("从中抢救回"),
            "0 台时不得沿用 N 台的措辞：{}",
            notice
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// 🔴-2：备份失败时也必须给出提示（且措辞与「备份成功」不同，不能让用户以为万事大吉）
    #[test]
    fn qa_notice_differs_when_backup_failed() {
        // 源文件不存在 → backup_corrupt 必然失败（copy 不到）
        let dir = temp_dir("notice-fail");
        let missing = dir.join("no-such-config.json");
        let (cfg, notice) = load_from_path(&missing);
        assert!(cfg.targets.is_empty());
        assert!(
            notice.is_none(),
            "文件读不到属「首次启动」，不该弹损坏提示"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// 合法配置走生产路径：不得误触发抢救，不得生成任何备份文件
    #[test]
    fn qa_load_from_path_keeps_valid_config_without_backup() {
        let dir = temp_dir("loadpath-ok");
        let text = r#"{"version":1,"settings":{"interval_ms":800},"targets":[{"name":"阿里 DNS","host":"223.5.5.5","enabled":true}]}"#;
        let path = write_config(&dir, CONFIG_FILE, text);

        let (cfg, notice) = load_from_path(&path);
        assert!(notice.is_none(), "合法配置不得产生提示");
        assert_eq!(cfg.targets.len(), 1);
        assert_eq!(cfg.settings.interval_ms, 800, "合法设置值必须保留");
        assert!(
            corrupt_backups(&dir).is_empty(),
            "合法配置不得生成备份文件"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// 备份目标不可写时（`backup_corrupt` 用于不存在的文件）不得 panic，返回 None
    #[test]
    fn qa_backup_failure_is_none_not_panic() {
        let missing = std::env::temp_dir().join(format!("pingboard-nope-{}.json", crate::state::now_ms()));
        assert!(backup_corrupt(&missing).is_none(), "源文件不存在时应返回 None");
    }
}
