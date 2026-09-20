// 与 Rust 后端 serde 序列化结构严格对应的 TypeScript 类型定义

/** 探测状态：idle 未开始 / resolving 解析中 / ok 最近一次成功 / timeout 超时 / failed 解析失败或系统错误 */
export type Status = "idle" | "resolving" | "ok" | "timeout" | "failed";

/** 单个目标的运行时状态（对应 Rust 的 TargetState） */
export interface TargetState {
  id: number;
  name: string;
  host: string;
  enabled: boolean;
  resolved_ip: string | null;
  status: Status;
  sent: number;
  received: number;
  failed: number;
  last_rtt_ms: number | null;
  min_rtt_ms: number | null;
  max_rtt_ms: number | null;
  avg_rtt_ms: number | null;
  sum_rtt_ms: number;
  ttl: number | null;
  consecutive_fail: number;
  loss_pct: number;
  last_success_ts: number | null;
  history: (number | null)[];
  running: boolean;
  last_error: string | null;
}

/** Ping 设置（对应 Rust 的 PingSettings） */
export interface PingSettings {
  interval_ms: number;
  timeout_ms: number;
  payload_size: number;
  ttl: number;
  max_threads: number;
  beep_on_fail: boolean;
  auto_start: boolean;
  history_len: number;
}

/** 聚合快照（对应 Rust 的 Snapshot），既用于 ping-snapshot 事件也用于 get_state 命令 */
export interface Snapshot {
  targets: TargetState[];
  settings: PingSettings;
  running: boolean;
  active_threads: number;
  total_sent: number;
  total_received: number;
  total_failed: number;
  loss_pct: number;
  started_at: number | null;
  updated_at: number;
}

/** 新增目标条目（对应 Rust 的 TargetEntry） */
export interface TargetEntry {
  name: string;
  host: string;
}

/** 详情面板日志条目 */
export interface LogEntry {
  ts: number;
  text: string;
  kind: "fail" | "recover" | "info";
}
