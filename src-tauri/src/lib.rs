// PingBoard 后端入口：装配插件、状态、命令与后台任务
mod commands;
mod config;
mod events;
mod export;
mod fonts;
mod import;
mod model;
mod pinger;
mod state;
mod stats;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        // 仅引入 dialog 插件（导出时选择保存路径）
        .plugin(tauri_plugin_dialog::init())
        .manage(state::AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::add_targets,
            commands::remove_targets,
            commands::update_target,
            commands::set_enabled,
            commands::start_pinging,
            commands::stop_pinging,
            commands::reset_stats,
            commands::update_settings,
            commands::resolve_host,
            commands::export_report,
            commands::get_config_path,
            import::read_import_file,
            commands::list_events,
            commands::clear_events,
            commands::export_events,
            commands::set_target_events,
            commands::list_system_fonts,
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            // 1) 读取配置并初始化状态（失败不阻塞启动）
            let cfg = config::load(&handle);
            {
                let st = app.state::<state::AppState>();
                st.init_from_config(&cfg);
            }

            // 1.1) 事件日志：注入默认目录、应用持久化配置并读回历史
            if let Ok(base_dir) = config::app_config_dir(&handle) {
                let st = app.state::<state::AppState>();
                st.configure_events(base_dir);
                st.load_events_from_file();
            }

            // 2) 启动聚合快照发射任务（每 500ms 一次）
            state::spawn_snapshot_emitter(handle.clone());

            // 3) 自动开始（可选）
            if cfg.settings.auto_start {
                let st = app.state::<state::AppState>();
                if let Err(e) = st.start(None) {
                    eprintln!("[pingboard] 自动启动失败：{}", e);
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                let app = window.app_handle();
                let st = app.state::<state::AppState>();
                if let Err(e) = st.save_config(app) {
                    eprintln!("[pingboard] 退出前保存配置失败：{}", e);
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("PingBoard 启动失败");
}
