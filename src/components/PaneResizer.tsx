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
  const [hovered, setHovered] = React.useState(false);
  const [dragging, setDragging] = React.useState(false);
  // 悬停或拖动中 → 线加粗并染色，提示「这里能拖」
  const hot = hovered || dragging;

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
        setDragging(false);
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
      title="拖动调整宽度（双击恢复默认；也可聚焦后用左右方向键）"
      // 🩸 热区仍是 6px（w-1.5），但**视觉线只有 1px** —— 此前整条 6px 都涂成
      // slate-300，看上去像一块灰色挡板（用户反馈「太粗」）。
      // 拆分后：布局宽度不变、可拖范围不变，只是「看起来细了」；
      // 悬停 / 拖动时线加粗到 3px 并染成天蓝，避免又回到「看不出能拖」。
      className="relative w-1.5 shrink-0 cursor-col-resize focus:outline-none"
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      onFocus={() => setHovered(true)}
      onBlur={() => setHovered(false)}
      onMouseDown={(e) => {
        e.preventDefault();
        draggingRef.current = {
          startX: e.clientX,
          last: e.clientX / currentZoomFactor(),
        };
        setDragging(true);
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
    >
      <span
        aria-hidden
        className={`pointer-events-none absolute inset-y-0 left-1/2 -translate-x-1/2 rounded-full transition-[width,background-color] duration-100 ${
          hot ? "w-[3px] bg-sky-500" : "w-px bg-slate-400 dark:bg-slate-500"
        }`}
      />
    </div>
  );
};

export default React.memo(PaneResizer);
