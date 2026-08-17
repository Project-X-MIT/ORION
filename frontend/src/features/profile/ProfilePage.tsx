import { Alert } from "../../shared/ui/Alert";
import { Avatar } from "../../shared/ui/Avatar";
import { Badge } from "../../shared/ui/Badge";
import { Card } from "../../shared/ui/Card";
import { DashboardLayout } from "../../shared/layouts/DashboardLayout";
import { formatProfileDate } from "./types";
import { useProfile } from "./hooks";
import { useAuth } from "../../providers/AuthProvider";
import { EloHistoryChart } from "../../shared/charts/EloHistoryChart";
import { PerformanceChart } from "../../shared/charts/PerformanceChart";
import { RankHistoryChart } from "../../shared/charts/RankHistoryChart";

export function ProfilePage({ userId }: { userId?: string }) {
  const { user, logout } = useAuth();
  const profile = useProfile(userId ?? user?.id);
  const displayUserId = userId ?? user?.id;

  if (!displayUserId || profile.isPending) {
    return <main aria-busy="true" aria-live="polite"><h1>Profile</h1><p>Loading profile…</p></main>;
  }
  if (profile.isError && !profile.data) {
    return <main><Alert title="Profile unavailable" variant="danger"><p>We could not load this profile. Redis failures are safe; please try again.</p><button type="button" onClick={() => void profile.refetch()}>Try again</button></Alert></main>;
  }
  if (!profile.data) return <main><p role="status">Profile not found.</p></main>;
  const data = profile.data;
  const rankMovement = data.rank_movement;
  return <DashboardLayout
    currentPath="/profile"
    navItems={[{ href: "/profile", label: "Profile", exact: true }, { href: "/quiz", label: "Quiz" }]}
    onSignOut={() => void logout()}
    pageTitle="Your profile"
    user={{ avatarUrl: data.avatar_url ?? undefined, name: data.display_name ?? data.username, secondaryText: `@${data.username}` }}
  >
    <div className="profile-page">
      {profile.isFetching ? <p aria-live="polite" role="status">Refreshing profile…</p> : null}
      <header className="profile-page__hero">
        <Avatar alt={data.display_name ?? data.username} size="xl" src={data.avatar_url ?? undefined} />
        <div><p className="profile-page__eyebrow">Public profile</p><h1>{data.display_name ?? data.username}</h1><p className="profile-page__username">@{data.username}</p>{data.bio ? <p>{data.bio}</p> : null}</div>
      </header>
      <section aria-label="Profile statistics" className="profile-stats">
        <Card><span className="profile-stat__label">Elo rating</span><strong>{data.rating ?? "—"}</strong></Card>
        <Card><span className="profile-stat__label">Global rank</span><strong>{data.global_rank ? `#${data.global_rank}` : "—"}</strong>{rankMovement !== null ? <Badge variant={rankMovement > 0 ? "success" : rankMovement < 0 ? "danger" : "neutral"}>{rankMovement > 0 ? `↑ ${rankMovement}` : rankMovement < 0 ? `↓ ${Math.abs(rankMovement)}` : "No change"}</Badge> : null}</Card>
        <Card><span className="profile-stat__label">Quizzes completed</span><strong>{data.quizzes_completed}</strong></Card>
        <Card><span className="profile-stat__label">Correct answers</span><strong>{data.correct_answers}</strong></Card>
      </section>
      <section aria-label="Rating and performance history" className="profile-charts">
        <EloHistoryChart points={data.rating_history} />
        <RankHistoryChart points={data.rank_history} />
        <PerformanceChart points={data.performance_history} />
      </section>
      <section aria-labelledby="profile-research-title" className="profile-research">
        <h2 id="profile-research-title">Published research</h2>
        {data.published_research.length === 0 ? <p role="status">No published reports yet.</p> : <div className="profile-research__list">{data.published_research.map((paper) => <Card key={paper.id} header={<div><h3>{paper.title}</h3><time dateTime={paper.published_at}>{formatProfileDate(paper.published_at)}</time></div>}><p>{paper.abstract || "No abstract provided."}</p><p>{paper.elo_awarded ? <Badge variant="success">Awarded {paper.elo_award ?? 0} Elo</Badge> : <Badge variant="neutral">Award pending</Badge>}{paper.evaluation_score !== null ? <span className="profile-research__score">Evaluation {paper.evaluation_score}/100</span> : null}{paper.evaluated_content_version !== null ? <span className="profile-research__score">Award version {paper.evaluated_content_version}</span> : null}</p></Card>)}</div>}
      </section>
    </div>
  </DashboardLayout>;
}
