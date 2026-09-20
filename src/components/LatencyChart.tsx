// 延迟折线图（纯 SVG 手写）：展示选中主机最近 N 次探测的 RTT 曲线与失败标记
import React from "react";
import { fmtMs } from "../lib/format";

export interface LatencyChartProps {
  data: (number | null)[];
  width?: number;
  height?: number;
  /** 平均值参考线 */
  avg?: number | null;
}

const PAD_LEFT = 44;
const PAD_RIGHT = 10;
const PAD_TOP = 12;
const PAD_BOTTOM = 20;

const LatencyChart: React.FC<LatencyChartProps> = ({
  data,
  width = 380,
  height = 170,
  avg = null,
}) => {
  const plotW = Math.max(10, width - PAD_LEFT - PAD_RIGHT);
  const plotH = Math.max(10, height - PAD_TOP - PAD_BOTTOM);
  const n = data.length;

  const values = data.filter((v): v is number => v !== null);
  const maxVal = values.length > 0 ? Math.max(...values) : 0;
  const scaleMax = Math.max(10, Math.ceil((maxVal * 1.15) / 10) * 10);

  const xAt = (i: number) => (n <= 1 ? PAD_LEFT : PAD_LEFT + (i / (n - 1)) * plotW);
  const yAt = (v: number) => PAD_TOP + plotH - (v / scaleMax) * plotH;

  // 构造成功点的折线路径，失败点处断开
  const segments: string[] = [];
  let current = "";
  data.forEach((v, i) => {
    if (v === null) {
      if (current) {
        segments.push(current);
        current = "";
      }
      return;
    }
    const cmd = current ? "L" : "M";
    current += `${cmd}${xAt(i).toFixed(2)},${yAt(v).toFixed(2)} `;
  });
  if (current) segments.push(current);

  // 纵轴刻度
  const ticks = [0, 0.25, 0.5, 0.75, 1].map((t) => Math.round(scaleMax * t));

  return (
    <svg
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      role="img"
      aria-label="延迟折线图"
    >
      {/* 横向网格线与纵轴标签 */}
      {ticks.map((tv) => {
        const y = yAt(tv);
        return (
          <g key={tv}>
            <line
              x1={PAD_LEFT}
              y1={y}
              x2={width - PAD_RIGHT}
              y2={y}
              className="stroke-slate-200 dark:stroke-slate-700"
              strokeWidth={1}
            />
            <text
              x={PAD_LEFT - 6}
              y={y + 3}
              textAnchor="end"
              className="fill-slate-400 dark:fill-slate-500"
              fontSize={9}
            >
              {tv}
            </text>
          </g>
        );
      })}

      {/* 失败点：底部红色竖线 */}
      {data.map((v, i) =>
        v === null ? (
          <line
            key={`f${i}`}
            x1={xAt(i)}
            y1={PAD_TOP + plotH}
            x2={xAt(i)}
            y2={PAD_TOP + plotH - 10}
            className="stroke-red-500"
            strokeWidth={2}
          />
        ) : null
      )}

      {/* 平均值参考线 */}
      {avg !== null && avg !== undefined && avg > 0 && (
        <line
          x1={PAD_LEFT}
          y1={yAt(avg)}
          x2={width - PAD_RIGHT}
          y2={yAt(avg)}
          className="stroke-sky-500"
          strokeWidth={1}
          strokeDasharray="4 3"
        />
      )}

      {/* 主折线 */}
      {segments.map((d, i) => (
        <path
          key={`s${i}`}
          d={d}
          fill="none"
          className="stroke-emerald-500"
          strokeWidth={1.8}
          strokeLinejoin="round"
          strokeLinecap="round"
        />
      ))}

      {/* 成功点标记 */}
      {data.map((v, i) =>
        v === null ? null : (
          <circle key={`p${i}`} cx={xAt(i)} cy={yAt(v)} r={1.8} className="fill-emerald-600" />
        )
      )}

      {/* 横轴说明 */}
      <text
        x={PAD_LEFT}
        y={height - 6}
        className="fill-slate-400 dark:fill-slate-500"
        fontSize={9}
      >
        最近 {n} 次
      </text>
      <text
        x={width - PAD_RIGHT}
        y={height - 6}
        textAnchor="end"
        className="fill-slate-400 dark:fill-slate-500"
        fontSize={9}
      >
        最新 (ms) · 峰值 {fmtMs(maxVal)}
      </text>
    </svg>
  );
};

export default React.memo(LatencyChart);
