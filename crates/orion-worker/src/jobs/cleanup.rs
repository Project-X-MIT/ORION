use tracing::info;

/// Retention cleanup shell; feature owners supply idempotent deletion bodies.
pub async fn run_cleanup(job_id: uuid::Uuid, policy: &str) {
    info!(%job_id, attempt = 1_u32, outcome = "succeeded", policy, duration_ms = 0_u64, "retention cleanup shell completed");
}
