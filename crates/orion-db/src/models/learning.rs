use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// An ordered, publishable section of the beginner trading course.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct CourseModule {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    pub description: Option<String>,
    pub display_order: i32,
    pub is_published: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A lesson belonging to one course module.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct CourseLesson {
    pub id: Uuid,
    pub module_id: Uuid,
    pub slug: String,
    pub title: String,
    pub summary: Option<String>,
    pub content: String,
    pub lesson_order: i32,
    pub estimated_minutes: i32,
    pub is_published: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A user's completion and recent-access state for one lesson.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct CourseProgress {
    pub user_id: Uuid,
    pub lesson_id: Uuid,
    pub completed: bool,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub last_accessed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

/// Completion derived from a user's progress across one published module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRow)]
pub struct ModuleCompletion {
    pub user_id: Uuid,
    pub module_id: Uuid,
    pub total_lessons: i64,
    pub completed_lessons: i64,
    pub completed: bool,
}

/// Completion derived from a user's progress across the published course.
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRow)]
pub struct CourseCompletion {
    pub user_id: Uuid,
    pub total_modules: i64,
    pub completed_modules: i64,
    pub total_lessons: i64,
    pub completed_lessons: i64,
    pub completed: bool,
}
