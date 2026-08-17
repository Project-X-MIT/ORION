//! Transport-neutral contracts for the beginner learning course.
//!
//! PostgreSQL remains authoritative for published content and user progress.
//! These types describe the stable read/write boundary used by the API,
//! cache, and repository adapters; they do not depend on Axum, SQLx, or Redis.
//!
//! TODO(DIV-04..DIV-06): route registration and Redis key registration belong
//! to Div; this module must not define route paths or cache-key authority.
//!
//! TODO(DB-01): the API/repository adapter must load and persist progress in
//! PostgreSQL. Redis may cache a disposable read projection, but must never be
//! the only copy of `CourseProgress`.
//!
//! TODO(Product): map `ContentLifecycle` from the approved product publication
//! contract before exposing content; publication is not inferred from cache
//! presence or a client-supplied flag.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// Version of the serialized learning contract.
pub const LEARNING_CONTRACT_VERSION: u16 = 1;

/// The logical course represented by the `course_modules` and
/// `course_lessons` tables. There is intentionally no course table in the
/// DB-05 baseline, so this reserved logical identity represents the single
/// beginner course without pretending that a course row exists.
pub const BEGINNER_COURSE_SLUG: &str = "beginner-trading";

/// Stable logical identity for the DB-05 beginner course aggregate.
///
/// This UUID is not a persistence row ID. Product/DB-01 may replace it with a
/// course-table identity in a versioned contract; until then, rejecting every
/// other UUID prevents arbitrary paths from being treated as courses.
pub const BEGINNER_COURSE_ID: Uuid = Uuid::from_u128(1);

#[must_use]
pub fn is_beginner_course_id(course_id: Uuid) -> bool {
    course_id == BEGINNER_COURSE_ID
}

/// Content lifecycle. A new version is published as a new content aggregate;
/// an old published aggregate is retired rather than mutated in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentLifecycle {
    Draft,
    Published,
    Retired,
}

impl ContentLifecycle {
    #[must_use]
    pub const fn is_public(self) -> bool {
        matches!(self, Self::Published)
    }

    /// Returns whether a lifecycle transition is allowed. Repeating the same
    /// transition is safe and idempotent for retries.
    #[must_use]
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Draft, Self::Draft | Self::Published)
                | (Self::Published, Self::Published | Self::Retired)
                | (Self::Retired, Self::Retired)
        )
    }

    pub fn transition_to(self, next: Self) -> Result<Self, LearningContractError> {
        if self.can_transition_to(next) {
            Ok(next)
        } else {
            Err(LearningContractError::InvalidLifecycleTransition {
                from: self,
                to: next,
            })
        }
    }
}

/// The public course aggregate returned by the learning API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Course {
    /// Stable logical identity used by the route and registered cache key.
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    pub description: Option<String>,
    /// Incremented when a new course content version replaces a retired one.
    pub version: u32,
    pub lifecycle: ContentLifecycle,
    /// Modules must be ordered by `display_order` ascending.
    pub modules: Vec<CourseModule>,
    /// Omitted for an anonymous request; populated from PostgreSQL for an
    /// authenticated request.
    pub progress: Option<CourseProgressSummary>,
}

impl Course {
    /// Sorts the aggregate using the same total order as the DB queries. The
    /// ID tie-breaker keeps the contract deterministic even for malformed data
    /// before validation reports the duplicate order.
    pub fn sort_deterministically(&mut self) {
        self.modules
            .sort_by_key(|module| (module.display_order, module.id));
        for module in &mut self.modules {
            module.sort_deterministically();
        }
    }

    pub fn validate(&self) -> Result<(), LearningContractError> {
        validate_uuid("course id", self.id)?;
        validate_slug("course slug", &self.slug)?;
        validate_required_text("course title", &self.title)?;
        validate_optional_text("course description", self.description.as_deref())?;
        if self.version == 0 {
            return Err(LearningContractError::InvalidVersion);
        }

        validate_unique_and_ordered(
            self.modules
                .iter()
                .map(|module| (module.display_order, module.id, module.slug.as_str())),
            "module",
        )?;
        for module in &self.modules {
            module.validate()?;
        }
        if let Some(progress) = &self.progress {
            progress.validate()?;
        }
        Ok(())
    }

    /// Returns the published lessons in the single stable order shared by
    /// retrieval, prerequisite checks, completion summaries, and next-lesson
    /// selection. Draft and retired content is never part of the public path.
    pub fn ordered_published_lessons(&self) -> Vec<&CourseLesson> {
        let mut lessons = self
            .modules
            .iter()
            .filter(|module| module.lifecycle.is_public())
            .flat_map(|module| {
                module
                    .lessons
                    .iter()
                    .filter(|lesson| lesson.lifecycle.is_public())
                    .map(move |lesson| (module.display_order, lesson))
            })
            .collect::<Vec<_>>();
        lessons
            .sort_by_key(|(module_order, lesson)| (*module_order, lesson.lesson_order, lesson.id));
        lessons.into_iter().map(|(_, lesson)| lesson).collect()
    }

    /// Sequential prerequisite policy: every earlier published lesson in the
    /// course must be complete before this lesson may be completed. This is
    /// derived from the DB-05 module/lesson order; no extra prerequisite table
    /// or provider-specific metadata is introduced.
    pub fn prerequisites_for(
        &self,
        lesson_id: Uuid,
    ) -> Result<LessonPrerequisites, LearningContractError> {
        self.validate()?;
        let lessons = self.ordered_published_lessons();
        let position = lessons
            .iter()
            .position(|lesson| lesson.id == lesson_id)
            .ok_or_else(|| self.lesson_lookup_error(lesson_id))?;
        Ok(LessonPrerequisites {
            lesson_id,
            required_lesson_ids: lessons[..position].iter().map(|lesson| lesson.id).collect(),
        })
    }

    /// Returns whether all derived prerequisites for a lesson are complete.
    pub fn is_lesson_unlocked(
        &self,
        lesson_id: Uuid,
        progress: &[CourseProgress],
    ) -> Result<bool, LearningContractError> {
        self.validate_progress_entries(progress)?;
        let prerequisites = self.prerequisites_for(lesson_id)?;
        Ok(prerequisites
            .required_lesson_ids
            .iter()
            .all(|required_id| is_completed(progress, *required_id)))
    }

    /// Selects the first incomplete published lesson. Because prerequisites
    /// are all earlier lessons in this same order, this selection is also the
    /// only valid next lesson and cannot skip ahead.
    pub fn next_lesson<'a>(
        &'a self,
        progress: &[CourseProgress],
    ) -> Result<NextLesson<'a>, LearningContractError> {
        self.validate()?;
        self.validate_progress_entries(progress)?;
        let published_modules = self
            .modules
            .iter()
            .filter(|module| module.lifecycle.is_public())
            .collect::<Vec<_>>();
        let has_empty_module = published_modules.iter().any(|module| {
            !module
                .lessons
                .iter()
                .any(|lesson| lesson.lifecycle.is_public())
        });
        if published_modules.is_empty() || has_empty_module {
            return Ok(NextLesson::NoPublishedLessons);
        }

        let lessons = self.ordered_published_lessons();
        if let Some(lesson) = lessons
            .iter()
            .copied()
            .find(|lesson| !is_completed(progress, lesson.id))
        {
            Ok(NextLesson::Available(lesson))
        } else {
            Ok(NextLesson::CourseCompleted)
        }
    }

    /// Completes a lesson only when its sequential prerequisites are complete.
    /// A repeated completion remains idempotent and does not re-check or alter
    /// the original completion timestamp.
    pub fn complete_lesson(
        &self,
        user_id: Uuid,
        lesson_id: Uuid,
        progress: &[CourseProgress],
        at: DateTime<Utc>,
    ) -> Result<CourseProgress, LearningContractError> {
        self.validate()?;
        validate_uuid("course progress user id", user_id)?;
        self.validate_progress_entries(progress)?;
        if progress.iter().any(|entry| entry.user_id != user_id) {
            return Err(LearningContractError::ProgressUserMismatch);
        }

        let lesson = self
            .ordered_published_lessons()
            .into_iter()
            .find(|lesson| lesson.id == lesson_id)
            .ok_or_else(|| self.lesson_lookup_error(lesson_id))?;
        let mut lesson_progress = progress
            .iter()
            .find(|entry| entry.lesson_id == lesson.id)
            .cloned()
            .unwrap_or_else(|| CourseProgress::not_started(user_id, lesson.id, at));

        if lesson_progress.state != ProgressState::Completed {
            let prerequisites = self.prerequisites_for(lesson.id)?;
            for required_id in prerequisites.required_lesson_ids {
                if !is_completed(progress, required_id) {
                    return Err(LearningContractError::PrerequisiteNotCompleted {
                        lesson_id: lesson.id,
                        prerequisite_id: required_id,
                    });
                }
            }
        }
        lesson_progress.complete(at)?;
        Ok(lesson_progress)
    }

    /// Derives authoritative completion counts from published content and the
    /// supplied PostgreSQL progress rows. A module with no published lessons
    /// is not complete, so an empty course cannot be reported as completed.
    pub fn progress_summary(
        &self,
        user_id: Uuid,
        progress: &[CourseProgress],
    ) -> Result<CourseProgressSummary, LearningContractError> {
        self.validate()?;
        validate_uuid("course progress user id", user_id)?;
        self.validate_progress_entries(progress)?;
        for entry in progress {
            if entry.user_id != user_id {
                return Err(LearningContractError::ProgressUserMismatch);
            }
        }

        let published_modules = self
            .modules
            .iter()
            .filter(|module| module.lifecycle.is_public())
            .collect::<Vec<_>>();
        let total_modules = published_modules.len() as u64;
        let mut completed_modules = 0;
        let mut total_lessons = 0;
        let mut completed_lessons = 0;

        for module in published_modules {
            let lessons = module
                .lessons
                .iter()
                .filter(|lesson| lesson.lifecycle.is_public())
                .collect::<Vec<_>>();
            let module_total = lessons.len() as u64;
            let module_completed = lessons
                .iter()
                .filter(|lesson| is_completed(progress, lesson.id))
                .count() as u64;
            total_lessons += module_total;
            completed_lessons += module_completed;
            if module_total > 0 && module_total == module_completed {
                completed_modules += 1;
            }
        }

        let completed = total_modules > 0 && completed_modules == total_modules;
        Ok(CourseProgressSummary {
            user_id,
            total_modules,
            completed_modules,
            total_lessons,
            completed_lessons,
            completed,
        })
    }

    fn validate_progress_entries(
        &self,
        progress: &[CourseProgress],
    ) -> Result<(), LearningContractError> {
        for (index, entry) in progress.iter().enumerate() {
            entry.validate()?;
            if progress[..index]
                .iter()
                .any(|previous| previous.lesson_id == entry.lesson_id)
            {
                return Err(LearningContractError::DuplicateProgress {
                    lesson_id: entry.lesson_id,
                });
            }
        }
        Ok(())
    }

    fn lesson_lookup_error(&self, lesson_id: Uuid) -> LearningContractError {
        if self
            .modules
            .iter()
            .flat_map(|module| module.lessons.iter())
            .any(|lesson| lesson.id == lesson_id)
        {
            LearningContractError::LessonNotPublished { lesson_id }
        } else {
            LearningContractError::LessonNotFound { lesson_id }
        }
    }
}

/// The derived prerequisite set for one published lesson.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LessonPrerequisites {
    pub lesson_id: Uuid,
    /// IDs are in the same stable course order as the lessons that require
    /// them. The list is empty for the first published lesson.
    pub required_lesson_ids: Vec<Uuid>,
}

/// Result of deterministic next-lesson selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NextLesson<'a> {
    Available(&'a CourseLesson),
    CourseCompleted,
    NoPublishedLessons,
}

impl<'a> NextLesson<'a> {
    #[must_use]
    pub const fn lesson(self) -> Option<&'a CourseLesson> {
        match self {
            Self::Available(lesson) => Some(lesson),
            Self::CourseCompleted | Self::NoPublishedLessons => None,
        }
    }
}

fn is_completed(progress: &[CourseProgress], lesson_id: Uuid) -> bool {
    progress
        .iter()
        .find(|entry| entry.lesson_id == lesson_id)
        .is_some_and(|entry| entry.state == ProgressState::Completed)
}

/// An ordered, publishable course module. Its fields mirror the DB-05
/// `course_modules` row; `lifecycle` is the domain representation of the
/// persistence layer's publication flag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CourseModule {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    pub description: Option<String>,
    pub display_order: i32,
    pub lifecycle: ContentLifecycle,
    pub lessons: Vec<CourseLesson>,
    pub progress: Option<ModuleProgressSummary>,
}

impl CourseModule {
    pub fn sort_deterministically(&mut self) {
        self.lessons
            .sort_by_key(|lesson| (lesson.lesson_order, lesson.id));
    }

    pub fn validate(&self) -> Result<(), LearningContractError> {
        validate_uuid("module id", self.id)?;
        validate_slug("module slug", &self.slug)?;
        validate_required_text("module title", &self.title)?;
        validate_optional_text("module description", self.description.as_deref())?;
        if self.display_order < 0 {
            return Err(LearningContractError::NegativeOrder {
                field: "module display_order",
            });
        }

        validate_unique_and_ordered(
            self.lessons
                .iter()
                .map(|lesson| (lesson.lesson_order, lesson.id, lesson.slug.as_str())),
            "lesson",
        )?;
        for lesson in &self.lessons {
            lesson.validate_for_module(self.id)?;
        }
        if let Some(progress) = &self.progress {
            progress.validate_for_module(self.id)?;
        }
        Ok(())
    }
}

/// A lesson belonging to one module. Content is returned only for a lesson
/// that has passed the publication filter at the repository boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CourseLesson {
    pub id: Uuid,
    pub module_id: Uuid,
    pub slug: String,
    pub title: String,
    pub summary: Option<String>,
    pub content: String,
    pub lesson_order: i32,
    pub estimated_minutes: i32,
    pub lifecycle: ContentLifecycle,
}

impl CourseLesson {
    pub fn validate(&self) -> Result<(), LearningContractError> {
        self.validate_for_module(self.module_id)
    }

    pub fn validate_for_module(
        &self,
        expected_module_id: Uuid,
    ) -> Result<(), LearningContractError> {
        validate_uuid("lesson id", self.id)?;
        validate_uuid("lesson module id", self.module_id)?;
        if self.module_id != expected_module_id {
            return Err(LearningContractError::ParentMismatch {
                child: "lesson",
                parent: "module",
            });
        }
        validate_slug("lesson slug", &self.slug)?;
        validate_required_text("lesson title", &self.title)?;
        validate_optional_text("lesson summary", self.summary.as_deref())?;
        validate_required_text("lesson content", &self.content)?;
        if self.lesson_order < 0 {
            return Err(LearningContractError::NegativeOrder {
                field: "lesson lesson_order",
            });
        }
        if self.estimated_minutes <= 0 {
            return Err(LearningContractError::InvalidDuration);
        }
        Ok(())
    }
}

/// Progress state is monotonic: a completed lesson can never become started
/// or not-started again through the public contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressState {
    NotStarted,
    Started,
    Completed,
}

/// Authoritative progress for one user and one lesson. A missing DB row is
/// represented as `NotStarted` by the adapter; Redis never creates progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CourseProgress {
    pub user_id: Uuid,
    pub lesson_id: Uuid,
    pub state: ProgressState,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub last_accessed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

impl CourseProgress {
    #[must_use]
    pub fn not_started(user_id: Uuid, lesson_id: Uuid, at: DateTime<Utc>) -> Self {
        Self {
            user_id,
            lesson_id,
            state: ProgressState::NotStarted,
            started_at: None,
            completed_at: None,
            last_accessed_at: None,
            updated_at: at,
        }
    }

    /// Applies a lesson access without regressing completion. Calling this on
    /// a completed lesson only updates access metadata.
    pub fn start(&mut self, at: DateTime<Utc>) -> Result<(), LearningContractError> {
        self.validate_ids()?;
        self.started_at.get_or_insert(at);
        self.last_accessed_at = Some(at);
        self.updated_at = at;
        if self.state == ProgressState::NotStarted {
            self.state = ProgressState::Started;
        }
        self.validate()
    }

    /// Marks a lesson complete idempotently. The first completion timestamp is
    /// retained when a client retries the same request.
    pub fn complete(&mut self, at: DateTime<Utc>) -> Result<(), LearningContractError> {
        self.validate_ids()?;
        self.started_at.get_or_insert(at);
        self.completed_at.get_or_insert(at);
        self.last_accessed_at = Some(at);
        self.updated_at = at;
        self.state = ProgressState::Completed;
        self.validate()
    }

    pub fn validate(&self) -> Result<(), LearningContractError> {
        self.validate_ids()?;
        match self.state {
            ProgressState::NotStarted => {
                if self.started_at.is_some()
                    || self.completed_at.is_some()
                    || self.last_accessed_at.is_some()
                {
                    return Err(LearningContractError::ProgressStateMismatch);
                }
            }
            ProgressState::Started => {
                if self.started_at.is_none() || self.completed_at.is_some() {
                    return Err(LearningContractError::ProgressStateMismatch);
                }
            }
            ProgressState::Completed => {
                if self.started_at.is_none() || self.completed_at.is_none() {
                    return Err(LearningContractError::ProgressStateMismatch);
                }
            }
        }

        if let (Some(started_at), Some(completed_at)) = (self.started_at, self.completed_at) {
            validate_timestamp_order("started_at", started_at, "completed_at", completed_at)?;
        }
        if let (Some(started_at), Some(last_accessed_at)) = (self.started_at, self.last_accessed_at)
        {
            validate_timestamp_order(
                "started_at",
                started_at,
                "last_accessed_at",
                last_accessed_at,
            )?;
        }
        if let (Some(completed_at), Some(last_accessed_at)) =
            (self.completed_at, self.last_accessed_at)
        {
            validate_timestamp_order(
                "completed_at",
                completed_at,
                "last_accessed_at",
                last_accessed_at,
            )?;
        }
        if let Some(started_at) = self.started_at {
            validate_timestamp_order("started_at", started_at, "updated_at", self.updated_at)?;
        }
        if let Some(completed_at) = self.completed_at {
            validate_timestamp_order("completed_at", completed_at, "updated_at", self.updated_at)?;
        }
        if let Some(last_accessed_at) = self.last_accessed_at {
            validate_timestamp_order(
                "last_accessed_at",
                last_accessed_at,
                "updated_at",
                self.updated_at,
            )?;
        }
        Ok(())
    }

    fn validate_ids(&self) -> Result<(), LearningContractError> {
        validate_uuid("progress user id", self.user_id)?;
        validate_uuid("progress lesson id", self.lesson_id)
    }
}

/// Completion counts for a single module, derived from published lessons and
/// PostgreSQL progress rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleProgressSummary {
    pub user_id: Uuid,
    pub module_id: Uuid,
    pub total_lessons: u64,
    pub completed_lessons: u64,
    pub completed: bool,
}

impl ModuleProgressSummary {
    pub fn validate_for_module(&self, module_id: Uuid) -> Result<(), LearningContractError> {
        validate_uuid("module progress user id", self.user_id)?;
        validate_uuid("module progress module id", self.module_id)?;
        if self.module_id != module_id {
            return Err(LearningContractError::ParentMismatch {
                child: "module progress",
                parent: "module",
            });
        }
        validate_counts(self.total_lessons, self.completed_lessons, self.completed)
    }
}

/// Completion counts for the published course aggregate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CourseProgressSummary {
    pub user_id: Uuid,
    pub total_modules: u64,
    pub completed_modules: u64,
    pub total_lessons: u64,
    pub completed_lessons: u64,
    pub completed: bool,
}

impl CourseProgressSummary {
    pub fn validate(&self) -> Result<(), LearningContractError> {
        validate_uuid("course progress user id", self.user_id)?;
        if self.completed_modules > self.total_modules {
            return Err(LearningContractError::CompletedExceedsTotal {
                completed: self.completed_modules,
                total: self.total_modules,
            });
        }
        validate_counts(
            self.total_lessons,
            self.completed_lessons,
            self.total_modules > 0 && self.completed_modules == self.total_modules,
        )?;
        if self.completed
            != (self.total_modules > 0 && self.completed_modules == self.total_modules)
        {
            return Err(LearningContractError::SummaryCompletionMismatch);
        }
        Ok(())
    }
}

/// Names used by adapters that prefer explicit contract terminology.
pub type CourseContract = Course;
pub type ModuleContract = CourseModule;
pub type LessonContract = CourseLesson;
pub type ProgressContract = CourseProgress;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum LearningContractError {
    #[error("{field} must be a non-nil UUID")]
    InvalidUuid { field: &'static str },
    #[error("{field} must not be blank")]
    BlankField { field: &'static str },
    #[error("{field} contains control characters")]
    ControlCharacter { field: &'static str },
    #[error("{field} must be a URL-safe slug")]
    InvalidSlug { field: &'static str },
    #[error("content version must be greater than zero")]
    InvalidVersion,
    #[error("{field} must be non-negative")]
    NegativeOrder { field: &'static str },
    #[error("lesson estimated duration must be greater than zero")]
    InvalidDuration,
    #[error("{child} does not belong to the expected {parent}")]
    ParentMismatch {
        child: &'static str,
        parent: &'static str,
    },
    #[error("lesson {lesson_id} was not found in the course")]
    LessonNotFound { lesson_id: Uuid },
    #[error("lesson {lesson_id} is not published")]
    LessonNotPublished { lesson_id: Uuid },
    #[error("{field} entries must have strictly increasing order and unique IDs/slugs")]
    InvalidOrdering { field: &'static str },
    #[error("lesson {lesson_id} requires incomplete lesson {prerequisite_id}")]
    PrerequisiteNotCompleted {
        lesson_id: Uuid,
        prerequisite_id: Uuid,
    },
    #[error("progress contains more than one row for lesson {lesson_id}")]
    DuplicateProgress { lesson_id: Uuid },
    #[error("progress rows belong to different users")]
    ProgressUserMismatch,
    #[error("progress state does not match its timestamps")]
    ProgressStateMismatch,
    #[error("a completed count ({completed}) cannot exceed total ({total})")]
    CompletedExceedsTotal { completed: u64, total: u64 },
    #[error("completion summary flag does not match its counts")]
    SummaryCompletionMismatch,
    #[error("{earlier} must not be after {later}")]
    InvalidTimestampOrder {
        earlier: &'static str,
        later: &'static str,
    },
    #[error("invalid content lifecycle transition from {from:?} to {to:?}")]
    InvalidLifecycleTransition {
        from: ContentLifecycle,
        to: ContentLifecycle,
    },
}

fn validate_uuid(field: &'static str, value: Uuid) -> Result<(), LearningContractError> {
    if value.is_nil() {
        Err(LearningContractError::InvalidUuid { field })
    } else {
        Ok(())
    }
}

fn validate_required_text(field: &'static str, value: &str) -> Result<(), LearningContractError> {
    if value.trim().is_empty() {
        return Err(LearningContractError::BlankField { field });
    }
    if value.chars().any(char::is_control) {
        return Err(LearningContractError::ControlCharacter { field });
    }
    Ok(())
}

fn validate_optional_text(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), LearningContractError> {
    if let Some(value) = value {
        validate_required_text(field, value)?;
    }
    Ok(())
}

fn validate_slug(field: &'static str, value: &str) -> Result<(), LearningContractError> {
    validate_required_text(field, value)?;
    if !value.chars().all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || matches!(character, '-')
    }) || value.starts_with('-')
        || value.ends_with('-')
    {
        return Err(LearningContractError::InvalidSlug { field });
    }
    Ok(())
}

fn validate_unique_and_ordered<'a, I>(
    entries: I,
    field: &'static str,
) -> Result<(), LearningContractError>
where
    I: IntoIterator<Item = (i32, Uuid, &'a str)>,
{
    let mut previous_order = None;
    let mut ids = Vec::new();
    let mut slugs = Vec::new();
    for (order, id, slug) in entries {
        if previous_order.is_some_and(|previous| order <= previous)
            || ids.contains(&id)
            || slugs.contains(&slug)
        {
            return Err(LearningContractError::InvalidOrdering { field });
        }
        previous_order = Some(order);
        ids.push(id);
        slugs.push(slug);
    }
    Ok(())
}

fn validate_counts(
    total: u64,
    completed: u64,
    complete: bool,
) -> Result<(), LearningContractError> {
    if completed > total {
        return Err(LearningContractError::CompletedExceedsTotal { completed, total });
    }
    if complete != (total > 0 && completed == total) {
        return Err(LearningContractError::SummaryCompletionMismatch);
    }
    Ok(())
}

fn validate_timestamp_order(
    earlier_name: &'static str,
    earlier: DateTime<Utc>,
    later_name: &'static str,
    later: DateTime<Utc>,
) -> Result<(), LearningContractError> {
    if earlier > later {
        Err(LearningContractError::InvalidTimestampOrder {
            earlier: earlier_name,
            later: later_name,
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn lesson(module_id: Uuid, order: i32, value: u128) -> CourseLesson {
        CourseLesson {
            id: id(value),
            module_id,
            slug: format!("lesson-{order}"),
            title: format!("Lesson {order}"),
            summary: None,
            content: "Safe lesson content".to_owned(),
            lesson_order: order,
            estimated_minutes: 10,
            lifecycle: ContentLifecycle::Published,
        }
    }

    fn module(order: i32, value: u128) -> CourseModule {
        let module_id = id(value);
        CourseModule {
            id: module_id,
            slug: format!("module-{order}"),
            title: format!("Module {order}"),
            description: None,
            display_order: order,
            lifecycle: ContentLifecycle::Published,
            lessons: vec![lesson(module_id, 1, value + 100)],
            progress: None,
        }
    }

    #[test]
    fn course_order_is_stable_after_sorting() {
        let mut course = Course {
            id: id(1),
            slug: BEGINNER_COURSE_SLUG.to_owned(),
            title: "Beginner Trading".to_owned(),
            description: None,
            version: 1,
            lifecycle: ContentLifecycle::Published,
            modules: vec![module(2, 3), module(1, 2)],
            progress: None,
        };

        course.sort_deterministically();
        assert_eq!(
            course
                .modules
                .iter()
                .map(|module| module.display_order)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert!(course.validate().is_ok());
    }

    #[test]
    fn lesson_order_is_stable_after_sorting() {
        let module_id = id(2);
        let mut course_module = module(1, module_id.as_u128());
        course_module.lessons = vec![lesson(module_id, 2, 202), lesson(module_id, 1, 201)];
        let mut course = Course {
            id: id(1),
            slug: BEGINNER_COURSE_SLUG.to_owned(),
            title: "Beginner Trading".to_owned(),
            description: None,
            version: 1,
            lifecycle: ContentLifecycle::Published,
            modules: vec![course_module],
            progress: None,
        };

        course.sort_deterministically();
        assert_eq!(
            course.modules[0]
                .lessons
                .iter()
                .map(|lesson| lesson.lesson_order)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert!(course.validate().is_ok());
    }

    #[test]
    fn duplicate_order_is_rejected_even_if_ids_provide_a_tiebreaker() {
        let mut course = Course {
            id: id(1),
            slug: BEGINNER_COURSE_SLUG.to_owned(),
            title: "Beginner Trading".to_owned(),
            description: None,
            version: 1,
            lifecycle: ContentLifecycle::Published,
            modules: vec![module(1, 2), module(1, 3)],
            progress: None,
        };
        course.sort_deterministically();
        assert_eq!(
            course.validate(),
            Err(LearningContractError::InvalidOrdering { field: "module" })
        );
    }

    #[test]
    fn prerequisites_and_next_lesson_follow_the_flattened_order() {
        let course = Course {
            id: id(1),
            slug: BEGINNER_COURSE_SLUG.to_owned(),
            title: "Beginner Trading".to_owned(),
            description: None,
            version: 1,
            lifecycle: ContentLifecycle::Published,
            modules: vec![module(1, 2), module(2, 3)],
            progress: None,
        };
        let lessons = course.ordered_published_lessons();
        let first_id = lessons[0].id;
        let second_id = lessons[1].id;

        let prerequisites = course.prerequisites_for(second_id).unwrap();
        assert_eq!(prerequisites.lesson_id, second_id);
        assert_eq!(prerequisites.required_lesson_ids, vec![first_id]);
        assert!(course.is_lesson_unlocked(first_id, &[]).unwrap());
        assert!(!course.is_lesson_unlocked(second_id, &[]).unwrap());
        assert_eq!(
            course.next_lesson(&[]).unwrap().lesson().unwrap().id,
            first_id
        );
    }

    #[test]
    fn completion_requires_prerequisites_and_is_authoritative() {
        let mut first_module = module(1, 2);
        let first_module_id = first_module.id;
        first_module.lessons.push(lesson(first_module_id, 2, 202));
        let course = Course {
            id: id(1),
            slug: BEGINNER_COURSE_SLUG.to_owned(),
            title: "Beginner Trading".to_owned(),
            description: None,
            version: 1,
            lifecycle: ContentLifecycle::Published,
            modules: vec![first_module, module(2, 3)],
            progress: None,
        };
        let lessons = course.ordered_published_lessons();
        let first_id = lessons[0].id;
        let second_id = lessons[1].id;
        let third_id = lessons[2].id;
        let user_id = id(50);
        let at = Utc::now();

        assert_eq!(
            course.complete_lesson(user_id, second_id, &[], at),
            Err(LearningContractError::PrerequisiteNotCompleted {
                lesson_id: second_id,
                prerequisite_id: first_id,
            })
        );

        let mut progress = Vec::new();
        progress.push(course.complete_lesson(user_id, first_id, &[], at).unwrap());
        assert_eq!(
            course.next_lesson(&progress).unwrap().lesson().unwrap().id,
            second_id
        );
        progress.push(
            course
                .complete_lesson(user_id, second_id, &progress, at + Duration::minutes(1))
                .unwrap(),
        );
        progress.push(
            course
                .complete_lesson(user_id, third_id, &progress, at + Duration::minutes(2))
                .unwrap(),
        );

        assert_eq!(
            course.next_lesson(&progress).unwrap(),
            NextLesson::CourseCompleted
        );
        let summary = course.progress_summary(user_id, &progress).unwrap();
        assert_eq!(summary.total_modules, 2);
        assert_eq!(summary.completed_modules, 2);
        assert_eq!(summary.total_lessons, 3);
        assert_eq!(summary.completed_lessons, 3);
        assert!(summary.completed);

        let repeated = course
            .complete_lesson(user_id, first_id, &progress, at + Duration::minutes(3))
            .unwrap();
        assert_eq!(repeated.state, ProgressState::Completed);
        assert_eq!(repeated.completed_at, Some(at));
    }

    #[test]
    fn next_lesson_does_not_claim_an_empty_public_course_is_complete() {
        let course = Course {
            id: id(1),
            slug: BEGINNER_COURSE_SLUG.to_owned(),
            title: "Beginner Trading".to_owned(),
            description: None,
            version: 1,
            lifecycle: ContentLifecycle::Published,
            modules: vec![CourseModule {
                lessons: Vec::new(),
                ..module(1, 2)
            }],
            progress: None,
        };

        assert_eq!(
            course.next_lesson(&[]).unwrap(),
            NextLesson::NoPublishedLessons
        );
        assert!(!course.progress_summary(id(50), &[]).unwrap().completed);
    }

    #[test]
    fn progress_is_resumable_and_completion_is_idempotent() {
        let started_at = Utc::now();
        let completed_at = started_at + Duration::minutes(5);
        let mut progress = CourseProgress::not_started(id(10), id(20), started_at);

        progress.start(started_at).unwrap();
        assert_eq!(progress.state, ProgressState::Started);
        progress.complete(completed_at).unwrap();
        progress
            .complete(completed_at + Duration::minutes(5))
            .unwrap();

        assert_eq!(progress.state, ProgressState::Completed);
        assert_eq!(progress.completed_at, Some(completed_at));
        assert_eq!(progress.started_at, Some(started_at));
        assert_eq!(
            progress.last_accessed_at,
            Some(completed_at + Duration::minutes(5))
        );
    }

    #[test]
    fn retried_completion_preserves_the_persisted_timestamp() {
        let course = Course {
            id: id(1),
            slug: BEGINNER_COURSE_SLUG.to_owned(),
            title: "Beginner Trading".to_owned(),
            description: None,
            version: 1,
            lifecycle: ContentLifecycle::Published,
            modules: vec![module(1, 2)],
            progress: None,
        };
        let user_id = id(50);
        let lesson_id = course.ordered_published_lessons()[0].id;
        let first_at = Utc::now();
        let second_at = first_at + Duration::minutes(1);

        let first = course
            .complete_lesson(user_id, lesson_id, &[], first_at)
            .unwrap();
        let replay = course
            .complete_lesson(user_id, lesson_id, std::slice::from_ref(&first), second_at)
            .unwrap();

        assert_eq!(first.state, ProgressState::Completed);
        assert_eq!(replay.state, ProgressState::Completed);
        assert_eq!(first.completed_at, Some(first_at));
        assert_eq!(replay.completed_at, Some(first_at));
        assert!(replay.last_accessed_at > first.last_accessed_at);
    }

    #[test]
    fn progress_cannot_regress_or_have_inconsistent_timestamps() {
        let at = Utc::now();
        let mut progress = CourseProgress::not_started(id(10), id(20), at);
        progress.complete(at).unwrap();
        assert_eq!(progress.state, ProgressState::Completed);

        progress.state = ProgressState::Started;
        assert_eq!(
            progress.validate(),
            Err(LearningContractError::ProgressStateMismatch)
        );
    }

    #[test]
    fn lifecycle_allows_publish_then_retire_but_not_revival() {
        assert!(ContentLifecycle::Draft.can_transition_to(ContentLifecycle::Published));
        assert!(ContentLifecycle::Published.can_transition_to(ContentLifecycle::Retired));
        assert!(!ContentLifecycle::Retired.can_transition_to(ContentLifecycle::Published));
    }

    #[test]
    fn beginner_course_identity_is_stable_and_rejects_other_uuids() {
        assert_eq!(
            BEGINNER_COURSE_ID,
            Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
        );
        assert!(is_beginner_course_id(BEGINNER_COURSE_ID));
        assert!(!is_beginner_course_id(Uuid::from_u128(2)));
    }

    #[test]
    fn invalid_content_and_parent_data_is_rejected() {
        let module_id = id(2);
        let invalid = CourseLesson {
            id: id(3),
            module_id: id(4),
            slug: "lesson".to_owned(),
            title: "Lesson".to_owned(),
            summary: None,
            content: "Content".to_owned(),
            lesson_order: 0,
            estimated_minutes: 10,
            lifecycle: ContentLifecycle::Published,
        };
        assert_eq!(
            invalid.validate_for_module(module_id),
            Err(LearningContractError::ParentMismatch {
                child: "lesson",
                parent: "module"
            })
        );
    }
}
