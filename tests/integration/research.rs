use std::{env, error::Error};

use orion_db::{models::ResearchPaperStatus, pool as db_pool, repositories::ResearchRepository};
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
        .research_by_author(author_id, 10, 0)
        .await?
        .iter()
        .any(|paper| paper.id == draft.id));

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
    assert!(repository
        .submitted_papers(10, 0)
        .await?
        .iter()
        .any(|paper| paper.id == draft.id));
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
    let evaluation = json!({ "rubric": "complete", "confidence": 0.98 });
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
    assert!(reviews[0].reviewed_at >= reviews[0].created_at);
    assert_eq!(repository.approve_paper(draft.id).await?, None);

    let published = repository
        .publish_paper(draft.id)
        .await?
        .expect("approved paper should publish");
    assert_eq!(published.parsed_status()?, ResearchPaperStatus::Published);
    assert!(published.published_at.is_some());
    assert!(repository.find_published_by_id(draft.id).await?.is_some());
    assert!(repository
        .published_research(10, 0)
        .await?
        .iter()
        .any(|paper| paper.id == draft.id));

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
    let manual_evaluation = json!({ "rubric": "manual", "confidence": 0.91 });
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
    let paper_evaluation = json!({ "source": "standalone-evaluator" });
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
    let rejected = repository
        .complete_review(
            rejected.id,
            reviewer_id,
            Some(20.0),
            "reject",
            Some("Needs more evidence"),
            None,
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

    assert!(repository
        .complete_review(
            failure_paper.id,
            reviewer_id,
            Some(80.0),
            "approve",
            None,
            None,
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

    database.cleanup().await?;
    Ok(())
}
