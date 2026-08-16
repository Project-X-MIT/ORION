use std::time::Duration;

use crate::{RedisClient, RedisClientError};

const FIXED_WINDOW: &str = r#"
local count = redis.call('INCR', KEYS[1])
if count == 1 then redis.call('PEXPIRE', KEYS[1], ARGV[1]) end
return count
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitDecision {
    pub allowed: bool,
    pub count: u64,
    pub limit: u64,
}

#[derive(Clone)]
pub struct RedisRateLimiter {
    redis: RedisClient,
}

impl RedisRateLimiter {
    #[must_use]
    pub const fn new(redis: RedisClient) -> Self {
        Self { redis }
    }

    pub async fn check(
        &self,
        key: impl Into<String>,
        limit: u64,
        window: Duration,
    ) -> Result<RateLimitDecision, RedisClientError> {
        let window_ms = i64::try_from(window.as_millis()).unwrap_or(i64::MAX).max(1);
        let count = self
            .redis
            .eval_i64(
                FIXED_WINDOW,
                vec![key.into().into()],
                vec![window_ms.into()],
            )
            .await?;
        let count = u64::try_from(count).unwrap_or(u64::MAX);
        Ok(RateLimitDecision {
            allowed: count <= limit,
            count,
            limit,
        })
    }
}
