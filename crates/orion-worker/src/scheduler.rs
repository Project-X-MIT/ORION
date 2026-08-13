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

// TODO(DIV-06, DIV-08): Reconcile this shared registration metadata with
// Div's merged implementations after those issues land. Phantom's body stays
// feature-owned; only the integration wiring should change at that point.
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
    use super::{worker_job, WORKER_JOB_REGISTRY};

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
}
