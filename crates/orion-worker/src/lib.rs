//! Shared scheduler and job-registration surface.
//!
//! Feature crates own job bodies; the platform owns execution semantics.

pub mod jobs;
pub mod scheduler;
