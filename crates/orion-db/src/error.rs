use thiserror::Error;

/// Stable database-layer failures that callers can safely translate into API
/// responses without exposing SQL, credentials, or driver internals.
#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("email is already registered")]
    DuplicateEmail,
    #[error("username is already registered")]
    DuplicateUsername,
    #[error("database constraint rejected the operation")]
    Constraint(#[source] sqlx::Error),
    #[error("database is temporarily unavailable")]
    Unavailable(#[source] sqlx::Error),
    #[error("database operation failed")]
    Unexpected(#[source] sqlx::Error),
}

impl DatabaseError {
    #[must_use]
    pub fn from_sqlx(error: sqlx::Error) -> Self {
        if let Some(database_error) = error.as_database_error() {
            return match database_error.constraint() {
                Some("users_email_unique_idx" | "users_email_key") => Self::DuplicateEmail,
                Some("users_username_unique_idx" | "users_username_key") => Self::DuplicateUsername,
                Some(_) => Self::Constraint(error),
                None => Self::Unexpected(error),
            };
        }

        if matches!(
            &error,
            sqlx::Error::Io(_)
                | sqlx::Error::Tls(_)
                | sqlx::Error::PoolTimedOut
                | sqlx::Error::PoolClosed
                | sqlx::Error::WorkerCrashed
        ) {
            Self::Unavailable(error)
        } else {
            Self::Unexpected(error)
        }
    }
}
