//! Public profile read route.
//!
//! PostgreSQL assembles the authoritative profile, rating, rank, performance,
//! and published-research projections. Redis only accelerates a complete,
//! versioned public DTO and is always treated as a disposable cache.

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    routing::get,
    Router,
};
use orion_common::{ErrorCode, MAX_PAGE_SIZE};
use orion_db::{
    error::DatabaseError, queries::leaderboard::rank_history_by_user_id,
    repositories::ProfileRepository,
};
use orion_domain::{
    profile::{
        PerformancePoint, ProfileDto, PublishedResearch, RankHistoryPoint, RatingHistoryPoint,
    },
    Rating, UserId, PROFILE_SCHEMA_VERSION,
};
use orion_redis::cache::profile::ProfileCache;
use serde::Deserialize;
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

use crate::{request_id, state::AppState, ApiProblem};

const DEFAULT_HISTORY_LIMIT: u32 = 100;

/// Public profile route. The user ID is intentionally the only lookup key;
/// email, password, lifecycle state, and private research are never exposed.
pub fn router() -> Router<AppState> {
    Router::new().route("/{user_id}", get(get_profile))
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct ProfileQuery {
    /// One bounded limit applies to each history collection in the response.
    limit: Option<u32>,
}

async fn get_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    Query(query): Query<ProfileQuery>,
) -> Result<impl axum::response::IntoResponse, ApiProblem> {
    let request_id = request_id(&headers);
    let user_id = Uuid::parse_str(&user_id).map_err(|_| {
        ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ErrorCode::ValidationFailed,
            "profile user id must be a valid UUID",
        )
        .with_request_id(request_id)
    })?;
    let limit = query.limit.unwrap_or(DEFAULT_HISTORY_LIMIT);
    if limit == 0 || limit > MAX_PAGE_SIZE {
        return Err(ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ErrorCode::ValidationFailed,
            "profile history limit is invalid",
        )
        .with_request_id(request_id));
    }

    let user_id = UserId::from_uuid(user_id);
    let cache = ProfileCache::new(state.redis.clone());
    let profile = match cache.get(user_id).await {
        Ok(Some(profile)) => {
            // A cached projection cannot outlive the authoritative account
            // lifecycle. This small existence check prevents a disabled or
            // deleted user from remaining publicly visible until TTL expiry.
            let repository = ProfileRepository::new(state.db.clone());
            let active = repository
                .active_user_exists(user_id.into_uuid())
                .await
                .map_err(|error| profile_problem(error.into(), request_id))?;
            active.then_some(profile)
        }
        Ok(None) | Err(_) => {
            let service = ProfileService::new(state.db.clone());
            let profile = service
                .load(user_id, DEFAULT_HISTORY_LIMIT)
                .await
                .map_err(|error| profile_problem(error, request_id))?;
            if let Some(profile) = &profile {
                // A cache write cannot make an authoritative response fail.
                let _ = cache.put(user_id, profile.clone()).await;
            }
            profile
        }
    };

    let mut profile = profile.ok_or_else(|| {
        ApiProblem::new(
            StatusCode::NOT_FOUND,
            ErrorCode::NotFound,
            "profile was not found",
        )
        .with_request_id(request_id)
    })?;
    profile.truncate_history(limit as usize);
    Ok(crate::success(&headers, profile))
}

#[derive(Clone)]
pub struct ProfileService {
    pool: PgPool,
}

impl ProfileService {
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn load(
        &self,
        user_id: UserId,
        limit: u32,
    ) -> Result<Option<ProfileDto>, ProfileServiceError> {
        let user_id_uuid = user_id.into_uuid();
        let repository = ProfileRepository::new(self.pool.clone());
        let Some(profile) = repository.find_by_user_id(user_id_uuid).await? else {
            return Ok(None);
        };

        let limit = i64::from(limit);
        let (rating_events, mut rank_rows, performance_rows, research_rows) = tokio::try_join!(
            repository.rating_history_by_user_id(user_id_uuid, limit),
            rank_history_by_user_id(&self.pool, user_id_uuid, limit, 0),
            repository.performance_history_by_user_id(user_id_uuid, limit),
            repository.published_research_by_user_id(user_id_uuid, limit),
        )?;

        let rank_movement = rank_rows.first().and_then(|row| row.rank_movement);
        rank_rows.reverse();

        Ok(Some(ProfileDto {
            schema_version: PROFILE_SCHEMA_VERSION,
            user_id,
            username: profile.username,
            display_name: profile.display_name,
            bio: profile.bio,
            avatar_url: profile.avatar_url,
            rating: profile.rating.map(to_rating).transpose()?,
            global_rank: profile.global_rank.map(to_rank).transpose()?,
            rank_movement,
            quizzes_completed: to_count(profile.quizzes_completed)?,
            correct_answers: to_count(profile.correct_answers)?,
            rating_history: rating_events
                .into_iter()
                .map(|event| {
                    Ok(RatingHistoryPoint {
                        occurred_at: event.created_at,
                        quiz_type: event.quiz_type,
                        rating_before: to_rating(event.user_rating_before)?,
                        rating_after: to_rating(event.user_rating_after)?,
                        rating_delta: event.rating_delta,
                        correct: event.correct,
                    })
                })
                .collect::<Result<Vec<_>, ProfileServiceError>>()?,
            rank_history: rank_rows
                .into_iter()
                .map(|row| {
                    Ok(RankHistoryPoint {
                        snapshot_at: row.snapshot_at,
                        previous_rank: row.previous_rank.map(to_rank).transpose()?,
                        current_rank: to_rank(row.current_rank)?,
                        rank_movement: row.rank_movement,
                    })
                })
                .collect::<Result<Vec<_>, ProfileServiceError>>()?,
            performance_history: performance_rows
                .into_iter()
                .map(|row| {
                    Ok(PerformancePoint {
                        completed_at: row.completed_at,
                        quiz_type: row.quiz_type,
                        total_questions: to_count32(row.total_questions)?,
                        correct_answers: to_count32(row.correct_answers)?,
                        score: to_count32(row.score)?,
                        rating_after: to_rating(row.rating_after)?,
                    })
                })
                .collect::<Result<Vec<_>, ProfileServiceError>>()?,
            published_research: research_rows
                .into_iter()
                .map(|row| {
                    Ok(PublishedResearch {
                        id: row.id,
                        title: row.title,
                        abstract_text: row.abstract_text,
                        published_at: row.published_at,
                        evaluation_score: row.evaluation_score,
                        evaluated_content_version: row
                            .evaluated_content_version
                            .map(to_count32)
                            .transpose()?,
                        elo_award: row.elo_award,
                        elo_awarded: row.elo_awarded,
                    })
                })
                .collect::<Result<Vec<_>, ProfileServiceError>>()?,
        }))
    }
}

fn to_rating(value: i32) -> Result<Rating, ProfileServiceError> {
    Rating::new(value).map_err(|_| ProfileServiceError::InvalidData)
}

fn to_rank(value: i64) -> Result<u64, ProfileServiceError> {
    u64::try_from(value)
        .ok()
        .filter(|value| *value > 0)
        .ok_or(ProfileServiceError::InvalidData)
}

fn to_count(value: i64) -> Result<u64, ProfileServiceError> {
    u64::try_from(value).map_err(|_| ProfileServiceError::InvalidData)
}

fn to_count32(value: i32) -> Result<u32, ProfileServiceError> {
    u32::try_from(value).map_err(|_| ProfileServiceError::InvalidData)
}

#[derive(Debug, Error)]
pub enum ProfileServiceError {
    #[error("profile database query failed")]
    Database(#[from] sqlx::Error),
    #[error("profile contains an invalid authoritative value")]
    InvalidData,
}

fn profile_problem(error: ProfileServiceError, request_id: orion_common::RequestId) -> ApiProblem {
    match error {
        ProfileServiceError::Database(error) => {
            ApiProblem::from(DatabaseError::from_sqlx(error)).with_request_id(request_id)
        }
        ProfileServiceError::InvalidData => ApiProblem::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::Internal,
            "internal server error",
        )
        .with_request_id(request_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_authoritative_values_are_not_silently_cached() {
        assert!(to_rating(-1).is_err());
        assert!(to_rank(0).is_err());
        assert!(to_count(-1).is_err());
        assert!(to_count32(-1).is_err());
    }

    #[test]
    fn valid_counts_and_ranks_are_bounded_types() {
        assert_eq!(to_count(4).unwrap(), 4);
        assert_eq!(to_count32(7).unwrap(), 7);
        assert_eq!(to_rank(2).unwrap(), 2);
    }
}
