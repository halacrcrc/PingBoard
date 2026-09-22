// 配置读写：config 文件位于 app_config_dir()/pingboard-config.json
use std::fs;
use std::path::PathBuf;

use tauri::{AppHandle, Manager};

use crate::model::AppConfig;
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

/// 读取配置：文件不存在或解析失败一律回退到默认值，绝不 panic / 阻塞启动
pub fn load(app: &AppHandle) -> AppConfig {
    let path = match config_path(app) {
        Ok(p) => p,
        Err(_) => return AppConfig::default(),
    };
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return AppConfig::default(),
    };
    match serde_json::from_str::<AppConfig>(&text) {
        Ok(mut cfg) => {
            stats::normalize_settings(&mut cfg.settings);
            if cfg.version == 0 {
                cfg.version = 1;
            }
            cfg
        }
        Err(_) => AppConfig::default(),
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
