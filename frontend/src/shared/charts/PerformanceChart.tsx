import { ChartTable, Sparkline } from "./Sparkline";
import type { ProfilePerformancePoint } from "../../features/profile/types";

export function PerformanceChart({ points }: { points: ProfilePerformancePoint[] }) {
  return <section aria-labelledby="profile-performance-chart-title" className="profile-chart-card">
    <h2 id="profile-performance-chart-title">Quiz performance</h2>
    <Sparkline ariaLabel="Quiz score history chart" color="#15803d" points={points.map((point) => ({ label: point.completed_at, value: point.score }))}>
      <ChartTable
        caption="Quiz performance data"
        headers={["Date", "Quiz", "Score", "Correct answers", "Questions", "Rating after"]}
        rows={points.map((point) => [new Date(point.completed_at).toLocaleDateString(), point.quiz_type, `${point.score}%`, point.correct_answers, point.total_questions, point.rating_after])}
      />
    </Sparkline>
  </section>;
}
