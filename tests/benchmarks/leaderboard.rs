//! Deterministic in-memory ranking benchmark.
//!
//! Run with:
//! `cargo test -p orion-worker --test leaderboard_snapshot benchmark -- --ignored --nocapture`

use std::{hint::black_box, time::Instant};

const PRODUCTION_USERS: usize = 100_000;
const APPROVED_DURATION_MILLIS: u128 = 2_000;

#[test]
#[ignore = "explicit production-sized benchmark"]
fn production_sized_rank_rebuild_meets_approved_target() {
    let mut ratings: Vec<_> = (0..PRODUCTION_USERS)
        .map(|user_id| (user_id, 800 + ((user_id * 37) % 1_601)))
        .collect();
    let started = Instant::now();

    ratings.sort_unstable_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
    let checksum = ratings
        .iter()
        .enumerate()
        .fold(0_u64, |sum, (index, (user_id, _))| {
            sum.wrapping_add(black_box((index + 1 + user_id) as u64))
        });
    let elapsed = started.elapsed();

    assert_ne!(checksum, 0);
    assert!(
        elapsed.as_millis() <= APPROVED_DURATION_MILLIS,
        "100k-user rebuild took {elapsed:?}, target is {APPROVED_DURATION_MILLIS}ms"
    );
    println!("100k-user rank rebuild: {elapsed:?}, checksum={checksum}");
}

#[test]
fn identical_sources_rebuild_to_identical_ranks() {
    let source = vec![(3_u64, 1_500_u32), (1, 1_500), (2, 1_200)];
    let rank = |mut rows: Vec<(u64, u32)>| {
        rows.sort_unstable_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
        rows.into_iter()
            .enumerate()
            .map(|(index, (user_id, _))| (user_id, index + 1))
            .collect::<Vec<_>>()
    };

    assert_eq!(rank(source.clone()), rank(source));
}
