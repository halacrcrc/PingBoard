// 迷你趋势图（纯 SVG 手写，不依赖任何图表库）
// 数据为最近 N 次探测的 rtt：数值为绿色条形，失败/超时（null）为底部红色短条
import React from "react";

export interface SparklineProps {
  /** 延迟序列，null 表示该次探测失败/超时 */
  data: (number | null)[];
  width?: number;
  height?: number;
  /** 总槽位数（用于固定横轴长度，默认取 data 长度） */
  slots?: number;
}

const Sparkline: React.FC<SparklineProps> = ({ data, width = 90, height = 24, slots }) => {
  const total = Math.max(slots ?? data.length, 1);
  const maxVal = data.reduce<number>((m, v) => (v !== null && v > m ? v : m), 0);
  const scaleMax = maxVal <= 0 ? 1 : maxVal;
  const step = width / total;
  const barW = Math.max(1, step - 0.6);

  // 只渲染最后 total 个点
  const view = data.slice(Math.max(0, data.length - total));
  const offset = total - view.length;

  return (
    <svg
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      className="overflow-visible"
      role="img"
      aria-label="延迟趋势"
    >
      {/* 底部基线 */}
      <line x1={0} y1={height - 0.5} x2={width} y2={height - 0.5} className="stroke-slate-300 dark:stroke-slate-600" strokeWidth={1} />
      {view.map((v, i) => {
        const x = (offset + i) * step;
        if (v === null) {
          // 失败：底部 3px 红色短条
          return (
            <rect
              key={i}
              x={x}
              y={height - 3}
              width={barW}
              height={3}
              className="fill-red-500"
            />
          );
        }
        const h = Math.max(2, (v / scaleMax) * (height - 4));
        return (
          <rect
            key={i}
            x={x}
            y={height - h}
            width={barW}
            height={h}
            className="fill-emerald-500"
            rx={0.5}
          />
        );
      })}
    </svg>
  );
};

export default React.memo(Sparkline);
