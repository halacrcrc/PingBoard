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
mod stats_store;
mod stats;

use tauri::Manager;

/// 窗口标题 = 基础标题 + 版本号。
///
/// 🩸 版本号的**唯一来源**是 `Cargo.toml`（提版本时与另外四处一起改），
/// 标题串里**不再写死**版本号 —— 否则提版本后标题会一直停在旧号，
/// 而这种「少改一处」的错误没有任何测试会红。
pub fn window_title(version: &str) -> String {
    format!("PingBoard — 多主机 Ping 监视器 v{version}")
}

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
            commands::take_startup_notice,
            commands::list_folders,
            commands::create_folder,
            commands::update_folder,
            commands::delete_folder,
            commands::move_targets,
        ])
        .setup(|app| {
            // 0) 窗口标题追加版本号（版本取 Cargo.toml，标题里不再写死一处）
            if let Some(win) = app.get_webview_window("main") {
                let ver = app.package_info().version.to_string();
                let _ = win.set_title(&window_title(&ver));
            }

            let handle = app.handle().clone();

            // 1) 读取配置并初始化状态（失败不阻塞启动）
            let (cfg, notice) = config::load(&handle);
            {
                let st = app.state::<state::AppState>();
                st.init_from_config(&cfg);
                // 配置损坏 / 抢救过的提示交给前端展示（release 无控制台，eprintln 看不见）
                st.set_config_notice(notice);
            }

            // 1.1) 事件日志：注入默认目录、应用持久化配置并读回历史
            if let Ok(base_dir) = config::app_config_dir(&handle) {
                let st = app.state::<state::AppState>();
                st.configure_events(base_dir);
                st.load_events_from_file();
            }

            // 1.2) 统计持久化：注入目录并读回上次运行的累计统计与趋势
            //      （未清空统计就关应用时，重启后数据仍在）
            if let Ok(base_dir) = config::app_config_dir(&handle) {
                let st = app.state::<state::AppState>();
                st.configure_stats(base_dir);
            }

            // 2) 启动聚合快照发射任务（每 500ms 一次）
            state::spawn_snapshot_emitter(handle.clone());

            // 2.1) 统计定时落盘（每 30s 一次）：防异常退出 / 断电丢数据
            {
                let handle = app.handle().clone();
                std::thread::spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_secs(30));
                    let st = handle.state::<state::AppState>();
                    st.save_stats_to_file();
                });
            }

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

#[cfg(test)]
mod tests {
    use super::window_title;

    /// 标题 = 基础标题 + 注入的版本号（版本号不得来自标题串内部）
    #[test]
    fn qa_window_title_carries_injected_version() {
        assert_eq!(window_title("9.9.9"), "PingBoard — 多主机 Ping 监视器 v9.9.9");
    }

    /// 用**真实包版本**调用时，标题必须正好以它结尾 ——
    /// 回滚成「标题里写死版本号」的写法时，提版本后这条会红。
    #[test]
    fn qa_window_title_uses_package_version_tail() {
        let v = env!("CARGO_PKG_VERSION");
        let t = window_title(v);
        assert!(
            t.ends_with(&format!("v{v}")),
            "标题尾部应为当前包版本 v{v}，实际：{t}"
        );
        assert_eq!(
            t.matches('v').count(),
            1,
            "标题里只应出现一个版本前缀，实际：{t}"
        );
    }

    /// 🩸 接线哨兵：纯函数测试抓不到「setup 里忘了调用」——
    /// 本项目已有 9 个「纯函数全绿但功能是坏的」先例，坏的正是 wiring。
    /// 这里直接断言本文件确实把结果 set_title 到了 main 窗口。
    #[test]
    fn qa_window_title_is_wired_into_setup() {
        let src = include_str!("lib.rs");
        // 🩸 needle 必须用 concat! 拆开构造：若直接写字面量，本行会出现在 src 里，
        // 断言就会「自指恒真」（第一次写这条时正是这么假绿的）。
        let call = concat!("set_title(&window", "_title(&ver))");
        assert!(
            src.contains(call),
            "setup 必须把 window_title 的结果 set_title 到窗口，否则标题不会变"
        );
        let win = concat!("get_webview_window(", "\"main\")");
        assert!(src.contains(win), "必须作用于 main 窗口");
    }
}
