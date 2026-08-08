use sqlx::{PgPool, Result};
use uuid::Uuid;

use crate::{
    models::{CourseCompletion, CourseLesson, CourseModule, CourseProgress, ModuleCompletion},
    queries::learning,
};

/// Read/write access to the beginner trading course and a user's progress.
#[derive(Debug, Clone)]
pub struct LearningRepository {
    pool: PgPool,
}

impl LearningRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn modules(&self) -> Result<Vec<CourseModule>> {
        learning::modules(&self.pool).await
    }

    pub async fn module_by_id(&self, module_id: Uuid) -> Result<Option<CourseModule>> {
        learning::module_by_id(&self.pool, module_id).await
    }

    pub async fn module_by_slug(&self, slug: &str) -> Result<Option<CourseModule>> {
        learning::module_by_slug(&self.pool, slug).await
    }

    pub async fn lessons_by_module_id(&self, module_id: Uuid) -> Result<Vec<CourseLesson>> {
        learning::lessons_by_module_id(&self.pool, module_id).await
    }

    pub async fn lesson_by_id(&self, lesson_id: Uuid) -> Result<Option<CourseLesson>> {
        learning::lesson_by_id(&self.pool, lesson_id).await
    }

    pub async fn lesson_by_module_and_slug(
        &self,
        module_id: Uuid,
        slug: &str,
    ) -> Result<Option<CourseLesson>> {
        learning::lesson_by_module_and_slug(&self.pool, module_id, slug).await
    }

    pub async fn progress_by_user_id(&self, user_id: Uuid) -> Result<Vec<CourseProgress>> {
        learning::progress_by_user_id(&self.pool, user_id).await
    }

    pub async fn progress_by_user_and_lesson(
        &self,
        user_id: Uuid,
        lesson_id: Uuid,
    ) -> Result<Option<CourseProgress>> {
        learning::progress_by_user_and_lesson(&self.pool, user_id, lesson_id).await
    }

    pub async fn module_completion(
        &self,
        user_id: Uuid,
        module_id: Uuid,
    ) -> Result<Option<ModuleCompletion>> {
        learning::module_completion(&self.pool, user_id, module_id).await
    }

    pub async fn course_completion(&self, user_id: Uuid) -> Result<CourseCompletion> {
        learning::course_completion(&self.pool, user_id).await
    }

    pub async fn upsert_progress(
        &self,
        user_id: Uuid,
        lesson_id: Uuid,
        completed: bool,
    ) -> Result<CourseProgress> {
        learning::upsert_progress(&self.pool, user_id, lesson_id, completed).await
    }

    pub async fn mark_lesson_complete(
        &self,
        user_id: Uuid,
        lesson_id: Uuid,
    ) -> Result<CourseProgress> {
        learning::mark_lesson_complete(&self.pool, user_id, lesson_id).await
    }

    pub async fn complete_lesson(&self, user_id: Uuid, lesson_id: Uuid) -> Result<CourseProgress> {
        learning::complete_lesson(&self.pool, user_id, lesson_id).await
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}
