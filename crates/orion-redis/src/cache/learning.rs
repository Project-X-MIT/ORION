//! Disposable cache for mostly-static, published learning content.
//!
//! This module deliberately caches only the published course aggregate. User
//! progress is never part of the cache payload; PostgreSQL remains the sole
//! authority for progress reads and writes.
//!
//! TODO(Div): export this module from `cache/mod.rs` after the shared cache
//! module registry is updated. The `cache.learning_course` key itself is
//! already registered by Div.

use chrono::{DateTime, Duration, Utc};
use orion_domain::learning::{ContentLifecycle, Course, LearningContractError};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{RedisClient, RedisClientError, RedisKey};

pub const LEARNING_COURSE_CACHE_TTL_SECONDS: i64 = 3_600;
pub const LEARNING_COURSE_CACHE_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Error)]
pub enum LearningCacheError {
    #[error("learning cache Redis operation failed")]
    Redis(#[from] RedisClientError),
    #[error("learning cache payload serialization failed")]
    Serialization(#[from] serde_json::Error),
    #[error("learning cache course contract is invalid")]
    Contract(#[from] LearningContractError),
    #[error("learning cache schema version is unsupported")]
    UnsupportedSchemaVersion,
    #[error("learning cache can store only published content")]
    NotPublished,
    #[error("learning cache cannot store user progress")]
    ContainsProgress,
    #[error("learning cache course ID does not match its key")]
    CourseIdMismatch,
    #[error("learning cache course version does not match its envelope")]
    VersionMismatch,
}

/// Versioned cache envelope for the public, published course aggregate.
///
/// `Course::progress` must be `None` before this value is stored. The
/// aggregate's publication lifecycle and version are retained so malformed or
/// accidentally private values cannot be accepted as cache content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishedLearningCourse {
    pub schema_version: u16,
    pub cached_at: DateTime<Utc>,
    pub course_version: u32,
    pub course: Course,
}

impl PublishedLearningCourse {
    #[must_use]
    pub fn is_fresh_at(&self, now: DateTime<Utc>) -> bool {
        let age = now.signed_duration_since(self.cached_at);
        age >= Duration::zero() && age < Duration::seconds(LEARNING_COURSE_CACHE_TTL_SECONDS)
    }
}

/// Returns a fresh published course from Redis.
///
/// A missing, stale, or future-dated entry is a cache miss. The API must load
/// the course from PostgreSQL and may refill the cache only after publication
/// validation succeeds.
pub async fn get_course(
    redis: &RedisClient,
    course_id: Uuid,
) -> Result<Option<PublishedLearningCourse>, LearningCacheError> {
    let key = RedisKey::LearningCourse { course_id }.to_string();
    let Some(payload) = redis.get(key).await? else {
        return Ok(None);
    };

    let entry = serde_json::from_str::<PublishedLearningCourse>(&payload)?;
    validate_entry(&entry, course_id)?;
    if entry.is_fresh_at(Utc::now()) {
        Ok(Some(entry))
    } else {
        Ok(None)
    }
}

/// Stores only a validated, published course aggregate. Progress-bearing
/// aggregates are rejected so Redis cannot become a source of user state.
pub async fn set_course(redis: &RedisClient, course: &Course) -> Result<(), LearningCacheError> {
    validate_published_course(course)?;

    let entry = PublishedLearningCourse {
        schema_version: LEARNING_COURSE_CACHE_SCHEMA_VERSION,
        cached_at: Utc::now(),
        course_version: course.version,
        course: course.clone(),
    };
    let payload = serde_json::to_string(&entry)?;
    redis
        .set_ex(
            RedisKey::LearningCourse {
                course_id: course.id,
            }
            .to_string(),
            payload,
            LEARNING_COURSE_CACHE_TTL_SECONDS,
        )
        .await?;
    Ok(())
}

/// Removes one course cache entry after a committed publication, retirement,
/// or version change. Deleting a missing key is intentionally idempotent.
pub async fn invalidate_course(
    redis: &RedisClient,
    course_id: Uuid,
) -> Result<(), LearningCacheError> {
    redis
        .delete(RedisKey::LearningCourse { course_id }.to_string())
        .await?;
    Ok(())
}

/// Invalidates the disposable course cache after an authorized publication
/// has committed in PostgreSQL.
///
/// Authorization and the publication transaction belong to the content
/// owner. Redis cannot authorize publication; callers must invoke this hook
/// only after the approved publication event/transaction succeeds. Repeated
/// delivery is safe because deleting an absent key succeeds.
pub async fn invalidate_after_publication(
    redis: &RedisClient,
    course_id: Uuid,
) -> Result<(), LearningCacheError> {
    invalidate_course(redis, course_id).await
}

fn validate_entry(
    entry: &PublishedLearningCourse,
    course_id: Uuid,
) -> Result<(), LearningCacheError> {
    if entry.schema_version != LEARNING_COURSE_CACHE_SCHEMA_VERSION {
        return Err(LearningCacheError::UnsupportedSchemaVersion);
    }
    if entry.course.id != course_id {
        return Err(LearningCacheError::CourseIdMismatch);
    }
    if entry.course_version != entry.course.version {
        return Err(LearningCacheError::VersionMismatch);
    }
    validate_published_course(&entry.course)
}

fn validate_published_course(course: &Course) -> Result<(), LearningCacheError> {
    course.validate()?;
    if course.lifecycle != ContentLifecycle::Published {
        return Err(LearningCacheError::NotPublished);
    }
    if course.progress.is_some() {
        return Err(LearningCacheError::ContainsProgress);
    }
    if course
        .modules
        .iter()
        .any(|module| module.lifecycle != ContentLifecycle::Published)
    {
        return Err(LearningCacheError::NotPublished);
    }
    if course.modules.iter().any(|module| {
        module
            .lessons
            .iter()
            .any(|lesson| lesson.lifecycle != ContentLifecycle::Published)
    }) {
        return Err(LearningCacheError::NotPublished);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone, Utc};
    use orion_domain::learning::{CourseLesson, CourseModule, CourseProgressSummary};

    use super::{
        validate_published_course, ContentLifecycle, LearningCacheError, PublishedLearningCourse,
        LEARNING_COURSE_CACHE_SCHEMA_VERSION, LEARNING_COURSE_CACHE_TTL_SECONDS,
    };
    use orion_domain::learning::Course;
    use uuid::Uuid;

    fn published_course() -> Course {
        let module_id = Uuid::from_u128(2);
        Course {
            id: Uuid::from_u128(1),
            slug: "beginner-trading".to_owned(),
            title: "Beginner Trading".to_owned(),
            description: Some("A safe course".to_owned()),
            version: 1,
            lifecycle: ContentLifecycle::Published,
            modules: vec![CourseModule {
                id: module_id,
                slug: "foundations".to_owned(),
                title: "Foundations".to_owned(),
                description: None,
                display_order: 1,
                lifecycle: ContentLifecycle::Published,
                lessons: vec![CourseLesson {
                    id: Uuid::from_u128(3),
                    module_id,
                    slug: "market-basics".to_owned(),
                    title: "Market Basics".to_owned(),
                    summary: None,
                    content: "Safe content".to_owned(),
                    lesson_order: 1,
                    estimated_minutes: 10,
                    lifecycle: ContentLifecycle::Published,
                }],
                progress: None,
            }],
            progress: None,
        }
    }

    #[test]
    fn cache_freshness_is_bounded_by_the_registered_budget() {
        let cached_at = Utc.with_ymd_and_hms(2026, 8, 16, 10, 0, 0).unwrap();
        let entry = PublishedLearningCourse {
            schema_version: LEARNING_COURSE_CACHE_SCHEMA_VERSION,
            cached_at,
            course_version: 1,
            course: published_course(),
        };

        assert!(
            entry.is_fresh_at(cached_at + Duration::seconds(LEARNING_COURSE_CACHE_TTL_SECONDS - 1))
        );
        assert!(
            !entry.is_fresh_at(cached_at + Duration::seconds(LEARNING_COURSE_CACHE_TTL_SECONDS))
        );
        assert!(!entry.is_fresh_at(cached_at - Duration::seconds(1)));
    }

    #[test]
    fn only_fully_published_content_is_cacheable() {
        let mut course = published_course();
        assert!(validate_published_course(&course).is_ok());

        course.lifecycle = ContentLifecycle::Draft;
        assert!(matches!(
            validate_published_course(&course),
            Err(LearningCacheError::NotPublished)
        ));

        course = published_course();
        course.progress = Some(CourseProgressSummary {
            user_id: Uuid::from_u128(9),
            total_modules: 1,
            completed_modules: 0,
            total_lessons: 1,
            completed_lessons: 0,
            completed: false,
        });
        assert!(matches!(
            validate_published_course(&course),
            Err(LearningCacheError::ContainsProgress)
        ));
    }

    #[test]
    fn envelope_rejects_mismatched_versions_and_ids() {
        let course = published_course();
        let mut entry = PublishedLearningCourse {
            schema_version: LEARNING_COURSE_CACHE_SCHEMA_VERSION,
            cached_at: Utc::now(),
            course_version: 2,
            course,
        };
        assert!(matches!(
            super::validate_entry(&entry, Uuid::from_u128(1)),
            Err(LearningCacheError::VersionMismatch)
        ));

        entry.course_version = 1;
        assert!(matches!(
            super::validate_entry(&entry, Uuid::from_u128(4)),
            Err(LearningCacheError::CourseIdMismatch)
        ));
    }
}
