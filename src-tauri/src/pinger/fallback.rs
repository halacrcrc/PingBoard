// 降级路径：调用系统 ping.exe
// 关键点：
//   * 必须带 CREATE_NO_WINDOW，否则每次探测都会闪出黑色控制台窗口
//   * 解析需同时兼容中英文输出：以是否出现 "TTL=" 判定成功；延迟用正则提取
//
// 关于编码：中文 Windows 下 ping.exe 输出为 GBK 字节流。这里使用
// String::from_utf8_lossy 解码——ASCII 字节（含 "TTL"、"="、数字、'ms'）
// 在任何编码下都会被原样保留（<0x80 的字节永远是合法的单字节 UTF-8），
// 因此判定与提取不依赖具体代码页，中英文输出均可正确解析。
use std::process::Command;

use regex::Regex;

use super::ProbeOutcome;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// 不创建控制台窗口
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 基于系统 ping.exe 的探测引擎
pub struct PingEngine {
    /// 中文输出：`时间=10ms` / `时间<1ms`
    re_time_cn: Regex,
    /// 英文输出：`time=10ms` / `time<1ms`
    re_time_en: Regex,
    /// 通用兜底：`=10ms` / `<1ms`（不依赖中英文关键词）
    re_time_any: Regex,
    /// TTL：`TTL=117`
    re_ttl: Regex,
    /// 解析/主机错误特征（英文，ASCII 安全）
    re_host_err: Regex,
}

impl PingEngine {
    pub fn new() -> Self {
        Self {
            // 三组正则统一为 (操作符)(数字) 两个捕获组，便于统一处理 `<1ms` → 0.5ms
            re_time_cn: Regex::new(r"时间\s*([=<])\s*(\d+)\s*ms").expect("正则编译失败"),
            re_time_en: Regex::new(r"(?i)time\s*([=<])\s*(\d+)\s*ms").expect("正则编译失败"),
            re_time_any: Regex::new(r"([=<])\s*(\d+)\s*ms").expect("正则编译失败"),
            re_ttl: Regex::new(r"(?i)TTL\s*=\s*(\d+)").expect("正则编译失败"),
            re_host_err: Regex::new(r"(?i)(could not find host|unknown host|general failure|name or service not known|transmit failed)")
                .expect("正则编译失败"),
        }
    }

    /// 执行一次探测。`ipv6` 为真时附加 `-6`。
    pub fn probe(&mut self, target: &str, timeout_ms: u64, ipv6: bool) -> ProbeOutcome {
        let timeout = timeout_ms.clamp(1, 60_000);

        let mut cmd = Command::new("ping.exe");
        if ipv6 {
            cmd.arg("-6");
        }
        cmd.args(["-n", "1", "-w", &timeout.to_string(), target]);

        // 隐藏控制台窗口
        #[cfg(windows)]
        {
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let output = match cmd.output() {
            Ok(o) => o,
            Err(e) => return ProbeOutcome::failed(format!("无法启动 ping.exe：{}", e)),
        };

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let text = format!("{}\n{}", stdout, stderr);

        self.parse(&text)
    }

    /// 解析 ping.exe 输出文本
    fn parse(&self, text: &str) -> ProbeOutcome {
        // 1) 成功判定：出现 TTL=（中英文输出均含该 ASCII 片段）
        if let Some(caps) = self.re_ttl.captures(text) {
            let ttl = caps.get(1).and_then(|m| m.as_str().parse::<u32>().ok());
            let rtt = self.extract_rtt(text);
            // TTL 存在说明收到回复；延迟缺失时按 0 处理（极少数情况）
            return ProbeOutcome::ok(rtt.unwrap_or(0.0), ttl);
        }

        // 2) 主机名/系统错误 → Failed
        if self.re_host_err.is_match(text) {
            let first_line = text
                .lines()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("")
                .trim()
                .to_string();
            return ProbeOutcome::failed(if first_line.is_empty() {
                "ping 失败".to_string()
            } else {
                first_line
            });
        }

        // 3) 其余情况（请求超时、100% 丢失等）一律记为超时
        ProbeOutcome::timeout()
    }

    /// 从输出中提取延迟（ms）。`<1ms` 记为 0.5ms。
    fn extract_rtt(&self, text: &str) -> Option<f64> {
        // 依次尝试：中文正则 → 英文正则 → 通用兜底正则（三者捕获组结构一致：1=操作符, 2=数字）
        for re in [&self.re_time_cn, &self.re_time_en, &self.re_time_any] {
            if let Some(caps) = re.captures(text) {
                let op = caps.get(1).map(|m| m.as_str()).unwrap_or("=");
                if let Some(v) = caps.get(2).and_then(|m| m.as_str().parse::<f64>().ok()) {
                    // `<1ms` 视为 0.5ms
                    return Some(if op == "<" { 0.5 } else { v });
                }
            }
        }
        None
    }
}

impl Default for PingEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Status;

    #[test]
    fn parses_chinese_reply() {
        let e = PingEngine::new();
        let out = e.parse("来自 223.5.5.5 的回复: 字节=32 时间=10ms TTL=117");
        assert_eq!(out.status, Status::Ok);
        assert_eq!(out.rtt_ms, Some(10.0));
        assert_eq!(out.ttl, Some(117));
    }

    #[test]
    fn parses_english_reply() {
        let e = PingEngine::new();
        let out = e.parse("Reply from 8.8.8.8: bytes=32 time=25ms TTL=55");
        assert_eq!(out.status, Status::Ok);
        assert_eq!(out.rtt_ms, Some(25.0));
        assert_eq!(out.ttl, Some(55));
    }

    #[test]
    fn parses_less_than_one_ms_as_half() {
        let e = PingEngine::new();
        let out = e.parse("Reply from 127.0.0.1: bytes=32 time<1ms TTL=128");
        assert_eq!(out.status, Status::Ok);
        assert_eq!(out.rtt_ms, Some(0.5));
    }

    #[test]
    fn parses_chinese_less_than_one_ms_as_half() {
        let e = PingEngine::new();
        let out = e.parse("来自 127.0.0.1 的回复: 字节=32 时间<1ms TTL=128");
        assert_eq!(out.status, Status::Ok);
        assert_eq!(out.rtt_ms, Some(0.5));
        assert_eq!(out.ttl, Some(128));
    }

    #[test]
    fn timeout_is_timeout_not_failed() {
        let e = PingEngine::new();
        let out = e.parse("请求超时。\n\n数据包: 已发送 = 1，已接收 = 0，丢失 = 1 (100% 丢失)");
        assert_eq!(out.status, Status::Timeout);
    }

    #[test]
    fn english_timeout() {
        let e = PingEngine::new();
        let out = e.parse("Request timed out.\n\nPackets: Sent = 1, Received = 0, Lost = 1 (100% loss)");
        assert_eq!(out.status, Status::Timeout);
    }

    #[test]
    fn host_error_is_failed() {
        let e = PingEngine::new();
        let out = e.parse("Ping request could not find host nope.invalid. Please check the name and try again.");
        assert_eq!(out.status, Status::Failed);
        assert!(out.error.is_some());
    }

    /* ===================== QA 新增降级解析用例 ===================== */

    /// `<1ms` 三种中英文写法都必须解析为 0.5ms（而不是 1.0ms）
    #[test]
    fn qa_less_than_one_ms_variants_are_half() {
        let e = PingEngine::new();
        for text in [
            "Reply from 127.0.0.1: bytes=32 time<1ms TTL=128",
            "来自 127.0.0.1 的回复: 字节=32 时间<1ms TTL=128",
            "来自 127.0.0.1 的回复: 字节=32 time<1ms TTL=128",
        ] {
            let out = e.parse(text);
            assert_eq!(out.status, Status::Ok, "应判定成功：{}", text);
            assert_eq!(out.rtt_ms, Some(0.5), "`<1ms` 必须记为 0.5ms：{}", text);
        }
    }

    /// TTL 与 time 同时存在时优先取 `=`/`<` 操作符语义
    #[test]
    fn qa_equal_time_parsed_verbatim() {
        let e = PingEngine::new();
        let out = e.parse("Reply from 8.8.8.8: bytes=32 time=25ms TTL=55");
        assert_eq!(out.status, Status::Ok);
        assert_eq!(out.rtt_ms, Some(25.0));
        assert_eq!(out.ttl, Some(55));
    }

    /// 回复文本中缺失 `TTL=` → 不得判定为成功（应记为超时，未验证收到回复）
    #[test]
    fn qa_missing_ttl_is_not_ok() {
        let e = PingEngine::new();
        let out = e.parse("来自 10.0.0.1 的回复: 字节=32 时间=10ms");
        assert_ne!(out.status, Status::Ok, "缺少 TTL= 不应判定为成功");
        assert_eq!(out.rtt_ms, None);
    }

    /// 无法解析的主机名（英文错误）→ Failed 且带 error
    #[test]
    fn qa_unresolvable_host_is_failed() {
        let e = PingEngine::new();
        let out = e.parse(
            "Ping request could not find host no-such-host-xyz.invalid. Please check the name and try again.",
        );
        assert_eq!(out.status, Status::Failed);
        assert!(out.error.is_some());
    }

    /// 英文 "Request timed out." 应判超时而非失败
    #[test]
    fn qa_english_timeout_is_timeout() {
        let e = PingEngine::new();
        let out = e.parse("Request timed out.\n\nPackets: Sent = 1, Received = 0, Lost = 1 (100% loss)");
        assert_eq!(out.status, Status::Timeout);
        assert!(out.error.is_none());
    }

    /// TTL 存在但无延迟时，仍判成功且 RTT 记为 0（极少数情况兜底）
    #[test]
    fn qa_ttl_without_time_is_ok_zero() {
        let e = PingEngine::new();
        let out = e.parse("Reply from 1.1.1.1: bytes=32 TTL=57");
        assert_eq!(out.status, Status::Ok);
        assert_eq!(out.rtt_ms, Some(0.0));
        assert_eq!(out.ttl, Some(57));
    }
}
