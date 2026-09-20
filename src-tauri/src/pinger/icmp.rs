// 主路径：Win32 IP Helper ICMP API
// 关键点：
//   * 使用 IcmpCreateFile / IcmpSendEcho / IcmpCloseHandle，**不需要管理员权限**
//     （绝不使用 SOCK_RAW / IPPROTO_ICMP 之类的原始套接字方案）
//   * 每个工作线程复用一个 IcmpCreateFile 句柄，循环收发，避免频繁创建/销毁
use std::ffi::c_void;
use std::io;
use std::mem::size_of;
use std::ptr;

use windows::Win32::Foundation::HANDLE;
use windows::Win32::NetworkManagement::IpHelper::{
    IcmpCloseHandle, IcmpCreateFile, IcmpSendEcho, ICMP_ECHO_REPLY, IP_OPTION_INFORMATION,
};

use super::ProbeOutcome;
use crate::model::Status;

/// IP_SUCCESS 状态码
const IP_SUCCESS: u32 = 0;
/// IP_REQ_TIMED_OUT 状态码 / Win32 错误码
const IP_REQ_TIMED_OUT: u32 = 11010;

/// ICMP 引擎：持有可复用的句柄与回复缓冲区
///
/// 说明：回复缓冲区使用 `Vec<u64>` 作为底层存储，保证天然 8 字节对齐，
/// 从而可以安全地把缓冲区首地址转换为 `*const ICMP_ECHO_REPLY`。
pub struct IcmpEngine {
    handle: HANDLE,
    reply_buf: Vec<u64>,
}

impl IcmpEngine {
    /// 打开 ICMP 句柄；失败（例如权限不足）时返回错误，由上层降级到 ping.exe
    pub fn new() -> io::Result<Self> {
        // windows 0.61 的 IcmpCreateFile 返回 Result<HANDLE>（句柄无效时给出 Win32 错误）
        let handle = match unsafe { IcmpCreateFile() } {
            Ok(h) => h,
            Err(e) => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("IcmpCreateFile 失败：{}", e),
                ))
            }
        };
        Ok(Self {
            handle,
            reply_buf: Vec::new(),
        })
    }

    /// 发送一次 ICMP Echo 请求
    pub fn probe(&mut self, ip: &str, timeout_ms: u64, payload_size: u16, ttl: u8) -> ProbeOutcome {
        let addr: std::net::Ipv4Addr = match ip.parse() {
            Ok(a) => a,
            Err(_) => return ProbeOutcome::failed(format!("非 IPv4 地址：{}", ip)),
        };
        // IcmpSendEcho 期望网络字节序的 IN_ADDR 值
        let dest: u32 = u32::from_ne_bytes(addr.octets());

        // 构造请求负载
        let request: Vec<u8> = (0..payload_size as usize).map(|i| (i % 256) as u8).collect();

        // 回复缓冲区：ICMP_ECHO_REPLY + 请求负载 + 8 字节 ICMP 头（按 8 字节对齐向上取整）
        let need_bytes = size_of::<ICMP_ECHO_REPLY>() + request.len() + 8;
        let need_u64 = need_bytes.div_ceil(8);
        if self.reply_buf.len() < need_u64 {
            self.reply_buf.resize(need_u64, 0);
        }
        let reply_size_bytes = (self.reply_buf.len() * size_of::<u64>()) as u32;

        let options = IP_OPTION_INFORMATION {
            Ttl: ttl,
            Tos: 0,
            Flags: 0,
            OptionsSize: 0,
            OptionsData: ptr::null_mut(),
        };

        // 空负载时传空指针 + 尺寸 0
        let request_ptr: *const c_void = if request.is_empty() {
            ptr::null()
        } else {
            request.as_ptr() as *const c_void
        };

        let timeout_u32 = if timeout_ms > u32::MAX as u64 {
            u32::MAX
        } else {
            timeout_ms as u32
        };

        let replies = unsafe {
            IcmpSendEcho(
                self.handle,
                dest,
                request_ptr,
                request.len() as u16,
                Some(&options as *const IP_OPTION_INFORMATION),
                self.reply_buf.as_mut_ptr() as *mut c_void,
                reply_size_bytes,
                timeout_u32,
            )
        };

        if replies == 0 {
            let err = io::Error::last_os_error();
            let code = err.raw_os_error().unwrap_or(0) as u32;
            if code == IP_REQ_TIMED_OUT {
                return ProbeOutcome::timeout();
            }
            return ProbeOutcome::failed(format!("IcmpSendEcho 失败（错误码 {}）：{}", code, err));
        }

        // 解析第一条回复（缓冲区已按 8 字节对齐）
        let reply = unsafe { &*(self.reply_buf.as_ptr() as *const ICMP_ECHO_REPLY) };
        if reply.Status == IP_SUCCESS {
            let rtt = reply.RoundTripTime as f64;
            let reply_ttl = reply.Options.Ttl;
            let ttl_out = if reply_ttl > 0 {
                reply_ttl as u32
            } else {
                ttl as u32
            };
            ProbeOutcome::ok(rtt, Some(ttl_out))
        } else if reply.Status == IP_REQ_TIMED_OUT {
            ProbeOutcome::timeout()
        } else {
            ProbeOutcome {
                status: Status::Failed,
                rtt_ms: None,
                ttl: None,
                error: Some(format!("ICMP 状态码 {}", reply.Status)),
            }
        }
    }
}

impl Drop for IcmpEngine {
    fn drop(&mut self) {
        unsafe {
            // 关闭句柄，忽略返回值
            let _ = IcmpCloseHandle(self.handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 运行时验证：普通用户权限下即可创建 ICMP 句柄并完成一次回环探测。
    /// 这条测试同时验证了 windows crate 的调用签名在运行期正确、且不依赖管理员权限。
    #[test]
    fn icmp_loopback_works_without_admin() {
        let mut engine = match IcmpEngine::new() {
            Ok(e) => e,
            Err(e) => {
                // 极少数环境完全禁用 ICMP，此时跳过（并打印原因），不判定为失败
                eprintln!("[skip] 当前环境无法创建 ICMP 句柄：{}", e);
                return;
            }
        };
        let out = engine.probe("127.0.0.1", 2000, 32, 128);
        eprintln!("[icmp] 回环探测结果：{:?}", out);
        assert_ne!(
            out.status,
            Status::Failed,
            "回环探测不应出现 Failed 状态：{:?}",
            out
        );
        assert_eq!(out.status, Status::Ok, "回环探测应当成功");
        assert!(out.rtt_ms.is_some(), "成功探测应当带有 RTT");
    }

    /// 校验网络字节序转换：192.168.1.1 的 IN_ADDR 值应为 0x0101A8C0（小端）
    #[test]
    fn addr_conversion_is_network_order() {
        let addr: std::net::Ipv4Addr = "192.168.1.1".parse().unwrap();
        assert_eq!(u32::from_ne_bytes(addr.octets()), 0x0101_A8C0);
    }

    /* ===================== QA 新增：真实网络端到端（不 mock） ===================== */

    fn engine_or_skip() -> Option<IcmpEngine> {
        match IcmpEngine::new() {
            Ok(e) => Some(e),
            Err(e) => {
                eprintln!("[skip] 当前环境无法创建 ICMP 句柄：{}", e);
                None
            }
        }
    }

    /// 回环探测必须成功且带 RTT / TTL
    #[test]
    fn qa_loopback_ok_with_rtt_and_ttl() {
        let mut e = match engine_or_skip() {
            Some(e) => e,
            None => return,
        };
        let out = e.probe("127.0.0.1", 2000, 32, 128);
        eprintln!("[qa-icmp] 127.0.0.1 => {:?}", out);
        assert_eq!(out.status, Status::Ok, "回环应成功：{:?}", out);
        assert!(out.rtt_ms.is_some(), "成功探测应带 RTT");
        assert!(out.ttl.is_some(), "成功探测应带 TTL");
    }

    /// 真实外网目标探测（阿里 DNS / 114 DNS）
    #[test]
    fn qa_external_ipv4_ok() {
        let mut e = match engine_or_skip() {
            Some(e) => e,
            None => return,
        };
        let mut any_ok = false;
        for host in ["223.5.5.5", "114.114.114.114"] {
            let out = e.probe(host, 2000, 32, 128);
            eprintln!("[qa-icmp] {} => {:?}", host, out);
            if out.status == Status::Ok {
                any_ok = true;
                assert!(out.rtt_ms.unwrap() >= 0.0);
                assert!(out.ttl.unwrap() > 0, "外网回复 TTL 应大于 0");
            }
        }
        assert!(any_ok, "至少应有一个外网目标探测成功（否则为环境网络限制）");
    }

    /// 不可达地址必须在 timeout 预算内返回，且不 panic / 不卡死
    #[test]
    fn qa_unreachable_returns_within_budget() {
        let mut e = match engine_or_skip() {
            Some(e) => e,
            None => return,
        };
        let start = std::time::Instant::now();
        let out = e.probe("192.0.2.1", 800, 32, 128);
        let elapsed = start.elapsed().as_millis();
        eprintln!("[qa-icmp] 192.0.2.1 => {:?} 用时 {}ms", out, elapsed);
        assert_ne!(out.status, Status::Ok, "保留测试网段不应判定成功");
        assert!(elapsed < 5000, "应快速返回，实际 {}ms", elapsed);
    }

    /// 非 IPv4 字符串 → Failed 且带可读错误
    #[test]
    fn qa_non_ipv4_input_is_failed() {
        let mut e = match engine_or_skip() {
            Some(e) => e,
            None => return,
        };
        let out = e.probe("no-such-host.invalid", 500, 32, 128);
        eprintln!("[qa-icmp] no-such-host.invalid => {:?}", out);
        assert_eq!(out.status, Status::Failed);
        assert!(out.error.is_some());
    }

    /// 复用一个句柄连续探测多次，`sent` 语义（每次调用一次结果）应稳定
    #[test]
    fn qa_handle_reuse_repeated_probes() {
        let mut e = match engine_or_skip() {
            Some(e) => e,
            None => return,
        };
        for i in 0..10 {
            let out = e.probe("127.0.0.1", 1000, 32, 128);
            assert_eq!(out.status, Status::Ok, "第 {} 次回环探测应成功", i + 1);
        }
    }
}
