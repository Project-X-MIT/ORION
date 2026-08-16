use std::time::Duration;

use thiserror::Error;
use uuid::Uuid;

use crate::{RedisClient, RedisClientError};

const ACQUIRE: &str = r#"
if redis.call('SET', KEYS[1], ARGV[1], 'NX', 'PX', ARGV[2]) then return 1 end
return 0
"#;
const RELEASE: &str = r#"
if redis.call('GET', KEYS[1]) == ARGV[1] then return redis.call('DEL', KEYS[1]) end
return 0
"#;
const RENEW: &str = r#"
if redis.call('GET', KEYS[1]) == ARGV[1] then return redis.call('PEXPIRE', KEYS[1], ARGV[2]) end
return 0
"#;

#[derive(Debug, Error)]
pub enum LockError {
    #[error("Redis lock operation failed")]
    Redis(#[from] RedisClientError),
    #[error("lock lease must be greater than zero")]
    InvalidLease,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockLease {
    key: String,
    owner_token: String,
}

impl LockLease {
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }
}

#[derive(Clone)]
pub struct DistributedLock {
    redis: RedisClient,
}

impl DistributedLock {
    #[must_use]
    pub const fn new(redis: RedisClient) -> Self {
        Self { redis }
    }

    pub async fn acquire(
        &self,
        key: impl Into<String>,
        lease: Duration,
    ) -> Result<Option<LockLease>, LockError> {
        let lease_ms = i64::try_from(lease.as_millis()).map_err(|_| LockError::InvalidLease)?;
        if lease_ms == 0 {
            return Err(LockError::InvalidLease);
        }
        let key = key.into();
        let owner_token = Uuid::new_v4().to_string();
        let acquired = self
            .redis
            .eval_i64(
                ACQUIRE,
                vec![key.clone().into()],
                vec![owner_token.clone().into(), lease_ms.into()],
            )
            .await?
            == 1;
        Ok(acquired.then_some(LockLease { key, owner_token }))
    }

    pub async fn renew(&self, lease: &LockLease, duration: Duration) -> Result<bool, LockError> {
        let lease_ms = i64::try_from(duration.as_millis()).map_err(|_| LockError::InvalidLease)?;
        if lease_ms == 0 {
            return Err(LockError::InvalidLease);
        }
        Ok(self
            .redis
            .eval_i64(
                RENEW,
                vec![lease.key.clone().into()],
                vec![lease.owner_token.clone().into(), lease_ms.into()],
            )
            .await?
            == 1)
    }

    pub async fn release(&self, lease: &LockLease) -> Result<bool, LockError> {
        Ok(self
            .redis
            .eval_i64(
                RELEASE,
                vec![lease.key.clone().into()],
                vec![lease.owner_token.clone().into()],
            )
            .await?
            == 1)
    }
}
