// 竖向分隔条：拖动调整相邻区域宽度（v1.2.1）
//
// 设计要点：
//   * **只报告位移增量**，具体宽度怎么钳制交给调用方（纯函数 clampPaneWidth），
//     便于单测各种极限组合。
//   * 🩸 **必须除以 currentZoomFactor()**（红线 R7）：界面缩放开启时
//     `clientX` 是视觉坐标，直接当 CSS px 用会把位移放大一个 zoom 倍，
//     表现为「125% 缩放下轻轻一拖就到头」。
//   * 支持键盘（左右方向键），并带 `aria-*` 语义。
import React from "react";
import { currentZoomFactor } from "../lib/format";

export interface PaneResizerProps {
  /** 本分隔条控制的是哪一侧：拖动时该侧宽度随位移同向变化 */
  /**
   * @param delta CSS px 位移增量（已做 zoom 修正）
   */
  onResize: (delta: number) => void;
  /** 双击回到默认宽度 */
  onReset: () => void;
  /** 无障碍标签 */
  label: string;
}

const PaneResizer: React.FC<PaneResizerProps> = ({ onResize, onReset, label }) => {
  const draggingRef = React.useRef<{ startX: number; last: number } | null>(null);

  // 用 window 监听而非元素自身：拖出分隔条范围后仍能继续跟随
  React.useEffect(() => {
    const onMove = (e: MouseEvent) => {
      const d = draggingRef.current;
      if (!d) return;
      // 🩸 R7：zoom 下 clientX 是视觉坐标，必须换算成 CSS px
      const cssX = e.clientX / currentZoomFactor();
      const delta = cssX - d.last;
      if (delta !== 0) {
        d.last = cssX;
        onResize(delta);
      }
    };
    const onUp = () => {
      if (draggingRef.current) {
        draggingRef.current = null;
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
      }
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, [onResize]);

  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      tabIndex={0}
      title={`拖动调整宽度（双击恢复默认）`}
      className="w-1 shrink-0 cursor-col-resize bg-slate-200 dark:bg-slate-700 hover:bg-sky-500 dark:hover:bg-sky-500 transition-colors"
      onMouseDown={(e) => {
        e.preventDefault();
        draggingRef.current = {
          startX: e.clientX,
          last: e.clientX / currentZoomFactor(),
        };
        document.body.style.cursor = "col-resize";
        document.body.style.userSelect = "none";
      }}
      onDoubleClick={onReset}
      onKeyDown={(e) => {
        const step = e.shiftKey ? 32 : 8;
        if (e.key === "ArrowLeft") {
          e.preventDefault();
          onResize(-step);
        } else if (e.key === "ArrowRight") {
          e.preventDefault();
          onResize(step);
        }
      }}
    />
  );
};

export default React.memo(PaneResizer);
