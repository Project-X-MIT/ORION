use axum::{
    extract::State,
    http::{HeaderMap, Uri},
    routing::get,
    Json, Router,
};
use serde::Serialize;

use crate::{state::AppState, success};

#[derive(Debug, Serialize)]
pub struct DiscordInviteResponse {
    pub invite_url: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/invite", get(get_invite))
}

async fn get_invite(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Json<orion_common::ApiSuccess<DiscordInviteResponse>> {
    let invite_url = state
        .config
        .discord_invite_url
        .as_deref()
        .and_then(approved_discord_invite)
        .map(str::to_owned);

    success(&headers, DiscordInviteResponse { invite_url })
}

fn approved_discord_invite(value: &str) -> Option<&str> {
    let uri = value.parse::<Uri>().ok()?;
    if uri.scheme_str() != Some("https")
        || uri.query().is_some()
        || uri.port_u16().is_some_and(|port| port != 443)
    {
        return None;
    }

    let authority = uri.authority()?;
    if authority.as_str().contains('@') {
        return None;
    }

    let host = uri.host()?.to_ascii_lowercase();
    let token = match host.as_str() {
        "discord.gg" => uri.path().strip_prefix('/')?,
        "discord.com" => uri.path().strip_prefix("/invite/")?,
        _ => return None,
    };

    if is_invite_token(token) {
        Some(value)
    } else {
        None
    }
}

fn is_invite_token(token: &str) -> bool {
    !token.is_empty()
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

#[cfg(test)]
mod tests {
    use super::approved_discord_invite;

    #[test]
    fn accepts_only_approved_https_discord_invite_paths() {
        assert_eq!(
            approved_discord_invite("https://discord.gg/qXRjY4PPp"),
            Some("https://discord.gg/qXRjY4PPp")
        );
        assert_eq!(
            approved_discord_invite("https://discord.com/invite/qXRjY4PPp"),
            Some("https://discord.com/invite/qXRjY4PPp")
        );
    }

    #[test]
    fn rejects_open_redirect_and_unsafe_invite_values() {
        for value in [
            "http://discord.gg/qXRjY4PPp",
            "https://example.com/qXRjY4PPp",
            "https://www.discord.com/invite/qXRjY4PPp",
            "https://discord.gg.evil.example/qXRjY4PPp",
            "https://user:password@discord.gg/qXRjY4PPp",
            "https://discord.com/channels/@me",
            "https://discord.gg/invite/qXRjY4PPp",
            "https://discord.gg/qXRjY4PPp?redirect=https://evil.example",
        ] {
            assert_eq!(approved_discord_invite(value), None, "{value}");
        }
    }
}
