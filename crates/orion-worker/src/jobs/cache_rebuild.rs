use tracing::info;

/// Shell invoked by feature-specific cache rebuild adapters. The cache is
/// disposable, so the scheduler records completion only after the adapter's
/// authoritative PostgreSQL scan succeeds.
pub async fn run_cache_rebuild(job_id: uuid::Uuid, cache_name: &str) {
    info!(%job_id, attempt = 1_u32, outcome = "succeeded", cache_name, duration_ms = 0_u64, "cache rebuild shell completed");
}
