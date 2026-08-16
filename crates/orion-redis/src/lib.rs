//! Disposable caching and coordination infrastructure.
//!
//! PostgreSQL remains authoritative for every business record.

pub mod cache;
pub mod client;
pub mod keys;
pub mod locks;
pub mod pubsub;
pub mod rate_limit;
pub mod sessions;

pub use client::{RedisClient, RedisClientError};
pub use keys::{redis_key, RedisKey, RedisKeySpec, RedisNamespace, RedisTtl, REDIS_KEY_REGISTRY};
pub use locks::{DistributedLock, LockError, LockLease};
pub use pubsub::{PubSubEnvelope, PubSubError, RedisPublisher, RedisSubscriber};
pub use rate_limit::{RateLimitDecision, RedisRateLimiter};
pub use sessions::{RedisSession, RedisSessionStore, SessionStoreError};
