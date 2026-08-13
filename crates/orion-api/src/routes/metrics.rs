use axum::{
    http::{header, HeaderValue},
    response::IntoResponse,
    routing::get,
    Router,
};

/// Minimal Prometheus exposition surface with bounded, non-personal labels.
/// Feature-specific counters can be appended by the owning module.
pub fn router() -> Router<crate::state::AppState> {
    Router::new().route("/metrics", get(metrics))
}

async fn metrics() -> impl IntoResponse {
    let body = concat!(
        "# HELP orion_api_up API process health\n",
        "# TYPE orion_api_up gauge\n",
        "orion_api_up 1\n",
        "# HELP orion_api_info API build metadata\n",
        "# TYPE orion_api_info gauge\n",
        "orion_api_info{service=\"orion-api\"} 1\n"
    );
    (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; version=0.0.4"),
        )],
        body,
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn exposition_has_no_personal_labels() {
        let body = "orion_api_info{service=\"orion-api\"} 1";
        assert!(!body.contains("email"));
        assert!(!body.contains("user_id"));
    }
}
