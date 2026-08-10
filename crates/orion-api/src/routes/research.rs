use axum::{
    body::to_bytes,
    extract::{FromRequest, Path, Query, Request, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use chrono::{DateTime, Utc};
use orion_common::{ErrorCode, MAX_PAGE_SIZE};
use orion_db::{
    error::DatabaseError,
    models::{ResearchPaper, ResearchPaperStatus},
    repositories::ResearchRepository,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use uuid::Uuid;

use crate::{request_id, routes::auth::AuthenticatedUser, state::AppState, ApiProblem};

const NOT_FOUND_MESSAGE: &str = "research paper was not found";
const INVALID_ID_MESSAGE: &str = "research id must be a valid UUID";
const MAX_RESEARCH_REQUEST_BYTES: usize = 1_048_576;
const MAX_TITLE_CHARS: usize = 200;
const MAX_ABSTRACT_CHARS: usize = 5_000;
const MAX_CONTENT_CHARS: usize = 500_000;
const PRIVATE_CACHE_CONTROL: &str = "private, no-store";

/// Routes for research authoring and the published research catalogue.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_published).post(create_draft))
        .route("/drafts", get(list_own_drafts))
        .route("/{research_id}", get(get_research).put(update_draft))
        .route("/{research_id}/submission", post(submit_paper))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResearchDraftRequest {
    pub title: String,
    #[serde(rename = "abstract", alias = "abstract_text", default)]
    pub abstract_text: String,
    pub content: String,
}

#[derive(Debug, Deserialize, Default)]
struct PaginationQuery {
    limit: Option<u32>,
    offset: Option<u64>,
}

#[derive(Debug, Serialize)]
struct ResearchListResponse {
    items: Vec<ResearchPaperResponse>,
    limit: u32,
    offset: u64,
    has_more: bool,
}

/// Fields deliberately shared by author status reads and published reads.
/// Reviewer identities, review payloads, and Elo award bookkeeping remain
/// internal to the DB/worker workflow.
#[derive(Debug, Serialize)]
pub struct ResearchPaperResponse {
    pub id: Uuid,
    pub author_id: Uuid,
    pub title: String,
    #[serde(rename = "abstract")]
    pub abstract_text: String,
    pub content: String,
    pub status: String,
    pub submitted_at: Option<DateTime<Utc>>,
    pub under_review_at: Option<DateTime<Utc>>,
    pub decided_at: Option<DateTime<Utc>>,
    pub published_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<ResearchPaper> for ResearchPaperResponse {
    fn from(paper: ResearchPaper) -> Self {
        Self {
            id: paper.id,
            author_id: paper.author_id,
            title: paper.title,
            abstract_text: paper.abstract_text,
            content: paper.content,
            status: paper.status,
            submitted_at: paper.submitted_at,
            under_review_at: paper.under_review_at,
            decided_at: paper.decided_at,
            published_at: paper.published_at,
            created_at: paper.created_at,
            updated_at: paper.updated_at,
        }
    }
}

async fn create_draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    user: AuthenticatedUser,
    ResearchJson(request): ResearchJson<ResearchDraftRequest>,
) -> Result<impl IntoResponse, ApiProblem> {
    let request_id = request_id(&headers);
    let input = validate_request(request, request_id)?;
    let paper = ResearchRepository::new(state.db)
        .create_draft(
            user.user.id,
            &input.title,
            &input.abstract_text,
            &input.content,
        )
        .await
        .map_err(|error| database_problem(error, request_id))?;

    Ok(private_success(
        &headers,
        ResearchPaperResponse::from(paper),
    ))
}

async fn update_draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(research_id): Path<String>,
    user: AuthenticatedUser,
    ResearchJson(request): ResearchJson<ResearchDraftRequest>,
) -> Result<impl IntoResponse, ApiProblem> {
    let request_id = request_id(&headers);
    let research_id = parse_research_id(&research_id, request_id)?;
    let input = validate_request(request, request_id)?;
    let repository = ResearchRepository::new(state.db);

    ensure_author_can_edit(&repository, research_id, user.user.id, request_id).await?;
    let paper = repository
        .update_draft(
            research_id,
            user.user.id,
            &input.title,
            &input.abstract_text,
            &input.content,
        )
        .await
        .map_err(|error| database_problem(error, request_id))?
        .ok_or_else(|| not_found(request_id))?;

    Ok(private_success(
        &headers,
        ResearchPaperResponse::from(paper),
    ))
}

async fn submit_paper(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(research_id): Path<String>,
    user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiProblem> {
    let request_id = request_id(&headers);
    let research_id = parse_research_id(&research_id, request_id)?;
    let repository = ResearchRepository::new(state.db);

    ensure_author_can_edit(&repository, research_id, user.user.id, request_id).await?;
    let paper = repository
        .submit_for_review(research_id, user.user.id)
        .await
        .map_err(|error| database_problem(error, request_id))?
        .ok_or_else(|| conflict(request_id, "research paper is no longer a draft"))?;

    Ok(private_success(
        &headers,
        ResearchPaperResponse::from(paper),
    ))
}

/// Anonymous callers can only see published papers.  An authenticated author
/// may also read their own unpublished paper to track its lifecycle status.
async fn get_research(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(research_id): Path<String>,
    authenticated: Result<AuthenticatedUser, ApiProblem>,
) -> Result<impl IntoResponse, ApiProblem> {
    let request_id = request_id(&headers);
    let research_id = parse_research_id(&research_id, request_id)?;
    let repository = ResearchRepository::new(state.db);

    let (paper, private_response) = match authenticated {
        Ok(user) => {
            let paper = repository
                .find_by_id(research_id)
                .await
                .map_err(|error| database_problem(error, request_id))?;
            match paper {
                Some(paper) if can_read_research(&paper, Some(user.user.id)) => (Some(paper), true),
                _ => (None, true),
            }
        }
        Err(error) if error.code == ErrorCode::Unauthenticated => (
            repository
                .find_published_by_id(research_id)
                .await
                .map_err(|error| database_problem(error, request_id))?,
            false,
        ),
        Err(error) => return Err(error),
    };

    let paper = paper
        .map(ResearchPaperResponse::from)
        .ok_or_else(|| not_found(request_id))?;
    if private_response {
        Ok(private_success(&headers, paper))
    } else {
        Ok(crate::success(&headers, paper).into_response())
    }
}

async fn list_published(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PaginationQuery>,
) -> Result<impl IntoResponse, ApiProblem> {
    let request_id = request_id(&headers);
    let (limit, offset) = pagination(query, request_id)?;
    let repository = ResearchRepository::new(state.db);
    let mut papers = repository
        .published_research(i64::from(limit) + 1, offset)
        .await
        .map_err(|error| database_problem(error, request_id))?;
    let has_more = papers.len() > usize::try_from(limit).expect("validated page limit fits");
    if has_more {
        papers.pop();
    }

    let response = ResearchListResponse {
        items: papers
            .into_iter()
            .map(ResearchPaperResponse::from)
            .collect(),
        limit,
        offset: u64::try_from(offset).expect("validated page offset fits in u64"),
        has_more,
    };
    Ok(private_success(&headers, response))
}

async fn list_own_drafts(
    State(state): State<AppState>,
    headers: HeaderMap,
    user: AuthenticatedUser,
    Query(query): Query<PaginationQuery>,
) -> Result<impl IntoResponse, ApiProblem> {
    let request_id = request_id(&headers);
    let (limit, offset) = pagination(query, request_id)?;
    let repository = ResearchRepository::new(state.db);
    let mut papers = repository
        .list_drafts_by_author_id(user.user.id, i64::from(limit) + 1, offset)
        .await
        .map_err(|error| database_problem(error, request_id))?;
    let has_more = papers.len() > usize::try_from(limit).expect("validated page limit fits");
    if has_more {
        papers.pop();
    }

    let response = ResearchListResponse {
        items: papers
            .into_iter()
            .map(ResearchPaperResponse::from)
            .collect(),
        limit,
        offset: u64::try_from(offset).expect("validated page offset fits in u64"),
        has_more,
    };
    Ok(crate::success(&headers, response))
}

async fn ensure_author_can_edit(
    repository: &ResearchRepository,
    research_id: Uuid,
    author_id: Uuid,
    request_id: orion_common::RequestId,
) -> Result<(), ApiProblem> {
    let paper = repository
        .find_by_id(research_id)
        .await
        .map_err(|error| database_problem(error, request_id))?;
    match paper
        .as_ref()
        .map(|paper| author_edit_access(paper, author_id))
    {
        Some(EditAccess::Allowed) => Ok(()),
        Some(EditAccess::Conflict) => {
            Err(conflict(request_id, "research paper is no longer a draft"))
        }
        Some(EditAccess::NotFound) | None => Err(not_found(request_id)),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditAccess {
    Allowed,
    Conflict,
    NotFound,
}

fn author_edit_access(paper: &ResearchPaper, author_id: Uuid) -> EditAccess {
    if paper.author_id != author_id {
        return EditAccess::NotFound;
    }
    if matches!(paper.parsed_status(), Ok(ResearchPaperStatus::Draft)) {
        EditAccess::Allowed
    } else {
        EditAccess::Conflict
    }
}

fn can_read_research(paper: &ResearchPaper, viewer_id: Option<Uuid>) -> bool {
    paper.parsed_status().is_ok()
        && (viewer_id == Some(paper.author_id)
            || matches!(paper.parsed_status(), Ok(ResearchPaperStatus::Published)))
}

struct ValidatedResearchRequest {
    title: String,
    abstract_text: String,
    content: String,
}

fn validate_request(
    request: ResearchDraftRequest,
    request_id: orion_common::RequestId,
) -> Result<ValidatedResearchRequest, ApiProblem> {
    Ok(ValidatedResearchRequest {
        title: sanitize_text(
            request.title,
            MAX_TITLE_CHARS,
            true,
            "title is required",
            "title exceeds the 200 character policy limit",
            request_id,
        )?,
        abstract_text: sanitize_text(
            request.abstract_text,
            MAX_ABSTRACT_CHARS,
            false,
            "abstract is required",
            "abstract exceeds the 5000 character policy limit",
            request_id,
        )?,
        content: sanitize_text(
            request.content,
            MAX_CONTENT_CHARS,
            true,
            "content is required",
            "content exceeds the 500000 character policy limit",
            request_id,
        )?,
    })
}

fn sanitize_text(
    value: String,
    max_chars: usize,
    required: bool,
    empty_message: &'static str,
    length_message: &'static str,
    request_id: orion_common::RequestId,
) -> Result<String, ApiProblem> {
    let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
    let trimmed = normalized.trim();
    if required && trimmed.is_empty() {
        return Err(validation(request_id, empty_message));
    }
    if trimmed.chars().count() > max_chars {
        return Err(validation(request_id, length_message));
    }
    if trimmed
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\t'))
        || contains_markup(trimmed)
        || contains_dangerous_scheme(trimmed)
    {
        return Err(validation(
            request_id,
            "research text contains disallowed control characters or markup",
        ));
    }

    Ok(trimmed.to_owned())
}

fn contains_markup(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.windows(2).any(|window| {
        window[0] == b'<'
            && (window[1].is_ascii_alphabetic() || matches!(window[1], b'/' | b'!' | b'?'))
    })
}

fn contains_dangerous_scheme(value: &str) -> bool {
    let lowercase = value.to_ascii_lowercase();
    ["javascript:", "vbscript:", "data:", "file:"]
        .iter()
        .any(|scheme| lowercase.contains(scheme))
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

fn parse_research_id(value: &str, request_id: orion_common::RequestId) -> Result<Uuid, ApiProblem> {
    Uuid::parse_str(value).map_err(|_| {
        ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ErrorCode::InvalidRequest,
            INVALID_ID_MESSAGE,
        )
        .with_request_id(request_id)
    })
}

fn database_problem(error: sqlx::Error, request_id: orion_common::RequestId) -> ApiProblem {
    ApiProblem::from(DatabaseError::from_sqlx(error)).with_request_id(request_id)
}

fn not_found(request_id: orion_common::RequestId) -> ApiProblem {
    ApiProblem::new(
        StatusCode::NOT_FOUND,
        ErrorCode::NotFound,
        NOT_FOUND_MESSAGE,
    )
    .with_request_id(request_id)
}

fn conflict(request_id: orion_common::RequestId, message: &'static str) -> ApiProblem {
    ApiProblem::new(StatusCode::CONFLICT, ErrorCode::Conflict, message).with_request_id(request_id)
}

fn validation(request_id: orion_common::RequestId, message: &'static str) -> ApiProblem {
    ApiProblem::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        ErrorCode::ValidationFailed,
        message,
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
struct ResearchJson<T>(T);

impl<S, T> FromRequest<S> for ResearchJson<T>
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
                "research requests must use application/json",
            )
            .with_request_id(request_id));
        }

        let body = to_bytes(request.into_body(), MAX_RESEARCH_REQUEST_BYTES)
            .await
            .map_err(|_| {
                ApiProblem::new(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    ErrorCode::ValidationFailed,
                    "research request exceeds the 1 MiB size limit",
                )
                .with_request_id(request_id)
            })?;
        serde_json::from_slice(&body).map(Self).map_err(|_| {
            ApiProblem::new(
                StatusCode::BAD_REQUEST,
                ErrorCode::InvalidRequest,
                "research request body is invalid JSON",
            )
            .with_request_id(request_id)
        })
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        extract::FromRequest,
        http::{header, Request, StatusCode},
    };
    use chrono::Utc;
    use serde_json::json;

    use orion_common::RequestId;

    use super::{
        author_edit_access, can_read_research, private_success, sanitize_text, EditAccess,
        ResearchDraftRequest, ResearchJson, ResearchPaper, ResearchPaperResponse,
        PRIVATE_CACHE_CONTROL,
    };

    fn paper(status: &str, author_id: uuid::Uuid) -> ResearchPaper {
        let now = Utc::now();
        ResearchPaper {
            id: uuid::Uuid::new_v4(),
            author_id,
            title: "Research".to_owned(),
            abstract_text: "Summary".to_owned(),
            content: "Content".to_owned(),
            status: status.to_owned(),
            submitted_at: None,
            under_review_at: None,
            decided_by: None,
            decided_at: None,
            published_at: None,
            evaluation_score: None,
            evaluation_result: None,
            elo_award: None,
            elo_awarded: false,
            elo_awarded_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn published_response_redacts_internal_reviewer_and_award_data() {
        let now = Utc::now();
        let paper = ResearchPaper {
            id: uuid::Uuid::new_v4(),
            author_id: uuid::Uuid::new_v4(),
            title: "Published paper".to_owned(),
            abstract_text: "Summary".to_owned(),
            content: "Content".to_owned(),
            status: "published".to_owned(),
            submitted_at: Some(now),
            under_review_at: Some(now),
            decided_by: Some(uuid::Uuid::new_v4()),
            decided_at: Some(now),
            published_at: Some(now),
            evaluation_score: Some(99.0),
            evaluation_result: Some(json!({"private": true})),
            elo_award: Some(25),
            elo_awarded: true,
            elo_awarded_at: Some(now),
            created_at: now,
            updated_at: now,
        };

        let response = serde_json::to_value(ResearchPaperResponse::from(paper)).unwrap();
        assert_eq!(response["status"], "published");
        assert_eq!(response["abstract"], "Summary");
        assert!(response.get("decided_by").is_none());
        assert!(response.get("evaluation_result").is_none());
        assert!(response.get("elo_award").is_none());
    }

    #[test]
    fn research_text_is_normalized_and_policy_checked() {
        let request_id = RequestId::from_uuid(uuid::Uuid::new_v4());
        assert_eq!(
            sanitize_text(
                "  line one\r\nline two  ".to_owned(),
                100,
                true,
                "required",
                "too long",
                request_id,
            )
            .unwrap(),
            "line one\nline two"
        );
        for fixture in [
            "<script>alert(1)</script>",
            "<svg onload=alert(1)>",
            "javascript:alert(1)",
            "VBScript:msgbox(1)",
            "data:text/html,<script>alert(1)</script>",
            "data:image/svg+xml;base64,PHN2ZyBvbmxvYWQ9YWxlcnQoMSk+",
            "file:///etc/passwd",
        ] {
            assert!(
                sanitize_text(
                    fixture.to_owned(),
                    100,
                    true,
                    "required",
                    "too long",
                    request_id,
                )
                .is_err(),
                "unsafe fixture should be rejected: {fixture}"
            );
        }
        assert!(sanitize_text(
            "\u{0}".to_owned(),
            100,
            true,
            "required",
            "too long",
            request_id,
        )
        .is_err());
        assert!(sanitize_text(
            "long".repeat(30),
            100,
            true,
            "required",
            "too long",
            request_id,
        )
        .is_err());
    }

    #[test]
    fn draft_request_rejects_unknown_fields() {
        let request = serde_json::json!({
            "title": "Research",
            "abstract": "Summary",
            "content": "Content",
            "unexpected": "must not be accepted"
        });

        assert!(serde_json::from_value::<ResearchDraftRequest>(request).is_err());
    }

    #[test]
    fn private_research_responses_disable_shared_caching() {
        let response = private_success(
            &axum::http::HeaderMap::new(),
            serde_json::json!({
                "status": "draft"
            }),
        );

        assert_eq!(
            response.headers().get(axum::http::header::CACHE_CONTROL),
            Some(&axum::http::HeaderValue::from_static(PRIVATE_CACHE_CONTROL))
        );
    }

    #[tokio::test]
    async fn binary_file_uploads_are_rejected() {
        let request = Request::builder()
            .header(
                header::CONTENT_TYPE,
                "multipart/form-data; boundary=unsafe-fixture",
            )
            .body(Body::from(b"\x89PNG\r\n\x1a\nunsafe fixture".to_vec()))
            .unwrap();

        let rejection = ResearchJson::<ResearchDraftRequest>::from_request(request, &())
            .await
            .expect_err("binary file uploads must not be accepted");
        assert_eq!(rejection.status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[test]
    fn authorization_and_lifecycle_policy_is_explicit() {
        let author_id = uuid::Uuid::new_v4();
        let other_user_id = uuid::Uuid::new_v4();
        let draft = paper("draft", author_id);
        let submitted = paper("submitted", author_id);
        let published = paper("published", author_id);

        assert_eq!(author_edit_access(&draft, author_id), EditAccess::Allowed);
        assert_eq!(
            author_edit_access(&draft, other_user_id),
            EditAccess::NotFound
        );
        assert_eq!(
            author_edit_access(&submitted, author_id),
            EditAccess::Conflict
        );

        assert!(can_read_research(&draft, Some(author_id)));
        assert!(!can_read_research(&draft, Some(other_user_id)));
        assert!(!can_read_research(&draft, None));
        assert!(can_read_research(&published, None));
        assert!(can_read_research(&published, Some(other_user_id)));
    }
}
