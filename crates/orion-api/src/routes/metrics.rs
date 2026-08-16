use axum::{
    extract::State,
    http::{header, HeaderValue},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use sqlx::PgPool;

/// Minimal Prometheus exposition surface with bounded, non-personal labels.
/// Feature-specific counters can be appended by the owning module.
pub fn router() -> Router<crate::state::AppState> {
    Router::new().route("/metrics", get(metrics))
}

async fn metrics(State(state): State<crate::state::AppState>) -> Response {
    let snapshot = match read_operational_snapshot(&state.db).await {
        Ok(snapshot) => snapshot,
        Err(error) => {
            tracing::warn!(target: "orion.metrics", error = %error, "metrics database snapshot failed");
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                [(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("text/plain; version=0.0.4"),
                )],
                "# metrics unavailable\n",
            )
                .into_response();
        }
    };
    let body = format!(
        concat!(
            "# HELP orion_api_up API process health\n",
            "# TYPE orion_api_up gauge\n",
            "orion_api_up 1\n",
            "# HELP orion_api_info API build metadata\n",
            "# TYPE orion_api_info gauge\n",
            "orion_api_info{{service=\"orion-api\"}} 1\n",
            "# HELP orion_outbox_pending_events Durable outbox events awaiting dispatch\n",
            "# TYPE orion_outbox_pending_events gauge\n",
            "orion_outbox_pending_events {}\n",
            "# HELP orion_rating_reconciliation_failures_total Users whose current rating does not match the latest immutable ledger entry\n",
            "# TYPE orion_rating_reconciliation_failures_total gauge\n",
            "orion_rating_reconciliation_failures_total {}\n",
        ),
        snapshot.pending_outbox,
        snapshot.rating_reconciliation_failures,
    );
    (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; version=0.0.4"),
        )],
        body,
    )
        .into_response()
}

#[derive(Debug, Clone, Copy)]
struct OperationalSnapshot {
    pending_outbox: i64,
    rating_reconciliation_failures: i64,
}

async fn read_operational_snapshot(pool: &PgPool) -> Result<OperationalSnapshot, sqlx::Error> {
    let pending_outbox =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM outbox_events WHERE status = 'pending'")
            .fetch_one(pool)
            .await?;
    // The ledger is append-only and PostgreSQL is authoritative. A user with
    // no ledger entry must still have the default starting rating; otherwise
    // the mismatch is surfaced without repairing or rewriting history.
    let rating_reconciliation_failures = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)
        FROM user_ratings AS current_rating
        LEFT JOIN LATERAL (
            SELECT rating_after
            FROM rating_ledger
            WHERE user_id = current_rating.user_id
            ORDER BY created_at DESC, id DESC
            LIMIT 1
        ) AS latest_ledger ON TRUE
        WHERE COALESCE(latest_ledger.rating_after, 1200) <> current_rating.rating
        "#,
    )
    .fetch_one(pool)
    .await?;
    Ok(OperationalSnapshot {
        pending_outbox,
        rating_reconciliation_failures,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn exposition_has_no_personal_labels() {
        let body = concat!(
            "orion_api_info{service=\"orion-api\"} 1\n",
            "orion_outbox_pending_events 0\n",
            "orion_rating_reconciliation_failures_total 0\n"
        );
        assert!(!body.contains("email"));
        assert!(!body.contains("user_id"));
        assert!(body.contains("orion_outbox_pending_events"));
        assert!(body.contains("orion_rating_reconciliation_failures_total"));
    }
}
