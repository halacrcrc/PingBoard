// Tauri invoke 命令封装：与 Rust 侧 #[tauri::command] 一一对应
import { invoke } from "@tauri-apps/api/core";
import type { ImportPayload, LogEvent, PingSettings, Snapshot, TargetEntry } from "../types";

/** 获取当前完整快照（目标列表 + 设置 + 汇总统计） */
export const getState = (): Promise<Snapshot> => invoke<Snapshot>("get_state");

/** 新增目标，返回新分配的自增 ID 列表 */
export const addTargets = (entries: TargetEntry[]): Promise<number[]> =>
  invoke<number[]>("add_targets", { entries });

/** 删除目标 */
export const removeTargets = (ids: number[]): Promise<void> =>
  invoke<void>("remove_targets", { ids });

/** 修改目标（备注名 / 主机名 / 启用状态） */
export const updateTarget = (
  id: number,
  name: string,
  host: string,
  enabled: boolean
): Promise<void> => invoke<void>("update_target", { id, name, host, enabled });

/** 批量设置启用状态 */
export const setEnabled = (ids: number[], enabled: boolean): Promise<void> =>
  invoke<void>("set_enabled", { ids, enabled });

/** 开始 Ping；ids 为 null 表示启动全部已启用目标 */
export const startPinging = (ids: number[] | null): Promise<void> =>
  invoke<void>("start_pinging", { ids });

/** 停止 Ping；ids 为 null 表示停止全部 */
export const stopPinging = (ids: number[] | null): Promise<void> =>
  invoke<void>("stop_pinging", { ids });

/** 重置统计；ids 为 null 表示重置全部 */
export const resetStats = (ids: number[] | null): Promise<void> =>
  invoke<void>("reset_stats", { ids });

/** 更新设置（后端会做范围校验并持久化） */
export const updateSettings = (settings: PingSettings): Promise<void> =>
  invoke<void>("update_settings", { settings });

/** 解析主机名为 IP 字符串 */
export const resolveHost = (host: string): Promise<string> =>
  invoke<string>("resolve_host", { host });

/** 导出报表；ids 为 null 表示导出全部 */
export const exportReport = (
  path: string,
  format: string,
  ids: number[] | null
): Promise<void> => invoke<void>("export_report", { path, format, ids });

/** 获取配置文件路径 */
export const getConfigPath = (): Promise<string> => invoke<string>("get_config_path");

/** 读取导入文件（txt/csv → 文本；xlsx/xls/ods → 表格）；解析动作仍在前端完成 */
export const readImportFile = (path: string): Promise<ImportPayload> =>
  invoke<ImportPayload>("read_import_file", { path });

/** 读取某主机的最近事件（选中主机回填），按新→旧返回 */
export const listEvents = (targetId: number, limit: number): Promise<LogEvent[]> =>
  invoke<LogEvent[]>("list_events", { targetId, limit });

/** 清空某主机的事件（内存 + 文件重写） */
export const clearEvents = (targetId: number): Promise<void> =>
  invoke<void>("clear_events", { targetId });

/** 导出事件为 CSV；targetId 为 null 表示导出全部主机 */
export const exportEvents = (
  path: string,
  targetId: number | null,
  tzOffsetMinutes: number
): Promise<void> => invoke<void>("export_events", { path, targetId, tzOffsetMinutes });

/** 设置单主机「记录事件」开关并持久化 */
export const setTargetEvents = (id: number, eventsOn: boolean): Promise<void> =>
  invoke<void>("set_target_events", { id, eventsOn });
