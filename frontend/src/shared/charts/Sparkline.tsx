import type { ReactNode } from "react";

export interface SparklinePoint {
  label: string;
  value: number;
}

interface SparklineProps {
  ariaLabel: string;
  children?: ReactNode;
  color?: string;
  points: SparklinePoint[];
}

function coordinates(points: SparklinePoint[]): string {
  if (points.length === 0) return "";
  const values = points.map((point) => point.value);
  const minimum = Math.min(...values);
  const maximum = Math.max(...values);
  const range = maximum - minimum || 1;
  return points
    .map((point, index) => {
      const x = points.length === 1 ? 50 : (index / (points.length - 1)) * 100;
      const y = 90 - ((point.value - minimum) / range) * 80;
      return `${x.toFixed(2)},${y.toFixed(2)}`;
    })
    .join(" ");
}

export function Sparkline({ ariaLabel, children, color = "#4f46e5", points }: SparklineProps) {
  const last = points.at(-1);
  const first = points[0];
  return <div className="profile-chart">
    <svg aria-label={ariaLabel} className="profile-chart__svg" role="img" viewBox="0 0 100 100">
      <title>{ariaLabel}</title>
      <polyline fill="none" points={coordinates(points)} stroke={color} strokeLinecap="round" strokeLinejoin="round" strokeWidth="3" vectorEffect="non-scaling-stroke" />
    </svg>
    <p className="profile-chart__summary" role="status">
      {points.length === 0 ? "No observations yet." : `From ${first?.value} to ${last?.value} across ${points.length} observations.`}
    </p>
    {children}
  </div>;
}

export function ChartTable({ caption, headers, rows }: { caption: string; headers: string[]; rows: ReactNode[][] }) {
  return <table className="ui-visually-hidden">
    <caption>{caption}</caption>
    <thead><tr>{headers.map((header) => <th key={header} scope="col">{header}</th>)}</tr></thead>
    <tbody>{rows.map((row, rowIndex) => <tr key={`${caption}-${rowIndex}`}>{row.map((cell, cellIndex) => <td key={`${rowIndex}-${cellIndex}`}>{cell}</td>)}</tr>)}</tbody>
  </table>;
}
