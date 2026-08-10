use std::{env, net::SocketAddr, time::Duration};

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppEnvironment {
    Development,
    Test,
    Production,
}

impl AppEnvironment {
    fn parse(value: &str) -> Result<Self, ConfigError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "development" | "dev" => Ok(Self::Development),
            "test" => Ok(Self::Test),
            "production" | "prod" => Ok(Self::Production),
            _ => Err(ConfigError::Invalid {
                key: "APP_ENV",
                reason: "expected development, test, or production",
            }),
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required configuration: {0}")]
    Missing(&'static str),
    #[error("invalid configuration for {key}: {reason}")]
    Invalid {
        key: &'static str,
        reason: &'static str,
    },
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub environment: AppEnvironment,
    pub bind_address: SocketAddr,
    pub database_url: String,
    pub redis_url: String,
    pub database_max_connections: u32,
    pub session_ttl: Duration,
    pub session_cookie_secure: bool,
    pub cors_origins: Vec<String>,
    pub request_timeout: Duration,
    pub shutdown_timeout: Duration,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        let environment = AppEnvironment::parse(&required("APP_ENV")?)?;
        let bind_address =
            required("API_BIND_ADDRESS")?
                .parse()
                .map_err(|_| ConfigError::Invalid {
                    key: "API_BIND_ADDRESS",
                    reason: "expected a host:port socket address",
                })?;
        let database_url = required("DATABASE_URL")?;
        let redis_url = required("REDIS_URL")?;
        let database_max_connections = positive_u32("DATABASE_MAX_CONNECTIONS", 10)?;
        let session_ttl = Duration::from_secs(positive_u64("SESSION_TTL_SECONDS", 86_400)?);
        let session_cookie_secure = boolean(
            "SESSION_COOKIE_SECURE",
            matches!(environment, AppEnvironment::Production),
        )?;
        if matches!(environment, AppEnvironment::Production) && !session_cookie_secure {
            return Err(ConfigError::Invalid {
                key: "SESSION_COOKIE_SECURE",
                reason: "must be true in production",
            });
        }
        let cors_origins = if matches!(environment, AppEnvironment::Production) {
            let origins = required("CORS_ALLOWED_ORIGINS")?
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>();
            if origins.is_empty() {
                return Err(ConfigError::Invalid {
                    key: "CORS_ALLOWED_ORIGINS",
                    reason: "must contain at least one origin in production",
                });
            }
            origins
        } else {
            csv("CORS_ALLOWED_ORIGINS", "http://localhost:5173")
        };
        let request_timeout = Duration::from_secs(positive_u64("REQUEST_TIMEOUT_SECONDS", 30)?);
        let shutdown_timeout = Duration::from_secs(positive_u64("SHUTDOWN_TIMEOUT_SECONDS", 15)?);

        Ok(Self {
            environment,
            bind_address,
            database_url,
            redis_url,
            database_max_connections,
            session_ttl,
            session_cookie_secure,
            cors_origins,
            request_timeout,
            shutdown_timeout,
        })
    }

    #[must_use]
    pub const fn is_production(&self) -> bool {
        matches!(self.environment, AppEnvironment::Production)
    }
}

fn required(key: &'static str) -> Result<String, ConfigError> {
    env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or(ConfigError::Missing(key))
}

fn positive_u64(key: &'static str, default: u64) -> Result<u64, ConfigError> {
    let value = env::var(key).unwrap_or_else(|_| default.to_string());
    value
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or(ConfigError::Invalid {
            key,
            reason: "expected a positive integer",
        })
}

fn positive_u32(key: &'static str, default: u32) -> Result<u32, ConfigError> {
    let value = env::var(key).unwrap_or_else(|_| default.to_string());
    value
        .parse::<u32>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or(ConfigError::Invalid {
            key,
            reason: "expected a positive integer",
        })
}

fn boolean(key: &'static str, default: bool) -> Result<bool, ConfigError> {
    let value = env::var(key).unwrap_or_else(|_| default.to_string());
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" => Ok(true),
        "0" | "false" | "no" => Ok(false),
        _ => Err(ConfigError::Invalid {
            key,
            reason: "expected true or false",
        }),
    }
}

fn csv(key: &'static str, default: &str) -> Vec<String> {
    env::var(key)
        .unwrap_or_else(|_| default.to_owned())
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::AppEnvironment;

    #[test]
    fn accepts_documented_environment_aliases() {
        assert_eq!(
            AppEnvironment::parse("development").unwrap(),
            AppEnvironment::Development
        );
        assert_eq!(
            AppEnvironment::parse("DEV").unwrap(),
            AppEnvironment::Development
        );
        assert_eq!(AppEnvironment::parse("test").unwrap(), AppEnvironment::Test);
        assert_eq!(
            AppEnvironment::parse("prod").unwrap(),
            AppEnvironment::Production
        );
    }

    #[test]
    fn rejects_unknown_environment() {
        assert!(AppEnvironment::parse("staging").is_err());
    }
}
