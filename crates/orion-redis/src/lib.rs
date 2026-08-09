//! Disposable caching and coordination infrastructure.
//!
//! PostgreSQL remains authoritative for every business record.

pub mod client;
pub mod keys;
pub mod sessions;

pub use client::{RedisClient, RedisClientError};
pub use keys::{redis_key, RedisKey, RedisKeySpec, RedisNamespace, RedisTtl, REDIS_KEY_REGISTRY};
pub use sessions::{RedisSession, RedisSessionStore, SessionStoreError};
