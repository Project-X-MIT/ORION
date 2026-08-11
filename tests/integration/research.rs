use std::{env, error::Error};

use orion_db::{models::ResearchPaperStatus, pool as db_pool, repositories::ResearchRepository};
use orion_worker::jobs::research_review::process_research_award;
use serde_json::json;
use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

struct TestDatabase {
    pool: PgPool,
    admin: PgPool,
    schema: String,
}

impl TestDatabase {
    async fn create() -> Result<Option<Self>, Box<dyn Error>> {
        let database_url = match env::var("ORION_TEST_DATABASE_URL")
            .or_else(|_| env::var("DATABASE_URL"))
        {
            Ok(database_url) => database_url,
            Err(_) => {
                eprintln!("Skipping research integration test: no PostgreSQL test URL configured");
                return Ok(None);
            }
        };
        let admin = db_pool::connect(&database_url).await?;
        let schema = format!("orion_research_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&admin)
            .await?;

        let search_path = format!("SET search_path TO {schema}, public");
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .after_connect(move |connection, _metadata| {
                let search_path = search_path.clone();
                Box::pin(async move {
                    sqlx::query(&search_path)
                        .execute(connection)
                        .await
                        .map(|_| ())
                })
            })
            .connect(&database_url)
            .await?;
        db_pool::migrate(&pool).await?;
        Ok(Some(Self {
            pool,
            admin,
            schema,
        }))
    }

    async fn cleanup(self) -> Result<(), Box<dyn Error>> {
        self.pool.close().await;
        sqlx::query(&format!("DROP SCHEMA {} CASCADE", self.schema))
            .execute(&self.admin)
            .await?;
        self.admin.close().await;
        Ok(())
    }
}

async fn insert_user(pool: &PgPool, user_id: Uuid, username: &str) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO users (id, email, username, password_hash) VALUES ($1, $2, $3, $4)")
        .bind(user_id)
        .bind(format!("{username}@research.test"))
        .bind(username)
        .bind("$argon2id$research-integration-test")
        .execute(pool)
        .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn research_lifecycle_acceptance_criteria() -> Result<(), Box<dyn Error>> {
    let Some(database) = TestDatabase::create().await? else {
        return Ok(());
    };
    let pool = database.pool.clone();
    let repository = ResearchRepository::new(pool.clone());
    let author_id = Uuid::new_v4();
    let reviewer_id = Uuid::new_v4();
    let rejected_author_id = Uuid::new_v4();
    let manual_author_id = Uuid::new_v4();
    let failure_author_id = Uuid::new_v4();

    insert_user(&pool, author_id, "research-author").await?;
    insert_user(&pool, reviewer_id, "research-reviewer").await?;
    insert_user(&pool, rejected_author_id, "rejected-author").await?;
    insert_user(&pool, manual_author_id, "manual-author").await?;
    insert_user(&pool, failure_author_id, "failure-author").await?;

    // Draft creation, retrieval, ownership, and editing.
    let draft = repository
        .create_draft(author_id, "Draft title", "Draft abstract", "Draft content")
        .await?;
    assert_eq!(draft.parsed_status()?, ResearchPaperStatus::Draft);
    assert_eq!(draft.author_id, author_id);
    assert!(draft.created_at <= draft.updated_at);
    assert!(repository
        .find_draft_by_id(draft.id, author_id)
        .await?
        .is_some());
    assert!(repository
        .find_draft_by_id(draft.id, reviewer_id)
        .await?
        .is_none());
    assert!(repository
        .research_by_author(author_id, 10, 0)
        .await?
        .iter()
        .any(|paper| paper.id == draft.id));
    assert!(repository
        .list_drafts_by_author_id(author_id, 10, 0)
        .await?
        .iter()
        .any(|paper| paper.id == draft.id));
    assert!(repository
        .list_drafts_by_author_id(reviewer_id, 10, 0)
        .await?
        .is_empty());
    // The public repository surface must not expose an unpublished paper.
    assert!(repository.find_published_by_id(draft.id).await?.is_none());
    assert!(repository
        .list_published(10, 0)
        .await?
        .iter()
        .all(|paper| paper.id != draft.id));

    let edited = repository
        .update_draft(
            draft.id,
            author_id,
            "Edited title",
            "Edited abstract",
            "Edited content",
        )
        .await?
        .expect("draft should be editable by its author");
    assert_eq!(edited.title, "Edited title");

    // Submission is auditable and closes draft-only editing.
    let submitted = repository
        .submit_for_review(draft.id, author_id)
        .await?
        .expect("draft should submit");
    assert_eq!(submitted.parsed_status()?, ResearchPaperStatus::Submitted);
    assert!(submitted.submitted_at.is_some());
    assert_eq!(
        repository
            .find_by_id(draft.id)
            .await?
            .unwrap()
            .parsed_status()?,
        ResearchPaperStatus::Submitted
    );
    assert!(repository.find_published_by_id(draft.id).await?.is_none());
    assert!(repository
        .list_drafts_by_author_id(author_id, 10, 0)
        .await?
        .iter()
        .all(|paper| paper.id != draft.id));
    assert!(repository
        .submitted_papers(10, 0)
        .await?
        .iter()
        .any(|paper| paper.id == draft.id));
    assert!(repository
        .submit_for_review(draft.id, reviewer_id)
        .await?
        .is_none());
    assert!(repository
        .submit_for_review(draft.id, author_id)
        .await?
        .is_none());
    assert!(repository
        .update_draft(
            draft.id,
            author_id,
            "must not change",
            "must not change",
            "must not change",
        )
        .await?
        .is_none());
    assert!(repository
        .update_draft(draft.id, reviewer_id, "wrong owner", "", "content")
        .await?
        .is_none());
    assert!(repository
        .find_draft_by_id(draft.id, reviewer_id)
        .await?
        .is_none());

    assert_eq!(repository.pending_review_count().await?, 1);
    assert_eq!(repository.list_pending_reviews(10, 0).await?.len(), 1);
    assert_eq!(repository.list_for_review(10, 0).await?.len(), 1);

    let under_review = repository
        .begin_review(draft.id)
        .await?
        .expect("submitted paper should enter review");
    assert_eq!(
        under_review.parsed_status()?,
        ResearchPaperStatus::UnderReview
    );
    assert!(under_review.under_review_at.is_some());
    assert_eq!(repository.pending_review_count().await?, 1);
    assert!(repository
        .list_pending_reviews(10, 0)
        .await?
        .iter()
        .any(|paper| paper.id == draft.id));
    assert!(repository
        .submit_for_review(draft.id, author_id)
        .await?
        .is_none());
    assert!(repository.publish_paper(draft.id).await?.is_none());

    let self_review = repository
        .insert_review(draft.id, author_id, Some(50.0), "approve", None, None)
        .await;
    assert!(self_review.is_err());

    // The database trigger rejects a direct invalid transition as well as the
    // repository's conditional writes rejecting stale/invalid operations.
    let invalid_transition =
        sqlx::query("UPDATE research_papers SET status = 'published' WHERE id = $1")
            .bind(draft.id)
            .execute(&pool)
            .await;
    assert!(invalid_transition.is_err());

    // Review persistence, evaluation aggregation, and decision auditability.
    let evaluation = json!({
        "rubric_version": 1,
        "evaluated_content_version": 7,
        "scores": {
            "relevance": 92,
            "methodology": 92,
            "evidence": 92,
            "originality": 92,
            "clarity_and_reproducibility": 92
        },
        "overall_score": 92,
        "recommendation": "approve",
        "rationale": "The evidence supports the conclusion.",
        "evidence": [{
            "reference": "Results",
            "finding": "The reported result is reproducible."
        }],
        "strengths": ["Clear methodology"],
        "concerns": ["The sample is limited"]
    });
    let approved = repository
        .complete_review(
            draft.id,
            reviewer_id,
            Some(92.0),
            "approve",
            Some("Strong result"),
            Some(&evaluation),
        )
        .await?
        .expect("under-review paper should be decided");
    assert_eq!(approved.parsed_status()?, ResearchPaperStatus::Approved);
    assert_eq!(approved.decided_by, Some(reviewer_id));
    assert!(approved.decided_at.is_some());
    assert_eq!(approved.evaluation_score, Some(92.0));
    assert_eq!(approved.evaluation_result, Some(evaluation.clone()));

    let reviews = repository.list_reviews_by_paper_id(draft.id).await?;
    assert_eq!(reviews.len(), 1);
    let review_id = reviews[0].id;
    assert_eq!(reviews[0].reviewer_id, reviewer_id);
    assert_eq!(reviews[0].score, Some(92.0));
    assert_eq!(reviews[0].recommendation, "approve");
    assert_eq!(reviews[0].comments.as_deref(), Some("Strong result"));
    assert_eq!(reviews[0].evaluation_result, Some(evaluation.clone()));
    assert_eq!(
        reviews[0].evaluation_result.as_ref().unwrap()["rubric_version"],
        1
    );
    assert_eq!(
        reviews[0].evaluation_result.as_ref().unwrap()["evaluated_content_version"],
        7
    );
    assert!(reviews[0].created_at <= reviews[0].reviewed_at);
    assert!(reviews[0].reviewed_at <= reviews[0].updated_at);
    let decision_notification: (serde_json::Value, String) = sqlx::query_as(
        "SELECT payload, status
         FROM outbox_events
         WHERE event_type = 'orion.notification.requested'
           AND payload ->> 'deduplication_key' = $1",
    )
    .bind(format!(
        "research-review:{}:review:{}:decision-notification",
        draft.id, review_id
    ))
    .fetch_one(&pool)
    .await?;
    assert_eq!(decision_notification.0["recipient_id"], json!(author_id));
    assert_eq!(decision_notification.0["kind"], "research_decision");
    assert_eq!(decision_notification.0["title"], "Research paper approved");
    assert_eq!(decision_notification.1, "pending");
    assert_eq!(repository.approve_paper(draft.id).await?, None);

    let published = repository
        .publish_paper(draft.id)
        .await?
        .expect("approved paper should publish");
    assert_eq!(published.parsed_status()?, ResearchPaperStatus::Published);
    assert_eq!(published.content, approved.content);
    assert!(published.published_at.is_some());
    let published_evaluated_content_version: i32 = sqlx::query_scalar(
        "SELECT (payload ->> 'evaluated_content_version')::integer
         FROM outbox_events
         WHERE event_type = 'orion.research.elo_award.requested'
           AND payload ->> 'paper_id' = $1",
    )
    .bind(draft.id.to_string())
    .fetch_one(&pool)
    .await?;
    assert_eq!(published_evaluated_content_version, 7);
    assert_eq!(repository.pending_review_count().await?, 0);
    assert!(repository
        .update_draft(
            draft.id,
            author_id,
            "must not change after publication",
            "must not change",
            "must not change",
        )
        .await?
        .is_none());
    let published_content_mutation = sqlx::query(
        "UPDATE research_papers SET content = 'must not change directly' WHERE id = $1",
    )
    .bind(draft.id)
    .execute(&pool)
    .await;
    assert!(published_content_mutation.is_err());
    assert!(repository.find_published_by_id(draft.id).await?.is_some());
    assert!(repository
        .published_research(10, 0)
        .await?
        .iter()
        .any(|paper| paper.id == draft.id));

    // A re-review starts from a new paper identity. The decided source stays
    // immutable, while the new version receives its own review and request
    // idempotency identity.
    let revision_id = Uuid::new_v4();
    let revision = repository
        .create_revision(
            draft.id,
            author_id,
            revision_id,
            "Revised title",
            "Revised abstract",
            "Revised content",
        )
        .await?
        .expect("published paper should create a new revision draft");
    assert_eq!(revision.id, revision_id);
    assert_eq!(revision.parsed_status()?, ResearchPaperStatus::Draft);
    let retried_revision = repository
        .create_revision(
            draft.id,
            author_id,
            revision_id,
            "Revised title",
            "Revised abstract",
            "Revised content",
        )
        .await?
        .expect("retry should return the same revision identity");
    assert_eq!(retried_revision.id, revision.id);
    assert_eq!(retried_revision.content, revision.content);
    assert_eq!(
        repository.find_by_id(draft.id).await?.unwrap().status,
        "published"
    );
    repository
        .submit_for_review(revision_id, author_id)
        .await?
        .expect("revision should submit");
    repository
        .begin_review(revision_id)
        .await?
        .expect("revision should enter review");
    let revision_evaluation = evaluation_json(94, "approve");
    repository
        .complete_review(
            revision_id,
            reviewer_id,
            Some(94.0),
            "approve",
            Some("The revised version is ready."),
            Some(&revision_evaluation),
        )
        .await?
        .expect("revision should be approved");
    repository
        .publish_paper(revision_id)
        .await?
        .expect("approved revision should publish");
    let original_request_key: String = sqlx::query_scalar(
        "SELECT payload ->> 'idempotency_key'
         FROM outbox_events
         WHERE event_type = 'orion.research.elo_award.requested'
           AND payload ->> 'paper_id' = $1",
    )
    .bind(draft.id.to_string())
    .fetch_one(&pool)
    .await?;
    let revision_request_key: String = sqlx::query_scalar(
        "SELECT payload ->> 'idempotency_key'
         FROM outbox_events
         WHERE event_type = 'orion.research.elo_award.requested'
           AND payload ->> 'paper_id' = $1",
    )
    .bind(revision_id.to_string())
    .fetch_one(&pool)
    .await?;
    assert_ne!(draft.id, revision_id);
    assert_ne!(original_request_key, revision_request_key);
    assert!(revision_request_key.contains(&revision_id.to_string()));
    let revision_evaluated_content_version: i32 = sqlx::query_scalar(
        "SELECT (payload ->> 'evaluated_content_version')::integer
         FROM outbox_events
         WHERE event_type = 'orion.research.elo_award.requested'
           AND payload ->> 'paper_id' = $1",
    )
    .bind(revision_id.to_string())
    .fetch_one(&pool)
    .await?;
    assert_eq!(revision_evaluated_content_version, 1);

    let decision_mutation =
        sqlx::query("UPDATE research_papers SET decided_by = NULL WHERE id = $1")
            .bind(draft.id)
            .execute(&pool)
            .await;
    assert!(decision_mutation.is_err());

    let review_recommendation_mutation =
        sqlx::query("UPDATE research_reviews SET recommendation = 'reject' WHERE id = $1")
            .bind(review_id)
            .execute(&pool)
            .await;
    assert!(review_recommendation_mutation.is_err());

    assert!(repository
        .update_review_evaluation(review_id, reviewer_id, Some(92.0), Some(&evaluation))
        .await?
        .is_none());
    assert!(repository
        .record_evaluation_result(draft.id, Some(92.0), Some(&evaluation))
        .await?
        .is_none());

    let review_delete = sqlx::query("DELETE FROM research_reviews WHERE id = $1")
        .bind(review_id)
        .execute(&pool)
        .await;
    assert!(review_delete.is_err());

    // Exercise the standalone review persistence and explicit approval APIs.
    let manual_paper = repository
        .create_draft(
            manual_author_id,
            "Manual review title",
            "Manual review abstract",
            "Manual review content",
        )
        .await?;
    repository
        .submit_for_review(manual_paper.id, manual_author_id)
        .await?
        .expect("manual paper should submit");
    repository
        .begin_review(manual_paper.id)
        .await?
        .expect("manual paper should enter review");
    let manual_review = repository
        .insert_review(
            manual_paper.id,
            reviewer_id,
            Some(88.0),
            "approved",
            Some("Persisted separately"),
            None,
        )
        .await?;
    assert_eq!(manual_review.parsed_recommendation()?.as_str(), "approve");
    assert!(repository
        .find_review_by_id(manual_review.id)
        .await?
        .is_some());
    assert!(repository.approve_paper(manual_paper.id).await?.is_none());
    let manual_evaluation = evaluation_json(95, "approve");
    let updated_manual_review = repository
        .update_review_evaluation(
            manual_review.id,
            reviewer_id,
            Some(95.0),
            Some(&manual_evaluation),
        )
        .await?
        .expect("reviewer should update its review evaluation");
    assert_eq!(updated_manual_review.score, Some(95.0));
    assert_eq!(
        updated_manual_review.evaluation_result,
        Some(manual_evaluation.clone())
    );
    let paper_evaluation = manual_evaluation.clone();
    let evaluated_manual_paper = repository
        .record_evaluation_result(manual_paper.id, Some(95.0), Some(&paper_evaluation))
        .await?
        .expect("under-review paper should store evaluation");
    assert_eq!(
        evaluated_manual_paper.evaluation_result,
        Some(paper_evaluation)
    );
    let manually_approved = repository
        .approve_paper(manual_paper.id)
        .await?
        .expect("matching persisted review should allow explicit approval");
    assert_eq!(
        manually_approved.parsed_status()?,
        ResearchPaperStatus::Approved
    );
    assert_eq!(manually_approved.decided_by, Some(reviewer_id));

    // Rejection is also audited, and rejected research cannot publish.
    let rejected = repository
        .create_draft(
            rejected_author_id,
            "Rejected title",
            "Rejected abstract",
            "Rejected content",
        )
        .await?;
    repository
        .submit_for_review(rejected.id, rejected_author_id)
        .await?
        .expect("rejected paper should submit");
    repository
        .begin_review(rejected.id)
        .await?
        .expect("rejected paper should enter review");
    let rejected_evaluation = evaluation_json(20, "reject");
    let rejected = repository
        .complete_review(
            rejected.id,
            reviewer_id,
            Some(20.0),
            "reject",
            Some("Needs more evidence"),
            Some(&rejected_evaluation),
        )
        .await?
        .expect("under-review paper should be rejected");
    assert_eq!(rejected.parsed_status()?, ResearchPaperStatus::Rejected);
    assert_eq!(rejected.decided_by, Some(reviewer_id));
    assert!(rejected.decided_at.is_some());
    assert!(repository.publish_paper(rejected.id).await?.is_none());
    assert!(repository
        .published_research(10, 0)
        .await?
        .iter()
        .all(|paper| paper.id != rejected.id));

    // Publication and the Elo award are one idempotent transaction.  Two
    // concurrent retries must produce exactly one rating change.
    let (first_award, second_award) = tokio::join!(
        repository.publish_and_award_elo(draft.id, 25),
        repository.publish_and_award_elo(draft.id, 25),
    );
    assert!(first_award?.is_some());
    assert!(second_award?.is_some());
    let rating: i32 = sqlx::query_scalar("SELECT rating FROM user_ratings WHERE user_id = $1")
        .bind(author_id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(rating, 1025);
    let ledger_entry = sqlx::query_as::<_, (String, Uuid, String, i32, i32, i32)>(
        "SELECT source_type, source_id, dedupe_key, rating_before, rating_after, rating_delta
         FROM rating_ledger
         WHERE user_id = $1",
    )
    .bind(author_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(ledger_entry.0, "research_review");
    assert_eq!(ledger_entry.1, draft.id);
    assert_eq!(ledger_entry.2, "user");
    assert_eq!(ledger_entry.3, 1000);
    assert_eq!(ledger_entry.4, 1025);
    assert_eq!(ledger_entry.5, 25);
    let ledger_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM rating_ledger WHERE user_id = $1 AND source_id = $2",
    )
    .bind(author_id)
    .bind(draft.id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(ledger_count, 1);
    let award_state = repository.elo_award_state(draft.id).await?.unwrap();
    assert_eq!(award_state.0, Some(25));
    assert!(award_state.1.is_some());
    assert_eq!(repository.elo_awarded(draft.id).await?, Some(true));

    let duplicate_award =
        sqlx::query("UPDATE research_papers SET elo_awarded = FALSE WHERE id = $1")
            .bind(draft.id)
            .execute(&pool)
            .await;
    assert!(duplicate_award.is_err());

    // Force a failure after the review row is written but before the paper
    // decision can commit; the transaction must roll back both changes.
    let failure_paper = repository
        .create_draft(
            failure_author_id,
            "Failure title",
            "Failure abstract",
            "Failure content",
        )
        .await?;
    repository
        .submit_for_review(failure_paper.id, failure_author_id)
        .await?
        .expect("failure paper should submit");
    repository
        .begin_review(failure_paper.id)
        .await?
        .expect("failure paper should enter review");

    assert!(repository
        .complete_review(
            failure_paper.id,
            reviewer_id,
            Some(80.0),
            "approve",
            None,
            None,
        )
        .await?
        .is_none());

    let missing_audit_review = sqlx::query(
        "UPDATE research_papers
         SET status = 'approved', decided_by = $2
         WHERE id = $1",
    )
    .bind(failure_paper.id)
    .bind(reviewer_id)
    .execute(&pool)
    .await;
    assert!(missing_audit_review.is_err());

    let failure_function = format!(
        "CREATE OR REPLACE FUNCTION research_test_force_failure()\n         RETURNS TRIGGER LANGUAGE plpgsql AS $$\n         BEGIN\n             IF NEW.id = '{}'::uuid AND NEW.status = 'approved' THEN\n                 RAISE EXCEPTION 'forced integration-test failure';\n             END IF;\n             RETURN NEW;\n         END;\n         $$",
        failure_paper.id
    );
    sqlx::query(&failure_function).execute(&pool).await?;
    sqlx::query(
        "CREATE TRIGGER research_test_force_failure_trg
         BEFORE UPDATE OF status ON research_papers
         FOR EACH ROW EXECUTE FUNCTION research_test_force_failure()",
    )
    .execute(&pool)
    .await?;

    let failure_evaluation = evaluation_json(80, "approve");
    assert!(repository
        .complete_review(
            failure_paper.id,
            reviewer_id,
            Some(80.0),
            "approve",
            None,
            Some(&failure_evaluation),
        )
        .await
        .is_err());

    sqlx::query("DROP TRIGGER research_test_force_failure_trg ON research_papers")
        .execute(&pool)
        .await?;
    sqlx::query("DROP FUNCTION research_test_force_failure()")
        .execute(&pool)
        .await?;

    let rolled_back = repository.find_by_id(failure_paper.id).await?.unwrap();
    assert_eq!(
        rolled_back.parsed_status()?,
        ResearchPaperStatus::UnderReview
    );
    assert!(repository
        .list_reviews_by_paper_id(failure_paper.id)
        .await?
        .is_empty());
    let rolled_back_notifications: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM outbox_events
         WHERE event_type = 'orion.notification.requested'
           AND payload ->> 'deduplication_key' LIKE $1",
    )
    .bind(format!("research-review:{}:%", failure_paper.id))
    .fetch_one(&pool)
    .await?;
    assert_eq!(rolled_back_notifications, 0);

    database.cleanup().await?;
    Ok(())
}

async fn approved_research_paper(
    repository: &ResearchRepository,
    author_id: Uuid,
    reviewer_id: Uuid,
    title: &str,
) -> Result<Uuid, sqlx::Error> {
    let evaluation = evaluation_json(90, "approve");
    let paper = repository
        .create_draft(author_id, title, "Abstract", "Research content")
        .await?;
    repository
        .submit_for_review(paper.id, author_id)
        .await?
        .expect("draft should submit");
    repository
        .begin_review(paper.id)
        .await?
        .expect("submitted paper should enter review");
    repository
        .complete_review(
            paper.id,
            reviewer_id,
            Some(90.0),
            "approve",
            Some("Accepted"),
            Some(&evaluation),
        )
        .await?
        .expect("under-review paper should be approved");
    Ok(paper.id)
}

fn evaluation_json(overall_score: u8, recommendation: &str) -> serde_json::Value {
    json!({
        "rubric_version": 1,
        "evaluated_content_version": 1,
        "scores": {
            "relevance": overall_score,
            "methodology": overall_score,
            "evidence": overall_score,
            "originality": overall_score,
            "clarity_and_reproducibility": overall_score
        },
        "overall_score": overall_score,
        "recommendation": recommendation,
        "rationale": "The submitted research was evaluated against the complete rubric.",
        "evidence": [{
            "reference": "Results section",
            "finding": "The reported result is supported by the submitted evidence."
        }],
        "strengths": ["The methodology is clearly described."],
        "concerns": ["The sample size could be expanded."]
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn research_elo_request_is_exactly_once_and_failures_are_retryable(
) -> Result<(), Box<dyn Error>> {
    let Some(database) = TestDatabase::create().await? else {
        return Ok(());
    };
    let pool = database.pool.clone();
    let repository = ResearchRepository::new(pool.clone());
    let author_id = Uuid::new_v4();
    let reviewer_id = Uuid::new_v4();
    insert_user(&pool, author_id, "award-author").await?;
    insert_user(&pool, reviewer_id, "award-reviewer").await?;

    let paper_id =
        approved_research_paper(&repository, author_id, reviewer_id, "Exactly once award").await?;
    // Set up a published, approved paper without the publication request so
    // these 100 attempts exercise the worker's concurrent insert path.
    sqlx::query(
        "UPDATE research_papers
         SET status = 'published', published_at = CURRENT_TIMESTAMP
         WHERE id = $1",
    )
    .bind(paper_id)
    .execute(&pool)
    .await?;
    let mut attempts = Vec::with_capacity(100);
    for _ in 0..100 {
        let attempt_pool = pool.clone();
        attempts.push(tokio::spawn(async move {
            process_research_award(&attempt_pool, paper_id).await
        }));
    }
    let mut enqueued = 0;
    for attempt in attempts {
        if attempt.await?? {
            enqueued += 1;
        }
    }
    assert_eq!(enqueued, 1, "concurrent attempts must enqueue once");

    let award_state: (bool, Option<i32>) =
        sqlx::query_as("SELECT elo_awarded, elo_award FROM research_papers WHERE id = $1")
            .bind(paper_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(award_state, (false, None));
    let review_id: Uuid = sqlx::query_scalar(
        "SELECT id
         FROM research_reviews
         WHERE paper_id = $1 AND reviewer_id = $2",
    )
    .bind(paper_id)
    .bind(reviewer_id)
    .fetch_one(&pool)
    .await?;
    let event: (String, serde_json::Value, String) = sqlx::query_as(
        "SELECT event_type, payload, status
         FROM outbox_events
         WHERE payload ->> 'paper_id' = $1",
    )
    .bind(paper_id.to_string())
    .fetch_one(&pool)
    .await?;
    let request_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM outbox_events
         WHERE event_type = 'orion.research.elo_award.requested'
           AND payload ->> 'paper_id' = $1",
    )
    .bind(paper_id.to_string())
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        request_count, 1,
        "100 award attempts must produce one request"
    );
    assert_eq!(event.0, "orion.research.elo_award.requested");
    assert_eq!(event.1["author_id"], json!(author_id));
    assert_eq!(event.1["paper_status"], "published");
    assert_eq!(event.1["recommendation"], "approve");
    assert_eq!(event.1["evaluation_score"], 90);
    assert_eq!(event.1["evaluated_content_version"], 1);
    assert_eq!(
        event.1["idempotency_key"],
        format!("research-paper:{paper_id}:review:{review_id}:elo-award")
    );
    assert_eq!(event.2, "pending");

    let ledger_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM rating_ledger
         WHERE source_type = 'research_review' AND source_id = $1",
    )
    .bind(paper_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(ledger_count, 0, "Phantom must not apply Yash's Elo");

    let retry_paper_id =
        approved_research_paper(&repository, author_id, reviewer_id, "Retryable award").await?;
    let failure_function = format!(
        "CREATE OR REPLACE FUNCTION research_outbox_test_force_failure()
         RETURNS TRIGGER LANGUAGE plpgsql AS $$
         BEGIN
             IF NEW.payload ->> 'paper_id' = '{}' THEN
                 RAISE EXCEPTION 'forced outbox failure';
             END IF;
             RETURN NEW;
         END;
         $$",
        retry_paper_id
    );
    sqlx::query(&failure_function).execute(&pool).await?;
    sqlx::query(
        "CREATE TRIGGER research_outbox_test_force_failure_trg
         BEFORE INSERT ON outbox_events
         FOR EACH ROW EXECUTE FUNCTION research_outbox_test_force_failure()",
    )
    .execute(&pool)
    .await?;

    let (failed_first, failed_second) = tokio::join!(
        repository.publish_paper(retry_paper_id),
        repository.publish_paper(retry_paper_id),
    );
    assert!(failed_first.is_err());
    assert!(failed_second.is_err());
    let rolled_back_outbox_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM outbox_events
         WHERE payload ->> 'paper_id' = $1",
    )
    .bind(retry_paper_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(rolled_back_outbox_count, 0);
    let rolled_back_status: String =
        sqlx::query_scalar("SELECT status FROM research_papers WHERE id = $1")
            .bind(retry_paper_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(rolled_back_status, "approved");

    sqlx::query("DROP TRIGGER research_outbox_test_force_failure_trg ON outbox_events")
        .execute(&pool)
        .await?;
    sqlx::query("DROP FUNCTION research_outbox_test_force_failure()")
        .execute(&pool)
        .await?;
    let (retry_first, retry_second) = tokio::join!(
        repository.publish_paper(retry_paper_id),
        repository.publish_paper(retry_paper_id),
    );
    let retried = [retry_first?, retry_second?]
        .into_iter()
        .filter(Option::is_some)
        .count();
    assert_eq!(retried, 1, "concurrent retries may publish only once");

    let retried_outbox_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM outbox_events
         WHERE payload ->> 'paper_id' = $1",
    )
    .bind(retry_paper_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(retried_outbox_count, 1);

    database.cleanup().await?;
    Ok(())
}
