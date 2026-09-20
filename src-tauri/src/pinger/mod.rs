// Ping 引擎：两级策略
//   1) 主路径：Win32 IP Helper ICMP API（IcmpSendEcho），普通用户权限即可运行
//   2) 降级路径：系统 ping.exe（隐藏窗口），仅在 ICMP 不可用时启用
pub mod fallback;
pub mod icmp;

use std::sync::atomic::{AtomicBool, Ordering};

use crate::model::Status;

/// 单次探测结果
#[derive(Debug, Clone)]
pub struct ProbeOutcome {
    pub status: Status,
    pub rtt_ms: Option<f64>,
    pub ttl: Option<u32>,
    pub error: Option<String>,
}

impl ProbeOutcome {
    pub fn ok(rtt_ms: f64, ttl: Option<u32>) -> Self {
        Self {
            status: Status::Ok,
            rtt_ms: Some(rtt_ms),
            ttl,
            error: None,
        }
    }

    pub fn timeout() -> Self {
        Self {
            status: Status::Timeout,
            rtt_ms: None,
            ttl: None,
            error: None,
        }
    }

    pub fn failed(msg: impl Into<String>) -> Self {
        Self {
            status: Status::Failed,
            rtt_ms: None,
            ttl: None,
            error: Some(msg.into()),
        }
    }
}

/// 探测引擎枚举；每个工作线程持有各自实例（ICMP 句柄在线程内复用）
pub enum Engine {
    Icmp(icmp::IcmpEngine),
    Fallback(fallback::PingEngine),
}

impl Engine {
    /// 创建引擎：优先 ICMP；一旦某个线程发现 ICMP 不可用，全局降级到 ping.exe
    pub fn new(icmp_failed: &AtomicBool) -> Self {
        if !icmp_failed.load(Ordering::SeqCst) {
            match icmp::IcmpEngine::new() {
                Ok(engine) => return Engine::Icmp(engine),
                Err(e) => {
                    icmp_failed.store(true, Ordering::SeqCst);
                    eprintln!("[pingboard] ICMP 不可用，已降级到 ping.exe：{}", e);
                }
            }
        }
        Engine::Fallback(fallback::PingEngine::new())
    }

    /// 探测一个 IPv4 地址
    pub fn probe(&mut self, ip: &str, timeout_ms: u64, payload_size: u16, ttl: u8) -> ProbeOutcome {
        match self {
            Engine::Icmp(e) => e.probe(ip, timeout_ms, payload_size, ttl),
            Engine::Fallback(e) => e.probe(ip, timeout_ms, false),
        }
    }
}
