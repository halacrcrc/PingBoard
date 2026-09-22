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
  /** 是否记录该主机的事件（运行时单主机开关） */
  events_on: boolean;
}

/** Ping 设置（对应 Rust 的 PingSettings） */
export interface PingSettings {
  interval_ms: number;
  timeout_ms: number;
  payload_size: number;
  ttl: number;
  max_threads: number;
  /** 是否启用 max_threads 并发上限（关闭后仅保留 4096 硬保护） */
  limit_max_threads: boolean;
  beep_on_fail: boolean;
  auto_start: boolean;
  history_len: number;
  /** 事件日志总开关（关闭后不再生成任何事件） */
  events_on: boolean;
  /** 事件展示等级 */
  events_level: EventLevel;
  /** 是否把事件持久化到文件 */
  events_persist: boolean;
  /** 事件保存目录（null 表示使用默认 app_config_dir） */
  events_dir: string | null;
  /** 每主机保留条数（50 / 200 / 1000） */
  events_keep: number;
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
  /** 自上次快照以来新增的事件（增量，空闲为 []） */
  events: LogEvent[];
}

/** 新增目标条目（对应 Rust 的 TargetEntry） */
export interface TargetEntry {
  name: string;
  host: string;
}

/** 文件导入解析结果（对应 Rust 的 ImportPayload） */
export type ImportPayload =
  | { kind: "text"; text: string }
  | { kind: "table"; rows: string[][] };

/** 事件类型（对应 Rust 的 EventKind，snake_case） */
export type EventKind =
  | "fault"
  | "unreachable"
  | "dns_fail"
  | "recover"
  | "first_ok"
  | "start"
  | "stop"
  | "config_change";

/** 事件等级（对应 Rust 的 EventLevel） */
export type EventLevel = "fault" | "standard" | "detail";

/** 事件日志条目（对应 Rust 的 LogEvent） */
export interface LogEvent {
  seq: number;
  ts: number;
  target_id: number;
  target_name: string;
  target_host: string;
  kind: EventKind;
  level: EventLevel;
  text: string;
}
