use std::time::Duration;

use fred::{
    interfaces::{ClientLike, EventInterface, KeysInterface, LuaInterface, PubsubInterface},
    prelude::{Client, Config, Error as FredError, PerformanceConfig},
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
        let performance = PerformanceConfig {
            // Fred intentionally defaults command timeouts to zero (unbounded).
            // Redis is disposable in ORION, so an unavailable cache must fail
            // within the caller's configured deadline and allow PostgreSQL
            // fallback rather than pinning the request forever.
            default_command_timeout: timeout,
            ..PerformanceConfig::default()
        };
        let client = Client::new(config, Some(performance), None, None);
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

    /// Deletes a key only when its value still equals the expected payload.
    /// The comparison and deletion execute atomically inside Redis so a stale
    /// cache invalidation cannot remove a newer fill between two commands.
    pub async fn delete_if_value(
        &self,
        key: impl Into<fred::types::Key>,
        expected_value: impl Into<fred::types::Value>,
    ) -> Result<bool, RedisClientError> {
        const DELETE_IF_VALUE_SCRIPT: &str = r#"
            if redis.call('GET', KEYS[1]) == ARGV[1] then
                return redis.call('DEL', KEYS[1])
            end
            return 0
        "#;

        let deleted: i64 = self
            .inner
            .eval(
                DELETE_IF_VALUE_SCRIPT,
                vec![key.into()],
                vec![expected_value.into()],
            )
            .await
            .map_err(RedisClientError::Command)?;
        Ok(deleted == 1)
    }

    pub(crate) async fn eval_i64(
        &self,
        script: &str,
        keys: Vec<fred::types::Key>,
        args: Vec<fred::types::Value>,
    ) -> Result<i64, RedisClientError> {
        self.inner
            .eval(script, keys, args)
            .await
            .map_err(RedisClientError::Command)
    }

    pub(crate) async fn publish(
        &self,
        channel: &str,
        payload: String,
    ) -> Result<i64, RedisClientError> {
        self.inner
            .publish(channel, payload)
            .await
            .map_err(RedisClientError::Command)
    }

    pub(crate) async fn subscribe(&self, channel: &str) -> Result<(), RedisClientError> {
        self.inner
            .subscribe(channel)
            .await
            .map_err(RedisClientError::Command)
    }

    pub(crate) fn message_rx(&self) -> tokio::sync::broadcast::Receiver<fred::types::Message> {
        self.inner.message_rx()
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{RedisClient, RedisClientError};

    #[tokio::test]
    async fn unavailable_redis_is_reported_as_a_connection_error() {
        let result = RedisClient::connect("redis://127.0.0.1:0", Duration::from_millis(100)).await;

        assert!(matches!(result, Err(RedisClientError::Connection(_))));
    }
}
