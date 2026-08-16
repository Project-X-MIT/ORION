use std::time::Duration;

use axum::http::{header, HeaderValue, Method};
use tower_http::cors::{AllowOrigin, CorsLayer};

/// Build an explicit, credential-safe CORS policy.
///
/// Origins are supplied by configuration; a wildcard origin is never emitted
/// when credentials are enabled. Invalid values are ignored here and should be
/// rejected by deployment configuration validation before startup.
pub fn layer(origins: &[String]) -> CorsLayer {
    let origins = origins
        .iter()
        .filter_map(|origin| origin.parse::<HeaderValue>().ok())
        .collect::<Vec<_>>();

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::CONTENT_TYPE,
            header::ACCEPT,
            header::AUTHORIZATION,
            HeaderName::from_static("x-request-id"),
        ])
        .allow_credentials(true)
        .max_age(Duration::from_secs(600))
}

use axum::http::header::HeaderName;

#[cfg(test)]
mod tests {
    use super::layer;

    #[test]
    fn invalid_origins_do_not_become_wildcards() {
        let policy = layer(&["not an origin".to_owned()]);
        let debug = format!("{policy:?}");
        assert!(!debug.contains("*"));
    }
}
