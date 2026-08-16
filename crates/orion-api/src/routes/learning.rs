//! HTTP adapters for the published beginner learning course.
//!
//! PostgreSQL is authoritative. These handlers use the completed learning
//! repository; Redis is used only as a disposable cache for published course
//! content and never for progress state.
//!
//! TODO(Div): mount [`router`] under `/api/v1/learning` and register the
//! operation metadata in the shared API registry.
//!
//! TODO(DB-01/Product): the DB-05 baseline has no `courses` table. The route
//! currently accepts only the reserved `BEGINNER_COURSE_ID`; replace that
//! provisional constant with the approved course-table identity when it lands.

use std::future::Future;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Router,
};
use chrono::{DateTime, Utc};
use orion_common::ErrorCode;
use orion_db::{
    error::DatabaseError,
    models::{
        CourseCompletion as DbCourseCompletion, CourseLesson as DbCourseLesson,
        CourseModule as DbCourseModule, CourseProgress as DbCourseProgress,
    },
    repositories::LearningRepository,
};
use orion_domain::learning::{
    is_beginner_course_id, ContentLifecycle, Course as DomainCourse, CourseLesson as DomainLesson,
    CourseModule as DomainModule, CourseProgress as DomainProgress, CourseProgressSummary,
    LearningContractError, ProgressState, BEGINNER_COURSE_ID, BEGINNER_COURSE_SLUG,
};
use orion_redis::cache::learning as learning_cache;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{request_id, routes::auth::AuthenticatedUser, state::AppState, ApiProblem};

const COURSE_TITLE: &str = "Beginner Trading";
const COURSE_DESCRIPTION: &str =
    "A deterministic beginner course for learning trading fundamentals.";

/// Isolated learning routes. Application mounting remains Div-owned.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/courses/{course_id}", get(get_course))
        .route("/modules/{module_id}", get(get_module))
        .route("/lessons/{lesson_id}", get(get_lesson))
        .route("/progress", get(get_progress))
        .route("/lessons/{lesson_id}/completion", post(complete_lesson))
}

/// Public course projection. Only published modules and lessons returned by
/// the DB queries are included.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CourseResponse {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub version: u32,
    pub modules: Vec<ModuleResponse>,
}

/// Public module projection with deterministic lesson ordering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleResponse {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    pub description: Option<String>,
    pub display_order: i32,
    pub lessons: Vec<LessonResponse>,
}

/// Public lesson projection. Internal DB timestamps and progress bookkeeping
/// are intentionally excluded from this content response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LessonResponse {
    pub id: Uuid,
    pub module_id: Uuid,
    pub slug: String,
    pub title: String,
    pub summary: Option<String>,
    pub content: String,
    pub lesson_order: i32,
    pub estimated_minutes: i32,
}

/// Authenticated progress projection. It contains only the caller's own
/// progress and never exposes another user's identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressResponse {
    pub items: Vec<ProgressItemResponse>,
    pub summary: CourseProgressSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressItemResponse {
    pub lesson_id: Uuid,
    pub state: ProgressState,
    pub completed: bool,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub last_accessed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LessonCompletionResponse {
    pub progress: ProgressItemResponse,
}

async fn get_course(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(course_id): Path<String>,
) -> Result<impl axum::response::IntoResponse, ApiProblem> {
    let request_id = request_id(&headers);
    let course_id = parse_uuid(&course_id, "course id", request_id)?;

    // Redis is disposable: a miss, stale entry, malformed payload, or outage
    // falls through to the authoritative DB-05 repository.
    let repository = LearningRepository::new(state.db);
    let cache_result = learning_cache::get_course(&state.redis, course_id)
        .await
        .map(|entry| entry.map(|entry| entry.course))
        .map_err(|_| ());
    let (course, cache_hit) = load_after_cache_read(cache_result, || async {
        load_domain_course(&repository, course_id, request_id).await
    })
    .await
        .map_err(|error| learning_route_problem(error, request_id))?;

    if !cache_hit {
        // The cache module revalidates publication and rejects progress-bearing
        // aggregates, so this refill can contain only mostly-static content.
        if learning_cache::set_course(&state.redis, &course)
            .await
            .is_err()
        {
            tracing::debug!("learning course cache refill skipped");
        }
    }

    Ok(crate::success(&headers, CourseResponse::from(course)))
}

async fn get_module(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(module_id): Path<String>,
) -> Result<impl axum::response::IntoResponse, ApiProblem> {
    let request_id = request_id(&headers);
    let module_id = parse_uuid(&module_id, "module id", request_id)?;
    let repository = LearningRepository::new(state.db);
    let module = repository
        .module_by_id(module_id)
        .await
        .map_err(|error| database_problem(error, request_id))?
        .ok_or_else(|| not_found(request_id, "module was not found"))?;
    let module = load_domain_module(&repository, module, request_id)
        .await
        .map_err(|error| learning_route_problem(error, request_id))?;

    Ok(crate::success(&headers, ModuleResponse::from(module)))
}

async fn get_lesson(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(lesson_id): Path<String>,
) -> Result<impl axum::response::IntoResponse, ApiProblem> {
    let request_id = request_id(&headers);
    let lesson_id = parse_uuid(&lesson_id, "lesson id", request_id)?;
    let repository = LearningRepository::new(state.db);
    let lesson = repository
        .lesson_by_id(lesson_id)
        .await
        .map_err(|error| database_problem(error, request_id))?
        .ok_or_else(|| not_found(request_id, "lesson was not found"))?;
    let lesson = domain_lesson(lesson).map_err(|error| contract_problem(error, request_id))?;

    Ok(crate::success(&headers, LessonResponse::from(lesson)))
}

async fn load_after_cache_read<T, F, Fut, E>(
    cache_result: Result<Option<T>, ()>,
    loader: F,
) -> Result<(T, bool), E>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    if let Ok(Some(value)) = cache_result {
        return Ok((value, true));
    }
    Ok((loader().await?, false))
}

async fn get_progress(
    State(state): State<AppState>,
    headers: HeaderMap,
    user: AuthenticatedUser,
) -> Result<impl axum::response::IntoResponse, ApiProblem> {
    let request_id = request_id(&headers);
    let user_id = user.user.id;
    let repository = LearningRepository::new(state.db);
    let rows = repository
        .progress_by_user_id(user_id)
        .await
        .map_err(|error| database_problem(error, request_id))?;
    let items = rows
        .into_iter()
        .map(|row| {
            domain_progress(row)
                .map(progress_item)
                .map_err(|error| contract_problem(error, request_id))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let summary = repository
        .course_completion(user_id)
        .await
        .map_err(|error| database_problem(error, request_id))
        .and_then(|summary| progress_summary(summary, request_id))?;

    Ok(crate::success(
        &headers,
        ProgressResponse { items, summary },
    ))
}

async fn complete_lesson(
    State(state): State<AppState>,
    headers: HeaderMap,
    user: AuthenticatedUser,
    Path(lesson_id): Path<String>,
) -> Result<impl axum::response::IntoResponse, ApiProblem> {
    let request_id = request_id(&headers);
    let lesson_id = parse_uuid(&lesson_id, "lesson id", request_id)?;
    let user_id = user.user.id;
    let repository = LearningRepository::new(state.db);

    repository
        .lesson_by_id(lesson_id)
        .await
        .map_err(|error| database_problem(error, request_id))?
        .ok_or_else(|| not_found(request_id, "lesson was not found"))?;

    let course = load_domain_course(&repository, BEGINNER_COURSE_ID, request_id)
        .await
        .map_err(|error| learning_route_problem(error, request_id))?;
    let existing = repository
        .progress_by_user_id(user_id)
        .await
        .map_err(|error| database_problem(error, request_id))?
        .into_iter()
        .map(domain_progress)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| contract_problem(error, request_id))?;

    course
        .complete_lesson(user_id, lesson_id, &existing, Utc::now())
        .map_err(|error| progress_problem(error, request_id))?;

    // PostgreSQL is the authoritative write. The completed DB-05 repository
    // owns this idempotent (user_id, lesson_id) upsert; keep SQL out of the
    // route so replaying completion cannot create a second progress row.
    let persisted = repository
        .complete_lesson(user_id, lesson_id)
        .await
        .map_err(|error| database_problem(error, request_id))?;
    let progress = domain_progress(persisted)
        .map(progress_item)
        .map_err(|error| contract_problem(error, request_id))?;

    Ok(crate::success(
        &headers,
        LessonCompletionResponse { progress },
    ))
}

async fn load_domain_course(
    repository: &LearningRepository,
    course_id: Uuid,
    request_id: orion_common::RequestId,
) -> Result<DomainCourse, LearningRouteError> {
    if !is_beginner_course_id(course_id) {
        return Err(LearningRouteError::Api(not_found(
            request_id,
            "course was not found",
        )));
    }
    let modules = repository
        .modules()
        .await
        .map_err(LearningRouteError::Database)?;
    if modules.is_empty() {
        return Err(LearningRouteError::Api(not_found(
            request_id,
            "course was not found",
        )));
    }

    let mut domain_modules = Vec::with_capacity(modules.len());
    for module in modules {
        domain_modules.push(load_domain_module(repository, module, request_id).await?);
    }

    let mut course = DomainCourse {
        id: course_id,
        slug: BEGINNER_COURSE_SLUG.to_owned(),
        title: COURSE_TITLE.to_owned(),
        description: Some(COURSE_DESCRIPTION.to_owned()),
        version: 1,
        lifecycle: ContentLifecycle::Published,
        modules: domain_modules,
        progress: None,
    };
    course.sort_deterministically();
    course.validate().map_err(LearningRouteError::Contract)?;
    Ok(course)
}

async fn load_domain_module(
    repository: &LearningRepository,
    module: DbCourseModule,
    request_id: orion_common::RequestId,
) -> Result<DomainModule, LearningRouteError> {
    let lessons = repository
        .lessons_by_module_id(module.id)
        .await
        .map_err(LearningRouteError::Database)?;
    let lessons = lessons
        .into_iter()
        .map(domain_lesson)
        .collect::<Result<Vec<_>, _>>()
        .map_err(LearningRouteError::Contract)?;
    let domain_module = DomainModule {
        id: module.id,
        slug: module.slug,
        title: module.title,
        description: module.description,
        display_order: module.display_order,
        lifecycle: lifecycle(module.is_published),
        lessons,
        progress: None,
    };
    domain_module
        .validate()
        .map_err(LearningRouteError::Contract)?;
    if !domain_module.lifecycle.is_public() {
        return Err(LearningRouteError::Api(not_found(
            request_id,
            "module was not found",
        )));
    }
    Ok(domain_module)
}

fn domain_lesson(lesson: DbCourseLesson) -> Result<DomainLesson, LearningContractError> {
    let domain_lesson = DomainLesson {
        id: lesson.id,
        module_id: lesson.module_id,
        slug: lesson.slug,
        title: lesson.title,
        summary: lesson.summary,
        content: lesson.content,
        lesson_order: lesson.lesson_order,
        estimated_minutes: lesson.estimated_minutes,
        lifecycle: lifecycle(lesson.is_published),
    };
    domain_lesson.validate()?;
    Ok(domain_lesson)
}

fn lifecycle(is_published: bool) -> ContentLifecycle {
    if is_published {
        ContentLifecycle::Published
    } else {
        ContentLifecycle::Draft
    }
}

fn domain_progress(row: DbCourseProgress) -> Result<DomainProgress, LearningContractError> {
    let state = if row.completed {
        ProgressState::Completed
    } else if row.started_at.is_some() || row.last_accessed_at.is_some() {
        ProgressState::Started
    } else {
        ProgressState::NotStarted
    };
    let progress = DomainProgress {
        user_id: row.user_id,
        lesson_id: row.lesson_id,
        state,
        started_at: row.started_at,
        completed_at: row.completed_at,
        last_accessed_at: row.last_accessed_at,
        updated_at: row.updated_at,
    };
    progress.validate()?;
    Ok(progress)
}

fn progress_item(progress: DomainProgress) -> ProgressItemResponse {
    ProgressItemResponse {
        lesson_id: progress.lesson_id,
        completed: progress.state == ProgressState::Completed,
        state: progress.state,
        started_at: progress.started_at,
        completed_at: progress.completed_at,
        last_accessed_at: progress.last_accessed_at,
        updated_at: progress.updated_at,
    }
}

fn progress_summary(
    row: DbCourseCompletion,
    request_id: orion_common::RequestId,
) -> Result<CourseProgressSummary, ApiProblem> {
    let summary = CourseProgressSummary {
        user_id: row.user_id,
        total_modules: non_negative_count(row.total_modules, request_id)?,
        completed_modules: non_negative_count(row.completed_modules, request_id)?,
        total_lessons: non_negative_count(row.total_lessons, request_id)?,
        completed_lessons: non_negative_count(row.completed_lessons, request_id)?,
        completed: row.completed,
    };
    summary
        .validate()
        .map_err(|error| contract_problem(error, request_id))?;
    Ok(summary)
}

fn non_negative_count(value: i64, request_id: orion_common::RequestId) -> Result<u64, ApiProblem> {
    u64::try_from(value).map_err(|_| {
        ApiProblem::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::Internal,
            "learning progress data is invalid",
        )
        .with_request_id(request_id)
    })
}

fn parse_uuid(
    value: &str,
    field: &'static str,
    request_id: orion_common::RequestId,
) -> Result<Uuid, ApiProblem> {
    Uuid::parse_str(value.trim()).map_err(|_| {
        validation(
            request_id,
            match field {
                "course id" => "course id must be a valid UUID",
                "module id" => "module id must be a valid UUID",
                _ => "lesson id must be a valid UUID",
            },
        )
    })
}

fn database_problem(error: sqlx::Error, request_id: orion_common::RequestId) -> ApiProblem {
    ApiProblem::from(DatabaseError::from_sqlx(error)).with_request_id(request_id)
}

fn contract_problem(
    _error: LearningContractError,
    request_id: orion_common::RequestId,
) -> ApiProblem {
    ApiProblem::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        ErrorCode::Internal,
        "learning content contract is invalid",
    )
    .with_request_id(request_id)
}

fn progress_problem(
    error: LearningContractError,
    request_id: orion_common::RequestId,
) -> ApiProblem {
    match error {
        LearningContractError::PrerequisiteNotCompleted { .. } => ApiProblem::new(
            StatusCode::CONFLICT,
            ErrorCode::Conflict,
            "lesson prerequisites are incomplete",
        )
        .with_request_id(request_id),
        LearningContractError::LessonNotFound { .. }
        | LearningContractError::LessonNotPublished { .. } => {
            not_found(request_id, "lesson was not found")
        }
        other => contract_problem(other, request_id),
    }
}

fn validation(request_id: orion_common::RequestId, message: &'static str) -> ApiProblem {
    ApiProblem::new(
        StatusCode::BAD_REQUEST,
        ErrorCode::ValidationFailed,
        message,
    )
    .with_request_id(request_id)
}

fn not_found(request_id: orion_common::RequestId, message: &'static str) -> ApiProblem {
    ApiProblem::new(StatusCode::NOT_FOUND, ErrorCode::NotFound, message).with_request_id(request_id)
}

#[derive(Debug)]
enum LearningRouteError {
    Api(ApiProblem),
    Database(sqlx::Error),
    Contract(LearningContractError),
}

fn learning_route_problem(
    error: LearningRouteError,
    request_id: orion_common::RequestId,
) -> ApiProblem {
    match error {
        LearningRouteError::Api(problem) => problem,
        LearningRouteError::Database(error) => database_problem(error, request_id),
        LearningRouteError::Contract(error) => contract_problem(error, request_id),
    }
}

impl From<LearningContractError> for LearningRouteError {
    fn from(error: LearningContractError) -> Self {
        Self::Contract(error)
    }
}

impl From<sqlx::Error> for LearningRouteError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

impl From<DomainLesson> for LessonResponse {
    fn from(lesson: DomainLesson) -> Self {
        Self {
            id: lesson.id,
            module_id: lesson.module_id,
            slug: lesson.slug,
            title: lesson.title,
            summary: lesson.summary,
            content: lesson.content,
            lesson_order: lesson.lesson_order,
            estimated_minutes: lesson.estimated_minutes,
        }
    }
}

impl From<DomainModule> for ModuleResponse {
    fn from(module: DomainModule) -> Self {
        Self {
            id: module.id,
            slug: module.slug,
            title: module.title,
            description: module.description,
            display_order: module.display_order,
            lessons: module
                .lessons
                .into_iter()
                .map(LessonResponse::from)
                .collect(),
        }
    }
}

impl From<DomainCourse> for CourseResponse {
    fn from(course: DomainCourse) -> Self {
        Self {
            id: course.id,
            slug: course.slug,
            title: course.title,
            description: course.description.unwrap_or_default(),
            version: course.version,
            modules: course
                .modules
                .into_iter()
                .map(ModuleResponse::from)
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        domain_progress, load_after_cache_read, parse_uuid, progress_item, progress_problem,
        CourseResponse, LessonResponse, ModuleResponse,
    };
    use axum::http::StatusCode;
    use chrono::{Duration, Utc};
    use orion_common::RequestId;
    use orion_db::models::CourseProgress as DbCourseProgress;
    use orion_domain::learning::LearningContractError;
    use uuid::Uuid;

    #[test]
    fn path_ids_are_strictly_parsed() {
        let request_id = RequestId::from_uuid(Uuid::from_u128(1));
        assert_eq!(
            parse_uuid(
                " 00000000-0000-0000-0000-000000000001 ",
                "lesson id",
                request_id
            )
            .unwrap(),
            Uuid::from_u128(1)
        );
        assert!(parse_uuid("not-a-uuid", "lesson id", request_id).is_err());
    }

    #[test]
    fn public_projections_exclude_persistence_timestamps_and_progress() {
        let lesson = LessonResponse {
            id: Uuid::from_u128(1),
            module_id: Uuid::from_u128(2),
            slug: "lesson".to_owned(),
            title: "Lesson".to_owned(),
            summary: Some("Summary".to_owned()),
            content: "Content".to_owned(),
            lesson_order: 1,
            estimated_minutes: 10,
        };
        let module = ModuleResponse {
            id: Uuid::from_u128(2),
            slug: "module".to_owned(),
            title: "Module".to_owned(),
            description: None,
            display_order: 1,
            lessons: vec![lesson],
        };
        let response = CourseResponse {
            id: Uuid::from_u128(3),
            slug: "beginner-trading".to_owned(),
            title: "Beginner Trading".to_owned(),
            description: "Description".to_owned(),
            version: 1,
            modules: vec![module],
        };
        let value = serde_json::to_value(response).unwrap();
        assert!(value["modules"][0]["lessons"][0]
            .get("updated_at")
            .is_none());
        assert!(value["modules"][0]["lessons"][0].get("completed").is_none());
    }

    #[test]
    fn database_progress_maps_to_a_private_authenticated_projection() {
        let completed_at = Utc::now();
        let row = DbCourseProgress {
            user_id: Uuid::from_u128(10),
            lesson_id: Uuid::from_u128(20),
            completed: true,
            started_at: Some(completed_at - Duration::minutes(5)),
            completed_at: Some(completed_at),
            last_accessed_at: Some(completed_at),
            updated_at: completed_at,
        };

        let item = progress_item(domain_progress(row).unwrap());
        assert!(item.completed);
        assert_eq!(item.lesson_id, Uuid::from_u128(20));
        let value = serde_json::to_value(item).unwrap();
        assert!(value.get("user_id").is_none());
    }

    #[test]
    fn incomplete_prerequisites_are_a_conflict_not_an_internal_error() {
        let request_id = RequestId::from_uuid(Uuid::from_u128(1));
        let problem = progress_problem(
            LearningContractError::PrerequisiteNotCompleted {
                lesson_id: Uuid::from_u128(2),
                prerequisite_id: Uuid::from_u128(3),
            },
            request_id,
        );
        assert_eq!(problem.status, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn redis_outage_falls_back_to_the_authoritative_loader() {
        let loaded = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let loaded_by_db = std::sync::Arc::clone(&loaded);
        let result: Result<(u32, bool), &str> =
            load_after_cache_read(Err(()), || async move {
                loaded_by_db.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(42)
            })
            .await;

        assert_eq!(result, Ok((42, false)));
        assert!(loaded.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn fresh_cache_hit_does_not_call_the_database_loader() {
        let result: Result<(u32, bool), &str> =
            load_after_cache_read(Ok(Some(7)), || async { Err("loader must not run") }).await;

        assert_eq!(result, Ok((7, true)));
    }
}
