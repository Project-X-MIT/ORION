//! In-memory Elo benchmark.
//!
//! Run with:
//! `cargo test -p orion-domain --test quiz elo_calculation_benchmark -- --ignored --nocapture`

use std::hint::black_box;
use std::time::Instant;

use super::quiz::elo::compute_elo;

const ITERATIONS: usize = 100_000;

/// Measures the pure Elo calculation without opening a database connection,
/// reading rating rows, writing events, or touching the persistence layer.
#[test]
#[ignore = "explicit in-memory benchmark"]
fn elo_calculation_benchmark_without_persistence() {
    let inputs = [
        (500.0, 500.0, 20.0, 1.0),
        (500.0, 500.0, 20.0, 0.0),
        (1000.0, 2000.0, 30.0, 1.0),
        (2000.0, 1000.0, 35.0, 0.0),
        (1500.0, 1700.0, 0.0, 0.0),
    ];
    let mut checksum = 0_i64;
    let started = Instant::now();

    for index in 0..ITERATIONS {
        let (player, question, k, sa) = inputs[index % inputs.len()];
        let result = compute_elo(
            black_box(player),
            black_box(question),
            black_box(k),
            black_box(sa),
        );
        checksum = checksum.wrapping_add(i64::from(black_box(result.rounded_delta)));
    }

    let elapsed = started.elapsed();
    let nanos_per_calculation = elapsed.as_secs_f64() * 1_000_000_000.0 / ITERATIONS as f64;

    assert_ne!(checksum, 0, "benchmark work must not be optimized away");
    println!(
        "Elo benchmark: {ITERATIONS} calculations in {elapsed:?} ({nanos_per_calculation:.2} ns/calculation), checksum={checksum}"
    );
}
