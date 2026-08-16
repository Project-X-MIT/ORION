use axum::http::Request;
use tracing::Level;
use uuid::Uuid;

/// Request tracing deliberately records only operational metadata. Headers,
/// cookies and response bodies are never rendered into logs.
#[must_use]
pub fn span<B>(request: &Request<B>) -> tracing::Span {
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok());
    let trace_id = trace_context(request);
    tracing::span!(
        Level::INFO,
        "http_request",
        method = %request.method(),
        path = %request.uri().path(),
        request_id = ?request_id,
        trace_id = ?trace_id,
    )
}

fn trace_context<B>(request: &Request<B>) -> Option<&str> {
    request
        .headers()
        .get("x-trace-id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= 128)
}

/// Values that must never be copied into structured telemetry.
#[must_use]
pub fn is_sensitive_field(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "authorization"
            | "cookie"
            | "set-cookie"
            | "password"
            | "password_hash"
            | "token"
            | "access_token"
            | "refresh_token"
            | "email"
    )
}

/// Redact a free-form diagnostic value before it is attached to a log event.
#[must_use]
pub fn redact(value: &str) -> &'static str {
    let _ = value;
    "[REDACTED]"
}

#[cfg(test)]
mod tests {
    use super::{is_sensitive_field, redact, trace_context};

    #[test]
    fn sensitive_fields_are_denied() {
        assert!(is_sensitive_field("Authorization"));
        assert!(is_sensitive_field("set-cookie"));
        assert!(!is_sensitive_field("status"));
        assert_eq!(redact("secret"), "[REDACTED]");
    }

    #[test]
    fn trace_context_is_bounded_before_logging() {
        let request = axum::http::Request::builder()
            .header("x-trace-id", " trace-123 ")
            .body(())
            .expect("request should build");
        assert_eq!(trace_context(&request), Some("trace-123"));

        let long_trace = axum::http::Request::builder()
            .header("x-trace-id", "x".repeat(129))
            .body(())
            .expect("request should build");
        assert_eq!(trace_context(&long_trace), None);
    }
}
