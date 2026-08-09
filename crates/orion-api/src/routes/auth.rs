use std::str::FromStr;

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use async_trait::async_trait;
use axum::{
    extract::{FromRequestParts, Json, State},
    http::{header, request::Parts, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use email_address::EmailAddress;
use orion_db::{
    models::{NewUser, User},
    repositories::UserRepository,
};
use orion_domain::{Identity, Role, UserId};
use orion_redis::SessionStoreError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{request_id, state::AppState, ApiProblem};

const SESSION_COOKIE: &str = "orion_session";
const MIN_PASSWORD_LENGTH: usize = 12;

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub username: String,
    pub password: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct AuthUserResponse {
    pub id: Uuid,
    pub email: String,
    pub username: String,
    pub display_name: Option<String>,
    pub status: String,
    pub role: Role,
}

#[derive(Debug, Serialize)]
struct AuthResponse {
    user: AuthUserResponse,
}

#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub user: User,
    pub identity: Identity,
}

impl AuthenticatedUser {
    pub fn require_role(&self, role: Role) -> Result<(), ApiProblem> {
        if self.identity.role == role || self.identity.role == Role::Admin {
            Ok(())
        } else {
            Err(ApiProblem::new(
                StatusCode::FORBIDDEN,
                orion_common::ErrorCode::Forbidden,
                "you do not have permission for this operation",
            ))
        }
    }
}

#[async_trait]
impl FromRequestParts<AppState> for AuthenticatedUser {
    type Rejection = ApiProblem;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let request_id = request_id(&parts.headers);
        let Some(session_id) = session_id_from_headers(&parts.headers) else {
            return Err(unauthenticated(request_id));
        };
        let session = match state.sessions.load(session_id).await {
            Ok(Some(session)) => session,
            Ok(None) | Err(SessionStoreError::Expired) => return Err(unauthenticated(request_id)),
            Err(error) => return Err(ApiProblem::from(error).with_request_id(request_id)),
        };
        let user = UserRepository::new(state.db.clone())
            .find_by_id(session.user_id.into_uuid())
            .await
            .map_err(|error| ApiProblem::from(error).with_request_id(request_id))?
            .ok_or_else(|| unauthenticated(request_id))?;
        if user.status != "active" {
            let _ = state.sessions.revoke(session_id).await;
            return Err(unauthenticated(request_id));
        }
        Ok(Self {
            user,
            identity: Identity {
                user_id: UserId::from_uuid(session.user_id.into_uuid()),
                role: Role::User,
            },
        })
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/me", get(me))
}

async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<RegisterRequest>,
) -> Result<Response, ApiProblem> {
    let request_id = request_id(&headers);
    let email =
        normalize_email(&request.email).map_err(|message| validation(request_id, message))?;
    if !state
        .login_limiter
        .allow(&headers, &email)
        .await
        .map_err(|error| ApiProblem::from(error).with_request_id(request_id))?
    {
        return Err(rate_limited(request_id));
    }
    let username =
        normalize_username(&request.username).map_err(|message| validation(request_id, message))?;
    validate_password(&request.password).map_err(|message| validation(request_id, message))?;
    let display_name = request
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if display_name.is_some_and(|value| value.chars().count() > 120) {
        return Err(validation(request_id, "display name is too long"));
    }
    let password_hash = hash_password(&request.password).map_err(|_| internal(request_id))?;
    let user = UserRepository::new(state.db.clone())
        .create(NewUser {
            email: &email,
            username: &username,
            password_hash: &password_hash,
            display_name,
        })
        .await
        .map_err(|error| ApiProblem::from(error).with_request_id(request_id))?;
    let session = state
        .sessions
        .issue(
            UserId::from_uuid(user.id),
            state.config.session_ttl.as_secs(),
        )
        .await
        .map_err(|error| ApiProblem::from(error).with_request_id(request_id))?;
    tracing::info!(user_id = %user.id, event = "auth.register");
    Ok(with_session_cookie(
        crate::success(
            &headers,
            AuthResponse {
                user: response_user(&user),
            },
        ),
        session.id,
        &state,
    ))
}

async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<LoginRequest>,
) -> Result<Response, ApiProblem> {
    let request_id = request_id(&headers);
    let email = normalize_email(&request.email).map_err(|_| unauthenticated(request_id))?;
    if !state
        .login_limiter
        .allow(&headers, &email)
        .await
        .map_err(|error| ApiProblem::from(error).with_request_id(request_id))?
    {
        return Err(rate_limited(request_id));
    }
    if request.password.is_empty() {
        return Err(unauthenticated(request_id));
    }
    let user = UserRepository::new(state.db.clone())
        .find_by_email(&email)
        .await
        .map_err(|error| ApiProblem::from(error).with_request_id(request_id))?;
    let Some(user) = user else {
        return Err(unauthenticated(request_id));
    };
    if user.status != "active" || !verify_password(&request.password, &user.password_hash) {
        return Err(unauthenticated(request_id));
    }
    if let Some(previous_session) = session_id_from_headers(&headers) {
        let _ = state.sessions.revoke(previous_session).await;
    }
    let session = state
        .sessions
        .issue(
            UserId::from_uuid(user.id),
            state.config.session_ttl.as_secs(),
        )
        .await
        .map_err(|error| ApiProblem::from(error).with_request_id(request_id))?;
    tracing::info!(user_id = %user.id, event = "auth.login");
    Ok(with_session_cookie(
        crate::success(
            &headers,
            AuthResponse {
                user: response_user(&user),
            },
        ),
        session.id,
        &state,
    ))
}

async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
    _user: AuthenticatedUser,
) -> Result<Response, ApiProblem> {
    let request_id = request_id(&headers);
    if let Some(session_id) = session_id_from_headers(&headers) {
        state
            .sessions
            .revoke(session_id)
            .await
            .map_err(|error| ApiProblem::from(error).with_request_id(request_id))?;
    }
    tracing::info!(user_id = %_user.user.id, event = "auth.logout");
    let mut response =
        crate::success(&headers, serde_json::json!({ "logged_out": true })).into_response();
    response
        .headers_mut()
        .insert(header::SET_COOKIE, expired_cookie(&state));
    Ok(response)
}

async fn me(headers: HeaderMap, user: AuthenticatedUser) -> impl IntoResponse {
    crate::success(
        &headers,
        AuthResponse {
            user: response_user(&user.user),
        },
    )
}

fn response_user(user: &User) -> AuthUserResponse {
    AuthUserResponse {
        id: user.id,
        email: user.email.clone(),
        username: user.username.clone(),
        display_name: user.display_name.clone(),
        status: user.status.clone(),
        role: Role::User,
    }
}

fn normalize_email(value: &str) -> Result<String, &'static str> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.len() > 320 || !EmailAddress::is_valid(&normalized) {
        return Err("email address is invalid");
    }
    Ok(normalized)
}

fn normalize_username(value: &str) -> Result<String, &'static str> {
    let normalized = value.trim().to_ascii_lowercase();
    let valid = (3..=32).contains(&normalized.len())
        && normalized
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-');
    if valid {
        Ok(normalized)
    } else {
        Err("username must be 3-32 letters, numbers, underscores, or hyphens")
    }
}

fn validate_password(value: &str) -> Result<(), &'static str> {
    if value.chars().count() < MIN_PASSWORD_LENGTH {
        Err("password must contain at least 12 characters")
    } else if value.chars().count() > 256 {
        Err("password is too long")
    } else {
        Ok(())
    }
}

fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
}

fn verify_password(password: &str, encoded: &str) -> bool {
    PasswordHash::new(encoded).ok().is_some_and(|hash| {
        Argon2::default()
            .verify_password(password.as_bytes(), &hash)
            .is_ok()
    })
}

fn session_id_from_headers(headers: &HeaderMap) -> Option<Uuid> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .map(str::trim)
        .find_map(|entry| entry.strip_prefix(&format!("{SESSION_COOKIE}=")))
        .and_then(|value| Uuid::from_str(value).ok())
}

fn with_session_cookie<T: Serialize>(
    body: axum::Json<orion_common::ApiSuccess<T>>,
    session_id: Uuid,
    state: &AppState,
) -> Response {
    let mut response = body.into_response();
    let cookie = format!(
        "{SESSION_COOKIE}={session_id}; Path=/; Max-Age={}; HttpOnly; SameSite=Lax{}",
        state.config.session_ttl.as_secs(),
        if state.config.session_cookie_secure {
            "; Secure"
        } else {
            ""
        },
    );
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        response.headers_mut().insert(header::SET_COOKIE, value);
    }
    response
}

fn expired_cookie(state: &AppState) -> HeaderValue {
    let value = format!(
        "{SESSION_COOKIE}=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax{}",
        if state.config.session_cookie_secure {
            "; Secure"
        } else {
            ""
        },
    );
    HeaderValue::from_str(&value).expect("static cookie attributes are valid")
}

fn validation(request_id: orion_common::RequestId, message: &'static str) -> ApiProblem {
    ApiProblem::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        orion_common::ErrorCode::ValidationFailed,
        message,
    )
    .with_request_id(request_id)
}

fn unauthenticated(request_id: orion_common::RequestId) -> ApiProblem {
    ApiProblem::new(
        StatusCode::UNAUTHORIZED,
        orion_common::ErrorCode::Unauthenticated,
        "invalid credentials",
    )
    .with_request_id(request_id)
}

fn internal(request_id: orion_common::RequestId) -> ApiProblem {
    ApiProblem::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        orion_common::ErrorCode::Internal,
        "internal server error",
    )
    .with_request_id(request_id)
}

fn rate_limited(request_id: orion_common::RequestId) -> ApiProblem {
    ApiProblem::new(
        StatusCode::TOO_MANY_REQUESTS,
        orion_common::ErrorCode::RateLimited,
        "too many authentication attempts",
    )
    .with_request_id(request_id)
}

#[cfg(test)]
mod tests {
    use super::{
        hash_password, normalize_email, normalize_username, session_id_from_headers,
        verify_password,
    };
    use axum::http::{header, HeaderMap, HeaderValue};
    use uuid::Uuid;

    #[test]
    fn normalizes_login_identifiers() {
        assert_eq!(
            normalize_email("  PERSON@Example.COM ").unwrap(),
            "person@example.com"
        );
        assert_eq!(normalize_username("  Div_123 ").unwrap(), "div_123");
        assert!(normalize_username("no spaces").is_err());
    }

    #[test]
    fn password_hashes_are_verifiable_but_not_reversible() {
        let password = "correct horse battery staple";
        let encoded = hash_password(password).unwrap();
        assert_ne!(encoded, password);
        assert!(verify_password(password, &encoded));
        assert!(!verify_password("wrong password", &encoded));
    }

    #[test]
    fn session_cookie_parser_accepts_only_uuid_values() {
        let id = Uuid::new_v4();
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_str(&format!("other=1; orion_session={id}; theme=dark")).unwrap(),
        );
        assert_eq!(session_id_from_headers(&headers), Some(id));

        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("orion_session=not-a-uuid"),
        );
        assert_eq!(session_id_from_headers(&headers), None);
    }
}
