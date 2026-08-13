use std::{env, sync::Arc, time::Duration};

use orion_redis::{DistributedLock, RedisClient, RedisRateLimiter};
use tokio::sync::Barrier;
use uuid::Uuid;

async fn live_client() -> Option<RedisClient> {
    let url = env::var("ORION_TEST_REDIS_URL")
        .or_else(|_| env::var("REDIS_URL"))
        .ok()?;
    RedisClient::connect(&url, Duration::from_secs(2))
        .await
        .ok()
}

#[tokio::test]
async fn lock_contention_expiry_and_stale_owner_release_are_safe() {
    let Some(redis) = live_client().await else {
        eprintln!("skipping live Redis lock test: ORION_TEST_REDIS_URL is unavailable");
        return;
    };
    let lock = DistributedLock::new(redis.clone());
    let key = format!("orion:v1:test:lock:{}", Uuid::new_v4());

    let stale_owner = lock
        .acquire(&key, Duration::from_millis(100))
        .await
        .expect("acquire initial lease")
        .expect("initial owner acquires lock");
    assert!(lock
        .acquire(&key, Duration::from_secs(1))
        .await
        .expect("contending acquire")
        .is_none());

    tokio::time::sleep(Duration::from_millis(150)).await;
    let current_owner = lock
        .acquire(&key, Duration::from_secs(1))
        .await
        .expect("acquire after expiry")
        .expect("new owner acquires expired lock");
    assert!(!lock
        .release(&stale_owner)
        .await
        .expect("stale release is a safe no-op"));
    assert!(!lock
        .renew(&stale_owner, Duration::from_secs(1))
        .await
        .expect("stale renewal is a safe no-op"));
    assert!(lock
        .release(&current_owner)
        .await
        .expect("current owner releases lock"));
    redis.close().await.expect("close Redis client");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fixed_window_rate_limit_is_atomic_under_concurrency() {
    const REQUESTS: usize = 64;
    const LIMIT: u64 = 17;

    let Some(redis) = live_client().await else {
        eprintln!("skipping live Redis rate-limit test: ORION_TEST_REDIS_URL is unavailable");
        return;
    };
    let limiter = RedisRateLimiter::new(redis.clone());
    let key = format!("orion:v1:test:rate_limit:{}", Uuid::new_v4());
    let barrier = Arc::new(Barrier::new(REQUESTS));
    let mut tasks = Vec::with_capacity(REQUESTS);

    for _ in 0..REQUESTS {
        let limiter = limiter.clone();
        let key = key.clone();
        let barrier = Arc::clone(&barrier);
        tasks.push(tokio::spawn(async move {
            barrier.wait().await;
            limiter.check(key, LIMIT, Duration::from_secs(5)).await
        }));
    }

    let mut counts = Vec::with_capacity(REQUESTS);
    let mut allowed = 0;
    for task in tasks {
        let decision = task
            .await
            .expect("join request")
            .expect("rate-limit request");
        counts.push(decision.count);
        allowed += usize::from(decision.allowed);
    }
    counts.sort_unstable();
    assert_eq!(counts, (1..=REQUESTS as u64).collect::<Vec<_>>());
    assert_eq!(allowed, LIMIT as usize);
    redis.close().await.expect("close Redis client");
}
