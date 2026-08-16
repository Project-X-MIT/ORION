//! Div-owned worker job registration metadata.
//!
//! Feature modules implement the bodies referenced here. This registry only
//! defines the stable job identity and execution contract used by the worker
//! runtime; it must not contain feature business logic.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerJobSpec {
    pub id: &'static str,
    pub registry_owner: &'static str,
    pub body_owner: &'static str,
    pub body_path: &'static str,
    pub trigger: &'static str,
}

pub const RESEARCH_REVIEW_JOB_ID: &str = "research_review";
pub const NOTIFICATION_JOB_ID: &str = "notification";

// Registry metadata is platform-owned; feature modules own the referenced
// bodies and may add fixtures without changing scheduler execution semantics.
pub const WORKER_JOB_REGISTRY: &[WorkerJobSpec] = &[
    WorkerJobSpec {
        id: RESEARCH_REVIEW_JOB_ID,
        registry_owner: "divi912",
        body_owner: "shivanshrawat13aug2007-commits",
        body_path: "orion_worker::jobs::research_review::process_research_award",
        trigger: "orion.research.elo_award.requested",
    },
    WorkerJobSpec {
        id: NOTIFICATION_JOB_ID,
        registry_owner: "divi912",
        body_owner: "divi912",
        body_path: "orion_worker::jobs::notification::process_notification",
        trigger: "orion.notification.requested",
    },
];

#[must_use]
pub fn worker_job(id: &str) -> Option<&'static WorkerJobSpec> {
    WORKER_JOB_REGISTRY.iter().find(|job| job.id == id)
}

#[cfg(test)]
mod tests {
    use super::{worker_job, RetryPolicy, WORKER_JOB_REGISTRY};
    use std::time::Duration;

    #[test]
    fn research_job_is_registered_by_div_and_implemented_by_phantom() {
        let job = worker_job("research_review").expect("research job should be registered");
        assert_eq!(job.registry_owner, "divi912");
        assert_eq!(job.body_owner, "shivanshrawat13aug2007-commits");
        assert_eq!(
            job.body_path,
            "orion_worker::jobs::research_review::process_research_award"
        );
        assert_eq!(job.trigger, "orion.research.elo_award.requested");
    }

    #[test]
    fn registered_job_ids_are_unique() {
        for (index, job) in WORKER_JOB_REGISTRY.iter().enumerate() {
            assert!(!WORKER_JOB_REGISTRY[index + 1..]
                .iter()
                .any(|other| other.id == job.id));
        }
    }

    #[test]
    fn retry_schedule_is_bounded_and_deterministic() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.delay(1, 42), policy.delay(1, 42));
        assert!(policy.delay(1, 42).unwrap() >= Duration::from_millis(800));
        assert!(policy.delay(1, 42).unwrap() <= Duration::from_millis(1_200));
        assert!(policy.delay(4, 42).unwrap() >= Duration::from_millis(6_400));
        assert!(policy.delay(4, 42).unwrap() <= Duration::from_millis(9_600));
        assert_eq!(policy.delay(5, 42), None);
    }

    #[test]
    fn registered_feature_fixtures_have_stable_execution_contracts() {
        assert!(WORKER_JOB_REGISTRY.iter().all(|job| {
            !job.id.is_empty()
                && !job.registry_owner.is_empty()
                && !job.body_owner.is_empty()
                && job.body_path.contains("::")
                && !job.trigger.is_empty()
        }));
    }
}
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobContext {
    pub job_id: Uuid,
    pub job_name: &'static str,
    pub attempt: u32,
    pub idempotency_key: String,
    pub request_id: Option<String>,
    pub trace_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub initial_delay: Duration,
    pub maximum_delay: Duration,
    pub jitter_percent: u8,
}

impl RetryPolicy {
    #[must_use]
    pub fn delay(self, attempt: u32, jitter_seed: u64) -> Option<Duration> {
        if attempt == 0 || attempt >= self.max_attempts {
            return None;
        }
        let exponent = attempt.saturating_sub(1).min(31);
        let base = self
            .initial_delay
            .saturating_mul(1_u32 << exponent)
            .min(self.maximum_delay);
        let spread = base.mul_f64(f64::from(self.jitter_percent.min(100)) / 100.0);
        let unit = (jitter_seed % 10_001) as f64 / 10_000.0;
        Some(
            base.saturating_sub(spread)
                .saturating_add(spread.mul_f64(unit * 2.0)),
        )
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            initial_delay: Duration::from_secs(1),
            maximum_delay: Duration::from_secs(60),
            jitter_percent: 20,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobOutcome {
    Succeeded,
    RetryWaiting,
    DeadLettered,
}

pub trait IdempotencyKey {
    fn idempotency_key(&self) -> String;
}
pub trait ConcurrencyKey {
    fn concurrency_key(&self) -> String;
}
