// Tauri 命令层：前端 invoke 的入口，全部返回 Result<T, String>
use tauri::{AppHandle, State};

use crate::model::{PingSettings, Snapshot, TargetEntry, TargetState};
use crate::state::AppState;
use crate::{config, export};

/// 获取完整聚合快照
#[tauri::command]
pub async fn get_state(state: State<'_, AppState>) -> Result<Snapshot, String> {
    Ok(state.snapshot())
}

/// 新增目标，返回新分配的 id 列表
#[tauri::command]
pub async fn add_targets(
    app: AppHandle,
    state: State<'_, AppState>,
    entries: Vec<TargetEntry>,
) -> Result<Vec<u64>, String> {
    let ids = state.add_targets(entries)?;
    let _ = state.save_config(&app);
    Ok(ids)
}

/// 删除目标
#[tauri::command]
pub async fn remove_targets(
    app: AppHandle,
    state: State<'_, AppState>,
    ids: Vec<u64>,
) -> Result<(), String> {
    state.remove_targets(&ids);
    state.save_config(&app)
}

/// 修改目标
#[tauri::command]
pub async fn update_target(
    app: AppHandle,
    state: State<'_, AppState>,
    id: u64,
    name: String,
    host: String,
    enabled: bool,
) -> Result<(), String> {
    state.update_target(id, name, host, enabled)?;
    state.save_config(&app)
}

/// 批量设置启用状态
#[tauri::command]
pub async fn set_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    ids: Vec<u64>,
    enabled: bool,
) -> Result<(), String> {
    state.set_enabled(&ids, enabled);
    state.save_config(&app)
}

/// 开始 Ping；ids 为 null 表示全部已启用目标
#[tauri::command]
pub async fn start_pinging(
    state: State<'_, AppState>,
    ids: Option<Vec<u64>>,
) -> Result<(), String> {
    state.start(ids)
}

/// 停止 Ping；ids 为 null 表示全部
#[tauri::command]
pub async fn stop_pinging(
    state: State<'_, AppState>,
    ids: Option<Vec<u64>>,
) -> Result<(), String> {
    state.stop(ids);
    Ok(())
}

/// 重置统计；ids 为 null 表示全部
#[tauri::command]
pub async fn reset_stats(
    state: State<'_, AppState>,
    ids: Option<Vec<u64>>,
) -> Result<(), String> {
    state.reset_stats(ids);
    Ok(())
}

/// 更新设置并持久化
#[tauri::command]
pub async fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: PingSettings,
) -> Result<(), String> {
    state.update_settings(settings)?;
    state.save_config(&app)
}

/// 解析主机名（阻塞操作放入阻塞线程池，避免卡住 UI）
#[tauri::command]
pub async fn resolve_host(host: String) -> Result<String, String> {
    let joined = tauri::async_runtime::spawn_blocking(move || {
        crate::state::resolve_host_blocking(&host)
    })
    .await
    .map_err(|e| format!("解析任务失败：{}", e))?;
    joined.map(|ip| ip.to_string())
}

/// 导出报表到指定路径
#[tauri::command]
pub async fn export_report(
    state: State<'_, AppState>,
    path: String,
    format: String,
    ids: Option<Vec<u64>>,
) -> Result<(), String> {
    let snapshot = state.snapshot();
    let targets: Vec<TargetState> = match ids {
        Some(list) => snapshot
            .targets
            .into_iter()
            .filter(|t| list.contains(&t.id))
            .collect(),
        None => snapshot.targets,
    };
    export::write_report(&path, &format, &targets)
}

/// 获取配置文件路径
#[tauri::command]
pub async fn get_config_path(app: AppHandle) -> Result<String, String> {
    config::config_path(&app).map(|p| p.to_string_lossy().to_string())
}
