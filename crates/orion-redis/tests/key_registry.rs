use std::collections::HashSet;

use orion_domain::UserId;
use orion_redis::{RedisKey, REDIS_KEY_REGISTRY};
use uuid::Uuid;

#[test]
fn redis_registry_is_unique_owned_and_documented() {
    let docs = include_str!("../../../docs/contracts/redis.md");
    let mut ids = HashSet::new();
    let mut patterns = HashSet::new();
    for key in REDIS_KEY_REGISTRY {
        assert!(ids.insert(key.id), "duplicate Redis key id: {}", key.id);
        assert!(
            patterns.insert(key.pattern),
            "duplicate Redis key pattern: {}",
            key.pattern
        );
        assert!(!key.owner.is_empty());
        assert!(!key.invalidation_rule.is_empty());
        assert!(
            docs.contains(&format!("`{}`", key.id)),
            "undocumented Redis key: {}",
            key.id
        );
    }
}

#[test]
fn typed_keys_use_the_versioned_root_namespace() {
    let id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("valid UUID");
    assert_eq!(
        RedisKey::Session { session_id: id }.to_string(),
        "orion:v1:session:00000000-0000-0000-0000-000000000001"
    );
    assert_eq!(
        RedisKey::Profile {
            user_id: UserId::from_uuid(id)
        }
        .to_string(),
        "orion:v1:cache:profile:00000000-0000-0000-0000-000000000001"
    );
}
