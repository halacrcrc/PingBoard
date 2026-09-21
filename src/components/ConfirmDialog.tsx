// 轻量确认对话框：替代 window.confirm（Tauri WebView 下不可靠）
// 样式沿用现有对话框：遮罩 + 白/深色卡片 + 底部按钮区；Z 层高于其它对话框
import React from "react";

export interface ConfirmDialogProps {
  open: boolean;
  title: string;
  message: React.ReactNode;
  confirmText?: string;
  danger?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

const ConfirmDialog: React.FC<ConfirmDialogProps> = ({
  open,
  title,
  message,
  confirmText = "确定",
  danger = false,
  onConfirm,
  onCancel,
}) => {
  // Esc 关闭（对话框打开时才挂监听）
  React.useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onCancel]);

  if (!open) return null;

  const confirmClass = danger
    ? "px-3 h-7 rounded bg-red-600 hover:bg-red-700 text-white"
    : "px-3 h-7 rounded bg-sky-600 hover:bg-sky-700 text-white";

  return (
    <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/40">
      <div
        className="w-[420px] max-h-[80vh] flex flex-col rounded-lg shadow-2xl bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700"
        role="dialog"
        aria-modal="true"
      >
        <div className="px-4 h-11 flex items-center border-b border-slate-200 dark:border-slate-700">
          <div className="font-semibold text-slate-800 dark:text-slate-100">{title}</div>
        </div>

        <div className="px-4 py-4 flex-1 overflow-auto text-[13px] leading-relaxed text-slate-700 dark:text-slate-200">
          {message}
        </div>

        <div className="px-4 h-12 flex items-center justify-end gap-2 border-t border-slate-200 dark:border-slate-700">
          <button
            className="px-3 h-7 rounded border border-slate-300 dark:border-slate-600 bg-white dark:bg-slate-800 hover:bg-slate-100 dark:hover:bg-slate-700 text-slate-700 dark:text-slate-200"
            onClick={onCancel}
          >
            取消
          </button>
          <button className={confirmClass} onClick={onConfirm}>
            {confirmText}
          </button>
        </div>
      </div>
    </div>
  );
};

export default ConfirmDialog;
