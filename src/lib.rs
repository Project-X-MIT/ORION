//! Cross-crate composition surface and integration-test host.
//!
//! Production business logic belongs to the owning crate. This package only
//! proves that the independently owned crates compose against shared contracts.

pub use orion_api as api;
pub use orion_common as common;
pub use orion_db as db;
pub use orion_domain as domain;
pub use orion_redis as redis;
pub use orion_worker as worker;
