//! Disposable caching and coordination infrastructure.
//!
//! PostgreSQL remains authoritative for every business record.

pub mod keys;

pub use keys::{redis_key, RedisKey, RedisKeySpec, RedisNamespace, RedisTtl, REDIS_KEY_REGISTRY};
