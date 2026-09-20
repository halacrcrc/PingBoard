// 统计逻辑（纯函数，便于单元测试）
use crate::model::{PingSettings, Status, TargetState};

/// 丢包率（%）：`failed / sent * 100`；`sent == 0` 时返回 0
pub fn loss_pct(sent: u64, failed: u64) -> f64 {
    if sent == 0 {
        0.0
    } else {
        failed as f64 / sent as f64 * 100.0
    }
}

/// 追加一条历史记录并裁掉最早的多余项，仅保留最近 `len` 条
pub fn push_history(history: &mut Vec<Option<f64>>, value: Option<f64>, len: usize) {
    history.push(value);
    if len > 0 && history.len() > len {
        let drop_n = history.len() - len;
        history.drain(0..drop_n);
    }
}

/// 成功探测后更新统计：
/// sent+1、received+1，更新 last/min/max/avg，重置连续失败计数，记录最后成功时间
pub fn apply_success(
    state: &mut TargetState,
    rtt_ms: f64,
    ttl: Option<u32>,
    now: u64,
    history_len: usize,
) {
    state.sent += 1;
    state.received += 1;
    state.last_rtt_ms = Some(rtt_ms);
    state.sum_rtt_ms += rtt_ms;
    state.avg_rtt_ms = Some(state.sum_rtt_ms / state.received as f64);
    state.min_rtt_ms = Some(match state.min_rtt_ms {
        Some(m) => m.min(rtt_ms),
        None => rtt_ms,
    });
    state.max_rtt_ms = Some(match state.max_rtt_ms {
        Some(m) => m.max(rtt_ms),
        None => rtt_ms,
    });
    if let Some(t) = ttl {
        if t > 0 {
            state.ttl = Some(t);
        }
    }
    state.last_success_ts = Some(now);
    state.consecutive_fail = 0;
    state.last_rtt_ms = Some(rtt_ms);
    state.loss_pct = loss_pct(state.sent, state.failed);
    state.status = Status::Ok;
    state.last_error = None;
    push_history(&mut state.history, Some(rtt_ms), history_len);
}

/// 失败 / 超时后更新统计：
/// sent+1、failed+1，连续失败计数 +1，最近延迟置空
pub fn apply_failure(state: &mut TargetState, history_len: usize, status: Status) {
    state.sent += 1;
    state.failed += 1;
    state.consecutive_fail += 1;
    state.last_rtt_ms = None;
    state.loss_pct = loss_pct(state.sent, state.failed);
    state.status = status;
    push_history(&mut state.history, None, history_len);
}

/// 归一化设置项，保证取值范围合法
pub fn normalize_settings(s: &mut PingSettings) {
    s.interval_ms = s.interval_ms.clamp(100, 600_000);
    s.timeout_ms = s.timeout_ms.clamp(100, 60_000);
    s.payload_size = s.payload_size.min(65_500);
    s.ttl = s.ttl.clamp(1, 255);
    s.max_threads = s.max_threads.clamp(1, 1024);
    s.history_len = s.history_len.clamp(10, 600);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TargetState;

    fn sample() -> TargetState {
        TargetState::new(1, "测试".into(), "127.0.0.1".into())
    }

    #[test]
    fn loss_pct_zero_sent() {
        assert_eq!(loss_pct(0, 0), 0.0);
    }

    #[test]
    fn loss_pct_basic() {
        assert!((loss_pct(4, 1) - 25.0).abs() < 1e-9);
        assert!((loss_pct(3, 3) - 100.0).abs() < 1e-9);
    }

    #[test]
    fn apply_success_updates_all_fields() {
        let mut s = sample();
        apply_success(&mut s, 10.0, Some(117), 111, 60);
        apply_success(&mut s, 30.0, Some(118), 222, 60);

        assert_eq!(s.sent, 2);
        assert_eq!(s.received, 2);
        assert_eq!(s.failed, 0);
        assert_eq!(s.last_rtt_ms, Some(30.0));
        assert_eq!(s.min_rtt_ms, Some(10.0));
        assert_eq!(s.max_rtt_ms, Some(30.0));
        assert_eq!(s.avg_rtt_ms, Some(20.0));
        assert_eq!(s.ttl, Some(118));
        assert_eq!(s.last_success_ts, Some(222));
        assert_eq!(s.consecutive_fail, 0);
        assert_eq!(s.loss_pct, 0.0);
        assert_eq!(s.status, Status::Ok);
        assert_eq!(s.history, vec![Some(10.0), Some(30.0)]);
    }

    #[test]
    fn apply_failure_increments_and_nulls_last() {
        let mut s = sample();
        apply_success(&mut s, 10.0, Some(117), 1, 60);
        apply_failure(&mut s, 60, Status::Timeout);

        assert_eq!(s.sent, 2);
        assert_eq!(s.received, 1);
        assert_eq!(s.failed, 1);
        assert_eq!(s.last_rtt_ms, None);
        assert_eq!(s.consecutive_fail, 1);
        assert_eq!(s.status, Status::Timeout);
        assert!((s.loss_pct - 50.0).abs() < 1e-9);
        assert_eq!(s.history, vec![Some(10.0), None]);
        // 失败不清除历史极值
        assert_eq!(s.min_rtt_ms, Some(10.0));
        assert_eq!(s.max_rtt_ms, Some(10.0));
    }

    #[test]
    fn history_trims_to_len() {
        let mut hist: Vec<Option<f64>> = Vec::new();
        for i in 0..100 {
            push_history(&mut hist, Some(i as f64), 60);
        }
        assert_eq!(hist.len(), 60);
        assert_eq!(hist[0], Some(40.0));
        assert_eq!(hist[59], Some(99.0));
    }

    #[test]
    fn normalize_clamps_out_of_range() {
        let mut s = PingSettings {
            interval_ms: 1,
            timeout_ms: 10,
            payload_size: 65_535,
            ttl: 0,
            max_threads: 0,
            beep_on_fail: false,
            auto_start: false,
            history_len: 1,
        };
        normalize_settings(&mut s);
        assert_eq!(s.interval_ms, 100);
        assert_eq!(s.timeout_ms, 100);
        assert_eq!(s.payload_size, 65_500);
        assert_eq!(s.ttl, 1);
        assert_eq!(s.max_threads, 1);
        assert_eq!(s.history_len, 10);
    }

    /* ===================== QA 新增边界用例 ===================== */

    /// 统计口径：loss_pct = failed / sent * 100，且 sent == 0 时不得出现 NaN / 除零
    #[test]
    fn qa_loss_pct_formula_and_no_nan() {
        // 除零保护
        assert_eq!(loss_pct(0, 0), 0.0);
        assert_eq!(loss_pct(0, 7), 0.0);
        assert!(!loss_pct(0, 7).is_nan());
        assert!(loss_pct(0, 7).is_finite());
        // 公式 = failed / sent * 100
        assert!((loss_pct(1, 1) - 100.0).abs() < 1e-9);
        assert!((loss_pct(8, 2) - 25.0).abs() < 1e-9);
        assert!((loss_pct(10, 1) - 10.0).abs() < 1e-9);
        assert!((loss_pct(10, 0) - 0.0).abs() < 1e-9);
        assert!((loss_pct(3, 1) - 33.3333333).abs() < 1e-6);
    }

    /// 首包：min/max 都等于该包；后续包正确更新极值
    #[test]
    fn qa_min_max_first_and_subsequent_packets() {
        let mut s = sample();
        apply_success(&mut s, 50.0, Some(64), 1, 60);
        assert_eq!(s.min_rtt_ms, Some(50.0), "首包 min 应等于该包 RTT");
        assert_eq!(s.max_rtt_ms, Some(50.0), "首包 max 应等于该包 RTT");
        assert_eq!(s.avg_rtt_ms, Some(50.0));

        apply_success(&mut s, 20.0, Some(64), 2, 60);
        assert_eq!(s.min_rtt_ms, Some(20.0));
        assert_eq!(s.max_rtt_ms, Some(50.0));

        apply_success(&mut s, 80.0, Some(64), 3, 60);
        assert_eq!(s.min_rtt_ms, Some(20.0));
        assert_eq!(s.max_rtt_ms, Some(80.0));
        // avg = (50+20+80)/3 = 50.0
        assert!((s.avg_rtt_ms.unwrap() - 50.0).abs() < 1e-9);
    }

    /// 连续失败计数：成功后必须清零
    #[test]
    fn qa_consecutive_fail_reset_on_success() {
        let mut s = sample();
        apply_failure(&mut s, 60, Status::Timeout);
        apply_failure(&mut s, 60, Status::Timeout);
        apply_failure(&mut s, 60, Status::Timeout);
        assert_eq!(s.consecutive_fail, 3);
        apply_success(&mut s, 12.0, Some(64), 100, 60);
        assert_eq!(s.consecutive_fail, 0, "成功一次后连续失败计数必须归零");
        assert_eq!(s.status, Status::Ok);
        assert_eq!(s.failed, 3);
        assert_eq!(s.received, 1);
        assert_eq!(s.sent, 4);
        // 丢包率随失败次数累计
        assert!((s.loss_pct - 75.0).abs() < 1e-9);
    }

    /// history 环形缓冲：失败/超时写入 None；保留最近 N 条
    #[test]
    fn qa_history_keeps_recent_and_none_for_failures() {
        let mut s = sample();
        for i in 0..5 {
            apply_success(&mut s, i as f64, Some(64), i as u64, 60);
        }
        for _ in 0..3 {
            apply_failure(&mut s, 60, Status::Timeout);
        }
        assert_eq!(s.history.len(), 8);
        assert_eq!(&s.history[0..5], &[Some(0.0), Some(1.0), Some(2.0), Some(3.0), Some(4.0)]);
        assert_eq!(&s.history[5..8], &[None, None, None], "失败/超时应写入 None");
    }

    /// history 环形缓冲恰好为 len：连续 100 次写入只保留最近 60 条
    #[test]
    fn qa_history_ring_buffer_exact_len() {
        let mut hist: Vec<Option<f64>> = Vec::new();
        for i in 0..100u32 {
            let v = if i % 10 == 9 { None } else { Some(i as f64) };
            push_history(&mut hist, v, 60);
        }
        assert_eq!(hist.len(), 60, "长度必须恰为 history_len");
        assert_eq!(hist[0], Some(40.0), "应保留最近 60 条（从第 40 次开始）");
        assert_eq!(hist[59], None, "最后一次（i=99）为失败 → None");
    }

    /// 设置归一化：上下界全覆盖
    #[test]
    fn qa_normalize_clamps_upper_and_lower() {
        let mut s = PingSettings {
            interval_ms: u64::MAX,
            timeout_ms: u64::MAX,
            payload_size: u16::MAX,
            ttl: 200,
            max_threads: usize::MAX,
            beep_on_fail: true,
            auto_start: true,
            history_len: usize::MAX,
        };
        normalize_settings(&mut s);
        assert_eq!(s.interval_ms, 600_000);
        assert_eq!(s.timeout_ms, 60_000);
        assert_eq!(s.payload_size, 65_500, "payload_size 上界应为 65500");
        // ttl 为 u8，合法上界即 255；此处验证合法值不被篡改
        assert_eq!(s.ttl, 200, "合法 TTL 不应被改动");
        assert_eq!(s.max_threads, 1024);
        assert_eq!(s.history_len, 600);

        // 合法值不应被改动
        let mut ok = PingSettings::default();
        normalize_settings(&mut ok);
        assert_eq!(ok.interval_ms, 1000);
        assert_eq!(ok.timeout_ms, 2000);
        assert_eq!(ok.payload_size, 32);
        assert_eq!(ok.ttl, 128);
        assert_eq!(ok.max_threads, 256);
        assert_eq!(ok.history_len, 60);
    }

    /// push_history 在 len == 0 时不裁剪也不 panic
    #[test]
    fn qa_push_history_len_zero_no_panic() {
        let mut h: Vec<Option<f64>> = Vec::new();
        push_history(&mut h, Some(1.0), 0);
        push_history(&mut h, None, 0);
        assert_eq!(h.len(), 2);
    }
}
