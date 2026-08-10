use std::time::Duration;

use fred::{
    interfaces::KeysInterface,
    prelude::{Client, ClientLike, Config, Error as FredError, PerformanceConfig},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RedisClientError {
    #[error("invalid Redis configuration")]
    Configuration,
    #[error("Redis connection failed")]
    Connection(#[source] FredError),
    #[error("Redis command failed")]
    Command(#[source] FredError),
}

/// Thin Redis client wrapper used by API state and session/cache modules.
/// PostgreSQL remains authoritative; this client is allowed to be unavailable
/// only for components that explicitly degrade to a database fallback.
#[derive(Clone)]
pub struct RedisClient {
    inner: Client,
}

impl RedisClient {
    pub async fn connect(url: &str, timeout: Duration) -> Result<Self, RedisClientError> {
        let config = Config::from_url(url).map_err(|_| RedisClientError::Configuration)?;
        let client = Client::new(config, Some(PerformanceConfig::default()), None, None);
        tokio::time::timeout(timeout, client.init())
            .await
            .map_err(|_| {
                RedisClientError::Connection(FredError::new(
                    fred::error::ErrorKind::Timeout,
                    "Redis connection timed out",
                ))
            })?
            .map_err(RedisClientError::Connection)?;
        Ok(Self { inner: client })
    }

    pub async fn ping(&self) -> Result<(), RedisClientError> {
        let _: String = self
            .inner
            .ping(None)
            .await
            .map_err(RedisClientError::Command)?;
        Ok(())
    }

    pub async fn get(
        &self,
        key: impl Into<fred::types::Key>,
    ) -> Result<Option<String>, RedisClientError> {
        let key = key.into();
        self.inner.get(key).await.map_err(RedisClientError::Command)
    }

    pub async fn set_ex<K, V>(
        &self,
        key: K,
        value: V,
        ttl_seconds: i64,
    ) -> Result<(), RedisClientError>
    where
        K: Into<fred::types::Key> + Send,
        V: TryInto<fred::types::Value> + Send,
        V::Error: Into<FredError> + Send,
    {
        let _: String = self
            .inner
            .set(
                key,
                value,
                Some(fred::types::Expiration::EX(ttl_seconds)),
                None,
                false,
            )
            .await
            .map_err(RedisClientError::Command)?;
        Ok(())
    }

    pub async fn delete(&self, key: impl Into<fred::types::Key>) -> Result<(), RedisClientError> {
        let key = key.into();
        let _: i64 = self
            .inner
            .del(key)
            .await
            .map_err(RedisClientError::Command)?;
        Ok(())
    }

    pub async fn increment(
        &self,
        key: impl Into<fred::types::Key>,
    ) -> Result<i64, RedisClientError> {
        let key = key.into();
        self.inner
            .incr(key)
            .await
            .map_err(RedisClientError::Command)
    }

    pub async fn expire(
        &self,
        key: impl Into<fred::types::Key>,
        seconds: i64,
    ) -> Result<(), RedisClientError> {
        let key = key.into();
        let _: i64 = self
            .inner
            .expire(key, seconds, None)
            .await
            .map_err(RedisClientError::Command)?;
        Ok(())
    }

    pub async fn close(&self) -> Result<(), RedisClientError> {
        self.inner.quit().await.map_err(RedisClientError::Command)
    }
}
