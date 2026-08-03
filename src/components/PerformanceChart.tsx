import { useMemo, useState } from "react";
import type { DataPoint } from "../types";

type Period = "1M" | "3M" | "YTD" | "1Y" | "ALL";

const periods: Period[] = ["1M", "3M", "YTD", "1Y", "ALL"];

function cutoffFor(period: Period) {
  const now = new Date();
  if (period === "ALL") return 0;
  if (period === "YTD") return new Date(now.getFullYear(), 0, 1).getTime();
  const days = period === "1M" ? 31 : period === "3M" ? 93 : 365;
  return now.getTime() - days * 24 * 60 * 60 * 1000;
}

function pathFor(values: number[], width: number, height: number, min: number, max: number) {
  const [domainMin, domainMax] = domainFor(min, max);
  const spread = domainMax - domainMin;
  return values
    .map((value, index) => {
      const x = values.length === 1 ? width : (index / (values.length - 1)) * width;
      const y = height - ((value - domainMin) / spread) * height;
      return `${index === 0 ? "M" : "L"}${x.toFixed(2)},${y.toFixed(2)}`;
    })
    .join(" ");
}

function spreadFor(min: number, max: number) {
  return Math.max(max - min, Math.max(max * 0.02, 1));
}

function domainFor(min: number, max: number) {
  const spread = spreadFor(min, max) * 1.16;
  const midpoint = (min + max) / 2;
  return [Math.max(0, midpoint - spread / 2), Math.max(midpoint + spread / 2, 1)] as const;
}

type PerformanceChartProps = {
  history: DataPoint[];
  format: (value: number) => string;
  title?: string;
  valueLabel?: string;
  baselineLabel?: string;
  emptyText?: string;
};

export function PerformanceChart({
  history,
  format,
  title = "Performance",
  valueLabel = "Portfolio value",
  baselineLabel = "Invested",
  emptyText = "The equity line will appear after the next scheduled snapshot.",
}: PerformanceChartProps) {
  const [period, setPeriod] = useState<Period>("1Y");
  const points = useMemo(
    () => history.filter((point) => new Date(point.timestamp).getTime() >= cutoffFor(period)),
    [history, period],
  );
  const chartValues = points.flatMap((point) => [point.value, point.invested]);
  const min = Math.min(...chartValues);
  const max = Math.max(...chartValues);
  const [domainMin, domainMax] = domainFor(min, max);
  const yAxisValues = [1, 0.75, 0.5, 0.25, 0].map(
    (position) => domainMin + position * (domainMax - domainMin),
  );
  const line = points.length > 1 ? pathFor(points.map((point) => point.value), 1000, 250, min, max) : "";
  const invested = points.length > 1 ? pathFor(points.map((point) => point.invested), 1000, 250, min, max) : "";

  return (
    <section className="panel chart-panel">
      <div className="panel-heading">
        <div>
          <p className="section-label">Equity</p>
          <h2>{title}</h2>
        </div>
        <div className="periods" aria-label="Chart period">
          {periods.map((item) => (
            <button className={period === item ? "active" : ""} key={item} onClick={() => setPeriod(item)}>
              {item}
            </button>
          ))}
        </div>
      </div>

      {points.length > 1 ? (
        <div className="chart-wrap">
          <svg className="chart" viewBox="0 0 1000 250" preserveAspectRatio="none" role="img" aria-label="Portfolio value over time">
            <defs>
              <linearGradient id="equity-fill" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0" stopColor="#b39a63" stopOpacity="0.12" />
                <stop offset="1" stopColor="#b39a63" stopOpacity="0" />
              </linearGradient>
            </defs>
            <path className="chart-grid" d="M0 62.5H1000M0 125H1000M0 187.5H1000" />
            <path className="invested-line" d={invested} />
            <path className="equity-area" d={`${line} L1000,250 L0,250 Z`} />
            <path className="equity-line" d={line} />
          </svg>
          <div className="chart-y-axis" aria-hidden="true">
            {yAxisValues.map((value, index) => <span key={index} style={{ top: `${index * 25}%` }}>{format(value)}</span>)}
          </div>
          <div className="chart-x-axis">
            <span>{new Date(points[0].timestamp).toLocaleDateString(undefined, { day: "numeric", month: "short" })}</span>
            <span>{new Date(points.at(-1)!.timestamp).toLocaleDateString(undefined, { day: "numeric", month: "short" })}</span>
          </div>
        </div>
      ) : (
        <div className="chart-empty">
          <span className="pulse-dot" />
          <strong>Tracking has started</strong>
          <p>{emptyText}</p>
        </div>
      )}
      <div className="legend"><span><i className="equity-key" />{valueLabel}</span><span><i />{baselineLabel}</span></div>
    </section>
  );
}
