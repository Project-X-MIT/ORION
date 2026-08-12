use std::time::Duration;

use orion_redis::{
    cache::research::{self, ResearchCacheInvalidationEvent},
    RedisClient,
};
use orion_worker::jobs::research_review::{
    invalidate_published_research_cache, invalidate_research_cache_after_policy_event,
};
use serde_json::json;
use uuid::Uuid;

/// Exercises the cache/job seam when an integration Redis endpoint is available.
/// The unique key keeps the test safe against a shared development Redis.
#[tokio::test]
async fn research_policy_event_hooks_invalidate_published_cache() {
    let Ok(redis_url) = std::env::var("REDIS_URL") else {
        return;
    };

    let redis = match RedisClient::connect(&redis_url, Duration::from_secs(2)).await {
        Ok(redis) => redis,
        Err(error) => {
            eprintln!("skipping Redis cache/job integration test: {error}");
            return;
        }
    };

    let research_id = Uuid::new_v4();
    let value = json!({
        "status": "published",
        "published_at": "publication-v1"
    });

    research::set_published(&redis, research_id, "publication-v1", &value)
        .await
        .expect("published cache fill should succeed");
    assert!(
        research::get_published::<serde_json::Value>(&redis, research_id)
            .await
            .expect("published cache read should succeed")
            .is_some()
    );
    assert!(
        !research::invalidate_if_version(&redis, research_id, "publication-v0")
            .await
            .expect("stale version check should succeed")
    );

    invalidate_research_cache_after_policy_event(
        &redis,
        research_id,
        ResearchCacheInvalidationEvent::Publication,
    )
    .await
    .expect("publication hook should invalidate the cache");
    assert!(
        research::get_published::<serde_json::Value>(&redis, research_id)
            .await
            .expect("cache read after publication should succeed")
            .is_none()
    );

    research::set_published(&redis, research_id, "publication-v2", &value)
        .await
        .expect_err("cache payload version must match the published version");
    let value_v2 = json!({
        "status": "published",
        "published_at": "publication-v2"
    });
    research::set_published(&redis, research_id, "publication-v2", &value_v2)
        .await
        .expect("second published cache fill should succeed");
    assert!(
        !research::invalidate_if_version(&redis, research_id, "publication-v1")
            .await
            .expect("older version invalidation should succeed")
    );
    assert_eq!(
        research::get_published::<serde_json::Value>(&redis, research_id)
            .await
            .expect("newer cache read should succeed")
            .expect("newer cache fill should remain")
            .published_version,
        "publication-v2"
    );
    invalidate_research_cache_after_policy_event(
        &redis,
        research_id,
        ResearchCacheInvalidationEvent::Withdrawal,
    )
    .await
    .expect("withdrawal hook should invalidate the cache");
    assert!(
        research::get_published::<serde_json::Value>(&redis, research_id)
            .await
            .expect("cache read after withdrawal should succeed")
            .is_none()
    );

    let value_v3 = json!({
        "status": "published",
        "published_at": "publication-v3"
    });
    research::set_published(&redis, research_id, "publication-v3", &value_v3)
        .await
        .expect("third published cache fill should succeed");
    invalidate_published_research_cache(&redis, research_id)
        .await
        .expect("publication wrapper should invalidate the cache");
    assert!(
        research::get_published::<serde_json::Value>(&redis, research_id)
            .await
            .expect("cache read after publication wrapper should succeed")
            .is_none()
    );

    redis
        .close()
        .await
        .expect("Redis integration client should close cleanly");
}
