import { ChartTable, Sparkline } from "./Sparkline";
import type { ProfileRatingPoint } from "../../features/profile/types";

export function EloHistoryChart({ points }: { points: ProfileRatingPoint[] }) {
  return <section aria-labelledby="profile-elo-chart-title" className="profile-chart-card">
    <h2 id="profile-elo-chart-title">Elo history</h2>
    <Sparkline ariaLabel="Elo rating history chart" color="#4f46e5" points={points.map((point) => ({ label: point.occurred_at, value: point.rating_after }))}>
      <ChartTable
        caption="Elo history data"
        headers={["Date", "Quiz", "Rating before", "Rating after", "Change"]}
        rows={points.map((point) => [new Date(point.occurred_at).toLocaleDateString(), point.quiz_type, point.rating_before, point.rating_after, point.rating_delta])}
      />
    </Sparkline>
  </section>;
}
