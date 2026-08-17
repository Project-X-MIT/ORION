import { ChartTable, Sparkline } from "./Sparkline";
import type { ProfileRankPoint } from "../../features/profile/types";

export function RankHistoryChart({ points }: { points: ProfileRankPoint[] }) {
  return <section aria-labelledby="profile-rank-chart-title" className="profile-chart-card">
    <h2 id="profile-rank-chart-title">Rank history</h2>
    <Sparkline ariaLabel="Global rank history chart; lower rank numbers are better" color="#0369a1" points={points.map((point) => ({ label: point.snapshot_at, value: point.current_rank }))}>
      <ChartTable
        caption="Rank history data"
        headers={["Snapshot", "Previous rank", "Current rank", "Movement"]}
        rows={points.map((point) => [new Date(point.snapshot_at).toLocaleDateString(), point.previous_rank ?? "—", point.current_rank, point.rank_movement ?? "—"])}
      />
    </Sparkline>
  </section>;
}
