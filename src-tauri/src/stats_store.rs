// 统计持久化（v1.1.10）：把 TargetState 的统计与趋势 history 落盘，重启后恢复
//
// 为什么独立成文件而不是塞进 config.json：
//   1) 写入频率差异巨大 —— 配置只在增删改时写，统计每 500ms 就变；
//      混在一起会让配置文件频繁落盘。
//   2) 职责分离 —— 配置是「用户意图」，统计是「运行时数据」。
//   3) 有先例 —— 事件日志已单独落盘 pingboard-events.jsonl。
//
// 兼容性约束（见 docs/folder-design.md 5.1 与兼容性检查 C3、C4）：
//   C3 「清空统计」必须**删除本文件**，否则重启后旧统计复活。
//   C4 解析失败必须**整体降级为空统计**，绝不触发「配置文件已损坏」提示 ——
//      它不是配置文件，那条链路会误导用户去恢复一个根本没坏的东西。
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::model::TargetState;

/// 持久化文件名（放在 app_config_dir 下）
pub const STATS_FILE: &str = "pingboard-stats.json";

/// 单个目标的统计快照。
///
/// 每个字段都带 serde 默认值：将来 TargetState 增删字段时，
/// 旧文件仍能读出一部分统计，而不是整份报废。
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct TargetStats {
    #[serde(default)]
    pub sent: u64,
    #[serde(default)]
    pub received: u64,
    #[serde(default)]
    pub failed: u64,
    #[serde(default)]
    pub sum_rtt_ms: f64,
    #[serde(default)]
    pub min_rtt_ms: Option<f64>,
    #[serde(default)]
    pub max_rtt_ms: Option<f64>,
    #[serde(default)]
    pub avg_rtt_ms: Option<f64>,
    #[serde(default)]
    pub last_rtt_ms: Option<f64>,
    #[serde(default)]
    pub ttl: Option<u32>,
    #[serde(default)]
    pub consecutive_fail: u32,
    #[serde(default)]
    pub loss_pct: f64,
    #[serde(default)]
    pub last_success_ts: Option<u64>,
    /// 趋势图数据（用户明确要求一并保留，重启后曲线与关闭前一致）
    #[serde(default)]
    pub history: Vec<Option<f64>>,
}

impl TargetStats {
    /// 从运行时状态取一份快照
    pub fn from_state(s: &TargetState) -> Self {
        Self {
            sent: s.sent,
            received: s.received,
            failed: s.failed,
            sum_rtt_ms: s.sum_rtt_ms,
            min_rtt_ms: s.min_rtt_ms,
            max_rtt_ms: s.max_rtt_ms,
            avg_rtt_ms: s.avg_rtt_ms,
            last_rtt_ms: s.last_rtt_ms,
            ttl: s.ttl,
            consecutive_fail: s.consecutive_fail,
            loss_pct: s.loss_pct,
            last_success_ts: s.last_success_ts,
            history: s.history.clone(),
        }
    }

    /// 写回运行时状态。
    ///
    /// 刻意**不写** `status` 与 `running`：它们是「本次运行的瞬时状态」，
    /// 重启后应回到「未开始 / 未运行」。若把上次的 `ok` 恢复出来，
    /// 用户会看到一台刚启动、其实还没 ping 过的机器显示「正常」。
    pub fn apply_to_state(&self, s: &mut TargetState) {
        s.sent = self.sent;
        s.received = self.received;
        s.failed = self.failed;
        s.sum_rtt_ms = self.sum_rtt_ms;
        s.min_rtt_ms = self.min_rtt_ms;
        s.max_rtt_ms = self.max_rtt_ms;
        s.avg_rtt_ms = self.avg_rtt_ms;
        s.last_rtt_ms = self.last_rtt_ms;
        s.ttl = self.ttl;
        s.consecutive_fail = self.consecutive_fail;
        s.loss_pct = self.loss_pct;
        s.last_success_ts = self.last_success_ts;
        s.history = self.history.clone();
        s.status = crate::model::Status::Idle;
        s.running = false;
        s.resolved_ip = None;
    }
}
/// 落盘文件的整体结构
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct StatsFile {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub saved_at: u64,
    /// key = 目标 id
    #[serde(default)]
    pub targets: HashMap<u64, TargetStats>,
}

/// 统计文件路径
pub fn stats_path(base_dir: &Path) -> PathBuf {
    base_dir.join(STATS_FILE)
}

/// 读取统计。任何失败都降级为空 map，绝不返回 Err。
///
/// 兼容性检查 C4：这里不走配置的「备份 + 抢救 + 弹提示」链路 ——
/// 一个统计文件写坏不该让用户去恢复「配置文件」。
pub fn load(base_dir: &Path) -> HashMap<u64, TargetStats> {
    let p = stats_path(base_dir);
    let text = match std::fs::read_to_string(&p) {
        Ok(t) => t,
        Err(_) => return HashMap::new(), // 首次启动 / 读不到
    };
    match serde_json::from_str::<StatsFile>(&text) {
        Ok(f) => f.targets,
        Err(e) => {
            eprintln!("[pingboard] 统计文件解析失败，已按空统计处理：{}", e);
            HashMap::new()
        }
    }
}

/// 写回统计。
///
/// `live_ids` 用于剔除已删除目标的条目 —— 否则文件会随使用无限膨胀。
/// `history_len` 按当前设置裁剪每台机的 history。
/// 写失败只打印告警，不得阻塞退出或停止流程。
pub fn save(
    base_dir: &Path,
    map: &HashMap<u64, TargetStats>,
    live_ids: &[u64],
    history_len: usize,
) -> Result<(), String> {
    let p = stats_path(base_dir);
    let mut targets: HashMap<u64, TargetStats> = HashMap::new();
    for (id, st) in map.iter() {
        if !live_ids.contains(id) {
            continue; // 目标已删除，不落盘
        }
        let mut st = st.clone();
        if history_len > 0 && st.history.len() > history_len {
            let drop_n = st.history.len() - history_len;
            st.history.drain(0..drop_n);
        }
        targets.insert(*id, st);
    }
    let file = StatsFile {
        version: 1,
        saved_at: crate::state::now_ms(),
        targets,
    };
    let text = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(&p, text).map_err(|e| format!("{}", e))
}

/// 删除统计文件（「清空统计」时调用）。
///
/// 兼容性检查 C3：必须是删除而不是写空 —— 写空的话，
/// 「清空统计」后重启会读回一个空文件，用户会以为统计没被清掉。
pub fn clear(base_dir: &Path) {
    let p = stats_path(base_dir);
    let _ = std::fs::write(&p, "{}");
    if let Err(e) = std::fs::remove_file(&p) {
        if e.kind() != std::io::ErrorKind::NotFound {
            eprintln!("[pingboard] 删除统计文件失败：{}", e);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Status;
    use std::fs;

    fn tmp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pb_stats_{}_{}", tag, crate::state::now_ms()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn full_state(id: u64) -> TargetState {
        let mut s = TargetState::new(id, "n".into(), format!("10.0.0.{}", id));
        s.status = Status::Ok;
        s.sent = 10;
        s.received = 9;
        s.failed = 1;
        s.sum_rtt_ms = 90.0;
        s.min_rtt_ms = Some(8.0);
        s.max_rtt_ms = Some(15.0);
        s.avg_rtt_ms = Some(10.0);
        s.last_rtt_ms = Some(11.0);
        s.ttl = Some(52);
        s.consecutive_fail = 0;
        s.loss_pct = 10.0;
        s.last_success_ts = Some(1_700_000_000_000);
        s.history = vec![Some(10.0), None, Some(12.0)];
        s
    }

    /// 往返：写入后读回，所有统计字段一致
    #[test]
    fn qa_stats_roundtrip_all_fields() {
        let d = tmp_dir("rt");
        let mut m = HashMap::new();
        m.insert(1u64, TargetStats::from_state(&full_state(1)));
        save(&d, &m, &[1], 60).unwrap();
        let back = load(&d);
        let got = back.get(&1).expect("应读回目标 1");
        assert_eq!(got.sent, 10);
        assert_eq!(got.received, 9);
        assert_eq!(got.failed, 1);
        assert_eq!(got.sum_rtt_ms, 90.0);
        assert_eq!(got.min_rtt_ms, Some(8.0));
        assert_eq!(got.ttl, Some(52));
        assert_eq!(got.loss_pct, 10.0);
        assert_eq!(got.last_success_ts, Some(1_700_000_000_000));
        assert_eq!(got.history, vec![Some(10.0), None, Some(12.0)], "趋势 history 必须完整保留");
        let _ = fs::remove_dir_all(d);
    }

    /// C4：文件损坏 / 内容不是对象时降级为空统计，绝不 panic
    #[test]
    fn qa_load_degrades_on_corrupt() {
        for bad in ["{ 不是 json", "[]", "null", "{\"targets\":5}"] {
            let d = tmp_dir("bad");
            fs::write(stats_path(&d), bad).unwrap();
            assert!(load(&d).is_empty(), "损坏内容 {:?} 应降级为空统计", bad);
            let _ = fs::remove_dir_all(d);
        }
    }

    /// 目标已删除时其统计条目不得落盘（否则文件无限膨胀）
    #[test]
    fn qa_save_prunes_deleted_targets() {
        let d = tmp_dir("prune");
        let mut m = HashMap::new();
        m.insert(1u64, TargetStats::from_state(&full_state(1)));
        m.insert(2u64, TargetStats::from_state(&full_state(2)));
        save(&d, &m, &[1], 60).unwrap();
        let back = load(&d);
        assert!(back.contains_key(&1), "存活目标应保留");
        assert!(!back.contains_key(&2), "已删除目标必须被剔除");
        let _ = fs::remove_dir_all(d);
    }

    /// history 按当前设置裁剪（调小 history_len 后不留超长数组）
    #[test]
    fn qa_save_trims_history_to_setting() {
        let d = tmp_dir("trim");
        let mut st = full_state(1);
        st.history = (0..50).map(|i| Some(i as f64)).collect();
        let mut m = HashMap::new();
        m.insert(1u64, TargetStats::from_state(&st));
        save(&d, &m, &[1], 10).unwrap();
        let back = load(&d);
        let h = &back.get(&1).unwrap().history;
        assert_eq!(h.len(), 10, "应裁剪到 10");
        assert_eq!(h[0], Some(40.0), "应保留最新的点（裁最早）");
        let _ = fs::remove_dir_all(d);
    }

    /// C3：clear 删除文件后 load 应读回空（重启不复活旧统计）
    #[test]
    fn qa_clear_removes_file_so_restart_does_not_revive() {
        let d = tmp_dir("clear");
        let mut m = HashMap::new();
        m.insert(1u64, TargetStats::from_state(&full_state(1)));
        save(&d, &m, &[1], 60).unwrap();
        assert!(stats_path(&d).exists(), "前提：文件应已写入");
        clear(&d);
        assert!(!stats_path(&d).exists(), "clear 应删除文件（不是写空）");
        assert!(load(&d).is_empty(), "重启后应读到空统计");
        clear(&d); // 幂等：文件不存在时再 clear 也不报错
        let _ = fs::remove_dir_all(d);
    }

    /// apply_to_state 不恢复 status / running（重启后应为「未开始 / 未运行」）
    #[test]
    fn qa_apply_does_not_restore_transient_status() {
        let saved = TargetStats::from_state(&full_state(1));
        let mut fresh = TargetState::new(1, "n".into(), "10.0.0.1".into());
        saved.apply_to_state(&mut fresh);
        assert_eq!(fresh.status, Status::Idle, "不得把上次的 Ok 恢复出来");
        assert!(!fresh.running, "不得把上次的运行中恢复出来");
        assert_eq!(fresh.sent, 10, "但累计统计必须恢复");
        assert_eq!(fresh.history.len(), 3, "趋势必须恢复");
    }
}
