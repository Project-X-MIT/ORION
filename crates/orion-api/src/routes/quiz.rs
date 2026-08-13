use std::collections::HashSet;

use axum::{
    body::to_bytes,
    extract::{FromRequest, FromRequestParts, Path, Query, Request, State},
    http::{header, request::Parts, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use chrono::{DateTime, Utc};
use orion_common::{ErrorCode, MAX_PAGE_SIZE};
use orion_db::{
    error::DatabaseError,
    models::{QuizAnswer, QuizAttempt, QuizQuestionWithOptions, QuizSettlementInput},
    repositories::QuizRepository,
};
use serde::{
    de::{DeserializeOwned, IgnoredAny},
    Deserialize, Serialize,
};
use uuid::Uuid;

use crate::{request_id, routes::auth::AuthenticatedUser, state::AppState, ApiProblem};

const PRIVATE_CACHE_CONTROL: &str = "private, no-store";
const MAX_QUIZ_REQUEST_BYTES: usize = 1_048_576;
const MAX_BASIC_ANSWERS: usize = 100;

/// Routes for authenticated quiz question retrieval and submission.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/basic", get(get_basic_questions))
        .route("/basic/attempts", post(submit_basic_attempt))
        .route("/advanced", get(get_advanced_questions))
        .route("/advanced/attempts", post(submit_advanced_attempt))
        .route("/attempts/{attempt_id}", get(get_attempt_result))
}

#[derive(Debug, Deserialize, Default)]
struct PaginationQuery {
    limit: Option<u32>,
    offset: Option<u64>,
}

#[derive(Debug)]
struct QuizPaginationQuery(PaginationQuery);

impl<S> FromRequestParts<S> for QuizPaginationQuery
where
    S: Send + Sync,
{
    type Rejection = ApiProblem;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        Query::<PaginationQuery>::from_request_parts(parts, state)
            .await
            .map(|Query(query)| Self(query))
            .map_err(|_| {
                ApiProblem::new(
                    StatusCode::BAD_REQUEST,
                    ErrorCode::InvalidRequest,
                    "quiz query parameters are invalid",
                )
                .with_request_id(request_id(&parts.headers))
            })
    }
}

#[derive(Debug, Serialize)]
struct BasicQuestionsResponse {
    items: Vec<BasicQuestionResponse>,
    limit: u32,
    offset: u64,
    has_more: bool,
}

/// The answer key, explanation, and internal rating data are deliberately not
/// part of the transport projection.
#[derive(Debug, Serialize)]
pub struct BasicQuestionResponse {
    pub id: Uuid,
    pub category: String,
    pub question_text: String,
    pub options: Vec<BasicOptionResponse>,
}

#[derive(Debug, Serialize)]
pub struct BasicOptionResponse {
    pub id: Uuid,
    pub option_text: String,
    pub position: i32,
}

impl From<QuizQuestionWithOptions> for BasicQuestionResponse {
    fn from(value: QuizQuestionWithOptions) -> Self {
        Self {
            id: value.question.id,
            category: value.question.category,
            question_text: value.question.question_text,
            options: value
                .options
                .into_iter()
                .map(|option| BasicOptionResponse {
                    id: option.id,
                    option_text: option.option_text,
                    position: option.position,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BasicSubmitRequest {
    attempt_id: Uuid,
    answers: Vec<BasicAnswerRequest>,
    #[serde(default)]
    started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    completed_at: Option<DateTime<Utc>>,
    #[serde(rename = "score", alias = "correct_answers", default)]
    _score: Option<IgnoredAny>,
    #[serde(rename = "outcome", alias = "correct", alias = "is_correct", default)]
    _outcome: Option<IgnoredAny>,
    #[serde(
        rename = "delta",
        alias = "rating_delta",
        alias = "point_delta",
        default
    )]
    _delta: Option<IgnoredAny>,
    #[serde(
        rename = "rating",
        alias = "rating_before",
        alias = "rating_after",
        default
    )]
    _rating: Option<IgnoredAny>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BasicAnswerRequest {
    question_id: Uuid,
    #[serde(alias = "selected_option_id")]
    option_id: Uuid,
    #[serde(rename = "score", alias = "correct_answers", default)]
    _score: Option<IgnoredAny>,
    #[serde(rename = "outcome", alias = "correct", alias = "is_correct", default)]
    _outcome: Option<IgnoredAny>,
    #[serde(
        rename = "delta",
        alias = "rating_delta",
        alias = "point_delta",
        default
    )]
    _delta: Option<IgnoredAny>,
    #[serde(
        rename = "rating",
        alias = "rating_before",
        alias = "rating_after",
        default
    )]
    _rating: Option<IgnoredAny>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AdvancedSubmitRequest {
    attempt_id: Uuid,
    #[serde(alias = "answers")]
    predictions: Vec<AdvancedPredictionRequest>,
    #[serde(default)]
    started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    completed_at: Option<DateTime<Utc>>,
    #[serde(rename = "score", alias = "correct_answers", default)]
    _score: Option<IgnoredAny>,
    #[serde(rename = "outcome", alias = "correct", alias = "is_correct", default)]
    _outcome: Option<IgnoredAny>,
    #[serde(
        rename = "delta",
        alias = "rating_delta",
        alias = "point_delta",
        default
    )]
    _delta: Option<IgnoredAny>,
    #[serde(
        rename = "rating",
        alias = "rating_before",
        alias = "rating_after",
        default
    )]
    _rating: Option<IgnoredAny>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AdvancedPredictionRequest {
    question_id: Uuid,
    #[serde(alias = "selected_option_id")]
    option_id: Uuid,
    #[serde(rename = "score", alias = "correct_answers", default)]
    _score: Option<IgnoredAny>,
    #[serde(rename = "outcome", alias = "correct", alias = "is_correct", default)]
    _outcome: Option<IgnoredAny>,
    #[serde(
        rename = "delta",
        alias = "rating_delta",
        alias = "point_delta",
        default
    )]
    _delta: Option<IgnoredAny>,
    #[serde(
        rename = "rating",
        alias = "rating_before",
        alias = "rating_after",
        default
    )]
    _rating: Option<IgnoredAny>,
}

#[derive(Debug, Serialize)]
struct BasicSubmissionResponse {
    attempt: BasicAttemptResponse,
    rating: BasicRatingResponse,
    answers: Vec<BasicAnswerResultResponse>,
}

#[derive(Debug, Serialize)]
struct AdvancedSubmissionResponse {
    attempt: BasicAttemptResponse,
    rating: BasicRatingResponse,
    predictions: Vec<BasicAnswerResultResponse>,
}

#[derive(Debug, Serialize)]
struct AttemptResultResponse {
    attempt: AttemptResultAttemptResponse,
    rating: BasicRatingResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    answers: Option<Vec<BasicAnswerResultResponse>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    predictions: Option<Vec<BasicAnswerResultResponse>>,
}

#[derive(Debug, Serialize)]
struct AttemptResultAttemptResponse {
    id: Uuid,
    quiz_type: String,
    status: String,
    total_questions: i32,
    correct_answers: i32,
    score: i32,
    rating_before: i32,
    rating_after: i32,
    started_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
}

impl From<QuizAttempt> for AttemptResultAttemptResponse {
    fn from(attempt: QuizAttempt) -> Self {
        Self {
            id: attempt.id,
            quiz_type: attempt.quiz_type,
            status: attempt.status,
            total_questions: attempt.total_questions,
            correct_answers: attempt.correct_answers,
            score: attempt.score,
            rating_before: attempt.rating_before,
            rating_after: attempt.rating_after,
            started_at: attempt.started_at,
            completed_at: attempt.completed_at,
        }
    }
}

#[derive(Debug, Serialize)]
struct BasicAttemptResponse {
    id: Uuid,
    status: String,
    total_questions: i32,
    correct_answers: i32,
    score: i32,
    rating_before: i32,
    rating_after: i32,
    started_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
}

impl From<QuizAttempt> for BasicAttemptResponse {
    fn from(attempt: QuizAttempt) -> Self {
        Self {
            id: attempt.id,
            status: attempt.status,
            total_questions: attempt.total_questions,
            correct_answers: attempt.correct_answers,
            score: attempt.score,
            rating_before: attempt.rating_before,
            rating_after: attempt.rating_after,
            started_at: attempt.started_at,
            completed_at: attempt.completed_at,
        }
    }
}

#[derive(Debug, Serialize)]
struct BasicRatingResponse {
    rating: i32,
    games_played: i32,
    wins: i32,
    losses: i32,
    draws: i32,
}

#[derive(Debug, Serialize)]
struct BasicAnswerResultResponse {
    question_id: Uuid,
    correct: bool,
    rating_delta: i32,
}

async fn get_basic_questions(
    State(state): State<AppState>,
    headers: HeaderMap,
    _user: AuthenticatedUser,
    QuizPaginationQuery(query): QuizPaginationQuery,
) -> Result<impl IntoResponse, ApiProblem> {
    let request_id = request_id(&headers);
    let (limit, offset) = pagination(query, request_id)?;
    let repository = QuizRepository::new(state.db);
    let mut questions = repository
        .basic_questions_with_options(i64::from(limit) + 1, offset)
        .await
        .map_err(|error| database_problem(error, request_id))?;
    let has_more = questions.len() > usize::try_from(limit).expect("validated page limit fits");
    if has_more {
        questions.pop();
    }

    Ok(private_success(
        &headers,
        BasicQuestionsResponse {
            items: questions
                .into_iter()
                .map(BasicQuestionResponse::from)
                .collect(),
            limit,
            offset: u64::try_from(offset).expect("validated page offset fits in u64"),
            has_more,
        },
    ))
}

async fn get_advanced_questions(
    State(state): State<AppState>,
    headers: HeaderMap,
    _user: AuthenticatedUser,
    QuizPaginationQuery(query): QuizPaginationQuery,
) -> Result<impl IntoResponse, ApiProblem> {
    let request_id = request_id(&headers);
    let (limit, offset) = pagination(query, request_id)?;
    let repository = QuizRepository::new(state.db);
    let mut questions = repository
        .advanced_questions_with_options(i64::from(limit) + 1, offset)
        .await
        .map_err(|error| database_problem(error, request_id))?;
    let has_more = questions.len() > usize::try_from(limit).expect("validated page limit fits");
    if has_more {
        questions.pop();
    }

    Ok(private_success(
        &headers,
        BasicQuestionsResponse {
            items: questions
                .into_iter()
                .map(BasicQuestionResponse::from)
                .collect(),
            limit,
            offset: u64::try_from(offset).expect("validated page offset fits in u64"),
            has_more,
        },
    ))
}

async fn submit_basic_attempt(
    State(state): State<AppState>,
    headers: HeaderMap,
    user: AuthenticatedUser,
    QuizJson(request): QuizJson<BasicSubmitRequest>,
) -> Result<impl IntoResponse, ApiProblem> {
    let request_id = request_id(&headers);
    let input = settlement_input(request, user.user.id, request_id)?;
    let repository = QuizRepository::new(state.db);
    let result = repository
        .settle_basic(input)
        .await
        .map_err(|error| submission_database_problem(error, request_id, "Basic"))?;

    let response = BasicSubmissionResponse {
        attempt: BasicAttemptResponse::from(result.attempt),
        rating: BasicRatingResponse {
            rating: result.user_rating.rating,
            games_played: result.user_rating.games_played,
            wins: result.user_rating.wins,
            losses: result.user_rating.losses,
            draws: result.user_rating.draws,
        },
        answers: result
            .events
            .into_iter()
            .map(|event| BasicAnswerResultResponse {
                question_id: event.question_id,
                correct: event.correct,
                rating_delta: event.rating_delta,
            })
            .collect(),
    };

    Ok(private_success(&headers, response))
}

async fn submit_advanced_attempt(
    State(state): State<AppState>,
    headers: HeaderMap,
    user: AuthenticatedUser,
    QuizJson(request): QuizJson<AdvancedSubmitRequest>,
) -> Result<impl IntoResponse, ApiProblem> {
    let request_id = request_id(&headers);
    let input = advanced_settlement_input(request, user.user.id, request_id)?;
    let repository = QuizRepository::new(state.db);
    let result = repository
        .settle_advanced(input)
        .await
        .map_err(|error| submission_database_problem(error, request_id, "Advanced"))?;

    let response = AdvancedSubmissionResponse {
        attempt: BasicAttemptResponse::from(result.attempt),
        rating: BasicRatingResponse {
            rating: result.user_rating.rating,
            games_played: result.user_rating.games_played,
            wins: result.user_rating.wins,
            losses: result.user_rating.losses,
            draws: result.user_rating.draws,
        },
        predictions: result
            .events
            .into_iter()
            .map(|event| BasicAnswerResultResponse {
                question_id: event.question_id,
                correct: event.correct,
                rating_delta: event.rating_delta,
            })
            .collect(),
    };

    Ok(private_success(&headers, response))
}

async fn get_attempt_result(
    State(state): State<AppState>,
    headers: HeaderMap,
    user: AuthenticatedUser,
    Path(attempt_id): Path<String>,
) -> Result<impl IntoResponse, ApiProblem> {
    let request_id = request_id(&headers);
    let attempt_id = parse_attempt_id(&attempt_id, request_id)?;
    let repository = QuizRepository::new(state.db);
    let result = repository
        .find_completed_result(attempt_id, user.user.id)
        .await
        .map_err(|error| database_problem(error, request_id))?
        .ok_or_else(|| not_found(request_id))?;

    let is_advanced = result.attempt.quiz_type == "advanced";
    let answer_results = result
        .events
        .into_iter()
        .map(|event| BasicAnswerResultResponse {
            question_id: event.question_id,
            correct: event.correct,
            rating_delta: event.rating_delta,
        })
        .collect();
    let (answers, predictions) = if is_advanced {
        (None, Some(answer_results))
    } else {
        (Some(answer_results), None)
    };
    let response = AttemptResultResponse {
        attempt: AttemptResultAttemptResponse::from(result.attempt),
        rating: BasicRatingResponse {
            rating: result.user_rating.rating,
            games_played: result.user_rating.games_played,
            wins: result.user_rating.wins,
            losses: result.user_rating.losses,
            draws: result.user_rating.draws,
        },
        answers,
        predictions,
    };

    Ok(private_success(&headers, response))
}

fn settlement_input(
    request: BasicSubmitRequest,
    user_id: Uuid,
    request_id: orion_common::RequestId,
) -> Result<QuizSettlementInput, ApiProblem> {
    if request.answers.is_empty() {
        return Err(validation(
            request_id,
            "answers must contain at least one item",
        ));
    }
    if request.answers.len() > MAX_BASIC_ANSWERS {
        return Err(validation(
            request_id,
            "a Basic Quiz submission may contain at most 100 answers",
        ));
    }

    let (started_at, completed_at) =
        submission_timestamps(request.started_at, request.completed_at, request_id)?;

    let mut seen_questions = HashSet::with_capacity(request.answers.len());
    if request
        .answers
        .iter()
        .any(|answer| !seen_questions.insert(answer.question_id))
    {
        return Err(validation(
            request_id,
            "answers may contain each question only once",
        ));
    }

    Ok(QuizSettlementInput {
        attempt_id: request.attempt_id,
        user_id,
        answers: request
            .answers
            .into_iter()
            .map(|answer| QuizAnswer::selected(answer.question_id, answer.option_id))
            .collect(),
        started_at,
        completed_at,
    })
}

fn advanced_settlement_input(
    request: AdvancedSubmitRequest,
    user_id: Uuid,
    request_id: orion_common::RequestId,
) -> Result<QuizSettlementInput, ApiProblem> {
    if request.predictions.is_empty() {
        return Err(validation(
            request_id,
            "predictions must contain at least one item",
        ));
    }
    if request.predictions.len() > MAX_BASIC_ANSWERS {
        return Err(validation(
            request_id,
            "an Advanced Quiz submission may contain at most 100 predictions",
        ));
    }

    let (started_at, completed_at) =
        submission_timestamps(request.started_at, request.completed_at, request_id)?;
    let mut seen_questions = HashSet::with_capacity(request.predictions.len());
    if request
        .predictions
        .iter()
        .any(|prediction| !seen_questions.insert(prediction.question_id))
    {
        return Err(validation(
            request_id,
            "predictions may contain each question only once",
        ));
    }

    Ok(QuizSettlementInput {
        attempt_id: request.attempt_id,
        user_id,
        answers: request
            .predictions
            .into_iter()
            .map(|prediction| QuizAnswer::selected(prediction.question_id, prediction.option_id))
            .collect(),
        started_at,
        completed_at,
    })
}

fn submission_timestamps(
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    request_id: orion_common::RequestId,
) -> Result<(DateTime<Utc>, DateTime<Utc>), ApiProblem> {
    let now = Utc::now();
    let completed_at = completed_at.unwrap_or(now);
    let started_at = started_at.unwrap_or(completed_at);
    if started_at > now || completed_at > now {
        return Err(validation(
            request_id,
            "submission timestamps cannot be in the future",
        ));
    }
    if completed_at < started_at {
        return Err(validation(
            request_id,
            "submission completion cannot be earlier than its start",
        ));
    }
    Ok((started_at, completed_at))
}

fn pagination(
    query: PaginationQuery,
    request_id: orion_common::RequestId,
) -> Result<(u32, i64), ApiProblem> {
    let limit = query.limit.unwrap_or(20);
    if !(1..=MAX_PAGE_SIZE).contains(&limit) {
        return Err(validation(request_id, "limit must be between 1 and 100"));
    }
    let offset = query.offset.unwrap_or(0);
    let offset =
        i64::try_from(offset).map_err(|_| validation(request_id, "offset is too large"))?;
    Ok((limit, offset))
}

fn parse_attempt_id(value: &str, request_id: orion_common::RequestId) -> Result<Uuid, ApiProblem> {
    Uuid::parse_str(value).map_err(|_| {
        ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ErrorCode::InvalidRequest,
            "attempt id must be a valid UUID",
        )
        .with_request_id(request_id)
    })
}

fn database_problem(error: sqlx::Error, request_id: orion_common::RequestId) -> ApiProblem {
    ApiProblem::from(DatabaseError::from_sqlx(error)).with_request_id(request_id)
}

/// Maps the settlement transaction's validation sentinel to the shared API
/// validation error. All other SQL failures retain the shared database
/// mapping, including availability and internal-error responses.
fn submission_database_problem(
    error: sqlx::Error,
    request_id: orion_common::RequestId,
    quiz_name: &'static str,
) -> ApiProblem {
    match error {
        sqlx::Error::RowNotFound | sqlx::Error::Protocol(_) => {
            if quiz_name == "Advanced" {
                validation(
                    request_id,
                    "predictions must reference active Advanced Quiz questions and their options",
                )
            } else {
                validation(
                    request_id,
                    "answers must reference active Basic Quiz questions and their options",
                )
            }
        }
        error => database_problem(error, request_id),
    }
}

fn validation(request_id: orion_common::RequestId, message: &'static str) -> ApiProblem {
    ApiProblem::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        ErrorCode::ValidationFailed,
        message,
    )
    .with_request_id(request_id)
}

fn not_found(request_id: orion_common::RequestId) -> ApiProblem {
    ApiProblem::new(
        StatusCode::NOT_FOUND,
        ErrorCode::NotFound,
        "quiz attempt was not found",
    )
    .with_request_id(request_id)
}

fn private_success<T: Serialize>(headers: &HeaderMap, data: T) -> Response {
    let mut response = crate::success(headers, data).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(PRIVATE_CACHE_CONTROL),
    );
    response
}

#[derive(Debug)]
struct QuizJson<T>(T);

impl<S, T> FromRequest<S> for QuizJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiProblem;

    async fn from_request(request: Request, _state: &S) -> Result<Self, Self::Rejection> {
        let request_id = request_id(request.headers());
        let content_type = request
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(';').next())
            .map(str::trim)
            .unwrap_or_default();
        if !content_type.eq_ignore_ascii_case("application/json") {
            return Err(ApiProblem::new(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                ErrorCode::InvalidRequest,
                "quiz requests must use application/json",
            )
            .with_request_id(request_id));
        }

        let body = to_bytes(request.into_body(), MAX_QUIZ_REQUEST_BYTES)
            .await
            .map_err(|_| {
                ApiProblem::new(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    ErrorCode::ValidationFailed,
                    "quiz request exceeds the 1 MiB size limit",
                )
                .with_request_id(request_id)
            })?;
        serde_json::from_slice(&body).map(Self).map_err(|_| {
            ApiProblem::new(
                StatusCode::BAD_REQUEST,
                ErrorCode::InvalidRequest,
                "quiz request body is invalid JSON",
            )
            .with_request_id(request_id)
        })
    }
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use chrono::{Duration, Utc};
    use orion_common::ErrorCode;
    use serde_json::{json, Value};

    use super::{
        advanced_settlement_input, database_problem, settlement_input, submission_database_problem,
        submission_timestamps, AdvancedSubmitRequest, BasicOptionResponse, BasicQuestionResponse,
        BasicSubmitRequest,
    };

    #[test]
    fn basic_question_projection_does_not_expose_answer_key_or_explanation() {
        let response = BasicQuestionResponse {
            id: uuid::Uuid::from_u128(1),
            category: "science".to_owned(),
            question_text: "What is water?".to_owned(),
            options: vec![BasicOptionResponse {
                id: uuid::Uuid::from_u128(2),
                option_text: "H2O".to_owned(),
                position: 0,
            }],
        };

        let value: Value = serde_json::to_value(response).expect("projection serializes");
        assert!(value.get("explanation").is_none());
        assert!(value["options"][0].get("is_correct").is_none());
    }

    #[test]
    fn client_scoring_fields_are_ignored_for_basic_and_advanced_submissions() {
        let attempt_id = uuid::Uuid::from_u128(1);
        let question_id = uuid::Uuid::from_u128(2);
        let option_id = uuid::Uuid::from_u128(3);
        let user_id = uuid::Uuid::from_u128(4);
        let request_id = orion_common::RequestId::from_uuid(uuid::Uuid::from_u128(5));

        let basic: BasicSubmitRequest = serde_json::from_value(json!({
            "attempt_id": attempt_id,
            "score": 999,
            "outcome": "win",
            "delta": 999,
            "rating": 9999,
            "answers": [{
                "question_id": question_id,
                "option_id": option_id,
                "score": 999,
                "outcome": "win",
                "delta": 999,
                "rating": 9999
            }]
        }))
        .expect("basic scoring fields are accepted and ignored");
        let basic_input = settlement_input(basic, user_id, request_id)
            .expect("basic submission input is derived from the selected option");
        assert_eq!(basic_input.answers.len(), 1);
        assert_eq!(basic_input.answers[0].question_id, question_id);
        assert_eq!(basic_input.answers[0].option_id, Some(option_id));

        let advanced: AdvancedSubmitRequest = serde_json::from_value(json!({
            "attempt_id": attempt_id,
            "correct_answers": 999,
            "correct": true,
            "rating_delta": 999,
            "rating_after": 9999,
            "predictions": [{
                "question_id": question_id,
                "option_id": option_id,
                "is_correct": true,
                "point_delta": 999,
                "rating_before": 9999
            }]
        }))
        .expect("advanced scoring fields are accepted and ignored");
        let advanced_input = advanced_settlement_input(advanced, user_id, request_id)
            .expect("advanced submission input is derived from the selected option");
        assert_eq!(advanced_input.answers.len(), 1);
        assert_eq!(advanced_input.answers[0].question_id, question_id);
        assert_eq!(advanced_input.answers[0].option_id, Some(option_id));
    }

    #[test]
    fn validation_failures_use_the_shared_validation_error() {
        let request_id = orion_common::RequestId::from_uuid(uuid::Uuid::from_u128(4));
        let now = Utc::now();
        let problem = submission_timestamps(
            Some(now - Duration::seconds(1)),
            Some(now - Duration::seconds(2)),
            request_id,
        )
        .expect_err("completion before start is invalid");

        assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(problem.code, ErrorCode::ValidationFailed);
        assert_eq!(problem.request_id, Some(request_id));
    }

    #[test]
    fn database_failures_use_shared_availability_and_internal_errors() {
        let request_id = orion_common::RequestId::from_uuid(uuid::Uuid::from_u128(6));

        let unavailable = database_problem(sqlx::Error::PoolClosed, request_id);
        assert_eq!(unavailable.status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(unavailable.code, ErrorCode::ServiceUnavailable);

        let unexpected = database_problem(sqlx::Error::RowNotFound, request_id);
        assert_eq!(unexpected.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(unexpected.code, ErrorCode::Internal);

        let validation = submission_database_problem(
            sqlx::Error::Protocol("domain validation failure".to_owned()),
            request_id,
            "Basic",
        );
        assert_eq!(validation.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(validation.code, ErrorCode::ValidationFailed);
    }
}
