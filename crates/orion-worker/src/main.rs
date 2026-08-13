use std::time::Duration;

use anyhow::{Context, Result};
use orion_db::pool::{connect_migrate_and_validate, PoolConfig};
use sqlx::PgPool;
use tokio::time::interval;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

use orion_worker::{
    jobs::research_review::{
        claim_research_review_job, fail_research_review_job, process_research_award,
    },
    scheduler::{self, RESEARCH_REVIEW_JOB_ID},
};

const DEFAULT_POLL_INTERVAL_SECONDS: u64 = 5;
const DEFAULT_RUNNING_LEASE_SECONDS: u64 = 300;
const DEFAULT_SHUTDOWN_TIMEOUT_SECONDS: u64 = 15;
const DEFAULT_DATABASE_ACQUIRE_TIMEOUT_SECONDS: u64 = 10;
const MAX_PENDING_JOBS_PER_POLL: i64 = 50;

#[derive(Debug, Clone)]
struct WorkerConfig {
    database_url: String,
    database_max_connections: u32,
    poll_interval: Duration,
    running_lease: Duration,
    shutdown_timeout: Duration,
}

impl WorkerConfig {
    fn from_env() -> Result<Self> {
        Ok(Self {
            database_url: required_env("DATABASE_URL")?,
            database_max_connections: positive_u32("DATABASE_MAX_CONNECTIONS", 10)?,
            poll_interval: Duration::from_secs(positive_u64(
                "WORKER_POLL_INTERVAL_SECONDS",
                DEFAULT_POLL_INTERVAL_SECONDS,
            )?),
            running_lease: Duration::from_secs(positive_u64(
                "WORKER_RUNNING_LEASE_SECONDS",
                DEFAULT_RUNNING_LEASE_SECONDS,
            )?),
            shutdown_timeout: Duration::from_secs(positive_u64(
                "SHUTDOWN_TIMEOUT_SECONDS",
                DEFAULT_SHUTDOWN_TIMEOUT_SECONDS,
            )?),
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let config = WorkerConfig::from_env().context("invalid worker configuration")?;
    let pool_config = PoolConfig {
        database_url: &config.database_url,
        max_connections: config.database_max_connections,
        min_connections: 0,
        acquire_timeout: Duration::from_secs(DEFAULT_DATABASE_ACQUIRE_TIMEOUT_SECONDS),
    };
    let pool = connect_migrate_and_validate(&pool_config)
        .await
        .context("worker database startup failed")?;

    tracing::info!(
        poll_interval_seconds = config.poll_interval.as_secs(),
        "orion-worker is ready"
    );
    run(pool.clone(), &config).await;

    tracing::info!("orion-worker is shutting down");
    let _ = tokio::time::timeout(config.shutdown_timeout, pool.close()).await;
    Ok(())
}

async fn run(pool: PgPool, config: &WorkerConfig) {
    let mut ticker = interval(config.poll_interval);
    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                if let Err(_error) = poll_registered_jobs(&pool, config.running_lease).await {
                    tracing::error!(target: "orion.worker", "worker poll failed; will retry");
                }
            }
            _ = &mut shutdown => break,
        }
    }
}

async fn poll_registered_jobs(pool: &PgPool, running_lease: Duration) -> Result<()> {
    for job in scheduler::WORKER_JOB_REGISTRY {
        match job.id {
            RESEARCH_REVIEW_JOB_ID => {
                recover_stale_research_jobs(pool, job.trigger, running_lease).await?;
                poll_research_review_jobs(pool, job.trigger).await?;
            }
            _ => tracing::debug!(
                target: "orion.worker",
                job_id = job.id,
                attempt = 0_u32,
                duration_ms = 0_u64,
                outcome = "deferred_to_outbox_dispatcher",
                "registered worker job is awaiting its durable dispatcher adapter"
            ),
        }
    }
    Ok(())
}

async fn poll_research_review_jobs(pool: &PgPool, event_type: &str) -> Result<()> {
    let jobs = sqlx::query_as::<_, (Uuid, Option<String>)>(
        "SELECT id, payload ->> 'paper_id'
         FROM outbox_events
         WHERE event_type = $1
           AND (
               job_status = 'queued'
               OR (
                   job_status = 'retry'
                   AND (job_next_retry_at IS NULL OR job_next_retry_at <= CURRENT_TIMESTAMP)
               )
           )
         ORDER BY created_at ASC, id ASC
         LIMIT $2",
    )
    .bind(event_type)
    .bind(MAX_PENDING_JOBS_PER_POLL)
    .fetch_all(pool)
    .await?;

    for (event_id, paper_id) in jobs {
        let Some(paper_id) = paper_id.and_then(|value| Uuid::parse_str(&value).ok()) else {
            let _ = claim_research_review_job(pool, event_id).await?;
            let _ =
                fail_research_review_job(pool, event_id, "invalid research review payload").await?;
            continue;
        };

        if let Err(error) = process_research_award(pool, paper_id).await {
            // The job body and database adapter classify the error before it
            // reaches durable metadata; raw error text is never logged here.
            let _ = claim_research_review_job(pool, event_id).await?;
            let _ = fail_research_review_job(pool, event_id, &error.to_string()).await?;
        }
    }
    Ok(())
}

async fn recover_stale_research_jobs(
    pool: &PgPool,
    event_type: &str,
    running_lease: Duration,
) -> Result<()> {
    let stale_jobs = sqlx::query_scalar::<_, Uuid>(
        "SELECT id
         FROM outbox_events
         WHERE event_type = $1
           AND job_status = 'running'
           AND job_started_at < CURRENT_TIMESTAMP
               - ($2::double precision * INTERVAL '1 second')
         ORDER BY job_started_at ASC, id ASC
         LIMIT $3",
    )
    .bind(event_type)
    .bind(running_lease.as_secs_f64())
    .bind(MAX_PENDING_JOBS_PER_POLL)
    .fetch_all(pool)
    .await?;

    for event_id in stale_jobs {
        let _ = fail_research_review_job(pool, event_id, "worker execution timed out").await?;
    }
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(target: "orion.worker", %error, "failed to install Ctrl-C handler");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                tracing::error!(target: "orion.worker", %error, "failed to install SIGTERM handler")
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

fn required_env(key: &'static str) -> Result<String> {
    std::env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .with_context(|| format!("missing required configuration: {key}"))
}

fn positive_u64(key: &'static str, default: u64) -> Result<u64> {
    let value = std::env::var(key).unwrap_or_else(|_| default.to_string());
    let parsed = value
        .parse::<u64>()
        .with_context(|| format!("{key} must be a positive integer"))?;
    if parsed == 0 {
        anyhow::bail!("{key} must be a positive integer");
    }
    Ok(parsed)
}

fn positive_u32(key: &'static str, default: u32) -> Result<u32> {
    let value = std::env::var(key).unwrap_or_else(|_| default.to_string());
    let parsed = value
        .parse::<u32>()
        .with_context(|| format!("{key} must be a positive integer"))?;
    if parsed == 0 {
        anyhow::bail!("{key} must be a positive integer");
    }
    Ok(parsed)
}

fn init_tracing() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("orion_worker=info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .init();
}
