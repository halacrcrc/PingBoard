// Tauri 命令层：前端 invoke 的入口，全部返回 Result<T, String>
use tauri::{AppHandle, State};

use crate::events::LogEvent;
use crate::model::{FolderEntry, PingSettings, Snapshot, TargetEntry};
use crate::state::{AppState, StopReason};
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
    state.save_config(&app)?;
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

/// 停止 Ping；ids 为 null 表示全部（用户主动停止 → 产生 stop 事件）
#[tauri::command]
pub async fn stop_pinging(
    state: State<'_, AppState>,
    ids: Option<Vec<u64>>,
) -> Result<(), String> {
    state.stop(ids, StopReason::User);
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
///
/// - `ids`：`Some` 仅导出这些 id，`None` 导出全部（**语义未变**）
/// - `filter`：`all`(默认/缺省) / `none_loss`(零丢包) / `all_failed`(全部未成功)。
///   **缺省或 `null` 等价于 `all`**，即筛选功能上线前的原始行为，保证向后兼容。
#[tauri::command]
pub async fn export_report(
    state: State<'_, AppState>,
    path: String,
    format: String,
    ids: Option<Vec<u64>>,
    filter: Option<String>,
) -> Result<(), String> {
    // ⚠️ 只取主机列表，必须走无副作用的 `export_targets()`：
    // `snapshot()` 会 drain_pending()，导出一次报表就会吃掉前端尚未消费的事件增量。
    let filter = export::parse_filter(filter.as_deref())?;
    let targets = state.export_targets(ids.as_deref());
    export::write_report(&path, &format, &targets, filter)
}

/* --------------------- 文件夹（v1.1.10） --------------------- */

/// 列出文件夹及各自台数（第 0 项固定为「临时区」）
#[tauri::command]
pub async fn list_folders(state: State<'_, AppState>) -> Result<Vec<FolderEntry>, String> {
    Ok(state.list_folders())
}

/// 新建文件夹，返回新 id
#[tauri::command]
pub async fn create_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    color: String,
) -> Result<u64, String> {
    let id = state.create_folder(&name, &color)?;
    state.save_config(&app)?;
    Ok(id)
}

/// 重命名 / 改颜色
#[tauri::command]
pub async fn update_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    id: u64,
    name: String,
    color: String,
) -> Result<(), String> {
    state.update_folder(id, &name, &color)?;
    state.save_config(&app)
}

/// 删除文件夹；其下主机迁至 `move_to`（`None` = 临时区）。返回迁移台数。
#[tauri::command]
pub async fn delete_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    id: u64,
    move_to: Option<u64>,
) -> Result<usize, String> {
    let n = state.delete_folder(id, move_to)?;
    state.save_config(&app)?;
    Ok(n)
}

/// 把主机移动到目标文件夹（`None` = 移出到临时区）。返回实际移动台数。
#[tauri::command]
pub async fn move_targets(
    app: AppHandle,
    state: State<'_, AppState>,
    ids: Vec<u64>,
    folder_id: Option<u64>,
) -> Result<usize, String> {
    let n = state.move_targets(&ids, folder_id)?;
    state.save_config(&app)?;
    Ok(n)
}

/// 取走启动期的一次性提示（配置损坏 / 已备份 / 抢救结果），**取走即清空**。
///
/// ⚠️ 不放在 `Snapshot` 里的原因见 `AppState::take_config_notice`：
/// 快照有多个消费者，one-shot 提示会被竞争消费者吞掉。本命令只有前端一个消费者。
#[tauri::command]
pub async fn take_startup_notice(
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    // 无提示时返回 Ok(None)（前端据此不弹 toast）；只有真的拿不到状态锁才会 Err
    Ok(state.take_config_notice())
}

/// 获取配置文件路径
#[tauri::command]
pub async fn get_config_path(app: AppHandle) -> Result<String, String> {
    config::config_path(&app).map(|p| p.to_string_lossy().to_string())
}

/// 读取某主机的最近事件（选中主机回填），按新→旧返回
#[tauri::command]
pub async fn list_events(
    state: State<'_, AppState>,
    target_id: u64,
    limit: usize,
) -> Result<Vec<LogEvent>, String> {
    Ok(state.list_events(target_id, limit))
}

/// 清空某主机的事件（内存 + 文件重写）
#[tauri::command]
pub async fn clear_events(state: State<'_, AppState>, target_id: u64) -> Result<(), String> {
    state.clear_events(target_id);
    Ok(())
}

/// 导出事件为 CSV（本地时间列）；target_id 为 null 表示导出全部主机
#[tauri::command]
pub async fn export_events(
    state: State<'_, AppState>,
    path: String,
    target_id: Option<u64>,
    tz_offset_minutes: i64,
) -> Result<(), String> {
    let events = state.collect_events(target_id);
    export::write_events_csv(&path, &events, tz_offset_minutes)
}

/// 设置单主机「记录事件」开关并持久化
#[tauri::command]
pub async fn set_target_events(
    app: AppHandle,
    state: State<'_, AppState>,
    id: u64,
    events_on: bool,
) -> Result<(), String> {
    state.set_target_events(id, events_on)?;
    state.save_config(&app)
}

/// 枚举系统已安装字体族名（读注册表 HKLM / HKCU Fonts 键，无子进程）。
/// 注册表读取放入阻塞线程池，避免大量枚举时卡住异步运行时。
#[tauri::command]
pub async fn list_system_fonts() -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(crate::fonts::list_system_fonts)
        .await
        .map_err(|e| format!("枚举字体任务失败：{}", e))
}
