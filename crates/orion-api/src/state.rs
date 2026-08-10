use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use orion_db::pool::{self, DatabaseStartupError, PoolConfig};
use orion_redis::{RedisClient, RedisClientError, RedisSessionStore};
use sqlx::PgPool;
use thiserror::Error;

use crate::{
    config::{AppConfig, ConfigError},
    middleware::rate_limit::LoginRateLimiter,
};

#[derive(Debug, Error)]
pub enum StartupError {
    #[error("configuration is invalid")]
    Configuration(#[source] ConfigError),
    #[error("PostgreSQL startup failed")]
    Database(#[source] DatabaseStartupError),
    #[error("Redis startup failed")]
    Redis(#[source] RedisClientError),
    #[error("{0} startup timed out")]
    Timeout(&'static str),
}

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub db: PgPool,
    pub redis: RedisClient,
    pub sessions: RedisSessionStore,
    pub login_limiter: LoginRateLimiter,
    ready: Arc<AtomicBool>,
    draining: Arc<AtomicBool>,
}

impl AppState {
    pub async fn connect(config: AppConfig) -> Result<Self, StartupError> {
        let config = Arc::new(config);
        let pool_config = PoolConfig {
            database_url: &config.database_url,
            max_connections: config.database_max_connections,
            min_connections: 0,
            acquire_timeout: config.request_timeout,
        };
        let db = tokio::time::timeout(
            config.request_timeout.saturating_mul(2),
            pool::connect_migrate_and_validate(&pool_config),
        )
        .await
        .map_err(|_| StartupError::Timeout("PostgreSQL"))?
        .map_err(StartupError::Database)?;
        let redis = RedisClient::connect(&config.redis_url, config.request_timeout)
            .await
            .map_err(StartupError::Redis)?;
        let sessions = RedisSessionStore::new(redis.clone());
        let login_limiter = LoginRateLimiter::new(redis.clone());
        let state = Self {
            config,
            db,
            redis,
            sessions,
            login_limiter,
            ready: Arc::new(AtomicBool::new(false)),
            draining: Arc::new(AtomicBool::new(false)),
        };
        state
            .check_dependencies()
            .await
            .map_err(|_| StartupError::Timeout("dependency health"))?;
        state.ready.store(true, Ordering::Release);
        Ok(state)
    }

    pub async fn check_dependencies(&self) -> Result<(), ()> {
        let db =
            tokio::time::timeout(self.config.request_timeout, pool::health_check(&self.db)).await;
        let redis = tokio::time::timeout(self.config.request_timeout, self.redis.ping()).await;
        match (db, redis) {
            (Ok(Ok(())), Ok(Ok(()))) => Ok(()),
            _ => Err(()),
        }
    }

    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire) && !self.is_draining()
    }

    #[must_use]
    pub fn is_draining(&self) -> bool {
        self.draining.load(Ordering::Acquire)
    }

    pub fn mark_draining(&self) {
        self.ready.store(false, Ordering::Release);
        self.draining.store(true, Ordering::Release);
    }

    pub async fn close(self) {
        self.mark_draining();
        let _ = self.redis.close().await;
        self.db.close().await;
    }
}
