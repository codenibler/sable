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

function pathFor(values: number[], width: number, height: number) {
  const min = Math.min(...values);
  const max = Math.max(...values);
  const spread = Math.max(max - min, Math.max(max * 0.02, 1));
  return values
    .map((value, index) => {
      const x = values.length === 1 ? width : (index / (values.length - 1)) * width;
      const y = height - ((value - min + spread * 0.08) / (spread * 1.16)) * height;
      return `${index === 0 ? "M" : "L"}${x.toFixed(2)},${y.toFixed(2)}`;
    })
    .join(" ");
}

export function PerformanceChart({ history, format }: { history: DataPoint[]; format: (value: number) => string }) {
  const [period, setPeriod] = useState<Period>("1Y");
  const points = useMemo(
    () => history.filter((point) => new Date(point.timestamp).getTime() >= cutoffFor(period)),
    [history, period],
  );
  const line = points.length > 1 ? pathFor(points.map((point) => point.value), 1000, 250) : "";
  const invested = points.length > 1 ? pathFor(points.map((point) => point.invested), 1000, 250) : "";

  return (
    <section className="panel chart-panel">
      <div className="panel-heading">
        <div>
          <p className="section-label">Equity</p>
          <h2>Performance</h2>
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
                <stop offset="0" stopColor="#83f5b3" stopOpacity="0.22" />
                <stop offset="1" stopColor="#83f5b3" stopOpacity="0" />
              </linearGradient>
            </defs>
            <path className="chart-grid" d="M0 62.5H1000M0 125H1000M0 187.5H1000" />
            <path className="invested-line" d={invested} />
            <path className="equity-area" d={`${line} L1000,250 L0,250 Z`} />
            <path className="equity-line" d={line} />
          </svg>
          <div className="chart-axis">
            <span>{new Date(points[0].timestamp).toLocaleDateString(undefined, { day: "numeric", month: "short" })}</span>
            <span>{format(points.at(-1)?.value ?? 0)}</span>
            <span>{new Date(points.at(-1)!.timestamp).toLocaleDateString(undefined, { day: "numeric", month: "short" })}</span>
          </div>
        </div>
      ) : (
        <div className="chart-empty">
          <span className="pulse-dot" />
          <strong>Tracking has started</strong>
          <p>The equity line will appear after the next scheduled snapshot.</p>
        </div>
      )}
      <div className="legend"><span><i className="equity-key" />Portfolio value</span><span><i />Invested</span></div>
    </section>
  );
}
